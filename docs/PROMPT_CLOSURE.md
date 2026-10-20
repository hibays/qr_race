# PROMPT.md closure plan

This is the layer-by-layer checklist for closing the original brief. It is a
status document, not a claim that every requirement is complete.

## Status matrix

| # | Layer / requirement | Status | Evidence / gap |
|---|---|---|---|
| 1 | `crates/` layering, single `src/` server binary | Done | workspace members; `qr_race_server` |
| 2 | zxing-cpp decoder, no rqrr/cvlite decode backend | Done | `qr_race_scan::ZxingDecoder` |
| 3 | 30 fps fast scan path | Done for clear codes | device 2.7–9 ms at 1280×720 |
| 4 | Multi-code / parallel crop decode | Partial | desktop `thread::scope`; mobile serial |
| 5 | WeChat detector + SR + decode | Done (own pipeline) | `qr_race_wechat`, ORT Android / tract desktop |
| 6 | Far/small-code scan-rate gain | Done (sample) | far QR decoded on pre-swap rxing build; zxing-cpp+color re-run pending |
| 7 | CNN backend speed selection | Done (tested: ORT/tract) | ORT 6–48 ms beats tract; st-zrt wraps ORT, lele N/A, Candle untested |
| 8 | Camera capture: Android | Done | 2K request (2560×1440, fallback), Y/U/V color planes → JNI → Rust; full-screen UI |
| 9 | Camera capture: desktop | Done | nokhwa backend |
| 10 | Camera capture: Web | Done (fake camera) | zxing-wasm reader; fake-camera hit 640×480/1.7 ms; physical hit pending |
| 11 | Camera capture: Apple | Not done | no iOS/macOS host on this machine |
| 12 | Server: rooms, roles, relays, lobby | Done | hub/API tests |
| 13 | Auth | Done (token-only) | TOTP removed from product flow; legacy optional code |
| 14 | Self-signed HTTPS + fingerprint | Done | Windows bridge, Linux WSL smoke, fingerprint CLI |
| 15 | Transport HTTP + WS | Done | REST/WS tests, tickets |
| 16 | Flutter multi-platform UI | Done Android/Windows/Web | 27 widget/unit tests |
| 17 | Python mod runtime | Removed | no RustPython dependency/UI; Rust extension seams remain |
| 18 | Admin-only module upload | Removed | admin is governance only; no upload UI |
| 19 | Auto-publish / dynamic prefix slots | Done | client grouping, published-text dedup |
| 20 | Guest/invite identities | Done | per-room guest endpoint, invite links |
| 21 | External app scheme jump | Done | chooser/clipboard fallback |
| 22 | QR generation via qrcode / zxing-wasm WebP | Done | native qrcode VP8L + Web zxing-wasm writer to WebP |
| 23 | Test infrastructure | Done | 134 Rust / 28 Flutter at last count; fuzz/smoke suites |
| 24 | Deployment docs | Done | Ubuntu/Windows/Android, self-host, TLS |
| 25 | WeChat CNN budget | Done (60 ms) | pre-swap rxing p95: fast 6.9 ms, CNN 6.9 ms, tiled far 21.8 ms; zxing-cpp re-test pending |

## Test gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd crates/qr_race_gui
flutter analyze
flutter test
```

Device-only checks:

```bash
# clear code: fast tier
# far/small code: CNN tier, inspect HUD "CNN ...ms"
# certificate: qr_race_server fingerprint --cert ...
```

## Remaining work, in order

1. **Larger field sweep.** Synthetic p95 (fast 6.9 ms, CNN 6.9 ms, tiled far
   21.8 ms) and one pre-swap real-camera run (p95 17.4 ms, CNN 59.1 ms, 5/1075 dropped)
   and a 17/24 synthetic far-code size/blur matrix are measured. A multi-device
   field detection-rate sweep remains.
2. **Apple capture.** `qr_race_capture_apple` + AVFoundation, then the same
   frame-never-in-Dart contract.
3. **Optional follow-ups.** st-zrt is only an ORT wrapper; lele is not an
   ONNX frontend; Candle was not benchmarked because ORT already met the
   budget. Keep the `Net` trait for future backends.

## Four-layer closure view

- **Algorithm layer:** zxing-cpp fast path (color RGB) + own WeChat CNN/SR tiered pipeline; QR generation via qrcode.
- **Platform layer:** Android/desktop capture and FFI; Web partial; Apple open.
- **Service layer:** rooms/roles/relay/auth/TLS/WS; token-only login.
- **Product layer:** Flutter UI, auto-publish, invites, deep links, deployment;
  Python modules are explicitly out of scope, with Rust extension seams kept.
















