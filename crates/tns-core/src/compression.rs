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

use std::io::{Read, Write};

use flate2::bufread::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::{Result, TnsError};

pub(crate) fn deflate_raw(data: &[u8], level: u32) -> Result<Vec<u8>> {
    if level > 9 {
        return Err(TnsError::field("deflate level", level.to_string()));
    }
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(level));
    encoder
        .write_all(data)
        .map_err(|err| TnsError::Deflate(err.to_string()))?;
    encoder
        .finish()
        .map_err(|err| TnsError::Deflate(err.to_string()))
}

pub(crate) fn inflate_raw(data: &[u8], max_output: usize) -> Result<Vec<u8>> {
    let mut decoder = DeflateDecoder::new(data);
    let mut output = Vec::with_capacity(data.len().saturating_mul(2).min(max_output));
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|err| TnsError::Deflate(err.to_string()))?;
        if read == 0 {
            break;
        }
        if output.len().saturating_add(read) > max_output {
            return Err(TnsError::LimitExceeded {
                kind: "inflated output",
                actual: output.len().saturating_add(read) as u64,
                limit: max_output as u64,
            });
        }
        output.extend_from_slice(&buffer[..read]);
    }
    if decoder.total_in() != data.len() as u64 {
        return Err(TnsError::Deflate(format!(
            "raw stream ended after {} of {} bytes",
            decoder.total_in(),
            data.len()
        )));
    }
    Ok(output)
}
