# Rust TNS

Rust TNS is a safe, reusable first implementation of the TI-Nspire .tns
container and its XML payload pipeline. It is intended for interoperability
work: it can inspect and unpack existing documents, and build ScriptApp,
PythonEditor, or folder-based documents without copying opaque reference
payloads.

The native core is in tns-core. The tns binary provides the practical
workflow, and tns-wasm exposes a small wasm-bindgen facade for browser
callers.

## Build

No system package or root access is needed. With a user-local Rust install:

    rustup toolchain install stable --profile minimal
    cargo build --release
    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings

The executable is target/release/tns.

## Command-line examples

Inspect a real or generated document. Tolerant mode is the compatibility
default and reports Luna's legacy method-13 metadata convention:

    tns inspect input.tns
    tns inspect input.tns --mode strict

Unpack XML and resources into a new directory. The command stages all output
before replacing the destination and refuses an existing destination unless
--force is explicit:

    tns unpack input.tns unpacked/
    tns verify input.tns unpacked/ --mode tolerant

Build a folder of Document.xml, Problem*.xml, and resources. XML is method-13
encoded by default and other files use ordinary raw DEFLATE:

    tns pack-xml unpacked/ rebuilt.tns --timlp 0601
    tns pack-xml xml-and-resources/ stored.tns --xml-method stored --force

Pack loose source:

    tns pack-lua hello.lua hello.tns --timlp 0500
    cat hello.lua | tns pack-lua - hello.tns
    tns pack-python main.py helpers.py python-app.tns

Generated XML uses semantic ScriptApp and PythonEditor templates. Lua source
is placed in safely split CDATA sections, including when it contains ]]>.
Python entries are ordinary method-8 source files and the first file is the
one named by the PythonEditor problem.

## Library use

The core API separates parsing, payload decoding, and writing:

    use tns_core::{decode_entry, ParseMode, ParseOptions, TnsContainer};

    let bytes = std::fs::read("document.tns")?;
    let options = ParseOptions {
        mode: ParseMode::Tolerant,
        ..ParseOptions::default()
    };
    let container = TnsContainer::parse(&bytes, options)?;
    for entry in &container.entries {
        let decoded = decode_entry(&bytes, entry, options)?;
        println!("{}: {} bytes", entry.name, decoded.bytes.len());
    }

The public core supports stored (method 0), raw DEFLATE (method 8), and
TI-Nspire method 13 payloads. TIXC encoding and decoding preserve canonical
UTF-8 XML, Unicode code points, repeated tag/attribute dictionaries,
self-closing tags, and CDATA. Unsupported XML declarations, comments,
DOCTYPE/other markup, data-descriptor local records, and unknown compression
methods produce explicit errors.

## Compatibility and limits

The default limits are deliberately finite: 512 MiB input, 256 MiB per entry,
512 MiB total directory-declared output, 65,535 entries, and 4,096-byte names.
Callers can lower them through ParseOptions and TixcLimits.

TNS metadata is not completely uniform across producers. Modern method-13
files normally describe final XML in the directory. Luna 2.x can describe the
encrypted method-13 payload instead. Tolerant decoding accepts the latter and
returns a warning; strict decoding rejects it. CRC and size mismatches that
match neither interpretation are still visible in tolerant mode and are
errors in strict mode.

The public regression suite creates all of its fixtures in memory. Private
compatibility review may use public downloads, but TI documents and other
third-party files are not included in this repository.

## Repository documents

* FORMAT.md describes the implemented byte pipeline.
* SECURITY.md documents hostile-input and output-safety rules.
* THIRD_PARTY.md records provenance and dependency licenses.
* LICENSE is the governing MPL 1.1 license for this modernization.

