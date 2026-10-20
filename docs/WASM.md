# Web / wasm scanning

The Rust wasm scanner was removed with the zxing-cpp migration. zxing-cpp is
C++ and needs an Emscripten toolchain plus a static C++ runtime for wasm, which
this workspace does not currently build.

Current state:

- Android and desktop scan natively with zxing-cpp.
- Web still builds, but camera scanning and QR rendering report unsupported:
  the Rust wasm crate and its generated artifacts were removed to eliminate the
  last rxing binary.
- A future Web backend should either build zxing-cpp with Emscripten or use the
  `zxing-wasm` browser package behind the same `ScannerBackend` interface.

The old fake-camera probe and Node wasm probe were removed with the crate.

