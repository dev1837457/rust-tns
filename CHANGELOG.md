# Changelog

All notable changes to Rust TNS are recorded here. The project is in the
0.x series, so public APIs and compatibility behavior may change.

## 0.1.0 — Unreleased

- Added a bounded Rust implementation of the ZIP-like TI-Nspire TNS
  container and TIXC XML pipeline.
- Added the tns CLI for inspection, strict or tolerant unpacking, verification,
  and packing Lua, Python, XML, and resource files.
- Added the tns-wasm inspection and per-entry decoding facade.
- Added regression coverage for malformed input, metadata conventions,
  resource preservation, path safety, limits, and atomic output workflows.
- Documented unsupported container/XML features, legacy method-13 3DES
  limitations, provenance, and the outstanding real-device validation step.
