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

//! Minimal browser-facing facade. The core remains usable from native Rust;
//! this crate only converts byte slices and string errors into wasm-bindgen
//! types.

use tns_core::{decode_entry, describe_container, ParseMode, ParseOptions, TnsContainer};
use wasm_bindgen::prelude::*;

fn options(tolerant: bool) -> ParseOptions {
    ParseOptions {
        mode: if tolerant {
            ParseMode::Tolerant
        } else {
            ParseMode::Strict
        },
        max_input_size: 64 * 1024 * 1024,
        max_entry_size: 32 * 1024 * 1024,
        max_total_uncompressed_size: 64 * 1024 * 1024,
        ..ParseOptions::default()
    }
}

#[wasm_bindgen]
pub fn inspect_tns(data: &[u8], tolerant: bool) -> Result<String, JsValue> {
    let container = TnsContainer::parse(data, options(tolerant)).map_err(error)?;
    describe_container(data, &container).map_err(error)
}

#[wasm_bindgen]
pub fn decode_tns_entry(data: &[u8], index: usize, tolerant: bool) -> Result<Vec<u8>, JsValue> {
    let parse_options = options(tolerant);
    let container = TnsContainer::parse(data, parse_options).map_err(error)?;
    let entry = container
        .entries
        .get(index)
        .ok_or_else(|| JsValue::from_str("entry index out of range"))?;
    decode_entry(data, entry, parse_options)
        .map(|decoded| decoded.bytes)
        .map_err(error)
}

fn error(error: tns_core::TnsError) -> JsValue {
    JsValue::from_str(&error.to_string())
}
