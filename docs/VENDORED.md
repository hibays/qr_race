# Vendored dependencies

Only one upstream Rust tree is vendored now:

| Local crate | Upstream | Version | Purpose |
|---|---|---|---|
| `crates/cvlite` | <https://github.com/wilinz/cvlite> | 0.1.0 | WeChat pipeline resize/color helpers (Apache-2.0) |

`qr_race_gui/web/scan/zxing-wasm` is the browser reader/writer wasm bundle
(<https://github.com/Sec-ant/zxing-wasm>, 3.1.6, MIT); it is a web asset, not a
Rust dependency.

`zxing-cpp` is now a normal crates.io dependency
(`zxing-cpp = "0.5.3"`). Desktop builds enable its `bundled` feature; Android
builds disable it and link the NDK-built `libZXing.so` through
`ZXING_CPP_LIB_DIR`. `scripts/build-android-ffi.ps1` builds the C++ core from
`.refs/zxing-cpp-src` (downloaded/build-time only, not committed) and packages
`libZXing.so` plus `libc++_shared.so` into `jniLibs/<abi>/` for each ABI.

YUV→RGB uses the crates.io `yuv` 0.8 SIMD crate. `zenyuv` is present only as a
transitive dependency of `zenwebp`; its decode API is not public in the
released 0.1.3 crate, so it is not called directly.

`qr_race_wechat` still uses `cvlite` for resizing; its decoder calls zxing-cpp
directly. No `wxing`, `wxscan`, TFLite, `rxing`, or `rustpython` dependency
remains in the workspace.

