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

//! Semantic XML templates used by the loose-source packers.

use std::collections::HashSet;

use crate::outer::{build_tns, validate_archive_name};
use crate::tixc::has_only_xml_characters;
use crate::{CompressionMethod, Result, TnsError, TnsWriteEntry, TnsWriteOptions};

const XML_DECL: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" ?>";
const LUA_HEADER: &[u8] =
    br#"<prob xmlns="urn:TI.Problem" ver="1.0" pbname=""><sym></sym><card clay="0" h1="10000" h2="10000" w1="10000" w2="10000"><isDummyCard>0</isDummyCard><flag>0</flag><wdgt xmlns:sc="urn:TI.ScriptApp" type="TI.ScriptApp" ver="1.0"><sc:mFlags>0</sc:mFlags><sc:value>-1</sc:value><sc:script version="512" id="0"><![CDATA["#;
const LUA_FOOTER: &[u8] = br#"]]></sc:script></wdgt></card></prob>"#;
const CDATA_RESTART: &[u8] = b"]]]]><![CDATA[>";

const DEFAULT_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" ?><doc ver="1.0"><settings><assessmentMode>0</assessmentMode><lastview>0</lastview><readOnlyMode>0</readOnlyMode><lang>en</lang><dfmt>0</dfmt><tfmt>0</tfmt><curr>0</curr><expf>1</expf><ddig>7</ddig><angf>1</angf><exapp>1</exapp><cplxf>1</cplxf><unit>1</unit><vectf>1</vectf><base>1</base><gg_ddig>4</gg_ddig><gg_graphang>1</gg_graphang><gg_geomang>2</gg_geomang><gg_axisendv>1</gg_axisendv><gg_tooltipfunman>0</gg_tooltipfunman><gg_autopoi>1</gg_autopoi><gg_calcmenu>0</gg_calcmenu><gg_hideplotlabels>0</gg_hideplotlabels></settings><rights></rights><nps>1</nps><imgsize>0</imgsize></doc>"#;

/// The minimal document metadata used for generated ScriptApp/PythonEditor
/// documents. It is readable XML rather than a copied compressed token blob.
pub fn default_document_xml() -> &'static [u8] {
    DEFAULT_DOCUMENT
}

/// Construct a ScriptApp problem containing one Lua source in CDATA.
pub fn lua_problem_xml(source: &[u8]) -> Result<Vec<u8>> {
    lua_problem_xml_with_limit(source, TnsWriteOptions::default().max_entry_size)
}

fn lua_problem_xml_with_limit(source: &[u8], max_output_size: usize) -> Result<Vec<u8>> {
    let source = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
    if !has_only_xml_characters(source) {
        return Err(TnsError::Xml(
            "Lua source must be UTF-8 without XML 1.0-forbidden characters".into(),
        ));
    }
    let restart_count = source.windows(3).filter(|window| *window == b"]]>").count();
    let output_size = XML_DECL
        .len()
        .checked_add(LUA_HEADER.len())
        .and_then(|size| size.checked_add(source.len()))
        .and_then(|size| size.checked_add(LUA_FOOTER.len()))
        .and_then(|size| {
            restart_count
                .checked_mul(CDATA_RESTART.len() - 3)
                .and_then(|growth| size.checked_add(growth))
        })
        .ok_or_else(|| TnsError::field("Lua problem XML", "size overflow"))?;
    if output_size > max_output_size {
        return Err(TnsError::LimitExceeded {
            kind: "Lua problem XML",
            actual: output_size as u64,
            limit: max_output_size as u64,
        });
    }
    let mut output = Vec::with_capacity(output_size);
    output.extend_from_slice(XML_DECL);
    output.extend_from_slice(LUA_HEADER);
    append_cdata_safe(&mut output, source);
    output.extend_from_slice(LUA_FOOTER);
    debug_assert_eq!(output.len(), output_size);
    Ok(output)
}

/// Construct the PythonEditor problem that selects `first_name` on open.
pub fn python_problem_xml(first_name: &str) -> Result<Vec<u8>> {
    validate_archive_name(first_name)?;
    if first_name.len() > 240 {
        return Err(TnsError::LimitExceeded {
            kind: "Python entry name",
            actual: first_name.len() as u64,
            limit: 240,
        });
    }
    if !first_name.to_ascii_lowercase().ends_with(".py") {
        return Err(TnsError::Xml("Python entry name must end in .py".into()));
    }
    let mut output = Vec::with_capacity(XML_DECL.len() + first_name.len() + 512);
    output.extend_from_slice(XML_DECL);
    output.extend_from_slice(
        br#"<prob xmlns="urn:TI.Problem" ver="1.0" pbname=""><sym></sym><card clay="0" h1="10000" h2="10000" w1="10000" w2="10000"><isDummyCard>0</isDummyCard><flag>0</flag><wdgt xmlns:py="urn:TI.PythonEditor" type="TI.PythonEditor" ver="1.0"><py:data><py:name>"#,
    );
    append_xml_text(&mut output, first_name.as_bytes());
    output.extend_from_slice(
        br#"</py:name><py:dirf>-10000000</py:dirf><py:mFlags>1024</py:mFlags><py:value>10</py:value></py:data></wdgt></card></prob>"#,
    );
    Ok(output)
}

/// Build the two XML entries required for a loose Lua source.
pub fn build_lua_tns(source: &[u8], options: &TnsWriteOptions) -> Result<Vec<u8>> {
    if source.len() > options.max_entry_size {
        return Err(TnsError::LimitExceeded {
            kind: "Lua source",
            actual: source.len() as u64,
            limit: options.max_entry_size as u64,
        });
    }
    let problem = lua_problem_xml_with_limit(source, options.max_entry_size)?;
    build_tns(
        &[
            TnsWriteEntry::new(
                "Document.xml",
                default_document_xml().to_vec(),
                CompressionMethod::TiMethod13,
            ),
            TnsWriteEntry::new("Problem1.xml", problem, CompressionMethod::TiMethod13),
        ],
        options,
    )
}

/// A named source file used by [`build_python_tns`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSource {
    pub name: String,
    pub data: Vec<u8>,
}

/// Build a PythonEditor document and one deflated entry per Python source.
pub fn build_python_tns(sources: &[NamedSource], options: &TnsWriteOptions) -> Result<Vec<u8>> {
    let first = sources
        .first()
        .ok_or_else(|| TnsError::field("Python sources", "at least one .py file is required"))?;
    if sources.len() > u16::MAX as usize - 2 {
        return Err(TnsError::LimitExceeded {
            kind: "Python source count",
            actual: sources.len() as u64,
            limit: (u16::MAX as usize - 2) as u64,
        });
    }
    let mut names = HashSet::with_capacity(sources.len());
    let mut total_size = 0usize;
    for source in sources {
        validate_archive_name(&source.name)?;
        if source.name.contains('/') || !source.name.to_ascii_lowercase().ends_with(".py") {
            return Err(TnsError::UnsafePath(source.name.clone()));
        }
        if !names.insert(source.name.as_str()) {
            return Err(TnsError::field(
                "Python entry name",
                format!("duplicate entry name {:?}", source.name),
            ));
        }
        if source.data.len() > options.max_entry_size {
            return Err(TnsError::LimitExceeded {
                kind: "Python source",
                actual: source.data.len() as u64,
                limit: options.max_entry_size as u64,
            });
        }
        total_size = total_size
            .checked_add(source.data.len())
            .ok_or_else(|| TnsError::field("total Python source size", "overflow"))?;
        if total_size > options.max_total_size {
            return Err(TnsError::LimitExceeded {
                kind: "total Python source size",
                actual: total_size as u64,
                limit: options.max_total_size as u64,
            });
        }
    }
    let mut entries = Vec::with_capacity(sources.len() + 2);
    entries.push(TnsWriteEntry::new(
        "Document.xml",
        default_document_xml().to_vec(),
        CompressionMethod::TiMethod13,
    ));
    entries.push(TnsWriteEntry::new(
        "Problem1.xml",
        python_problem_xml(&first.name)?,
        CompressionMethod::TiMethod13,
    ));
    for source in sources {
        entries.push(TnsWriteEntry::new(
            source.name.clone(),
            source.data.clone(),
            CompressionMethod::Deflate,
        ));
    }
    build_tns(&entries, options)
}

fn append_cdata_safe(output: &mut Vec<u8>, source: &[u8]) {
    let mut position = 0;
    while let Some(relative) = source[position..]
        .windows(3)
        .position(|window| window == b"]]>")
    {
        let end = position + relative;
        output.extend_from_slice(&source[position..end]);
        // Preserve the three source characters by placing the two closing
        // brackets in one section and the greater-than sign in the next.
        output.extend_from_slice(CDATA_RESTART);
        position = end + 3;
    }
    output.extend_from_slice(&source[position..]);
}

fn append_xml_text(output: &mut Vec<u8>, text: &[u8]) {
    for byte in text {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' => output.extend_from_slice(b"&quot;"),
            _ => output.push(*byte),
        }
    }
}
