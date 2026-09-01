## Summary

Describe the change and why it is needed.

## Validation

- [ ] cargo fmt --all --check
- [ ] cargo clippy --workspace --all-targets -- -D warnings
- [ ] cargo test --workspace
- [ ] cargo build --workspace --release
- [ ] cargo check -p tns-wasm --target wasm32-unknown-unknown
- [ ] cargo audit --deny warnings

## Review checklist

- [ ] New behavior has focused tests or an explanation for why tests are not
      applicable.
- [ ] Bounds, path safety, strict/tolerant semantics, and atomic output
      behavior remain explicit.
- [ ] Documentation makes limitations and validation scope clear.
- [ ] No secrets, local paths, proprietary TI material, downloaded .tns files,
      generated corpus artifacts, or reference source trees are included.
- [ ] Provenance and license notices are accurate.
