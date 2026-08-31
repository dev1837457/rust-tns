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

//! The ZIP-like outer TNS container and payload dispatch.

use std::collections::HashSet;
use std::fmt;

use crc32fast::hash;

use crate::compression::{deflate_raw, inflate_raw};
use crate::method13::{decode_method13_to_xml, encode_tixc_to_method13, Method13Options};
use crate::{CompressionMethod, Result, TixcLimits, TnsError};

const LOCAL_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
const CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const ZIP_EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const TIPD_SIGNATURE: &[u8; 4] = b"TIPD";
const TIMLP_PREFIX: &[u8; 6] = b"*TIMLP";
const DEFAULT_MAX_INPUT_SIZE: usize = 512 * 1024 * 1024;

/// Strict parsing rejects metadata mismatches; tolerant parsing preserves
/// compatibility with older Luna output and records a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    Strict,
    Tolerant,
}

/// Bounds applied to hostile or accidentally enormous containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    pub mode: ParseMode,
    pub max_input_size: usize,
    pub max_entries: usize,
    pub max_name_size: usize,
    pub max_entry_size: usize,
    pub max_total_uncompressed_size: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            mode: ParseMode::Tolerant,
            max_input_size: DEFAULT_MAX_INPUT_SIZE,
            max_entries: 65_535,
            max_name_size: 4_096,
            max_entry_size: 256 * 1024 * 1024,
            max_total_uncompressed_size: 512 * 1024 * 1024,
        }
    }
}

/// The four ASCII digits used in the first `*TIMLP####` local header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimlpVersion([u8; 4]);

impl TimlpVersion {
    pub const V0500: Self = Self(*b"0500");
    pub const V0601: Self = Self(*b"0601");

    pub fn parse(value: &str) -> Result<Self> {
        let bytes = value.as_bytes();
        if bytes.len() != 4 || !bytes.iter().all(u8::is_ascii_digit) {
            return Err(TnsError::field(
                "TIMLP version",
                "expected exactly four ASCII digits",
            ));
        }
        Ok(Self([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn as_bytes(self) -> [u8; 4] {
        self.0
    }
}

impl Default for TimlpVersion {
    fn default() -> Self {
        Self::V0601
    }
}

impl fmt::Display for TimlpVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(std::str::from_utf8(&self.0).expect("TIMLP digits are ASCII"))
    }
}

/// Which local-header family described an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalHeaderKind {
    Zip,
    Timlp(TimlpVersion),
}

/// Whether the central directory was the authoritative entry source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySource {
    CentralDirectory,
    LocalScan,
}

/// One parsed entry in the outer container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TnsEntry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub local_header_offset: usize,
    pub data_offset: usize,
    pub local_header_kind: LocalHeaderKind,
    pub source: EntrySource,
}

impl TnsEntry {
    pub fn data_end(&self) -> Result<usize> {
        self.data_offset
            .checked_add(self.compressed_size as usize)
            .ok_or_else(|| TnsError::field("entry data range", "offset overflow"))
    }

    pub fn compression_method(&self) -> Result<CompressionMethod> {
        CompressionMethod::try_from(self.method)
    }
}

/// Kind of end-of-central-directory marker observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EocdKind {
    Zip,
    Tipd,
}

/// Parsed TNS container metadata. The original bytes remain owned by the
/// caller, allowing payload access without copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TnsContainer {
    pub entries: Vec<TnsEntry>,
    pub first_timlp_version: Option<TimlpVersion>,
    pub eocd_kind: Option<EocdKind>,
    pub central_directory_offset: Option<usize>,
}

impl TnsContainer {
    pub fn parse(data: &[u8], options: ParseOptions) -> Result<Self> {
        if data.len() > options.max_input_size {
            return Err(TnsError::LimitExceeded {
                kind: "input",
                actual: data.len() as u64,
                limit: options.max_input_size as u64,
            });
        }

        let first_timlp_version = if data.starts_with(TIMLP_PREFIX) {
            Some(parse_timlp_version(data, 0)?.0)
        } else {
            None
        };
        let eocd = find_eocd(data, options.mode);
        if let Some((eocd_offset, kind)) = eocd {
            match parse_central_directory(data, eocd_offset, kind, options) {
                Ok((entries, central_directory_offset)) => {
                    return Ok(Self {
                        entries,
                        first_timlp_version,
                        eocd_kind: Some(kind),
                        central_directory_offset: Some(central_directory_offset),
                    });
                }
                Err(error) if options.mode == ParseMode::Strict => return Err(error),
                Err(_) => {}
            }
        } else if options.mode == ParseMode::Strict {
            return Err(TnsError::InvalidSignature {
                kind: "end-of-central-directory",
                offset: data.len(),
            });
        }

        let entries = scan_local_headers(data, options)?;
        if entries.is_empty() {
            return Err(TnsError::InvalidSignature {
                kind: "TNS local header",
                offset: 0,
            });
        }
        Ok(Self {
            entries,
            first_timlp_version,
            eocd_kind: eocd.map(|(_, kind)| kind),
            central_directory_offset: None,
        })
    }

    pub fn payload<'a>(&self, data: &'a [u8], entry: &TnsEntry) -> Result<&'a [u8]> {
        let end = entry.data_end()?;
        if end > data.len() || entry.data_offset > end {
            return Err(TnsError::truncated(
                entry.data_offset,
                end.saturating_sub(data.len()),
            ));
        }
        Ok(&data[entry.data_offset..end])
    }
}

/// Result of decoding one entry, including how ambiguous metadata was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryData {
    pub bytes: Vec<u8>,
    pub metadata: EntryMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMetadata {
    pub status: MetadataStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataStatus {
    ValidFinal,
    LegacyPayload,
    Mismatch,
}

/// Decode method 0, method 8, or method 13 and validate CRC/size metadata.
pub fn decode_entry(data: &[u8], entry: &TnsEntry, options: ParseOptions) -> Result<EntryData> {
    if data.len() > options.max_input_size {
        return Err(TnsError::LimitExceeded {
            kind: "input",
            actual: data.len() as u64,
            limit: options.max_input_size as u64,
        });
    }
    if entry.compressed_size as usize > options.max_entry_size {
        return Err(TnsError::LimitExceeded {
            kind: "compressed entry",
            actual: entry.compressed_size as u64,
            limit: options.max_entry_size as u64,
        });
    }
    let container = TnsContainer {
        entries: Vec::new(),
        first_timlp_version: None,
        eocd_kind: None,
        central_directory_offset: None,
    };
    let payload = container.payload(data, entry)?;
    let method = entry.compression_method()?;
    let (decoded, status, mut warnings) = match method {
        CompressionMethod::Stored => {
            let (status, warning) = check_final_metadata(entry, payload, payload, options.mode)?;
            (payload.to_vec(), status, warning.into_iter().collect())
        }
        CompressionMethod::Deflate => {
            let decoded = inflate_raw(payload, options.max_entry_size)?;
            let (status, warning) = check_final_metadata(entry, payload, &decoded, options.mode)?;
            (decoded, status, warning.into_iter().collect())
        }
        CompressionMethod::TiMethod13 => {
            let decoded = decode_method13_to_xml(
                payload,
                TixcLimits {
                    max_input_size: options.max_entry_size,
                    max_output_size: options.max_entry_size,
                    ..TixcLimits::default()
                },
            )?;
            let final_matches =
                entry.uncompressed_size as usize == decoded.len() && entry.crc32 == hash(&decoded);
            let payload_matches =
                entry.uncompressed_size as usize == payload.len() && entry.crc32 == hash(payload);
            if final_matches {
                (decoded, MetadataStatus::ValidFinal, Vec::new())
            } else if payload_matches {
                let warning = format!(
                    "{} uses legacy method-13 metadata: directory CRC/size describe the encoded payload ({} bytes), not final XML ({} bytes)",
                    entry.name,
                    payload.len(),
                    decoded.len()
                );
                if options.mode == ParseMode::Strict {
                    return Err(TnsError::MetadataMismatch {
                        name: entry.name.clone(),
                        detail: warning,
                    });
                }
                (decoded, MetadataStatus::LegacyPayload, vec![warning])
            } else {
                let detail = format!(
                    "directory CRC/size {:08x}/{} match neither final XML {:08x}/{} nor encoded payload {:08x}/{}",
                    entry.crc32,
                    entry.uncompressed_size,
                    hash(&decoded),
                    decoded.len(),
                    hash(payload),
                    payload.len()
                );
                if options.mode == ParseMode::Strict {
                    return Err(TnsError::MetadataMismatch {
                        name: entry.name.clone(),
                        detail,
                    });
                }
                (decoded, MetadataStatus::Mismatch, vec![detail])
            }
        }
    };
    if decoded.len() > options.max_entry_size {
        return Err(TnsError::LimitExceeded {
            kind: "decoded entry",
            actual: decoded.len() as u64,
            limit: options.max_entry_size as u64,
        });
    }
    Ok(EntryData {
        bytes: decoded,
        metadata: EntryMetadata {
            status,
            warnings: std::mem::take(&mut warnings),
        },
    })
}

fn check_final_metadata(
    entry: &TnsEntry,
    payload: &[u8],
    decoded: &[u8],
    mode: ParseMode,
) -> Result<(MetadataStatus, Option<String>)> {
    let final_matches = entry.uncompressed_size as usize == decoded.len()
        && entry.crc32 == hash(decoded)
        && entry.compressed_size as usize == payload.len();
    if final_matches {
        return Ok((MetadataStatus::ValidFinal, None));
    }
    let detail = format!(
        "directory CRC/size/compressed-size {:08x}/{}/{} do not match decoded {:08x}/{}/{}",
        entry.crc32,
        entry.uncompressed_size,
        entry.compressed_size,
        hash(decoded),
        decoded.len(),
        payload.len()
    );
    if mode == ParseMode::Strict {
        return Err(TnsError::MetadataMismatch {
            name: entry.name.clone(),
            detail,
        });
    }
    Ok((MetadataStatus::Mismatch, Some(detail)))
}

/// A high-level entry passed to [`build_tns`]. Method 13 entries take readable
/// XML here; the TIXC and encrypted payload are generated by the writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TnsWriteEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub method: CompressionMethod,
}

impl TnsWriteEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>, method: CompressionMethod) -> Self {
        Self {
            name: name.into(),
            data,
            method,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataStyle {
    Final,
    Payload,
}

/// Writer settings for TNS output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TnsWriteOptions {
    pub timlp_version: TimlpVersion,
    pub method13: Method13Options,
    pub resource_compression_level: u32,
    pub metadata_style: MetadataStyle,
    pub use_tipd_eocd: bool,
    pub max_name_size: usize,
    pub max_entry_size: usize,
    pub max_total_size: usize,
}

impl Default for TnsWriteOptions {
    fn default() -> Self {
        Self {
            timlp_version: TimlpVersion::default(),
            method13: Method13Options::default(),
            resource_compression_level: 9,
            metadata_style: MetadataStyle::Final,
            use_tipd_eocd: true,
            max_name_size: 4_096,
            max_entry_size: 256 * 1024 * 1024,
            max_total_size: 512 * 1024 * 1024,
        }
    }
}

struct PreparedEntry {
    name: Vec<u8>,
    payload: Vec<u8>,
    method: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
}

/// Build a TNS container with a first TIMLP local header, standard central
/// directory records, and a TIPD EOCD by default.
pub fn build_tns(entries: &[TnsWriteEntry], options: &TnsWriteOptions) -> Result<Vec<u8>> {
    if entries.is_empty() {
        return Err(TnsError::field(
            "TNS entries",
            "at least one entry is required",
        ));
    }
    if entries.len() > 65_535 {
        return Err(TnsError::LimitExceeded {
            kind: "entry count",
            actual: entries.len() as u64,
            limit: 65_535,
        });
    }
    if options.resource_compression_level > 9 {
        return Err(TnsError::field(
            "deflate level",
            options.resource_compression_level.to_string(),
        ));
    }
    if options.method13.compression_level > 9 {
        return Err(TnsError::field(
            "method-13 compression level",
            options.method13.compression_level.to_string(),
        ));
    }
    let mut total_input_size = 0usize;
    let mut names = HashSet::with_capacity(entries.len());
    let mut prepared = Vec::with_capacity(entries.len());
    for entry in entries {
        validate_archive_name(&entry.name)?;
        if !names.insert(entry.name.as_str()) {
            return Err(TnsError::field(
                "TNS entry name",
                format!("duplicate entry name {:?}", entry.name),
            ));
        }
        if entry.data.len() > options.max_entry_size {
            return Err(TnsError::LimitExceeded {
                kind: "input entry",
                actual: entry.data.len() as u64,
                limit: options.max_entry_size as u64,
            });
        }
        total_input_size = total_input_size
            .checked_add(entry.data.len())
            .ok_or_else(|| TnsError::field("total input size", "overflow"))?;
        if total_input_size > options.max_total_size {
            return Err(TnsError::LimitExceeded {
                kind: "total input size",
                actual: total_input_size as u64,
                limit: options.max_total_size as u64,
            });
        }
        let name = entry.name.as_bytes().to_vec();
        let max_name_size = options.max_name_size.min(u16::MAX as usize);
        if name.len() > max_name_size {
            return Err(TnsError::LimitExceeded {
                kind: "entry name",
                actual: name.len() as u64,
                limit: max_name_size as u64,
            });
        }
        let (payload, crc32, uncompressed_size) = match entry.method {
            CompressionMethod::Stored => (entry.data.clone(), hash(&entry.data), entry.data.len()),
            CompressionMethod::Deflate => (
                deflate_raw(&entry.data, options.resource_compression_level)?,
                hash(&entry.data),
                entry.data.len(),
            ),
            CompressionMethod::TiMethod13 => {
                let tixc = crate::encode_tixc(&entry.data)?;
                let payload = encode_tixc_to_method13(&tixc, &options.method13)?;
                match options.metadata_style {
                    MetadataStyle::Final => (payload, hash(&entry.data), entry.data.len()),
                    MetadataStyle::Payload => (payload.clone(), hash(&payload), payload.len()),
                }
            }
        };
        let compressed_size =
            u32::try_from(payload.len()).map_err(|_| TnsError::LimitExceeded {
                kind: "compressed entry",
                actual: payload.len() as u64,
                limit: u32::MAX as u64,
            })?;
        if payload.len() > options.max_entry_size {
            return Err(TnsError::LimitExceeded {
                kind: "compressed entry",
                actual: payload.len() as u64,
                limit: options.max_entry_size as u64,
            });
        }
        let uncompressed_size =
            u32::try_from(uncompressed_size).map_err(|_| TnsError::LimitExceeded {
                kind: "uncompressed entry",
                actual: uncompressed_size as u64,
                limit: u32::MAX as u64,
            })?;
        prepared.push(PreparedEntry {
            name,
            payload,
            method: entry.method.into(),
            crc32,
            compressed_size,
            uncompressed_size,
        });
    }

    let output_size = prepared
        .iter()
        .enumerate()
        .try_fold(22usize, |total, (index, entry)| {
            let local_header_size = if index == 0 { 36usize } else { 30usize };
            total
                .checked_add(local_header_size)
                .and_then(|value| value.checked_add(entry.name.len()))
                .and_then(|value| value.checked_add(entry.payload.len()))
                .and_then(|value| value.checked_add(46))
                .and_then(|value| value.checked_add(entry.name.len()))
                .ok_or_else(|| TnsError::field("TNS output size", "overflow"))
        })?;
    if output_size > options.max_total_size {
        return Err(TnsError::LimitExceeded {
            kind: "TNS output",
            actual: output_size as u64,
            limit: options.max_total_size as u64,
        });
    }
    if output_size > u32::MAX as usize {
        return Err(TnsError::LimitExceeded {
            kind: "TNS output",
            actual: output_size as u64,
            limit: u32::MAX as u64,
        });
    }
    let mut output = Vec::with_capacity(output_size);
    let mut local_offsets = Vec::with_capacity(prepared.len());
    for (index, entry) in prepared.iter().enumerate() {
        let local_offset = u32::try_from(output.len()).map_err(|_| TnsError::LimitExceeded {
            kind: "TNS offset",
            actual: output.len() as u64,
            limit: u32::MAX as u64,
        })?;
        local_offsets.push(local_offset);
        if index == 0 {
            output.extend_from_slice(TIMLP_PREFIX);
            output.extend_from_slice(&options.timlp_version.as_bytes());
        } else {
            output.extend_from_slice(LOCAL_SIGNATURE);
        }
        append_local_fixed_header(&mut output, entry)?;
        output.extend_from_slice(&entry.name);
        output.extend_from_slice(&entry.payload);
    }

    let central_offset = u32::try_from(output.len()).map_err(|_| TnsError::LimitExceeded {
        kind: "central directory offset",
        actual: output.len() as u64,
        limit: u32::MAX as u64,
    })?;
    for (entry, local_offset) in prepared.iter().zip(local_offsets.iter().copied()) {
        output.extend_from_slice(CENTRAL_SIGNATURE);
        put_u16(&mut output, 20);
        put_u16(&mut output, 20);
        let flags = if entry.name.iter().any(|byte| !byte.is_ascii()) {
            0x0800
        } else {
            0
        };
        put_u16(&mut output, flags);
        put_u16(&mut output, entry.method);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, entry.crc32);
        put_u32(&mut output, entry.compressed_size);
        put_u32(&mut output, entry.uncompressed_size);
        put_u16(&mut output, entry.name.len() as u16);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, 0x20);
        put_u32(&mut output, local_offset);
        output.extend_from_slice(&entry.name);
    }
    let central_size = output.len() - central_offset as usize;
    let central_size = u32::try_from(central_size).map_err(|_| TnsError::LimitExceeded {
        kind: "central directory",
        actual: central_size as u64,
        limit: u32::MAX as u64,
    })?;
    if options.use_tipd_eocd {
        output.extend_from_slice(TIPD_SIGNATURE);
    } else {
        output.extend_from_slice(ZIP_EOCD_SIGNATURE);
    }
    put_u16(&mut output, 0);
    put_u16(&mut output, 0);
    put_u16(&mut output, prepared.len() as u16);
    put_u16(&mut output, prepared.len() as u16);
    put_u32(&mut output, central_size);
    put_u32(&mut output, central_offset);
    put_u16(&mut output, 0);
    Ok(output)
}

fn append_local_fixed_header(output: &mut Vec<u8>, entry: &PreparedEntry) -> Result<()> {
    put_u16(output, 20);
    let flags = if entry.name.iter().any(|byte| !byte.is_ascii()) {
        0x0800
    } else {
        0
    };
    put_u16(output, flags);
    put_u16(output, entry.method);
    put_u16(output, 0);
    put_u16(output, 0);
    put_u32(output, entry.crc32);
    put_u32(output, entry.compressed_size);
    put_u32(output, entry.uncompressed_size);
    put_u16(output, entry.name.len() as u16);
    put_u16(output, 0);
    Ok(())
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub fn validate_archive_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(TnsError::UnsafePath(name.to_owned()));
    }
    if name.starts_with('/') || name.starts_with('\\') || name.contains('\\') || name.contains(':')
    {
        return Err(TnsError::UnsafePath(name.to_owned()));
    }
    for component in name.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(TnsError::UnsafePath(name.to_owned()));
        }
    }
    Ok(())
}

pub fn describe_container(data: &[u8], container: &TnsContainer) -> Result<String> {
    let mut lines = Vec::with_capacity(container.entries.len() + 1);
    let timlp = container
        .first_timlp_version
        .map_or_else(|| "none".to_owned(), |version| version.to_string());
    let eocd = match container.eocd_kind {
        Some(EocdKind::Tipd) => "TIPD",
        Some(EocdKind::Zip) => "PK EOCD",
        None => "local scan",
    };
    lines.push(format!(
        "container: entries={} first-timlp={} eocd={} central-offset={}",
        container.entries.len(),
        timlp,
        eocd,
        container
            .central_directory_offset
            .map_or_else(|| "none".to_owned(), |offset| format!("0x{offset:x}"))
    ));
    for entry in &container.entries {
        let payload = container.payload(data, entry)?;
        let crc_note = if entry.method == 0 {
            let actual = hash(payload);
            if actual == entry.crc32 {
                " crc=ok".to_owned()
            } else {
                format!(" crc-got={actual:08x}")
            }
        } else {
            String::new()
        };
        lines.push(format!(
            "{} method={} comp={} uncomp={} crc={:08x} local=0x{:x} data=0x{:x} source={:?}{}",
            display_name(&entry.name),
            entry.method,
            entry.compressed_size,
            entry.uncompressed_size,
            entry.crc32,
            entry.local_header_offset,
            entry.data_offset,
            entry.source,
            crc_note
        ));
    }
    Ok(lines.join("\n"))
}

fn display_name(name: &str) -> String {
    name.chars()
        .flat_map(|character| character.escape_default())
        .collect()
}

fn find_eocd(data: &[u8], mode: ParseMode) -> Option<(usize, EocdKind)> {
    if data.len() < 22 {
        return None;
    }
    for offset in (0..=data.len() - 22).rev() {
        let kind = if data[offset..].starts_with(TIPD_SIGNATURE) {
            EocdKind::Tipd
        } else if data[offset..].starts_with(ZIP_EOCD_SIGNATURE) {
            EocdKind::Zip
        } else {
            continue;
        };
        let Some(comment_len) = read_u16(data, offset + 20).ok().map(usize::from) else {
            continue;
        };
        let Some(end) = offset
            .checked_add(22)
            .and_then(|value| value.checked_add(comment_len))
        else {
            continue;
        };
        if end <= data.len() && (mode == ParseMode::Tolerant || end == data.len()) {
            return Some((offset, kind));
        }
    }
    None
}

fn parse_central_directory(
    data: &[u8],
    eocd_offset: usize,
    _kind: EocdKind,
    options: ParseOptions,
) -> Result<(Vec<TnsEntry>, usize)> {
    let disk_number = read_u16(data, eocd_offset + 4)?;
    let central_disk = read_u16(data, eocd_offset + 6)?;
    let disk_entries = read_u16(data, eocd_offset + 8)? as usize;
    let total_entries = read_u16(data, eocd_offset + 10)? as usize;
    let central_size = read_u32(data, eocd_offset + 12)? as usize;
    let central_offset = read_u32(data, eocd_offset + 16)? as usize;
    if disk_number != 0 || central_disk != 0 || disk_entries != total_entries {
        return Err(TnsError::Unsupported(
            "multi-disk or inconsistent EOCD records are not supported".into(),
        ));
    }
    if total_entries == 0 {
        return Err(TnsError::field(
            "central entry count",
            "a TNS container must contain at least one entry",
        ));
    }
    if total_entries > options.max_entries {
        return Err(TnsError::LimitExceeded {
            kind: "entry count",
            actual: total_entries as u64,
            limit: options.max_entries as u64,
        });
    }
    let candidates = [
        Some(central_offset),
        central_offset.checked_sub(6),
        central_offset.checked_add(6),
    ];
    let mut first_error = None;
    for candidate in candidates.into_iter().flatten() {
        match parse_central_at(
            data,
            candidate,
            eocd_offset,
            total_entries,
            central_size,
            options,
        ) {
            Ok(entries) => return Ok((entries, candidate)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    Err(first_error.unwrap_or(TnsError::InvalidSignature {
        kind: "central directory",
        offset: central_offset,
    }))
}

fn parse_central_at(
    data: &[u8],
    mut offset: usize,
    eocd_offset: usize,
    total_entries: usize,
    central_size: usize,
    options: ParseOptions,
) -> Result<Vec<TnsEntry>> {
    let central_start = offset;
    let central_end = offset
        .checked_add(central_size)
        .ok_or_else(|| TnsError::field("central directory", "range overflow"))?;
    if central_end > data.len() {
        return Err(TnsError::truncated(offset, central_end - data.len()));
    }
    if central_end != eocd_offset {
        return Err(TnsError::field(
            "central directory size",
            format!("directory ends at 0x{central_end:x}, EOCD starts at 0x{eocd_offset:x}"),
        ));
    }
    let mut entries = Vec::with_capacity(total_entries);
    let mut names = HashSet::with_capacity(total_entries);
    let mut local_offsets = HashSet::with_capacity(total_entries);
    let mut local_ranges = Vec::with_capacity(total_entries);
    let mut total_uncompressed = 0usize;
    for _ in 0..total_entries {
        require_signature(data, offset, CENTRAL_SIGNATURE, "central directory")?;
        let fixed_end = offset
            .checked_add(46)
            .ok_or_else(|| TnsError::field("central record", "offset overflow"))?;
        if fixed_end > data.len() {
            return Err(TnsError::truncated(offset, fixed_end - data.len()));
        }
        let flags = read_u16(data, offset + 8)?;
        let method = read_u16(data, offset + 10)?;
        let central_crc = read_u32(data, offset + 16)?;
        let central_csize = read_u32(data, offset + 20)?;
        let central_usize = read_u32(data, offset + 24)?;
        let name_len = read_u16(data, offset + 28)? as usize;
        let extra_len = read_u16(data, offset + 30)? as usize;
        let comment_len = read_u16(data, offset + 32)? as usize;
        let disk_start = read_u16(data, offset + 34)?;
        let local_offset = read_u32(data, offset + 42)? as usize;
        if disk_start != 0 {
            return Err(TnsError::Unsupported(
                "multi-disk central records are not supported".into(),
            ));
        }
        if name_len > options.max_name_size {
            return Err(TnsError::LimitExceeded {
                kind: "entry name",
                actual: name_len as u64,
                limit: options.max_name_size as u64,
            });
        }
        let name_start = fixed_end;
        let record_end = name_start
            .checked_add(name_len)
            .and_then(|value| value.checked_add(extra_len))
            .and_then(|value| value.checked_add(comment_len))
            .ok_or_else(|| TnsError::field("central record", "range overflow"))?;
        if record_end > data.len() {
            return Err(TnsError::truncated(name_start, record_end - data.len()));
        }
        if record_end > central_end {
            return Err(TnsError::field(
                "central record",
                format!("record with a {name_len}-byte name extends past the directory"),
            ));
        }
        let name = decode_name(
            &data[name_start..name_start + name_len],
            flags,
            options.mode,
        )?;
        validate_archive_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(TnsError::field(
                "central entry name",
                format!("duplicate entry name {name:?}"),
            ));
        }

        let mut local = None;
        for candidate in [
            Some(local_offset),
            local_offset.checked_sub(6),
            local_offset.checked_add(6),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(parsed) = parse_local(data, candidate, options) {
                local = Some((candidate, parsed));
                break;
            }
        }
        let (actual_local_offset, local) = local.ok_or_else(|| TnsError::InvalidField {
            kind: "central record",
            detail: format!("local header for {name:?} is not valid"),
        })?;
        if local.name_len != name_len
            || data.get(local.name_start..local.name_start + local.name_len)
                != data.get(name_start..name_start + name_len)
        {
            return Err(TnsError::field("local/central name", format!("{name:?}")));
        }
        if method != local.method
            || flags != local.flags
            || central_crc != local.crc32
            || central_csize != local.compressed_size
            || central_usize != local.uncompressed_size
        {
            return Err(TnsError::field(
                "local/central metadata",
                format!("fields differ for {name:?}"),
            ));
        }
        if !local_offsets.insert(actual_local_offset) {
            return Err(TnsError::field(
                "local header offset",
                format!("more than one central entry points to 0x{actual_local_offset:x}"),
            ));
        }
        let compressed_size = central_csize;
        let uncompressed_size = central_usize;
        let crc32 = central_crc;
        let end = local
            .data_offset
            .checked_add(compressed_size as usize)
            .ok_or_else(|| TnsError::field("entry data range", "offset overflow"))?;
        if end > data.len() {
            return Err(TnsError::truncated(local.data_offset, end - data.len()));
        }
        if actual_local_offset >= central_start || end > central_start {
            return Err(TnsError::field(
                "entry data range",
                format!("{name:?} overlaps the central directory"),
            ));
        }
        if local_ranges
            .iter()
            .any(|&(start, previous_end)| actual_local_offset < previous_end && start < end)
        {
            return Err(TnsError::field(
                "entry data range",
                format!("{name:?} overlaps another local record"),
            ));
        }
        local_ranges.push((actual_local_offset, end));
        if compressed_size as usize > options.max_entry_size {
            return Err(TnsError::LimitExceeded {
                kind: "compressed entry",
                actual: compressed_size as u64,
                limit: options.max_entry_size as u64,
            });
        }
        if uncompressed_size as usize > options.max_entry_size {
            return Err(TnsError::LimitExceeded {
                kind: "directory uncompressed entry",
                actual: uncompressed_size as u64,
                limit: options.max_entry_size as u64,
            });
        }
        total_uncompressed = total_uncompressed
            .checked_add(uncompressed_size as usize)
            .ok_or_else(|| TnsError::field("total uncompressed size", "overflow"))?;
        if total_uncompressed > options.max_total_uncompressed_size {
            return Err(TnsError::LimitExceeded {
                kind: "total uncompressed size",
                actual: total_uncompressed as u64,
                limit: options.max_total_uncompressed_size as u64,
            });
        }
        entries.push(TnsEntry {
            name,
            method,
            flags,
            crc32,
            compressed_size,
            uncompressed_size,
            local_header_offset: actual_local_offset,
            data_offset: local.data_offset,
            local_header_kind: local.kind,
            source: EntrySource::CentralDirectory,
        });
        offset = record_end;
    }
    if entries.len() != total_entries {
        return Err(TnsError::field("central entry count", "count mismatch"));
    }
    if offset != central_end {
        return Err(TnsError::field(
            "central directory size",
            format!("{} unparsed bytes remain", central_end - offset),
        ));
    }
    Ok(entries)
}

struct LocalRecord {
    kind: LocalHeaderKind,
    flags: u16,
    method: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    name_len: usize,
    name_start: usize,
    data_offset: usize,
}

fn parse_local(data: &[u8], offset: usize, options: ParseOptions) -> Result<LocalRecord> {
    let (fixed_offset, kind) = if data
        .get(offset..)
        .is_some_and(|bytes| bytes.starts_with(LOCAL_SIGNATURE))
    {
        (
            offset
                .checked_add(4)
                .ok_or_else(|| TnsError::field("local record", "offset overflow"))?,
            LocalHeaderKind::Zip,
        )
    } else if data
        .get(offset..)
        .is_some_and(|bytes| bytes.starts_with(TIMLP_PREFIX))
    {
        let fixed_offset = offset
            .checked_add(10)
            .ok_or_else(|| TnsError::field("local record", "offset overflow"))?;
        (
            fixed_offset,
            LocalHeaderKind::Timlp(parse_timlp_version(data, offset)?.0),
        )
    } else {
        return Err(TnsError::InvalidSignature {
            kind: "local header",
            offset,
        });
    };
    let fixed_end = fixed_offset
        .checked_add(26)
        .ok_or_else(|| TnsError::field("local record", "offset overflow"))?;
    if fixed_end > data.len() {
        return Err(TnsError::truncated(fixed_offset, fixed_end - data.len()));
    }
    let method = read_u16(data, fixed_offset + 4)?;
    let flags = read_u16(data, fixed_offset + 2)?;
    let supported_flags = 0x0800 | if method == 8 { 0x0006 } else { 0 };
    if flags & !supported_flags != 0 {
        return Err(TnsError::Unsupported(format!(
            "ZIP flags 0x{flags:04x} are not supported for method {method}"
        )));
    }
    let crc32 = read_u32(data, fixed_offset + 10)?;
    let compressed_size = read_u32(data, fixed_offset + 14)?;
    let uncompressed_size = read_u32(data, fixed_offset + 18)?;
    let name_len = read_u16(data, fixed_offset + 22)? as usize;
    let extra_len = read_u16(data, fixed_offset + 24)? as usize;
    if name_len > options.max_name_size {
        return Err(TnsError::LimitExceeded {
            kind: "entry name",
            actual: name_len as u64,
            limit: options.max_name_size as u64,
        });
    }
    if compressed_size as usize > options.max_entry_size {
        return Err(TnsError::LimitExceeded {
            kind: "compressed entry",
            actual: compressed_size as u64,
            limit: options.max_entry_size as u64,
        });
    }
    if uncompressed_size as usize > options.max_entry_size {
        return Err(TnsError::LimitExceeded {
            kind: "local uncompressed entry",
            actual: uncompressed_size as u64,
            limit: options.max_entry_size as u64,
        });
    }
    let name_start = fixed_end;
    let data_offset = name_start
        .checked_add(name_len)
        .and_then(|value| value.checked_add(extra_len))
        .ok_or_else(|| TnsError::field("local record", "data offset overflow"))?;
    let end = data_offset
        .checked_add(compressed_size as usize)
        .ok_or_else(|| TnsError::field("local record", "data range overflow"))?;
    if end > data.len() {
        return Err(TnsError::truncated(data_offset, end - data.len()));
    }
    Ok(LocalRecord {
        kind,
        flags,
        method,
        crc32,
        compressed_size,
        uncompressed_size,
        name_len,
        name_start,
        data_offset,
    })
}

fn scan_local_headers(data: &[u8], options: ParseOptions) -> Result<Vec<TnsEntry>> {
    let mut entries = Vec::new();
    let mut names = HashSet::new();
    let mut total_uncompressed = 0usize;
    let mut offset = 0usize;
    loop {
        let starts_local = data.get(offset..).is_some_and(|bytes| {
            bytes.starts_with(LOCAL_SIGNATURE) || bytes.starts_with(TIMLP_PREFIX)
        });
        if !starts_local {
            break;
        }
        if entries.len() >= options.max_entries {
            return Err(TnsError::LimitExceeded {
                kind: "entry count",
                actual: entries.len() as u64 + 1,
                limit: options.max_entries as u64,
            });
        }
        let local = parse_local(data, offset, options)?;
        let name_end = local
            .name_start
            .checked_add(local.name_len)
            .ok_or_else(|| TnsError::field("local name", "range overflow"))?;
        let name = decode_name(&data[local.name_start..name_end], local.flags, options.mode)?;
        validate_archive_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(TnsError::field(
                "local entry name",
                format!("duplicate entry name {name:?}"),
            ));
        }
        total_uncompressed = match total_uncompressed.checked_add(local.uncompressed_size as usize)
        {
            Some(value) => value,
            None => {
                return Err(TnsError::field("total uncompressed size", "overflow"));
            }
        };
        if total_uncompressed > options.max_total_uncompressed_size {
            return Err(TnsError::LimitExceeded {
                kind: "total uncompressed size",
                actual: total_uncompressed as u64,
                limit: options.max_total_uncompressed_size as u64,
            });
        }
        entries.push(TnsEntry {
            name,
            method: local.method,
            flags: local.flags,
            crc32: local.crc32,
            compressed_size: local.compressed_size,
            uncompressed_size: local.uncompressed_size,
            local_header_offset: offset,
            data_offset: local.data_offset,
            local_header_kind: local.kind,
            source: EntrySource::LocalScan,
        });
        offset = local
            .data_offset
            .checked_add(local.compressed_size as usize)
            .ok_or_else(|| TnsError::field("local record", "data range overflow"))?;
    }
    Ok(entries)
}

fn parse_timlp_version(data: &[u8], offset: usize) -> Result<(TimlpVersion, usize)> {
    let end = offset
        .checked_add(10)
        .ok_or_else(|| TnsError::field("TIMLP local header", "range overflow"))?;
    if end > data.len() {
        return Err(TnsError::truncated(offset, end - data.len()));
    }
    if !data[offset..].starts_with(TIMLP_PREFIX) {
        return Err(TnsError::InvalidSignature {
            kind: "TIMLP local header",
            offset,
        });
    }
    let version = std::str::from_utf8(&data[offset + 6..offset + 10])
        .map_err(|_| TnsError::field("TIMLP version", "not ASCII"))?;
    Ok((TimlpVersion::parse(version)?, 10))
}

fn decode_name(bytes: &[u8], flags: u16, mode: ParseMode) -> Result<String> {
    if flags & 0x0800 != 0 {
        return String::from_utf8(bytes.to_vec())
            .map_err(|_| TnsError::field("entry name", "invalid UTF-8"));
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(value) => Ok(value),
        Err(error) if mode == ParseMode::Tolerant => {
            Ok(String::from_utf8_lossy(error.as_bytes()).into_owned())
        }
        Err(_) => Err(TnsError::field(
            "entry name",
            "invalid UTF-8 without UTF-8 flag",
        )),
    }
}

fn require_signature(
    data: &[u8],
    offset: usize,
    signature: &[u8],
    kind: &'static str,
) -> Result<()> {
    if data
        .get(offset..)
        .is_none_or(|bytes| !bytes.starts_with(signature))
    {
        return Err(TnsError::InvalidSignature { kind, offset });
    }
    Ok(())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| TnsError::field("u16 field", "offset overflow"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| TnsError::truncated(offset, end.saturating_sub(data.len())))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| TnsError::field("u32 field", "offset overflow"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| TnsError::truncated(offset, end.saturating_sub(data.len())))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
