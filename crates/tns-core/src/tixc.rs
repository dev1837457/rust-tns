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

//! Encoder and decoder for the TIXC0100 XML token stream.
//!
//! TIXC is a small byte-oriented grammar used inside method-13 entries. It
//! has two dictionaries (tag and attribute names), shorthand text tables, and
//! a compact UTF-8 representation. This module intentionally implements the
//! grammar directly so malformed input can be rejected before it causes an
//! unbounded allocation or an ambiguous XML result.

use std::collections::{HashMap, HashSet};

use crate::{Result, TnsError};

const TAG_NAME_TERMINATORS: &[u8] = &[0x0e, 0x0f, b' ', b'/', b'>'];
const ETA_TABLE: &[u8; 32] = b" etaionsrhAlcduFmfpgybwvkxqjz,.'";
const DIGIT_TABLE: &[u8; 32] = b"0123456789AN,.EFx; (){}[]^+-/*PB";
const CDATA_OPEN: &[u8] = b"<![CDATA[";
const CDATA_CLOSE: &[u8] = b"]]>";

/// Resource limits applied while expanding a TIXC stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TixcLimits {
    pub max_input_size: usize,
    pub max_output_size: usize,
    pub max_tag_name_size: usize,
    pub max_attribute_name_size: usize,
    pub max_depth: usize,
}

impl Default for TixcLimits {
    fn default() -> Self {
        Self {
            max_input_size: 256 * 1024 * 1024,
            max_output_size: 256 * 1024 * 1024,
            max_tag_name_size: 64,
            max_attribute_name_size: 256,
            max_depth: 1024,
        }
    }
}

/// Expand a TIXC0100 stream to the canonical readable XML form.
pub fn decode_tixc(data: &[u8]) -> Result<Vec<u8>> {
    decode_tixc_with_limits(data, TixcLimits::default())
}

/// Expand a TIXC0100 stream with caller-selected allocation limits.
pub fn decode_tixc_with_limits(data: &[u8], limits: TixcLimits) -> Result<Vec<u8>> {
    if data.len() > limits.max_input_size {
        return Err(TnsError::LimitExceeded {
            kind: "TIXC input",
            actual: data.len() as u64,
            limit: limits.max_input_size as u64,
        });
    }
    if !data.starts_with(b"TIXC0100") {
        return Err(TnsError::tixc("stream does not start with TIXC0100"));
    }
    let dash = 8;
    if data.get(dash) != Some(&b'-') {
        return Err(TnsError::tixc("missing version separator in header"));
    }
    let question = find_byte(data, b'?', dash + 1)
        .ok_or_else(|| TnsError::tixc("missing header terminator"))?;
    if question + 1 >= data.len() || &data[question..question + 2] != b"?>" {
        return Err(TnsError::tixc("invalid TIXC header terminator"));
    }
    let version = &data[dash + 1..question];
    if version != b"1.0" {
        return Err(TnsError::tixc("only XML version 1.0 is supported"));
    }

    let declaration_capacity = b"<?xml version=\"\" encoding=\"UTF-8\" ?>".len() + version.len();
    let mut output = Vec::with_capacity(declaration_capacity.min(limits.max_output_size));
    append_limited(
        &mut output,
        b"<?xml version=\"",
        limits.max_output_size,
        "TIXC output",
    )?;
    append_limited(&mut output, version, limits.max_output_size, "TIXC output")?;
    append_limited(
        &mut output,
        b"\" encoding=\"UTF-8\" ?>",
        limits.max_output_size,
        "TIXC output",
    )?;

    let mut tag_dictionary: Vec<Vec<u8>> = Vec::with_capacity(256);
    let mut attribute_dictionary: Vec<Vec<u8>> = Vec::with_capacity(256);
    let mut token = Vec::new();
    let mut text_decoder = TextDecoder::default();
    let mut state = 1u8;
    let mut cdata_match = 0usize;
    let mut pos = question + 2;
    let mut element_stack: Vec<Vec<u8>> = Vec::new();
    let mut current_start_tag: Option<Vec<u8>> = None;
    let mut current_attributes: HashSet<Vec<u8>> = HashSet::new();
    let mut saw_root = false;
    let mut root_closed = false;

    while pos < data.len() {
        let byte = data[pos];
        match state {
            1 => {
                if byte != b'<' {
                    return Err(TnsError::tixc("expected root '<' after TIXC header"));
                }
                append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                pos += 1;
                state = 2;
            }
            2 => {
                if TAG_NAME_TERMINATORS.contains(&byte) {
                    let had_tag_name = !token.is_empty();
                    let tag_name = if had_tag_name {
                        Some(token.clone())
                    } else {
                        None
                    };
                    if !token.is_empty() {
                        if token.len() > limits.max_tag_name_size {
                            return Err(TnsError::tixc(format!(
                                "tag token exceeds {} bytes",
                                limits.max_tag_name_size
                            )));
                        }
                        if tag_dictionary.len() < 256 {
                            tag_dictionary.push(std::mem::take(&mut token));
                        } else {
                            token.clear();
                        }
                    }
                    if let Some(tag_name) = tag_name {
                        validate_xml_name(&tag_name, "tag")?;
                        current_start_tag = Some(tag_name);
                    }
                    match byte {
                        0x0e => {
                            append_limited(
                                &mut output,
                                b"></",
                                limits.max_output_size,
                                "TIXC output",
                            )?;
                            complete_start_tag(
                                &mut element_stack,
                                &mut current_start_tag,
                                &mut current_attributes,
                                &mut saw_root,
                                &mut root_closed,
                                false,
                                limits.max_depth,
                            )?;
                            state = 12;
                        }
                        0x0f => {
                            append_limited(
                                &mut output,
                                b" ",
                                limits.max_output_size,
                                "TIXC output",
                            )?;
                            state = 13;
                        }
                        b' ' => {
                            if !had_tag_name && output.ends_with(b"<") {
                                return Err(TnsError::tixc("empty tag before attribute separator"));
                            }
                            append_limited(
                                &mut output,
                                b" ",
                                limits.max_output_size,
                                "TIXC output",
                            )?;
                            state = 4;
                        }
                        b'/' => {
                            append_limited(
                                &mut output,
                                b"/",
                                limits.max_output_size,
                                "TIXC output",
                            )?;
                            state = if had_tag_name { 9 } else { 10 };
                        }
                        b'>' => {
                            if token.is_empty() && output.ends_with(b"<") {
                                return Err(TnsError::tixc("empty tag before '>'"));
                            }
                            append_limited(
                                &mut output,
                                b">",
                                limits.max_output_size,
                                "TIXC output",
                            )?;
                            complete_start_tag(
                                &mut element_stack,
                                &mut current_start_tag,
                                &mut current_attributes,
                                &mut saw_root,
                                &mut root_closed,
                                false,
                                limits.max_depth,
                            )?;
                            state = 3;
                        }
                        _ => unreachable!(),
                    }
                    pos += 1;
                } else {
                    if token.len() >= limits.max_tag_name_size {
                        return Err(TnsError::tixc(format!(
                            "tag token exceeds {} bytes",
                            limits.max_tag_name_size
                        )));
                    }
                    token.push(byte);
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                }
            }
            3 => {
                text_decoder.reset();
                if byte == b'<' {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 17;
                } else if byte == 0x0c {
                    append_limited(&mut output, b"<", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 11;
                } else if byte == 0x0e {
                    append_limited(&mut output, b"</", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 12;
                } else {
                    token.clear();
                    state = 8;
                }
            }
            4 => {
                if byte == b'=' {
                    if token.is_empty() {
                        return Err(TnsError::tixc("empty attribute name"));
                    }
                    validate_xml_name(&token, "attribute")?;
                    if !current_attributes.insert(token.clone()) {
                        return Err(TnsError::tixc("duplicate attribute name"));
                    }
                    if attribute_dictionary.len() < 256 {
                        attribute_dictionary.push(std::mem::take(&mut token));
                    } else {
                        token.clear();
                    }
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 5;
                } else {
                    if token.len() >= limits.max_attribute_name_size {
                        return Err(TnsError::tixc(format!(
                            "attribute token exceeds {} bytes",
                            limits.max_attribute_name_size
                        )));
                    }
                    token.push(byte);
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                }
            }
            5 => {
                pos += 1;
                if byte == b'"' {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    state = 6;
                } else if byte != b' ' {
                    return Err(TnsError::tixc(
                        "expected quote or space after attribute '='",
                    ));
                }
            }
            6 => {
                if byte == b'"' && text_decoder.state == 0 {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 7;
                } else {
                    text_decoder.decode_byte(byte, &mut output, limits.max_output_size)?;
                    pos += 1;
                }
            }
            7 => match byte {
                b'>' => {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    complete_start_tag(
                        &mut element_stack,
                        &mut current_start_tag,
                        &mut current_attributes,
                        &mut saw_root,
                        &mut root_closed,
                        false,
                        limits.max_depth,
                    )?;
                    pos += 1;
                    state = 3;
                }
                b'/' => {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 10;
                }
                b' ' => {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 4;
                }
                0x0f => {
                    append_limited(&mut output, b" ", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 13;
                }
                _ => {
                    return Err(TnsError::tixc(format!(
                        "unexpected byte 0x{byte:02x} after attribute value"
                    )))
                }
            },
            8 => {
                if byte == b'<' && text_decoder.state == 0 {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 2;
                } else if byte == 0x0c && text_decoder.state == 0 {
                    append_limited(&mut output, b"<", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 11;
                } else if byte == 0x0e && text_decoder.state == 0 {
                    append_limited(&mut output, b"</", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    token.clear();
                    state = 12;
                } else {
                    text_decoder.decode_byte(byte, &mut output, limits.max_output_size)?;
                    pos += 1;
                }
            }
            9 => {
                if byte != b'>' {
                    return Err(TnsError::tixc("expected '>' after '/'"));
                }
                append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                complete_start_tag(
                    &mut element_stack,
                    &mut current_start_tag,
                    &mut current_attributes,
                    &mut saw_root,
                    &mut root_closed,
                    true,
                    limits.max_depth,
                )?;
                pos += 1;
                state = 3;
            }
            10 => {
                if byte != b'>' {
                    return Err(TnsError::tixc("expected '>' after '/'"));
                }
                append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                complete_start_tag(
                    &mut element_stack,
                    &mut current_start_tag,
                    &mut current_attributes,
                    &mut saw_root,
                    &mut root_closed,
                    true,
                    limits.max_depth,
                )?;
                pos += 1;
                state = 3;
            }
            11 => {
                let index = byte as usize;
                let tag = tag_dictionary.get(index).ok_or_else(|| {
                    TnsError::tixc(format!("tag dictionary index {index} out of range"))
                })?;
                append_limited(&mut output, tag, limits.max_output_size, "TIXC output")?;
                current_start_tag = Some(tag.clone());
                pos += 1;
                state = 14;
            }
            12 => {
                let index = byte as usize;
                let tag = tag_dictionary.get(index).ok_or_else(|| {
                    TnsError::tixc(format!("tag dictionary index {index} out of range"))
                })?;
                append_limited(&mut output, tag, limits.max_output_size, "TIXC output")?;
                append_limited(&mut output, b">", limits.max_output_size, "TIXC output")?;
                close_tag(&mut element_stack, &mut root_closed, tag)?;
                pos += 1;
                state = 3;
            }
            13 => {
                let index = byte as usize;
                let attribute = attribute_dictionary.get(index).ok_or_else(|| {
                    TnsError::tixc(format!("attribute dictionary index {index} out of range"))
                })?;
                if !current_attributes.insert(attribute.clone()) {
                    return Err(TnsError::tixc("duplicate attribute name"));
                }
                append_limited(
                    &mut output,
                    attribute,
                    limits.max_output_size,
                    "TIXC output",
                )?;
                append_limited(&mut output, b"=\"", limits.max_output_size, "TIXC output")?;
                pos += 1;
                text_decoder.reset();
                state = 6;
            }
            14 => match byte {
                b' ' => {
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 4;
                }
                b'/' => {
                    pos += 1;
                    state = 15;
                }
                0x0f => {
                    append_limited(&mut output, b" ", limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    state = 13;
                }
                _ => {
                    append_limited(&mut output, b">", limits.max_output_size, "TIXC output")?;
                    complete_start_tag(
                        &mut element_stack,
                        &mut current_start_tag,
                        &mut current_attributes,
                        &mut saw_root,
                        &mut root_closed,
                        false,
                        limits.max_depth,
                    )?;
                    state = 3;
                }
            },
            15 => {
                if byte == b'>' {
                    append_limited(&mut output, b"/>", limits.max_output_size, "TIXC output")?;
                    complete_start_tag(
                        &mut element_stack,
                        &mut current_start_tag,
                        &mut current_attributes,
                        &mut saw_root,
                        &mut root_closed,
                        true,
                        limits.max_depth,
                    )?;
                    pos += 1;
                    state = 3;
                } else {
                    return Err(TnsError::tixc("expected '>' after '/'"));
                }
            }
            17 => {
                if byte == b'!' {
                    if element_stack.is_empty() || root_closed {
                        return Err(TnsError::tixc("CDATA is only allowed inside an element"));
                    }
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    cdata_match = 2;
                    state = 18;
                } else {
                    state = 2;
                }
            }
            18 => {
                if cdata_match >= CDATA_OPEN.len() {
                    state = 19;
                    token.clear();
                } else {
                    if byte != CDATA_OPEN[cdata_match] {
                        return Err(TnsError::tixc("invalid CDATA opener"));
                    }
                    append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                    pos += 1;
                    cdata_match += 1;
                }
            }
            19 => {
                append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                pos += 1;
                if byte == b']' {
                    cdata_match = 1;
                    state = 20;
                }
            }
            20 => {
                append_limited(&mut output, &[byte], limits.max_output_size, "TIXC output")?;
                pos += 1;
                if cdata_match < CDATA_CLOSE.len() && byte == CDATA_CLOSE[cdata_match] {
                    cdata_match += 1;
                } else {
                    state = 19;
                    cdata_match = 0;
                }
                if cdata_match == CDATA_CLOSE.len() {
                    state = 3;
                    cdata_match = 0;
                }
            }
            other => return Err(TnsError::tixc(format!("unsupported parser state {other}"))),
        }
    }

    if state != 3 {
        return Err(TnsError::tixc(format!(
            "stream ended in parser state {state}"
        )));
    }
    if !saw_root || !root_closed || !element_stack.is_empty() {
        return Err(TnsError::tixc(
            "stream ended before the XML root was closed",
        ));
    }
    if !has_only_xml_characters(&output) {
        return Err(TnsError::tixc(
            "decoded XML is not valid UTF-8 XML 1.0 text",
        ));
    }
    Ok(output)
}

/// Encode canonical UTF-8 XML into a TIXC0100 stream.
pub fn encode_tixc(xml: &[u8]) -> Result<Vec<u8>> {
    let (version, body) = parse_xml_declaration(xml)?;
    let mut output = Vec::with_capacity(xml.len());
    output.extend_from_slice(b"TIXC0100-");
    output.extend_from_slice(version);
    output.extend_from_slice(b"?>");

    let mut tag_dictionary: HashMap<Vec<u8>, u8> = HashMap::new();
    let mut next_tag_index = 0u16;
    let mut position = 0usize;
    while position < body.len() {
        if body[position..].starts_with(CDATA_OPEN) {
            let content_start = position + CDATA_OPEN.len();
            let close_relative = find_window(&body[content_start..], CDATA_CLOSE)
                .ok_or_else(|| TnsError::tixc("unterminated CDATA section"))?;
            let end = content_start + close_relative + CDATA_CLOSE.len();
            output.extend_from_slice(&body[position..end]);
            position = end;
            continue;
        }

        if body[position] != b'<' {
            let end = body[position..]
                .iter()
                .position(|byte| *byte == b'<')
                .map_or(body.len(), |offset| position + offset);
            encode_text(&body[position..end], &mut output)?;
            position = end;
            continue;
        }

        if body[position..].starts_with(b"</") {
            let end = find_byte(body, b'>', position + 2)
                .ok_or_else(|| TnsError::tixc("unterminated closing tag"))?;
            let name = &body[position + 2..end];
            let index = *tag_dictionary.get(name).ok_or_else(|| {
                TnsError::tixc(format!("close tag has no dictionary entry: {name:?}"))
            })?;
            output.extend_from_slice(&[0x0e, index]);
            position = end + 1;
            continue;
        }
        if body[position..].starts_with(b"<?") {
            return Err(TnsError::tixc(
                "processing instructions are supported only in the XML declaration",
            ));
        }
        if body[position..].starts_with(b"<!") {
            return Err(TnsError::tixc("only CDATA markup is supported"));
        }

        let name_start = position + 1;
        let tag_name_end = name_end(body, name_start);
        if tag_name_end == name_start {
            return Err(TnsError::tixc(format!(
                "empty tag name at offset {position}"
            )));
        }
        let name = &body[name_start..tag_name_end];
        if name.len() > 64 {
            return Err(TnsError::tixc("tag name exceeds 64 bytes"));
        }
        let used_tag_reference = if let Some(index) = tag_dictionary.get(name) {
            output.extend_from_slice(&[0x0c, *index]);
            true
        } else {
            if next_tag_index >= 256 {
                return Err(TnsError::tixc("tag dictionary exhausted"));
            }
            let index = next_tag_index as u8;
            next_tag_index += 1;
            tag_dictionary.insert(name.to_vec(), index);
            output.push(b'<');
            output.extend_from_slice(name);
            false
        };

        let mut cursor = tag_name_end;
        let mut has_attributes = false;
        loop {
            cursor = skip_whitespace(body, cursor);
            if cursor >= body.len() {
                return Err(TnsError::tixc("unterminated start tag"));
            }
            if body[cursor..].starts_with(b"/>") {
                output.extend_from_slice(b"/>");
                position = cursor + 2;
                break;
            }
            if body[cursor] == b'>' {
                if !used_tag_reference || has_attributes {
                    output.push(b'>');
                }
                position = cursor + 1;
                break;
            }

            let attribute_start = cursor;
            let attribute_end = name_end(body, cursor);
            if attribute_end == attribute_start {
                return Err(TnsError::tixc(format!(
                    "empty attribute name at offset {cursor}"
                )));
            }
            let attribute = &body[attribute_start..attribute_end];
            cursor = skip_whitespace(body, attribute_end);
            if cursor >= body.len() || body[cursor] != b'=' {
                return Err(TnsError::tixc(format!(
                    "expected '=' after attribute {attribute:?}"
                )));
            }
            cursor = skip_whitespace(body, cursor + 1);
            if cursor >= body.len() || body[cursor] != b'"' {
                return Err(TnsError::tixc(format!(
                    "expected double quote for attribute {attribute:?}"
                )));
            }
            let value_start = cursor + 1;
            let value_end = find_byte(body, b'"', value_start).ok_or_else(|| {
                TnsError::tixc(format!("unterminated value for attribute {attribute:?}"))
            })?;
            // TI's own expander has been observed to truncate at 0x0f
            // attribute references even though existing decoders understand
            // the token. Keep generated streams conservative and literal.
            output.push(b' ');
            output.extend_from_slice(attribute);
            output.extend_from_slice(b"=\"");
            encode_text(&body[value_start..value_end], &mut output)?;
            output.push(b'"');
            has_attributes = true;
            cursor = value_end + 1;
        }
    }

    let decoded = decode_tixc_with_limits(
        &output,
        TixcLimits {
            max_output_size: xml.len().saturating_add(1),
            ..TixcLimits::default()
        },
    )?;
    if decoded != xml {
        let first = decoded
            .iter()
            .zip(xml.iter())
            .position(|(left, right)| left != right)
            .unwrap_or(decoded.len().min(xml.len()));
        let got_start = first.saturating_sub(48);
        let want_start = first.saturating_sub(48);
        let got_end = (first + 96).min(decoded.len());
        let want_end = (first + 96).min(xml.len());
        return Err(TnsError::tixc(format!(
            "internal encode/decode mismatch at byte {first}: got {:?}, expected {:?}",
            &decoded[got_start..got_end],
            &xml[want_start..want_end]
        )));
    }
    Ok(output)
}

fn parse_xml_declaration(xml: &[u8]) -> Result<(&[u8], &[u8])> {
    if !xml.starts_with(b"<?xml version=\"") {
        return Err(TnsError::Xml(
            "XML must start with <?xml version=\"...\" encoding=\"UTF-8\" ?>".into(),
        ));
    }
    if !has_only_xml_characters(xml) {
        return Err(TnsError::Xml(
            "XML input is not valid UTF-8 XML 1.0 text".into(),
        ));
    }
    let version_start = b"<?xml version=\"".len();
    let version_end = find_byte(xml, b'"', version_start)
        .ok_or_else(|| TnsError::Xml("unterminated XML version".into()))?;
    let declaration_end = find_window(xml, b"?>")
        .ok_or_else(|| TnsError::Xml("unterminated XML declaration".into()))?;
    let expected_prefix_end = version_end + b"\" encoding=\"UTF-8\" ".len();
    if expected_prefix_end != declaration_end
        || &xml[version_end..declaration_end] != b"\" encoding=\"UTF-8\" "
    {
        return Err(TnsError::Xml(
            "XML declaration must use canonical UTF-8 form with a trailing space".into(),
        ));
    }
    let version = &xml[version_start..version_end];
    if version != b"1.0" {
        return Err(TnsError::Xml("only XML version 1.0 is supported".into()));
    }
    Ok((version, &xml[declaration_end + 2..]))
}

fn encode_text(data: &[u8], output: &mut Vec<u8>) -> Result<()> {
    let mut position = 0usize;
    while position < data.len() {
        let byte = data[position];
        if byte < 0x80 {
            let (digit_end, digit_indexes) = table_run(data, position, true);
            let (eta_end, eta_indexes) = table_run(data, position, false);
            if digit_indexes.len() >= 6 && digit_indexes.len() >= eta_indexes.len() {
                pack_table_run(0xf0, 0x0f, &digit_indexes, output);
                position = digit_end;
                continue;
            }
            if eta_indexes.len() >= 6 {
                pack_table_run(0xa0, 0x0a, &eta_indexes, output);
                position = eta_end;
                continue;
            }
            if byte == b'\t' || byte == b'\n' || byte == b'\r' || (0x20..=0x7e).contains(&byte) {
                output.push(byte);
                position += 1;
                continue;
            }
            return Err(TnsError::tixc(format!(
                "unsupported XML text control byte 0x{byte:02x}"
            )));
        }

        let size = if (0xc0..=0xdf).contains(&byte) {
            2
        } else if (0xe0..=0xef).contains(&byte) {
            3
        } else if (0xf0..=0xf7).contains(&byte) {
            4
        } else {
            return Err(TnsError::tixc(format!(
                "invalid UTF-8 lead byte 0x{byte:02x}"
            )));
        };
        if position + size > data.len() {
            return Err(TnsError::tixc("truncated UTF-8 sequence"));
        }
        let sequence = &data[position..position + size];
        if std::str::from_utf8(sequence).is_err() {
            return Err(TnsError::tixc("invalid UTF-8 sequence"));
        }
        match size {
            2 => {
                output.push((sequence[0] & 0x1c) >> 2);
                output.push(((sequence[0] & 0x03) << 6) | (sequence[1] & 0x3f));
            }
            3 => {
                output.push(0x80);
                output.push(((sequence[0] & 0x0f) << 4) | ((sequence[1] & 0x3c) >> 2));
                output.push(((sequence[1] & 0x03) << 6) | (sequence[2] & 0x3f));
            }
            4 => {
                output.push(0x08);
                output.push(((sequence[0] & 0x07) << 2) | ((sequence[1] & 0x30) >> 4));
                output.push(((sequence[1] & 0x0f) << 4) | ((sequence[2] & 0x3c) >> 2));
                output.push(((sequence[2] & 0x03) << 6) | (sequence[3] & 0x3f));
            }
            _ => unreachable!(),
        }
        position += size;
    }
    Ok(())
}

fn table_run(data: &[u8], start: usize, digit: bool) -> (usize, Vec<u8>) {
    let table = if digit { DIGIT_TABLE } else { ETA_TABLE };
    let end = if digit { 14 } else { 15 };
    let mut position = start;
    let mut indexes = Vec::new();
    while position < data.len() {
        let index = table[..end]
            .iter()
            .position(|candidate| *candidate == data[position]);
        match index {
            Some(index)
                if (digit && data[position] != b'A' && data[position] != b'N')
                    || (!digit && data[position] != b'A') =>
            {
                indexes.push(index as u8);
                position += 1;
            }
            _ => break,
        }
    }
    (position, indexes)
}

fn pack_table_run(prefix: u8, terminator: u8, indexes: &[u8], output: &mut Vec<u8>) {
    if indexes.is_empty() {
        return;
    }
    output.push(prefix | indexes[0]);
    let mut position = 1usize;
    while position + 1 < indexes.len() {
        output.push((indexes[position] << 4) | indexes[position + 1]);
        position += 2;
    }
    if position < indexes.len() {
        output.push((indexes[position] << 4) | terminator);
    } else {
        output.push(terminator << 4);
    }
}

#[derive(Debug, Default)]
struct TextDecoder {
    state: u8,
    count: u8,
    temporary: u8,
}

impl TextDecoder {
    fn reset(&mut self) {
        self.state = 0;
        self.count = 0;
        self.temporary = 0;
    }

    fn decode_byte(&mut self, byte: u8, output: &mut Vec<u8>, limit: usize) -> Result<()> {
        match self.state {
            0 => {
                if byte < 0xa0 {
                    let needs_utf =
                        (byte < 0x20 && ((0xffff_d9ffu32 >> byte) & 1) != 0) || byte >= 0x7f;
                    if needs_utf {
                        if byte & 0xf0 == 0x80 {
                            self.count = (byte & 0x0f) + 1;
                            self.state = 28;
                            return Ok(());
                        }
                        if byte >= 8 {
                            match byte {
                                8 => {
                                    self.state = 30;
                                    return Ok(());
                                }
                                11 => {
                                    self.state = 33;
                                    return Ok(());
                                }
                                _ => {
                                    return Err(TnsError::tixc(format!(
                                        "unsupported text control byte 0x{byte:02x}"
                                    )))
                                }
                            }
                        }
                        self.temporary = ((byte & 7) | 0xf0) << 2;
                        self.count = 1;
                        self.state = 27;
                        return Ok(());
                    }
                    append_limited(output, &[byte], limit, "TIXC output")?;
                    return Ok(());
                }
                if byte & 0xf0 == 0xa0 {
                    let character = ETA_TABLE[(byte & 0x0f) as usize];
                    match character {
                        b'F' => self.state = 24,
                        b'A' => self.state = 0,
                        _ => {
                            self.state = 23;
                            append_limited(output, &[character], limit, "TIXC output")?;
                        }
                    }
                    return Ok(());
                }
                if byte & 0xf0 == 0xf0 {
                    let character = DIGIT_TABLE[(byte & 0x0f) as usize];
                    match character {
                        b'A' => self.state = 26,
                        b'F' => self.state = 0,
                        _ => {
                            self.state = 25;
                            append_special_digit(output, character, limit)?;
                        }
                    }
                    return Ok(());
                }
                return Err(TnsError::tixc(format!(
                    "unsupported text byte 0x{byte:02x}"
                )));
            }
            23 => {
                let first = ETA_TABLE[(byte >> 4) as usize];
                if first == b'F' {
                    append_limited(
                        output,
                        &[ETA_TABLE[((byte & 0x0f) + 16) as usize]],
                        limit,
                        "TIXC output",
                    )?;
                    return Ok(());
                }
                if first == b'A' {
                    self.state = 0;
                    return Ok(());
                }
                append_limited(output, &[first], limit, "TIXC output")?;
                let second = ETA_TABLE[(byte & 0x0f) as usize];
                if second == b'F' {
                    self.state = 24;
                } else if second == b'A' {
                    self.state = 0;
                } else {
                    append_limited(output, &[second], limit, "TIXC output")?;
                }
            }
            24 => {
                append_limited(
                    output,
                    &[ETA_TABLE[((byte >> 4) + 16) as usize]],
                    limit,
                    "TIXC output",
                )?;
                let second = ETA_TABLE[(byte & 0x0f) as usize];
                if second == b'F' {
                } else if second == b'A' {
                    self.state = 0;
                } else {
                    append_limited(output, &[second], limit, "TIXC output")?;
                    self.state = 23;
                }
            }
            25 => {
                let first = DIGIT_TABLE[(byte >> 4) as usize];
                if first == b'A' {
                    let second = DIGIT_TABLE[((byte & 0x0f) + 16) as usize];
                    append_special_digit(output, second, limit)?;
                    return Ok(());
                }
                if first == b'F' {
                    self.state = 0;
                    return Ok(());
                }
                append_special_digit(output, first, limit)?;
                let second = DIGIT_TABLE[(byte & 0x0f) as usize];
                if second == b'A' {
                    self.state = 26;
                } else if second == b'F' {
                    self.state = 0;
                } else {
                    append_special_digit(output, second, limit)?;
                }
            }
            26 => {
                let first = DIGIT_TABLE[((byte >> 4) + 16) as usize];
                append_special_digit(output, first, limit)?;
                let second = DIGIT_TABLE[(byte & 0x0f) as usize];
                if second == b'A' {
                } else if second == b'F' {
                    self.state = 0;
                } else {
                    self.state = 25;
                    append_special_digit(output, second, limit)?;
                }
            }
            27 => {
                self.state = 0;
                self.count = 0;
                append_limited(
                    output,
                    &[self.temporary | (byte >> 6), (byte & 0x3f) | 0x80],
                    limit,
                    "TIXC output",
                )?;
            }
            28 => {
                self.state = 29;
                self.temporary = byte;
                append_limited(output, &[(byte >> 4) | 0xe0], limit, "TIXC output")?;
            }
            29 => {
                self.count = self.count.saturating_sub(1);
                if self.count == 0 {
                    self.state = 0;
                } else {
                    self.state = 28;
                }
                append_limited(
                    output,
                    &[
                        (byte >> 6) | (((self.temporary & 0x0f) | 0xe0) << 2),
                        (byte & 0x3f) | 0x80,
                    ],
                    limit,
                    "TIXC output",
                )?;
            }
            30 => {
                self.temporary = byte;
                self.state = 31;
                if byte >= 0x20 {
                    self.count = 2;
                    append_limited(output, &[0xf8, (byte >> 2) | 0x80], limit, "TIXC output")?;
                } else {
                    self.count = 1;
                    append_limited(output, &[(byte >> 2 & 7) | 0xf0], limit, "TIXC output")?;
                }
            }
            31 | 35 => {
                self.count = 1;
                self.state = 32;
                append_limited(
                    output,
                    &[(byte >> 4) | (((self.temporary & 3) | 0xf8) << 4)],
                    limit,
                    "TIXC output",
                )?;
                self.temporary = byte;
            }
            32 | 36 => {
                self.state = 0;
                self.count = 0;
                append_limited(
                    output,
                    &[
                        (byte >> 6) | (((self.temporary & 0x0f) | 0xe0) << 2),
                        (byte & 0x3f) | 0x80,
                    ],
                    limit,
                    "TIXC output",
                )?;
            }
            33 => {
                self.state = 34;
                if byte >= 4 {
                    append_limited(
                        output,
                        &[u8::from(byte & 0x40 != 0) | 0xfc, (byte & 0x3f) | 0x80],
                        limit,
                        "TIXC output",
                    )?;
                } else {
                    append_limited(output, &[(byte & 3) | 0xf8], limit, "TIXC output")?;
                }
            }
            34 => {
                self.state = 35;
                self.temporary = byte;
                self.count = 1;
                append_limited(output, &[(byte >> 2) | 0x80], limit, "TIXC output")?;
            }
            other => return Err(TnsError::tixc(format!("unsupported text state {other}"))),
        }
        Ok(())
    }
}

fn append_special_digit(output: &mut Vec<u8>, byte: u8, limit: usize) -> Result<()> {
    let bytes = match byte {
        b'N' => "−".as_bytes(),
        b'E' => &[0xef, 0x80, 0x80],
        b'P' => b"))",
        b'B' => b"][",
        _ => std::slice::from_ref(&byte),
    };
    append_limited(output, bytes, limit, "TIXC output")
}

fn append_limited(
    output: &mut Vec<u8>,
    bytes: &[u8],
    limit: usize,
    kind: &'static str,
) -> Result<()> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(TnsError::LimitExceeded {
            kind,
            actual: u64::MAX,
            limit: limit as u64,
        })?;
    if new_len > limit {
        return Err(TnsError::LimitExceeded {
            kind,
            actual: new_len as u64,
            limit: limit as u64,
        });
    }
    output.extend_from_slice(bytes);
    Ok(())
}

fn complete_start_tag(
    element_stack: &mut Vec<Vec<u8>>,
    current_start_tag: &mut Option<Vec<u8>>,
    current_attributes: &mut HashSet<Vec<u8>>,
    saw_root: &mut bool,
    root_closed: &mut bool,
    self_closing: bool,
    max_depth: usize,
) -> Result<()> {
    let tag = current_start_tag
        .take()
        .ok_or_else(|| TnsError::tixc("start tag terminator has no tag name"))?;
    current_attributes.clear();
    if *root_closed && element_stack.is_empty() {
        return Err(TnsError::tixc("multiple XML roots are not supported"));
    }
    *saw_root = true;
    if !self_closing {
        if element_stack.len() >= max_depth {
            return Err(TnsError::LimitExceeded {
                kind: "TIXC nesting depth",
                actual: element_stack.len() as u64 + 1,
                limit: max_depth as u64,
            });
        }
        element_stack.push(tag);
    } else if element_stack.is_empty() {
        *root_closed = true;
    }
    Ok(())
}

fn validate_xml_name(name: &[u8], kind: &'static str) -> Result<()> {
    let name = std::str::from_utf8(name)
        .map_err(|_| TnsError::tixc(format!("{kind} name is not valid UTF-8")))?;
    let mut characters = name.chars();
    let first = characters
        .next()
        .ok_or_else(|| TnsError::tixc(format!("empty {kind} name")))?;
    if !is_xml_name_start(first) || characters.any(|character| !is_xml_name_character(character)) {
        return Err(TnsError::tixc(format!("invalid XML {kind} name {name:?}")));
    }
    Ok(())
}

fn is_xml_name_start(character: char) -> bool {
    matches!(character,
        ':' | 'A'..='Z' | '_' | 'a'..='z'
            | '\u{00c0}'..='\u{00d6}' | '\u{00d8}'..='\u{00f6}'
            | '\u{00f8}'..='\u{02ff}' | '\u{0370}'..='\u{037d}'
            | '\u{037f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}'
            | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}'
            | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}'
            | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}

fn is_xml_name_character(character: char) -> bool {
    is_xml_name_start(character)
        || matches!(character,
            '-' | '.' | '0'..='9' | '\u{00b7}' | '\u{0300}'..='\u{036f}'
                | '\u{203f}'..='\u{2040}')
}

pub(crate) fn has_only_xml_characters(data: &[u8]) -> bool {
    std::str::from_utf8(data).is_ok_and(|text| {
        text.chars().all(|character| {
            matches!(character,
                '\u{0009}' | '\u{000a}' | '\u{000d}'
                    | '\u{0020}'..='\u{d7ff}'
                    | '\u{e000}'..='\u{fffd}'
                    | '\u{10000}'..='\u{10ffff}')
        })
    })
}

fn close_tag(element_stack: &mut Vec<Vec<u8>>, root_closed: &mut bool, tag: &[u8]) -> Result<()> {
    let expected = element_stack
        .pop()
        .ok_or_else(|| TnsError::tixc("closing tag has no open element"))?;
    if expected != tag {
        return Err(TnsError::tixc(format!(
            "closing tag {:?} does not match {:?}",
            tag, expected
        )));
    }
    if element_stack.is_empty() {
        *root_closed = true;
    }
    Ok(())
}

fn find_byte(data: &[u8], value: u8, start: usize) -> Option<usize> {
    data.get(start..)?
        .iter()
        .position(|byte| *byte == value)
        .map(|offset| start + offset)
}

fn find_window(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    data.windows(needle.len())
        .position(|window| window == needle)
}

fn name_end(data: &[u8], start: usize) -> usize {
    data.get(start..)
        .and_then(|rest| {
            rest.iter()
                .position(|byte| b" \t\r\n/=>".contains(byte))
                .map(|offset| start + offset)
        })
        .unwrap_or(data.len())
}

fn skip_whitespace(data: &[u8], mut position: usize) -> usize {
    while position < data.len() && b" \t\r\n".contains(&data[position]) {
        position += 1;
    }
    position
}
