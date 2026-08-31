# Minimal browser demo

Install wasm-pack in a user-local tool directory if it is not already
available, then from the repository root run:

    wasm-pack build crates/tns-wasm --target web --out-dir ../../wasm-demo/pkg

Serve this directory over HTTP (for example with any local static server) and
open index.html. The page only parses and inspects the selected file; it does
not upload it anywhere.

