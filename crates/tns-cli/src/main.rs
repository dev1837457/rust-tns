/*
 * This file is part of the Rust TNS modernization and is made available
 * under the Mozilla Public License Version 1.1. See the repository LICENSE
 * file for the complete terms.
 */

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use tempfile::{Builder, NamedTempFile};
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
    let data = fs::read(input)?;
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
    let container = TnsContainer::parse(&data, options)?;
    println!("{}", describe_container(&data, &container)?);
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
                    entry.name,
                    decoded.bytes.len()
                );
                for warning in decoded.metadata.warnings {
                    eprintln!("warning: {warning}");
                }
            }
            Err(error) => println!("  {}: decode-error={error}", entry.name),
        }
    }
    Ok(())
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
    let data = fs::read(input)?;
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
    let container = TnsContainer::parse(&data, options)?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = Builder::new().prefix(".tns-unpack-").tempdir_in(parent)?;
    let mut names = HashSet::new();
    for entry in &container.entries {
        validate_archive_name(&entry.name)?;
        if !names.insert(entry.name.clone()) {
            return Err(format!("duplicate entry name {:?}", entry.name).into());
        }
        let decoded = decode_entry(&data, entry, options)?;
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
        println!(
            "wrote {} ({} bytes)",
            destination.strip_prefix(stage.path())?.display(),
            decoded.bytes.len()
        );
    }
    let stage_path = stage.keep();
    if output.exists() {
        if output.is_dir() {
            fs::remove_dir_all(output)?;
        } else {
            fs::remove_file(output)?;
        }
    }
    fs::rename(stage_path, output)?;
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
    let data = fs::read(input)?;
    let options = ParseOptions {
        mode,
        ..ParseOptions::default()
    };
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
        io::stdin().read_to_end(&mut source)?;
        source
    } else {
        fs::read(input)?
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
    let mut names = HashSet::new();
    let mut sources = Vec::with_capacity(inputs.len());
    for input in inputs {
        let name = input
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("Python input has no valid file name: {}", input.display()))?
            .to_owned();
        if !names.insert(name.clone()) {
            return Err(format!("duplicate Python file name {name:?}").into());
        }
        sources.push(NamedSource {
            name,
            data: fs::read(input)?,
        });
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
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, io::Error>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let file_type = child.file_type()?;
        if child.file_name() == "_artifacts" {
            continue;
        }
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            validate_archive_name(&relative)?;
            files.push((relative, fs::read(path)?));
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
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
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
