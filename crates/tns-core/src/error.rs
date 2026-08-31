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

use std::path::PathBuf;

use thiserror::Error;

/// Result type used by the TNS core library.
pub type Result<T> = std::result::Result<T, TnsError>;

/// Errors returned while parsing, transforming, or writing TNS data.
#[derive(Debug, Error)]
pub enum TnsError {
    #[error("input is truncated at offset 0x{offset:x}: need {needed} more bytes")]
    Truncated { offset: usize, needed: usize },

    #[error("invalid {kind} signature at offset 0x{offset:x}")]
    InvalidSignature { kind: &'static str, offset: usize },

    #[error("invalid {kind} field: {detail}")]
    InvalidField { kind: &'static str, detail: String },

    #[error("unsupported compression method {0}")]
    UnsupportedMethod(u16),

    #[error("unsupported TNS feature: {0}")]
    Unsupported(String),

    #[error("resource limit exceeded for {kind}: {actual} > {limit}")]
    LimitExceeded {
        kind: &'static str,
        actual: u64,
        limit: u64,
    },

    #[error("unsafe archive path {0:?}")]
    UnsafePath(String),

    #[error("metadata mismatch for {name}: {detail}")]
    MetadataMismatch { name: String, detail: String },

    #[error("CRC-32 mismatch for {name}: expected {expected:08x}, got {actual:08x}")]
    CrcMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },

    #[error("deflate stream error: {0}")]
    Deflate(String),

    #[error("TIXC error: {0}")]
    Tixc(String),

    #[error("method-13 error: {0}")]
    Method13(String),

    #[error("XML error: {0}")]
    Xml(String),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl TnsError {
    pub(crate) fn truncated(offset: usize, needed: usize) -> Self {
        Self::Truncated { offset, needed }
    }

    pub(crate) fn field(kind: &'static str, detail: impl Into<String>) -> Self {
        Self::InvalidField {
            kind,
            detail: detail.into(),
        }
    }

    pub(crate) fn tixc(detail: impl Into<String>) -> Self {
        Self::Tixc(detail.into())
    }

    pub(crate) fn method13(detail: impl Into<String>) -> Self {
        Self::Method13(detail.into())
    }
}
