# Release steps

These steps describe a source release of Rust TNS. The crates are intentionally
marked publish = false; do not publish them to crates.io as part of v0.1.

## Maintainer checklist

1. Confirm that the canonical GitHub repository and package metadata point to
   https://github.com/dev1837457/rust-tns, then verify each documentation and
   README link.
2. Update the workspace version and this changelog. Keep all workspace crate
   versions aligned and keep publish = false unless crate publication is an
   explicit, separately reviewed decision.
3. Review [README.md](README.md), [LEGAL](LEGAL), and
   [THIRD_PARTY.md](THIRD_PARTY.md) for accurate provenance. Confirm that no
   .tns files, TI software, proprietary DLLs, reference source trees, local
   paths, secrets, or private review artifacts are staged.
4. Run the complete pre-release validation:

       cargo fmt --all --check
       cargo clippy --workspace --all-targets -- -D warnings
       cargo test --workspace
       cargo build --workspace --release
       rustup target add wasm32-unknown-unknown
       cargo check -p tns-wasm --target wasm32-unknown-unknown
       cargo audit --deny warnings

Use the supported cargo-audit command above; cargo audit audit --deny warnings
is rejected by cargo-audit 0.22.2 as an unrecognized subcommand.

5. Review the final diff and links, then commit the release preparation and
   create an annotated tag such as v0.1.0. The GitHub release should link to
   the source tag and clearly repeat the v0.1 limitations.
6. If binary artifacts are published later, build them from the tagged source
   in a clean environment and include checksums. Never attach downloaded
   third-party .tns files, proprietary TI software, or Phoenix components.

Actual acceptance by TI-Nspire hardware or TI software remains the outstanding
real-device validation step. Do not label a source release as calculator
hardware-certified or as tested with proprietary TI/Phoenix software.
