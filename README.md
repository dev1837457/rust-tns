# Rust TNS

[![CI](https://github.com/dev1837457/rust-tns/actions/workflows/ci.yml/badge.svg)](https://github.com/dev1837457/rust-tns/actions/workflows/ci.yml)
[![License: MPL 1.1](https://img.shields.io/badge/license-MPL--1.1-blue.svg)](LICENSE)

Rust TNS is an independent, safe Rust implementation of the ZIP-like
TI-Nspire .tns container and its XML payload pipeline. It is for
interoperability work: inspect and unpack existing documents, preserve
resource entries, and build ScriptApp, PythonEditor, or folder-based
documents without bundling proprietary TI software or opaque reference
payloads.

The workspace contains:

- tns-core, a bounded library for parsing, decoding, encoding, and writing
  the implemented TNS subset;
- tns, a command-line tool for inspection, unpacking, verification, and
  packing Lua, Python, XML, and resource files; and
- tns-wasm, a small wasm-bindgen facade for browser-side inspection and
  per-entry decoding.

This is the first public v0.1 release line. The API and compatibility surface
may change before 1.0.

## Features

- Parses and writes the ZIP-shaped TNS outer container, including the
  TI-specific first TIMLP header and TIPD end marker.
- Supports stored entries (method 0), raw DEFLATE entries (method 8), and
  TI method-13 XML entries.
- Encodes and decodes the TIXC0100 XML token stream, including UTF-8,
  Unicode code points, repeated tags and attributes, self-closing tags, and
  CDATA.
- Generates semantic ScriptApp/Lua and PythonEditor documents.
- Preserves arbitrary resource bytes when packing and unpacking.
- Applies finite input, expansion, name, entry-count, and output limits.
- Uses strict and tolerant parsing modes, with warnings for the older
  method-13 metadata convention.
- Publishes complete staged output and uses atomic file replacement workflows
  where the host platform supports them.
- Does not use unsafe Rust.

## Install and build

Rust stable and Cargo are required. A user-local installation is sufficient;
the native build does not require a system package manager, root access, or
the proprietary TI/Phoenix software.

From a checkout:

    rustup toolchain install stable --profile minimal
    cargo build --workspace --release
    cargo test --workspace

The CLI binary is target/release/tns. To install it into Cargo's user-local
bin directory:

    cargo install --path crates/tns-cli --locked

The crates are not published to crates.io as part of v0.1. Clone or download
the source tree and build them locally.

For the complete validation commands used by maintainers, see
[CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md).

## Command-line use

Run tns after cargo install, or replace tns below with
cargo run --quiet --bin tns -- when working directly from the checkout.

Inspect a document. Tolerant mode is the compatibility-oriented default;
strict mode rejects metadata inconsistencies and requires a complete,
well-formed outer directory:

    tns inspect input.tns
    tns inspect input.tns --mode strict

Unpack every supported entry into a new directory. Output is staged before
publication, and an existing destination requires explicit --force:

    tns unpack input.tns unpacked/
    tns verify input.tns unpacked/ --mode strict

Pack a directory containing Document.xml, Problem*.xml, and resources. XML
uses method 13 by default; other files use raw DEFLATE:

    tns pack-xml unpacked/ rebuilt.tns --timlp 0601
    tns pack-xml xml-and-resources/ stored.tns --xml-method stored --force

Pack Lua source as a ScriptApp, including source read from standard input:

    tns pack-lua hello.lua hello.tns --timlp 0500
    cat hello.lua | tns pack-lua - hello.tns

Pack one or more Python files as a PythonEditor document. The first source
file is the one named by the generated problem:

    tns pack-python main.py helpers.py python-app.tns

The packers reject unsafe archive names, refuse to replace existing output
unless --force is supplied, and bound input and generated output sizes.
Use tns --help and the subcommand help pages for the current option list.

## Library use

The core API separates container parsing, payload decoding, and writing. This
example reads a document, applies the default finite limits, and prints the
decoded size of every entry:

    use std::error::Error;
    use std::fs;
    use tns_core::{decode_entry, ParseMode, ParseOptions, TnsContainer};

    fn main() -> Result<(), Box<dyn Error>> {
        let bytes = fs::read("document.tns")?;
        let options = ParseOptions {
            mode: ParseMode::Strict,
            ..ParseOptions::default()
        };
        let container = TnsContainer::parse(&bytes, options)?;

        for entry in &container.entries {
            let decoded = decode_entry(&bytes, entry, options)?;
            println!("{}: {} bytes", entry.name, decoded.bytes.len());
        }
        Ok(())
    }

The public core supports stored, raw DEFLATE, and method-13 payloads.
TIXC encoding and decoding use canonical UTF-8 XML and reject unsupported
declarations, comments, DOCTYPE/other markup, invalid UTF-8, forbidden XML
1.0 code points, malformed token states, and unknown compression methods.
Applications handling untrusted input should prefer ParseMode::Strict and
lower the ParseOptions, TnsWriteOptions, and TixcLimits values for their
workload.

## WASM status

tns-wasm currently exposes inspection and per-entry decoding only:
inspect_tns and decode_tns_entry. Browser-side construction, streaming, and a
JavaScript package distribution are outside v0.1.

The facade can be compile-checked with:

    rustup target add wasm32-unknown-unknown
    cargo check -p tns-wasm --target wasm32-unknown-unknown

To build the included local demo, install wasm-pack and generate its ignored
output directory:

    cargo install wasm-pack --locked
    wasm-pack build crates/tns-wasm --target web --out-dir ../../wasm-demo/pkg
    python3 -m http.server 8000 --directory wasm-demo

Open http://localhost:8000/ in a browser. The demo parses the selected file
locally; it does not upload the file anywhere. See
[wasm-demo/README.md](wasm-demo/README.md).

## Compatibility and v0.1 limitations

Rust TNS implements a documented interoperability subset, not every TNS
variant. Compatibility targets decoded entry identity and safe handling of
real-world metadata differences. Tolerant mode understands the legacy Luna
method-13 convention in which directory size and CRC describe the encoded
payload; strict mode accepts only final-XML metadata.

The v0.1 implementation intentionally does not support ZIP64, data
descriptors, multi-disk archives, ZIP-layer encryption, or compression
methods other than 0, 8, and 13. Repacking preserves decoded names and
resource bytes, not original timestamps, comments, extra fields, ordering,
compressed bytes, or every container metadata detail. The XML codec accepts
the supported canonical subset; it is not a general XML parser.

Method 13's fixed-key 3DES construction is legacy format compatibility, not
authenticated encryption and not a security boundary. The WASM facade uses
smaller browser-oriented limits and does not stream large documents.

Native defaults cap input and aggregate output at 512 MiB, each entry and
TIXC expansion at 256 MiB, archive names at 4,096 bytes, and the entry count
at 65,535. The WASM facade caps the container and aggregate output at 64 MiB
and each entry at 32 MiB. Callers can lower these limits.

The regression suite and compatibility review use synthetic fixtures and
non-redistributed public documents. Actual TI-Nspire hardware and TI
software acceptance remain the outstanding real-device validation step.
This project makes no calculator hardware certification claim and does not
claim proprietary TI/Phoenix testing.

## Security

Treat .tns files as hostile input. The parser and writers enforce bounds,
validate ranges and metadata, reject unsafe output paths, and stage output
before publication. Read [SECURITY.md](SECURITY.md) before integrating the
library into a service or processing files from untrusted sources. Please
report suspected vulnerabilities privately as described there rather than
opening a public issue.

## Project status

v0.1 is a reviewed, test-backed interoperability baseline and the first
public release-preparation line. It is suitable for experimentation, format
research, and cautious local workflows. It is not a promise of complete
support for every TI-Nspire producer or of acceptance by TI
hardware/software. Contributions that add focused tests, document observed
format behavior, or improve bounded error handling are welcome.

## Documentation

- [FORMAT.md](FORMAT.md) describes the implemented byte pipeline and
  compatibility rules.
- [SECURITY.md](SECURITY.md) covers hostile input and output safety.
- [THIRD_PARTY.md](THIRD_PARTY.md) records provenance and dependency licenses.
- [CHANGELOG.md](CHANGELOG.md) records the v0.1 release-line history.
- [CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md) describe
  development and source-release workflows.

## Acknowledgements and provenance

The implementation was informed by two public reference implementations:

- [Luna by the ndless-nspire project](https://github.com/ndless-nspire/Luna),
  a C command-line converter for Lua, Python, XML, and TNS resources.
- [TnsTools by MaksimirKurtov](https://github.com/MaksimirKurtov/TnsTools),
  a pure-Python decoder and method-13/TIXC encoder.

They were used for interoperability research and comparison of documented
format behavior. No Luna C source, MiniZip or DES source, TnsTools Python
source, downloaded .tns file, TI OS image, proprietary DLL, or reference
source tree is vendored or redistributed here. The Rust implementation is
independent code released under the MPL 1.1; see [LEGAL](LEGAL) and
[THIRD_PARTY.md](THIRD_PARTY.md) for the audited provenance and dependency
notices.

## License and trademark disclaimer

Rust TNS is distributed under the [Mozilla Public License Version 1.1](LICENSE).
The source notices identify the Rust TNS modernization as the Original Code
and Initial Developer for this repository's code.

TI-Nspire, TI, and related product names are trademarks of their respective
owners. Rust TNS is not affiliated with, endorsed by, sponsored by, or
certified by Texas Instruments. The names are used only to identify the
interoperability target. No TI software or proprietary TI/Phoenix component
is included.
