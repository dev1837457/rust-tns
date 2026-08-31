/*
 * This file is part of the Rust TNS modernization.
 *
 * The contents of this file are subject to the Mozilla Public License
 * Version 1.1 (the "License"); you may not use this file except in
 * compliance with the License. You may obtain a copy of the License at
 * https://www.mozilla.org/MPL/1.1/.
 *
 * Software distributed under the License is distributed on an "AS IS"
 * basis, WITHOUT WARRANTY OF ANY KIND, either express or implied. See the
 * License for the specific language governing rights and limitations under
 * the License.
 *
 * The Original Code is Rust TNS modernization.
 * The Initial Developer of the Original Code is TNS modernization contributors.
 * Portions created by the Initial Developer are Copyright (C) 2026
 * TNS modernization contributors. All Rights Reserved.
 */

//! Safe, bounded building blocks for TI-Nspire `.tns` documents.
//!
//! The crate deliberately keeps the ZIP-like outer container separate from
//! the TI XML codecs. This makes it possible to inspect or preserve resource
//! entries without attempting to interpret them.

mod compression;
mod error;
mod method13;
mod outer;
mod templates;
mod tixc;

pub use error::{Result, TnsError};
pub use method13::{
    decode_method13_to_tixc, decode_method13_to_tixc_with_limit, decode_method13_to_xml,
    encode_tixc_to_method13, Method13Options,
};
pub use outer::{
    build_tns, decode_entry, describe_container, validate_archive_name, EntryData, EntryMetadata,
    EntrySource, EocdKind, LocalHeaderKind, MetadataStatus, MetadataStyle, ParseMode, ParseOptions,
    TimlpVersion, TnsContainer, TnsEntry, TnsWriteEntry, TnsWriteOptions,
};
pub use templates::{
    build_lua_tns, build_python_tns, default_document_xml, lua_problem_xml, python_problem_xml,
    NamedSource,
};
pub use tixc::{decode_tixc, decode_tixc_with_limits, encode_tixc, TixcLimits};

/// ZIP method identifiers used by TNS files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum CompressionMethod {
    Stored = 0,
    Deflate = 8,
    TiMethod13 = 13,
}

impl TryFrom<u16> for CompressionMethod {
    type Error = TnsError;

    fn try_from(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::Stored),
            8 => Ok(Self::Deflate),
            13 => Ok(Self::TiMethod13),
            other => Err(TnsError::UnsupportedMethod(other)),
        }
    }
}

impl From<CompressionMethod> for u16 {
    fn from(value: CompressionMethod) -> Self {
        value as u16
    }
}
