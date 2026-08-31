/*
 * This file is part of the Rust TNS modernization and is made available
 * under the Mozilla Public License Version 1.1. See the repository LICENSE
 * file for the complete terms.
 *
 * The Original Code is Rust TNS modernization.
 * The Initial Developer is TNS modernization contributors.
 * Portions created by the Initial Developer are Copyright (C) 2026
 * TNS modernization contributors. All Rights Reserved.
 */

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use tempfile::{Builder, NamedTempFile, TempDir};
use tns_core::{
    build_lua_tns, build_python_tns, build_tns, decode_entry, describe_container,
    validate_archive_name, CompressionMethod, MetadataStatus, NamedSource, ParseMode, ParseOptions,
    TimlpVersion, TnsContainer, TnsError, TnsWriteEntry, TnsWriteOptions,
};

#[derive(Debug, Parser)]
#[command(
    name = "tns",
    version,
    about = "Inspect, unpack, and build TI-Nspire TNS documents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse the outer container and print entry metadata and codec diagnostics.
    Inspect {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = CliMode::Tolerant)]
        mode: CliMode,
    },
    /// Decode every supported entry into an output directory.
    Unpack {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, value_enum, default_value_t = CliMode::Tolerant)]
        mode: CliMode,
    },
    /// Compare decoded entry bytes with a folder of XML/resources.
    Verify {
        input: PathBuf,
        expected: PathBuf,
        #[arg(long, value_enum, default_value_t = CliMode::Strict)]
        mode: CliMode,
    },
    /// Pack a folder containing XML documents and arbitrary resource files.
    PackXml {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "0601")]
        timlp: String,
        #[arg(long, value_enum, default_value_t = XmlMethod::Method13)]
        xml_method: XmlMethod,
    },
    /// Pack one UTF-8 Lua source file (or '-' for stdin) as a ScriptApp.
    PackLua {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "0601")]
        timlp: String,
    },
    /// Pack one or more Python files as a PythonEditor document.
    PackPython {
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "0601")]
        timlp: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Strict,
    Tolerant,
}

impl From<CliMode> for ParseMode {
    fn from(value: CliMode) -> Self {
        match value {
            CliMode::Strict => Self::Strict,
            CliMode::Tolerant => Self::Tolerant,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum XmlMethod {
    Stored,
    Method13,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tns: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { input, mode } => inspect(&input, mode.into())?,
        Command::Unpack {
            input,
            output,
            force,
            mode,
        } => unpack(&input, &output, force, mode.into())?,
        Command::Verify {
            input,
            expected,
            mode,
        } => verify(&input, &expected, mode.into())?,
        Command::PackXml {
            input,
            output,
            force,
            timlp,
            xml_method,
        } => pack_xml(&input, &output, force, &timlp, xml_method)?,
        Command::PackLua {
            input,
            output,
            force,
            timlp,
        } => pack_lua(&input, &output, force, &timlp)?,
        Command::PackPython {
            inputs,
            output,
            force,
            timlp,
        } => pack_python(&inputs, &output, force, &timlp)?,
    }
    Ok(())
}

fn inspect(input: &Path, mode: ParseMode) -> Result<(), Box<dyn std::error::Error>> {
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
    let data = read_file_bounded(input, options.max_input_size, "input")?;
    let container = TnsContainer::parse(&data, options)?;
    println!("{}", describe_container(&data, &container)?);
    let mut decode_errors = 0usize;
    for entry in &container.entries {
        match decode_entry(&data, entry, options) {
            Ok(decoded) => {
                let status = match decoded.metadata.status {
                    MetadataStatus::ValidFinal => "metadata=final-ok",
                    MetadataStatus::LegacyPayload => "metadata=legacy-payload",
                    MetadataStatus::Mismatch => "metadata=mismatch",
                };
                println!(
                    "  {}: decoded={} bytes {status}",
                    display_name(&entry.name),
                    decoded.bytes.len()
                );
                for warning in decoded.metadata.warnings {
                    eprintln!("warning: {warning}");
                }
            }
            Err(error) => {
                decode_errors += 1;
                println!("  {}: decode-error={error}", display_name(&entry.name));
            }
        }
    }
    if decode_errors == 0 {
        Ok(())
    } else {
        Err(format!(
            "{decode_errors} entr{} failed to decode",
            if decode_errors == 1 { "y" } else { "ies" }
        )
        .into())
    }
}

fn unpack(
    input: &Path,
    output: &Path,
    force: bool,
    mode: ParseMode,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.exists() && !force {
        return Err(format!(
            "output directory {} already exists; pass --force to replace it",
            output.display()
        )
        .into());
    }
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
    let data = read_file_bounded(input, options.max_input_size, "input")?;
    let container = TnsContainer::parse(&data, options)?;
    let parent = parent_directory(output);
    fs::create_dir_all(parent)?;
    let stage = Builder::new().prefix(".tns-unpack-").tempdir_in(parent)?;
    let mut names = HashSet::new();
    let mut total_decoded = 0usize;
    let mut written = Vec::with_capacity(container.entries.len());
    for entry in &container.entries {
        validate_archive_name(&entry.name)?;
        if !names.insert(entry.name.clone()) {
            return Err(format!("duplicate entry name {:?}", entry.name).into());
        }
        let decoded = decode_entry(&data, entry, options)?;
        total_decoded = total_decoded
            .checked_add(decoded.bytes.len())
            .ok_or("total decoded size overflow")?;
        if total_decoded > options.max_total_uncompressed_size {
            return Err(TnsError::LimitExceeded {
                kind: "total decoded output",
                actual: total_decoded as u64,
                limit: options.max_total_uncompressed_size as u64,
            }
            .into());
        }
        for warning in &decoded.metadata.warnings {
            eprintln!("warning: {warning}");
        }
        let destination = stage.path().join(&entry.name);
        if let Some(directory) = destination.parent() {
            fs::create_dir_all(directory)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        file.write_all(&decoded.bytes)?;
        file.sync_all()?;
        written.push((entry.name.clone(), decoded.bytes.len()));
    }
    if let Ok(directory) = File::open(stage.path()) {
        directory.sync_all()?;
    }
    publish_directory(stage, output, force)?;
    for (name, size) in written {
        println!("wrote {} ({size} bytes)", display_name(&name));
    }
    Ok(())
}

fn pack_xml(
    input: &Path,
    output: &Path,
    force: bool,
    timlp: &str,
    xml_method: XmlMethod,
) -> Result<(), Box<dyn std::error::Error>> {
    if !input.is_dir() {
        return Err(format!("XML input is not a directory: {}", input.display()).into());
    }
    let mut files = Vec::new();
    collect_files(input, input, &mut files)?;
    if files.is_empty() {
        return Err("XML input directory contains no regular files".into());
    }
    files.sort_by(|left, right| entry_order(&left.0, &right.0));
    let xml_compression = match xml_method {
        XmlMethod::Stored => CompressionMethod::Stored,
        XmlMethod::Method13 => CompressionMethod::TiMethod13,
    };
    let entries = files
        .into_iter()
        .map(|(name, data)| {
            let method = if name.to_ascii_lowercase().ends_with(".xml") {
                xml_compression
            } else {
                CompressionMethod::Deflate
            };
            TnsWriteEntry::new(name, data, method)
        })
        .collect::<Vec<_>>();
    let bytes = build_tns(&entries, &write_options(timlp)?)?;
    atomic_write(output, &bytes, force)?;
    println!("wrote {} ({} bytes)", output.display(), bytes.len());
    Ok(())
}

fn verify(
    input: &Path,
    expected: &Path,
    mode: ParseMode,
) -> Result<(), Box<dyn std::error::Error>> {
    if !expected.is_dir() {
        return Err(format!("expected folder is not a directory: {}", expected.display()).into());
    }
    let mut expected_files = Vec::new();
    collect_files(expected, expected, &mut expected_files)?;
    let expected_files = expected_files.into_iter().collect::<HashMap<_, _>>();
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
    let data = read_file_bounded(input, options.max_input_size, "input")?;
    let container = TnsContainer::parse(&data, options)?;
    let mut actual_names = HashSet::new();
    let mut mismatches = Vec::new();
    for entry in &container.entries {
        let decoded = decode_entry(&data, entry, options)?;
        for warning in &decoded.metadata.warnings {
            eprintln!("warning: {warning}");
        }
        actual_names.insert(entry.name.clone());
        match expected_files.get(&entry.name) {
            Some(want) if want == &decoded.bytes => {}
            Some(_) => mismatches.push(format!("content differs: {}", entry.name)),
            None => mismatches.push(format!("unexpected decoded entry: {}", entry.name)),
        }
    }
    for name in expected_files.keys() {
        if !actual_names.contains(name) {
            mismatches.push(format!("missing decoded entry: {name}"));
        }
    }
    if mismatches.is_empty() {
        println!(
            "verified {} against {} ({} entries)",
            input.display(),
            expected.display(),
            actual_names.len()
        );
        Ok(())
    } else {
        Err(format!("verification failed: {}", mismatches.join("; ")).into())
    }
}

fn pack_lua(
    input: &Path,
    output: &Path,
    force: bool,
    timlp: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = if input == Path::new("-") {
        let mut source = Vec::new();
        let limit = TnsWriteOptions::default().max_entry_size;
        io::stdin()
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut source)?;
        enforce_size("Lua source", source.len(), limit)?;
        source
    } else {
        read_file_bounded(
            input,
            TnsWriteOptions::default().max_entry_size,
            "Lua source",
        )?
    };
    let bytes = build_lua_tns(&source, &write_options(timlp)?)?;
    atomic_write(output, &bytes, force)?;
    println!("wrote {} ({} bytes)", output.display(), bytes.len());
    Ok(())
}

fn pack_python(
    inputs: &[PathBuf],
    output: &Path,
    force: bool,
    timlp: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_sources = u16::MAX as usize - 2;
    if inputs.len() > max_sources {
        return Err(TnsError::LimitExceeded {
            kind: "Python source count",
            actual: inputs.len() as u64,
            limit: max_sources as u64,
        }
        .into());
    }
    let mut names = HashSet::new();
    let mut sources = Vec::with_capacity(inputs.len());
    let limits = TnsWriteOptions::default();
    let mut total_size = 0usize;
    for input in inputs {
        let name = input
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("Python input has no valid file name: {}", input.display()))?
            .to_owned();
        if !names.insert(name.clone()) {
            return Err(format!("duplicate Python file name {name:?}").into());
        }
        let data = read_file_bounded(input, limits.max_entry_size, "Python source")?;
        total_size = total_size
            .checked_add(data.len())
            .ok_or("total Python source size overflow")?;
        enforce_size("total Python source", total_size, limits.max_total_size)?;
        sources.push(NamedSource { name, data });
    }
    let bytes = build_python_tns(&sources, &write_options(timlp)?)?;
    atomic_write(output, &bytes, force)?;
    println!("wrote {} ({} bytes)", output.display(), bytes.len());
    Ok(())
}

fn write_options(timlp: &str) -> Result<TnsWriteOptions, TnsError> {
    Ok(TnsWriteOptions {
        timlp_version: TimlpVersion::parse(timlp)?,
        ..TnsWriteOptions::default()
    })
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = TnsWriteOptions::default();
    let mut total_size = 0usize;
    collect_files_inner(
        root,
        directory,
        files,
        &mut total_size,
        limits.max_entry_size,
        limits.max_total_size,
    )
}

fn collect_files_inner(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
    total_size: &mut usize,
    max_entry_size: usize,
    max_total_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, io::Error>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let file_type = child.file_type()?;
        if file_type.is_dir() {
            collect_files_inner(
                root,
                &path,
                files,
                total_size,
                max_entry_size,
                max_total_size,
            )?;
        } else if file_type.is_file() {
            if files.len() >= u16::MAX as usize {
                return Err(TnsError::LimitExceeded {
                    kind: "entry count",
                    actual: files.len() as u64 + 1,
                    limit: u16::MAX as u64,
                }
                .into());
            }
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or_else(|| format!("input path is not valid UTF-8: {}", path.display()))?
                .replace('\\', "/");
            validate_archive_name(&relative)?;
            let data = read_file_bounded(&path, max_entry_size, "input entry")?;
            *total_size = total_size
                .checked_add(data.len())
                .ok_or("total input size overflow")?;
            enforce_size("total input size", *total_size, max_total_size)?;
            files.push((relative, data));
        } else {
            return Err(format!("unsupported input file type: {}", path.display()).into());
        }
    }
    Ok(())
}

fn entry_order(left: &str, right: &str) -> Ordering {
    fn key(name: &str) -> (u8, u32, &str) {
        let basename = name.rsplit('/').next().unwrap_or(name);
        if basename.eq_ignore_ascii_case("Document.xml") {
            return (0, 0, name);
        }
        let lower = basename.to_ascii_lowercase();
        if lower.starts_with("problem") && lower.ends_with(".xml") {
            let digits = &lower[7..lower.len() - 4];
            if let Ok(number) = digits.parse::<u32>() {
                return (1, number, name);
            }
        }
        (2, 0, name)
    }
    key(left).cmp(&key(right))
}

fn atomic_write(path: &Path, bytes: &[u8], force: bool) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() && !force {
        return Err(format!(
            "output file {} already exists; pass --force to replace it",
            path.display()
        )
        .into());
    }
    let parent = parent_directory(path);
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if force {
        temporary.persist(path).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
    }
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn read_file_bounded(
    path: &Path,
    limit: usize,
    kind: &'static str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    if let Ok(metadata) = file.metadata() {
        if metadata.len() > limit as u64 {
            return Err(TnsError::LimitExceeded {
                kind,
                actual: metadata.len(),
                limit: limit as u64,
            }
            .into());
        }
    }
    let mut data = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut data)?;
    enforce_size(kind, data.len(), limit)?;
    Ok(data)
}

fn enforce_size(kind: &'static str, actual: usize, limit: usize) -> Result<(), TnsError> {
    if actual > limit {
        return Err(TnsError::LimitExceeded {
            kind,
            actual: actual as u64,
            limit: limit as u64,
        });
    }
    Ok(())
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn display_name(name: &str) -> String {
    name.chars()
        .flat_map(|character| character.escape_default())
        .collect()
}

fn publish_directory(
    stage: TempDir,
    output: &Path,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stage_path = stage.keep();
    match rename_noreplace(&stage_path, output) {
        Ok(()) => {}
        Err(error) if force && error.kind() == io::ErrorKind::AlreadyExists => {
            if let Err(exchange_error) = exchange_paths(&stage_path, output) {
                if let Err(cleanup_error) = remove_path(&stage_path) {
                    return Err(format!(
                        "{exchange_error}; also failed to clean staged output {}: {cleanup_error}",
                        stage_path.display()
                    )
                    .into());
                }
                return Err(exchange_error.into());
            }
            if let Err(cleanup_error) = remove_path(&stage_path) {
                return match exchange_paths(&stage_path, output) {
                    Ok(()) => {
                        let _ = remove_path(&stage_path);
                        Err(format!(
                            "could not remove replaced output; replacement was rolled back: {cleanup_error}"
                        )
                        .into())
                    }
                    Err(rollback_error) => Err(format!(
                        "new output was published, but the old output remains at {}: {cleanup_error}; rollback also failed: {rollback_error}",
                        stage_path.display()
                    )
                    .into()),
                };
            }
        }
        Err(error) => {
            if let Err(cleanup_error) = remove_path(&stage_path) {
                return Err(format!(
                    "{error}; also failed to clean staged output {}: {cleanup_error}",
                    stage_path.display()
                )
                .into());
            }
            return Err(error.into());
        }
    }
    if let Ok(directory) = File::open(parent_directory(output)) {
        directory.sync_all()?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
))]
fn rename_noreplace(old: &Path, new: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        old,
        rustix::fs::CWD,
        new,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(io::Error::from)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
)))]
fn rename_noreplace(old: &Path, new: &Path) -> io::Result<()> {
    if new.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    fs::rename(old, new)
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
))]
fn exchange_paths(left: &Path, right: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        left,
        rustix::fs::CWD,
        right,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .map_err(io::Error::from)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
)))]
fn exchange_paths(left: &Path, right: &Path) -> io::Result<()> {
    let parent = parent_directory(right);
    let backup = Builder::new().prefix(".tns-replaced-").tempdir_in(parent)?;
    let backup_path = backup.keep();
    fs::remove_dir(&backup_path)?;
    fs::rename(right, &backup_path)?;
    if let Err(error) = fs::rename(left, right) {
        return match fs::rename(&backup_path, right) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(io::Error::other(format!(
                "publication failed: {error}; old-output rollback from {} also failed: {rollback_error}",
                backup_path.display()
            ))),
        };
    }
    if let Err(error) = fs::rename(&backup_path, left) {
        let move_new_error = fs::rename(right, left).err();
        let restore_old_error = fs::rename(&backup_path, right).err();
        return match (move_new_error, restore_old_error) {
            (None, None) => Err(error),
            (move_new_error, restore_old_error) => Err(io::Error::other(format!(
                "could not retain the old output for cleanup: {error}; new-output rollback error: {move_new_error:?}; old-output restore error: {restore_old_error:?}"
            ))),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_output_names_use_the_current_directory() {
        assert_eq!(parent_directory(Path::new("document.tns")), Path::new("."));
    }

    #[test]
    fn atomic_file_write_requires_force_and_preserves_old_bytes_on_refusal() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("document.tns");
        fs::write(&output, b"old").unwrap();
        assert!(atomic_write(&output, b"new", false).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"old");
        atomic_write(&output, b"new", true).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"new");
    }

    #[test]
    fn staged_directory_replacement_is_complete_and_force_gated() {
        let parent = tempfile::tempdir().unwrap();
        let output = parent.path().join("decoded");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("old.bin"), b"old").unwrap();

        let refused = Builder::new()
            .prefix(".tns-test-")
            .tempdir_in(parent.path())
            .unwrap();
        fs::write(refused.path().join("new.bin"), b"new").unwrap();
        assert!(publish_directory(refused, &output, false).is_err());
        assert_eq!(fs::read(output.join("old.bin")).unwrap(), b"old");
        assert!(!output.join("new.bin").exists());

        let replacement = Builder::new()
            .prefix(".tns-test-")
            .tempdir_in(parent.path())
            .unwrap();
        fs::write(replacement.path().join("new.bin"), b"new").unwrap();
        publish_directory(replacement, &output, true).unwrap();
        assert!(!output.join("old.bin").exists());
        assert_eq!(fs::read(output.join("new.bin")).unwrap(), b"new");
    }

    #[test]
    fn resource_collection_preserves_artifact_named_directories_and_is_bounded() {
        let input = tempfile::tempdir().unwrap();
        let artifacts = input.path().join("_artifacts");
        fs::create_dir(&artifacts).unwrap();
        fs::write(artifacts.join("resource.bin"), b"resource").unwrap();
        let mut files = Vec::new();
        collect_files(input.path(), input.path(), &mut files).unwrap();
        assert_eq!(
            files,
            vec![("_artifacts/resource.bin".into(), b"resource".to_vec())]
        );
        assert!(read_file_bounded(&artifacts.join("resource.bin"), 1, "test").is_err());
    }
}
