# Third-party provenance and licenses

## Interoperability references

This modernization was informed by two public reference implementations:

* Luna (https://github.com/ndless-nspire/Luna), a mature C packer for
  Lua/Python/XML to TNS. Luna is MPL 1.1, with additional notices for its
  derived MiniZip and DES components. Its source and license were inspected
  locally; this repository does not copy large Luna code blocks or ship its
  C sources.
* TnsTools (https://github.com/MaksimirKurtov/TnsTools), a Python TNS/TIXC
  implementation under the MIT license. Its container, method-13, and TIXC
  structure informed the corresponding independent Rust implementation and
  compatibility tests. No Python source is vendored here. Its required notice
  is retained below because the Rust implementation closely follows those
  format observations.

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
* rustix (native CLI targets) — Apache-2.0 OR MIT
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

## TnsTools MIT notice

MIT License

Copyright (c) 2026 tnstools contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
