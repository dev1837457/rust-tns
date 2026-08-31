# Third-party provenance and licenses

## Interoperability references

This modernization was informed by two public reference implementations:

* Luna (https://github.com/ndless-nspire/Luna), a mature C packer for
  Lua/Python/XML to TNS. Luna is MPL 1.1, with additional notices for its
  derived MiniZip and DES components. Its source and license were inspected
  locally; this repository does not copy large Luna code blocks or ship its
  C sources.
* TnsTools (https://github.com/MaksimirKurtov/TnsTools), a Python TNS/TIXC
  implementation under the MIT license. Its typed-container and
  method-13/TIXC observations informed compatibility tests. No Python source
  is vendored here.

The Rust implementation is released under MPL 1.1. The initial developer
notice and the complete license are in LICENSE; LEGAL contains the
source-origin notice.

TI-Nspire, TI, and related product names are trademarks of their owners. TI
software, OS images, proprietary DLLs, and third-party documents are not
included or required by the native build.

## Direct Rust dependencies

The direct dependencies used by the workspace are permissively licensed:

* crc32fast — Apache-2.0 OR MIT
* des — Apache-2.0 OR MIT
* flate2 — Apache-2.0 OR MIT
* thiserror — Apache-2.0 OR MIT
* clap — Apache-2.0 OR MIT
* tempfile — Apache-2.0 OR MIT
* wasm-bindgen — Apache-2.0 OR MIT

Cargo resolves and records exact versions in Cargo.lock. Transitive dependency
notices remain with their respective crates in the Cargo registry;
redistributors should retain those notices as required by each license.

## Compatibility corpus

During local review, small public downloads from ticalc.org's TI-Nspire
archive (https://www.ticalc.org/pub/nspire/) were kept under
/tmp/tns-public-corpus.*, outside Git, and removed/recreated as needed. The
selection included Lua, Python, calculator/document, and resource-bearing
documents. The Brendan Kelly Publishing free-files page was not downloaded
because it returned HTTP 406 to the available clients. No downloaded TNS file
is part of this repository.

