/*
 * This file is part of the Rust TNS modernization and is made available
 * under the Mozilla Public License Version 1.1. See the repository LICENSE
 * file for the complete terms.
 */

use std::fs;
use std::path::Path;

use tns_core::{
    build_lua_tns, build_python_tns, build_tns, decode_entry, decode_method13_to_tixc, decode_tixc,
    encode_tixc, lua_problem_xml, CompressionMethod, EntrySource, EocdKind, LocalHeaderKind,
    MetadataStatus, MetadataStyle, NamedSource, ParseMode, ParseOptions, TimlpVersion,
    TnsContainer, TnsError, TnsWriteEntry, TnsWriteOptions,
};

const XML: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" ?><root xmlns=\"urn:test\" label=\"h\xc3\xa9\xf0\x9f\x98\x80\"><item id=\"1\">alpha &amp; beta</item><item id=\"2\">\xe4\xb8\x96\xe7\x95\x8c \xf0\x9f\x98\x80 long text text text text text text text text text text text text</item><empty/><empty/></root>";

fn parse_options(mode: ParseMode) -> ParseOptions {
    ParseOptions {
        mode,
        ..ParseOptions::default()
    }
}

#[test]
fn tixc_unicode_repeated_tags_attributes_and_self_closing_roundtrip() {
    let encoded = encode_tixc(XML).expect("encode synthetic XML");
    assert!(encoded.starts_with(b"TIXC0100-1.0?>"));
    assert_eq!(decode_tixc(&encoded).unwrap(), XML);
}

#[test]
fn lua_cdata_end_sequence_is_split_semantically() {
    let source = b"-- Unicode: cafe\xCC\x81 \xF0\x9F\x98\x80\nlocal value = \"]]>\n".to_vec();
    let problem = lua_problem_xml(&source).unwrap();
    assert!(problem
        .windows(b"]]><![CDATA[".len())
        .any(|window| window == b"]]><![CDATA["));
    assert_eq!(
        decode_tixc(&encode_tixc(&problem).unwrap()).unwrap(),
        problem
    );
}

#[test]
fn loose_lua_and_python_packers_produce_decodable_documents() {
    let lua_source = b"print(\"hello \xF0\x9F\x8C\x99\")\nlocal marker = \"]]>\"\n";
    let lua_bytes = build_lua_tns(lua_source, &TnsWriteOptions::default()).unwrap();
    let lua_container = TnsContainer::parse(&lua_bytes, parse_options(ParseMode::Strict)).unwrap();
    assert_eq!(lua_container.entries.len(), 2);
    let lua_problem = decode_entry(
        &lua_bytes,
        &lua_container.entries[1],
        parse_options(ParseMode::Strict),
    )
    .unwrap();
    assert!(lua_problem
        .bytes
        .windows(b"]]><![CDATA[".len())
        .any(|window| { window == b"]]><![CDATA[" }));
    assert!(lua_problem
        .bytes
        .windows("hello 🌙".len())
        .any(|window| { window == "hello 🌙".as_bytes() }));

    let python_sources = vec![
        NamedSource {
            name: "main.py".into(),
            data: b"print('main')\n".to_vec(),
        },
        NamedSource {
            name: "helper.py".into(),
            data: b"VALUE = 42\n".to_vec(),
        },
    ];
    let python_bytes = build_python_tns(&python_sources, &TnsWriteOptions::default()).unwrap();
    let python_container =
        TnsContainer::parse(&python_bytes, parse_options(ParseMode::Strict)).unwrap();
    let main = python_container
        .entries
        .iter()
        .find(|entry| entry.name == "main.py")
        .unwrap();
    let helper = python_container
        .entries
        .iter()
        .find(|entry| entry.name == "helper.py")
        .unwrap();
    assert_eq!(
        decode_entry(&python_bytes, main, parse_options(ParseMode::Strict))
            .unwrap()
            .bytes,
        b"print('main')\n"
    );
    assert_eq!(
        decode_entry(&python_bytes, helper, parse_options(ParseMode::Strict))
            .unwrap()
            .bytes,
        b"VALUE = 42\n"
    );
    let problem = python_container
        .entries
        .iter()
        .find(|entry| entry.name == "Problem1.xml")
        .unwrap();
    let problem = decode_entry(&python_bytes, problem, parse_options(ParseMode::Strict)).unwrap();
    assert!(problem
        .bytes
        .windows(b"main.py".len())
        .any(|window| window == b"main.py"));
}

#[test]
fn method_0_8_13_and_timlp_variants_roundtrip() {
    let mut write_options = TnsWriteOptions {
        timlp_version: TimlpVersion::V0500,
        use_tipd_eocd: false,
        ..TnsWriteOptions::default()
    };
    let resource = b"\x00\x01 synthetic resource \xF0\x9F\x8C\x99".to_vec();
    let entries = vec![
        TnsWriteEntry::new("Document.xml", XML.to_vec(), CompressionMethod::TiMethod13),
        TnsWriteEntry::new(
            "raw/resource.bin",
            resource.clone(),
            CompressionMethod::Stored,
        ),
        TnsWriteEntry::new(
            "compressed/resource.dat",
            b"deflate resource ".repeat(128),
            CompressionMethod::Deflate,
        ),
    ];
    let bytes = build_tns(&entries, &write_options).unwrap();
    let container = TnsContainer::parse(&bytes, parse_options(ParseMode::Strict)).unwrap();
    assert_eq!(container.first_timlp_version, Some(TimlpVersion::V0500));
    assert_eq!(container.eocd_kind, Some(EocdKind::Zip));
    assert_eq!(
        container.entries[0].local_header_kind,
        LocalHeaderKind::Timlp(TimlpVersion::V0500)
    );
    assert_eq!(container.entries[1].local_header_kind, LocalHeaderKind::Zip);
    let expected = [XML.to_vec(), resource, b"deflate resource ".repeat(128)];
    for (entry, expected) in container.entries.iter().zip(expected) {
        let decoded = decode_entry(&bytes, entry, parse_options(ParseMode::Strict)).unwrap();
        assert_eq!(decoded.bytes, expected);
        assert_eq!(decoded.metadata.status, MetadataStatus::ValidFinal);
    }

    write_options.timlp_version = TimlpVersion::V0601;
    write_options.use_tipd_eocd = true;
    let bytes = build_tns(&entries, &write_options).unwrap();
    let container = TnsContainer::parse(&bytes, parse_options(ParseMode::Strict)).unwrap();
    assert_eq!(container.first_timlp_version, Some(TimlpVersion::V0601));
    assert_eq!(container.eocd_kind, Some(EocdKind::Tipd));
}

#[test]
fn standard_zip_local_header_variant_is_accepted() {
    let tns = build_tns(
        &[
            TnsWriteEntry::new("Document.xml", XML.to_vec(), CompressionMethod::Stored),
            TnsWriteEntry::new(
                "resource.bin",
                b"normal zip local record".to_vec(),
                CompressionMethod::Deflate,
            ),
        ],
        &TnsWriteOptions {
            use_tipd_eocd: false,
            ..TnsWriteOptions::default()
        },
    )
    .unwrap();
    let original_central = tns
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    let mut standard = Vec::with_capacity(tns.len() - 6);
    standard.extend_from_slice(b"PK\x03\x04");
    standard.extend_from_slice(&tns[10..]);
    let central = original_central - 6;
    let mut central_cursor = central;
    while central_cursor + 46 <= standard.len()
        && &standard[central_cursor..central_cursor + 4] == b"PK\x01\x02"
    {
        let local_offset = u32::from_le_bytes(
            standard[central_cursor + 42..central_cursor + 46]
                .try_into()
                .unwrap(),
        );
        if local_offset > 0 {
            standard[central_cursor + 42..central_cursor + 46]
                .copy_from_slice(&(local_offset - 6).to_le_bytes());
        }
        let name_len = u16::from_le_bytes(
            standard[central_cursor + 28..central_cursor + 30]
                .try_into()
                .unwrap(),
        ) as usize;
        let extra_len = u16::from_le_bytes(
            standard[central_cursor + 30..central_cursor + 32]
                .try_into()
                .unwrap(),
        ) as usize;
        let comment_len = u16::from_le_bytes(
            standard[central_cursor + 32..central_cursor + 34]
                .try_into()
                .unwrap(),
        ) as usize;
        central_cursor += 46 + name_len + extra_len + comment_len;
    }
    let eocd = standard
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap();
    standard[eocd + 16..eocd + 20].copy_from_slice(&(central as u32).to_le_bytes());

    let container = TnsContainer::parse(&standard, parse_options(ParseMode::Strict)).unwrap();
    assert!(container.first_timlp_version.is_none());
    assert_eq!(container.entries.len(), 2);
    assert_eq!(container.entries[0].local_header_kind, LocalHeaderKind::Zip);
    let decoded = decode_entry(
        &standard,
        &container.entries[1],
        parse_options(ParseMode::Strict),
    )
    .unwrap();
    assert_eq!(decoded.bytes, b"normal zip local record");
}

#[test]
fn legacy_payload_metadata_is_warned_in_tolerant_mode_and_rejected_strictly() {
    let write_options = TnsWriteOptions {
        metadata_style: MetadataStyle::Payload,
        ..TnsWriteOptions::default()
    };
    let bytes = build_tns(
        &[TnsWriteEntry::new(
            "Problem1.xml",
            XML.to_vec(),
            CompressionMethod::TiMethod13,
        )],
        &write_options,
    )
    .unwrap();
    let container = TnsContainer::parse(&bytes, parse_options(ParseMode::Tolerant)).unwrap();
    let decoded = decode_entry(
        &bytes,
        &container.entries[0],
        parse_options(ParseMode::Tolerant),
    )
    .unwrap();
    assert_eq!(decoded.bytes, XML);
    assert_eq!(decoded.metadata.status, MetadataStatus::LegacyPayload);
    assert_eq!(decoded.metadata.warnings.len(), 1);
    assert!(matches!(
        decode_entry(
            &bytes,
            &container.entries[0],
            parse_options(ParseMode::Strict)
        ),
        Err(TnsError::MetadataMismatch { .. })
    ));
}

#[test]
fn local_scan_handles_missing_eocd_in_tolerant_mode() {
    let bytes = build_lua_tns(b"print('synthetic')", &TnsWriteOptions::default()).unwrap();
    let without_eocd = bytes[..bytes.len() - 22].to_vec();
    let container = TnsContainer::parse(&without_eocd, parse_options(ParseMode::Tolerant)).unwrap();
    assert_eq!(container.entries.len(), 2);
    assert!(container
        .entries
        .iter()
        .all(|entry| entry.source == EntrySource::LocalScan));
    assert!(TnsContainer::parse(&without_eocd, parse_options(ParseMode::Strict)).is_err());
}

#[test]
fn hostile_names_and_malformed_tixc_are_rejected() {
    let error = build_tns(
        &[TnsWriteEntry::new(
            "../outside.bin",
            vec![1, 2, 3],
            CompressionMethod::Stored,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, TnsError::UnsafePath(_)));
    assert!(decode_tixc(b"TIXC0100-1.0?>").is_err());
    assert!(encode_tixc(b"").is_err());
    assert!(decode_tixc(b"TIXC0100-1.0?><x>").is_err());
    assert!(decode_tixc(b"TIXC0100-1.0?><x><![CDATA[unterminated</x>").is_err());
}

#[test]
fn truncated_tampered_and_traversal_containers_fail_safely() {
    let mut bytes = build_tns(
        &[TnsWriteEntry::new(
            "safe.bin",
            b"synthetic payload".to_vec(),
            CompressionMethod::Stored,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap();
    assert!(TnsContainer::parse(&bytes[..5], parse_options(ParseMode::Strict)).is_err());

    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    bytes[central + 16..central + 20].copy_from_slice(&0xfeed_beefu32.to_le_bytes());
    let container = TnsContainer::parse(&bytes, parse_options(ParseMode::Strict)).unwrap();
    assert!(matches!(
        decode_entry(
            &bytes,
            &container.entries[0],
            parse_options(ParseMode::Strict)
        ),
        Err(TnsError::MetadataMismatch { .. })
    ));

    let mut unknown = build_tns(
        &[TnsWriteEntry::new(
            "unknown.bin",
            b"method test".to_vec(),
            CompressionMethod::Stored,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap();
    unknown[14..16].copy_from_slice(&99u16.to_le_bytes());
    let unknown_container =
        TnsContainer::parse(&unknown, parse_options(ParseMode::Strict)).unwrap();
    assert!(matches!(
        decode_entry(
            &unknown,
            &unknown_container.entries[0],
            parse_options(ParseMode::Strict)
        ),
        Err(TnsError::UnsupportedMethod(99))
    ));

    let mut traversal = build_tns(
        &[TnsWriteEntry::new(
            "safe.bin",
            b"path test".to_vec(),
            CompressionMethod::Stored,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap();
    let central = traversal
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    traversal[36..44].copy_from_slice(b"../badxx");
    traversal[central + 46..central + 54].copy_from_slice(b"../badxx");
    assert!(matches!(
        TnsContainer::parse(&traversal, parse_options(ParseMode::Strict)),
        Err(TnsError::UnsafePath(_))
    ));

    let deflated = build_tns(
        &[TnsWriteEntry::new(
            "cut.bin",
            b"raw deflate that will be truncated".repeat(32),
            CompressionMethod::Deflate,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap();
    assert!(TnsContainer::parse(
        &deflated[..deflated.len() - 23],
        parse_options(ParseMode::Strict)
    )
    .is_err());

    let method13 = build_tns(
        &[TnsWriteEntry::new(
            "method13.xml",
            XML.to_vec(),
            CompressionMethod::TiMethod13,
        )],
        &TnsWriteOptions::default(),
    )
    .unwrap();
    let method13_container =
        TnsContainer::parse(&method13, parse_options(ParseMode::Strict)).unwrap();
    let payload = method13_container
        .payload(&method13, &method13_container.entries[0])
        .unwrap();
    assert!(decode_method13_to_tixc(&payload[..payload.len() - 8]).is_err());
}

#[test]
fn tixc_depth_limit_is_enforced() {
    let mut xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\" ?>".to_vec();
    for _ in 0..4 {
        xml.extend_from_slice(b"<n>");
    }
    for _ in 0..4 {
        xml.extend_from_slice(b"</n>");
    }
    let encoded = encode_tixc(&xml).unwrap();
    let error = tns_core::decode_tixc_with_limits(
        &encoded,
        tns_core::TixcLimits {
            max_depth: 3,
            ..tns_core::TixcLimits::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, TnsError::LimitExceeded { .. }));
}

#[test]
fn large_text_is_bounded_and_roundtrips() {
    let mut xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\" ?><large>".to_vec();
    xml.extend(std::iter::repeat_n(b'a', 200_000));
    xml.extend_from_slice(b"</large>");
    let encoded = encode_tixc(&xml).unwrap();
    assert_eq!(decode_tixc(&encoded).unwrap(), xml);
    let limited = tns_core::decode_tixc_with_limits(
        &encoded,
        tns_core::TixcLimits {
            max_output_size: 128,
            ..tns_core::TixcLimits::default()
        },
    );
    assert!(matches!(limited, Err(TnsError::LimitExceeded { .. })));
}

#[test]
fn luna_sample_is_decodable_when_present_but_not_part_of_public_corpus() {
    let path = Path::new("/tmp/luna-sample.tns");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(path).unwrap();
    let container = TnsContainer::parse(&bytes, parse_options(ParseMode::Tolerant)).unwrap();
    assert_eq!(container.entries.len(), 2);
    let mut warning_count = 0;
    for entry in &container.entries {
        let decoded = decode_entry(&bytes, entry, parse_options(ParseMode::Tolerant)).unwrap();
        assert!(decoded.bytes.starts_with(b"<?xml version="));
        warning_count += decoded.metadata.warnings.len();
        assert!(matches!(
            decoded.metadata.status,
            MetadataStatus::LegacyPayload
        ));
    }
    assert_eq!(warning_count, 2);
}
