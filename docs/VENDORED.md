# Vendored dependencies

Only two upstream trees are vendored now; the old wxscan/wxing/TFLite copies
were deleted when zxing-cpp replaced both `rxing` and `wxing`.

| Local crate | Upstream | Version | Purpose |
|---|---|---|---|
| `crates/cvlite` | <https://github.com/wilinz/cvlite> | 0.1.0 | WeChat pipeline resize/color helpers (Apache-2.0) |
| `crates/zxing-cpp` | <https://github.com/zxing-cpp/zxing-cpp> | 0.5.3 | Rust binding + bundled C++ barcode reader/writer (Apache-2.0) |
| `crates/qr_race_gui/web/scan/zxing-wasm` | <https://github.com/Sec-ant/zxing-wasm> | 3.1.6 | Browser zxing-cpp reader/writer wasm (MIT) |

`crates/zxing-cpp` is normally a crates.io package; it is vendored so three
build issues could be fixed locally:

- the C++ link-map flag must be `-Wl,-Map=...` on non-MSVC toolchains;
- MSVC debug builds use the release CRT to match Rust's `/MD` link (otherwise
  `__imp__CrtDbgReport` is unresolved);
- Android links a prebuilt NDK `libZXing.so` instead of invoking CMake, because
  the crate's CMake call passes a conflicting generic `--target`.

Desktop targets build the bundled C++ with CMake. Android builds are driven by
`scripts/build-android-ffi.ps1`, which configures the NDK toolchain and packages
`libZXing.so` plus `libc++_shared.so` into `jniLibs/arm64-v8a/`.

`qr_race_wechat` still uses `cvlite` for resizing; its decoder now calls
zxing-cpp directly. No `wxing`, `wxscan`, TFLite, `rxing`, or `rustpython`
dependency remains in the workspace.

