# Independent second-pass review

Review completed 2026-08-31. The repository already contained a coherent v0.1
implementation, so no recovery or replacement implementation was required.
The review covered the native core, CLI, WASM facade, synthetic regressions,
the ignored compatibility corpus, and the local Luna and TnsTools reference
sources. No third-party TNS document was added to Git.

## Confirmed findings and fixes

- Lua source containing `]]>` lost those three bytes when CDATA was split.
  Generated ScriptApp XML now uses the standard adjacent-section sequence and
  regression tests reconstruct and compare the original source bytes.
- The TIXC encoder's repeated-attribute reference token round-tripped in the
  pure decoder but is documented in the locally inspected TnsTools provenance
  notes as truncating in TI's Phoenix expander. The decoder still accepts the
  token, while the encoder conservatively emits repeated attributes literally.
- Malformed TIXC could produce invalid XML: non-exact headers and XML versions,
  invalid UTF-8/names, duplicate attributes, malformed self-closing states,
  misplaced CDATA, and XML 1.0-forbidden characters were insufficiently
  rejected. The state machine now rejects those cases, including raw-XML
  method-13 fallback data. XML version 1.0 is the supported canonical form.
- Strict outer parsing trusted inconsistent local/central fields and loose
  central-directory boundaries. It now requires matching names, flags,
  methods, CRCs, and sizes; exact directory and EOCD boundaries; consistent
  single-disk metadata; unique, non-overlapping local ranges; and supported
  ZIP flags. Encrypted, descriptor, multi-disk, and other unsupported records
  fail closed.
- Tolerant recovery searched arbitrary payload bytes for local-header
  signatures and could synthesize phantom entries. Recovery now follows a
  contiguous sequence of declared local-record boundaries from byte zero. An
  entry-count off-by-one was also corrected.
- Raw DEFLATE decoding accepted bytes after the end of a stream. Methods 8 and
  13 now require the complete declared payload to be consumed.
- Method-13 decoding could clone an oversized encrypted body before applying
  the TIXC output limit, and the inflated intermediate TIXC had an independent
  larger limit. Payload and inflated-TIXC limits are now caller-bounded before
  expansion.
- Writer limits did not cover archive names, aggregate input/output, generated
  Lua CDATA amplification, compressed output, or invalid compression levels.
  All are checked before the corresponding large allocation or serialization.
  Python generation also validates source count, duplicate names, aggregate
  size, and Luna's 240-byte selected-filename limit.
- CLI file and stdin reads, collected source trees, and aggregate decoded
  output were not all bounded. The CLI now applies core limits, rejects
  symlinks/special files and non-UTF-8 filesystem names, preserves legitimate
  `_artifacts` resources, terminal-escapes archive names, and makes `inspect`
  fail when an entry cannot be decoded.
- Forced directory unpack removed the old destination before the staged
  directory was published. Publication now uses no-clobber rename and, on the
  current Linux target and other supported Unix targets, atomic rename
  exchange; the portable path retains rollback behavior. File output remains
  same-directory, flushed, atomic, and no-clobber unless `--force` is explicit.
- TIXC text decoding allocated a temporary vector for nearly every input byte.
  It now appends directly to the already bounded output buffer.
- Browser defaults inherited the much larger native limits. The WASM facade
  now caps containers/aggregate output at 64 MiB and entries at 32 MiB.
- Source notices and third-party provenance were incomplete. Rust and demo
  source notices now identify the MPL 1.1 Original Code and Initial Developer;
  the exact inspected TnsTools MIT notice and the direct `rustix` dependency
  license are recorded in `THIRD_PARTY.md`.

Focused regression coverage was added for each behavior above, including
truncation and every-byte mutation sweeps that exercise strict and tolerant
container parsing plus TIXC decoding without panics.

## Remaining v0.1 limitations

- ZIP64, data descriptors, multi-disk archives, ZIP-layer encryption, and
  methods other than 0, 8, and TI method 13 are intentionally unsupported.
- Repacking preserves decoded entry names and resource bytes, not ZIP extras,
  comments, timestamps, external attributes, original ordering, compressed
  bytes, or other byte-for-byte container metadata.
- Unflagged non-UTF-8 names are rejected in strict mode and replacement-decoded
  in tolerant mode; CP437/legacy filename bytes are not round-trip preserved.
- The encoder accepts the documented canonical XML subset. Comments, DOCTYPE,
  non-declaration processing instructions, non-canonical declarations, and
  unobserved TIXC states remain out of scope. Raw-XML method-13 fallback is
  UTF-8/XML-character checked but is not a full semantic XML validation API.
- Method-13's fixed-key 3DES construction is legacy format compatibility, not
  authenticated encryption and not a security boundary.
- The review used synthetic tests, local reference source, TnsTools' pure
  decoder, and public documents. It did not run TI calculator hardware or
  proprietary TI/Phoenix software. The attribute-token policy follows the
  checked-in finding in the locally inspected TnsTools reference notes.
- The WASM crate exposes inspection and per-entry decoding only; browser-side
  construction and streaming are not part of v0.1.
- Atomic directory exchange is used on supported targets/filesystems. The
  portable fallback uses same-parent rename plus rollback and therefore cannot
  provide the same single-syscall replacement guarantee.

## Verification evidence

The final tree passed:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo check -p tns-wasm --target wasm32-unknown-unknown
cargo audit audit --deny warnings
```

The test result was 25 passing tests (4 CLI unit, 1 method-13 unit, and 20 core
regressions), plus successful doc-test targets. RustSec loaded 1,233 advisories
and scanned 56 locked dependencies with no vulnerability or warning. The
project contains no `unsafe` Rust.

Five ignored public fixtures were checked against the recorded SHA-256 values,
strictly inspected, unpacked, rebuilt, and strictly verified: `2048.tns`,
`Molar-Concentration.tns`, `SimplexMethod.tns`, `Timer.tns`, and
`ToDoManagerEnglish.tns`. All 64 entries matched. TnsTools' pure decoder then
decoded every XML entry from each Rust rebuild byte-for-byte identically to the
Rust-unpacked XML. The separate Luna-generated sample produced exactly two
expected legacy-metadata warnings in tolerant mode and failed in strict mode.
CLI no-clobber/force smoke checks passed for both file and directory output.
