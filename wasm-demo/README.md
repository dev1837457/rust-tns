# Minimal browser demo

This demo uses the tns-wasm facade to inspect a selected .tns file in the
browser. The file stays local; the page does not upload it. Browser-side
packing and streaming are not part of v0.1.

Install wasm-pack in a user-local tool directory if it is not already
available, then from the repository root run:

    wasm-pack build crates/tns-wasm --target web --out-dir ../../wasm-demo/pkg

Serve this directory over HTTP:

    python3 -m http.server 8000 --directory wasm-demo

Then open http://localhost:8000/. The generated wasm-demo/pkg directory is
ignored by Git. See the repository [README](../README.md) for the current
WASM status and [LICENSE](../LICENSE) for licensing.
