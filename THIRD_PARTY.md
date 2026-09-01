# Third-party provenance and licenses

## Interoperability references

Rust TNS is independent Rust code. The following projects were consulted for
format behavior, interoperability research, and comparison testing:

- [Luna by the ndless-nspire project](https://github.com/ndless-nspire/Luna)
  is a C command-line converter for Lua, Python, XML, and TNS resources. Its
  repository says Luna is licensed under MPL 1.1. The audited source notice
  identifies the Original Code as "Luna code" and Olivier ARMAND as the
  Initial Developer. Luna also contains separately noticed DES and MiniZip
  components. None of those source files are included here.
- [TnsTools by MaksimirKurtov](https://github.com/MaksimirKurtov/TnsTools)
  is a pure-Python TNS/TIXC implementation under the MIT license. Its
  repository LICENSE identifies the copyright notice as
  Copyright (c) 2026 tnstools contributors. No TnsTools source or substantial
  portion is vendored here; only documented behavior informed this
  independent implementation.

The project's governing license is the MPL 1.1 in [LICENSE](LICENSE). The
reference projects' licenses do not replace or relicense Rust TNS code. See
[LEGAL](LEGAL) for the source-origin summary. No proprietary TI software,
OS image, DLL, third-party document, or reference source tree is included or
required by the native build.

## Direct Rust dependencies

The direct dependencies used by the workspace are permissively licensed:

- crc32fast — Apache-2.0 OR MIT
- des — Apache-2.0 OR MIT
- flate2 — Apache-2.0 OR MIT
- thiserror — Apache-2.0 OR MIT
- clap — Apache-2.0 OR MIT
- tempfile — Apache-2.0 OR MIT
- rustix (native CLI targets) — Apache-2.0 OR MIT
- wasm-bindgen — Apache-2.0 OR MIT

Cargo resolves exact versions in [Cargo.lock](Cargo.lock). Transitive
dependency notices remain with their respective crates in the Cargo registry;
redistributors should retain those notices as required by each license.

## Compatibility corpus

The compatibility review used synthetic in-memory fixtures and a private,
non-redistributed collection of public documents held outside this
repository. No downloaded .tns file or generated compatibility artifact is
part of the source distribution. The repository ignores local corpus
directories, .tns files, generated TIXC files, build output, and private
review material; see [.gitignore](.gitignore).

## Audited TnsTools license text

The following is the exact license text in the audited TnsTools repository.
It is retained as a provenance record only; no TnsTools code is distributed
by Rust TNS.

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
