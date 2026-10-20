# Vendored WeChat scanner crates

Upstream: <https://github.com/wilinz/wxscan-rs> (Apache-2.0), plus
<https://github.com/wilinz/cvlite> and <https://github.com/wilinz/wxing>.
Downloaded from crates.io:

| Local crate | Upstream | Version | Built in workspace |
|---|---|---|---|
| `crates/cvlite` | cvlite | 0.1.0 | yes |
| `crates/wxing` | wxing | 0.1.0 | yes |
| `crates/wxscan` | wxscan | 0.1.1 | yes (`tract` backend) |
| `crates/wxscan-tflite` | wxscan-tflite | 0.1.1 | no (needs `TFLITE_LIB_DIR`) |
| `crates/wxscan-ffi` | wxscan-ffi | 0.1.1 | no (C ABI reference) |

The upstream `Cargo.toml` files are normalized registry manifests; path
dependencies have been changed to the local siblings. `Cargo.toml.orig` and
`version.txt` are kept for provenance. Licenses are in each crate directory.

`qr_race_wechat` uses `crates/wxscan`'s algorithm and `Net` trait, with a
pure-Rust tract backend over our `models/detect.onnx` and `models/sr.onnx`.
The TFLite/FFI crates are kept as reference for the next backend comparison;
they are not workspace members because their build scripts need external
LiteRT libraries.

The long-term plan is to replace the borrowed `TractNet` adapter with our own
`qr_race_infer` backend (ORT/st-zrt/Candle candidates) and keep the pipeline
in `qr_race_wechat` rather than depending on a registry crate.
