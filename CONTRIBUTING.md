# Contributing to Rust TNS

Thank you for helping improve this interoperability project. Focused fixes,
small regression tests, format observations, and clearer documentation are
especially useful.

## Before opening an issue or pull request

Check existing issues and read [SECURITY.md](SECURITY.md). Do not open a
public issue for a suspected vulnerability. Do not upload TI software,
proprietary DLLs, OS images, copyrighted calculator documents, or downloaded
third-party .tns files unless you have clear redistribution rights. Prefer a
small in-memory fixture, a redacted byte sequence, or a hash and description
of an external file.

## Development workflow

Use Rust stable and work from the repository root:

    rustup toolchain install stable --profile minimal
    cargo fmt --all
    cargo test --workspace

Keep changes narrow and preserve the bounded parsing, path-safety, and
atomic-output behavior established by the current implementation. New format
support should include focused tests and a note in [FORMAT.md](FORMAT.md) or
the relevant API documentation. Avoid unsafe Rust and avoid adding a
dependency when the standard library or an existing dependency is sufficient.

The public regression suite is synthetic and does not require a downloaded
compatibility corpus. Local corpus files, generated TIXC streams, build
output, and private review material are ignored by [.gitignore](.gitignore).

## Required checks

Before requesting review, run the same checks used by CI:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo build --workspace --release
    rustup target add wasm32-unknown-unknown
    cargo check -p tns-wasm --target wasm32-unknown-unknown
    cargo audit --deny warnings

The final command is provided by cargo-audit. If it is not installed, install
it in your user-local Cargo bin directory with cargo install cargo-audit
--locked. The supported cargo-audit invocation is cargo audit --deny warnings.
The duplicated form cargo audit audit --deny warnings is rejected by
cargo-audit 0.22.2 as an unrecognized subcommand.

## Pull requests

Describe the user-visible behavior, the safety or compatibility implications,
and the checks you ran. Include tests for regressions and update the
changelog or documentation when behavior changes. Keep provenance explicit:
interoperability research is welcome, but do not copy or vendor reference
source trees or third-party documents.

Please do not describe synthetic or public-corpus results as TI-Nspire
hardware certification or proprietary TI/Phoenix testing. Actual acceptance by
TI-Nspire hardware or TI software remains a separate real-device validation
step.

## License

By contributing, you agree that your contributions are offered under the
repository's [Mozilla Public License Version 1.1](LICENSE), subject to the
license's terms.
