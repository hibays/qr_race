# PROMPT.md closure plan

This is the layer-by-layer checklist for closing the original brief. It is a
status document, not a claim that every requirement is complete.

## Status matrix

| # | Layer / requirement | Status | Evidence / gap |
|---|---|---|---|
| 1 | `crates/` layering, single `src/` server binary | Done | workspace members; `qr_race_server` |
| 2 | rxing decoder, no rqrr/cvlite decode backend | Done | `qr_race_scan::RxingDecoder` |
| 3 | 30 fps fast scan path | Done for clear codes | device 2.7–9 ms at 1280×720 |
| 4 | Multi-code / parallel crop decode | Partial | desktop `thread::scope`; mobile serial |
| 5 | WeChat detector + SR + decode | Done (own pipeline) | `qr_race_wechat`, ONNX via tract |
| 6 | Far/small-code scan-rate gain | Partial | tiered 384 → crop → SR → scheduled 768; live far sample needs tuning |
| 7 | CNN backend speed selection | Partial | tract measured; ORT/st-zrt/Candle comparison open |
| 8 | Camera capture: Android | Done | Camera2 Y plane → JNI → Rust; no frames in Dart |
| 9 | Camera capture: desktop | Done | nokhwa backend |
| 10 | Camera capture: Web | Partial | wasm + getUserMedia; browser matrix not exhaustive |
| 11 | Camera capture: Apple | Not done | no iOS/macOS host on this machine |
| 12 | Server: rooms, roles, relays, lobby | Done | hub/API tests |
| 13 | Auth | Done (token-only) | TOTP removed from product flow; legacy optional code |
| 14 | Self-signed HTTPS + fingerprint | Done | Windows bridge, Linux WSL smoke, fingerprint CLI |
| 15 | Transport HTTP + WS | Done | REST/WS tests, tickets |
| 16 | Flutter multi-platform UI | Done Android/Windows/Web | 30 widget/unit tests |
| 17 | Pluggable mods (RustPython) | Partial | upload/distribution/persistence; VM init/execution blocked |
| 18 | Admin-only module upload | Done | role checks/tests |
| 19 | Auto-publish / dynamic prefix slots | Done | client grouping, published-text dedup |
| 20 | Guest/invite identities | Done | per-room guest endpoint, invite links |
| 21 | External app scheme jump | Done | chooser/clipboard fallback |
| 22 | QR generation via rxing WebP | Done | lossless VP8L, rcrm-aligned params |
| 23 | Test infrastructure | Done | 142 Rust / 30 Flutter at last count; fuzz/smoke suites |
| 24 | Deployment docs | Done | Ubuntu/Windows/Android, self-host, TLS |
| 25 | WeChat CNN 30 fps | Not met | CNN 23–34 ms at 384, ~90 ms at 768; fast path remains 30 fps |

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

1. **Backend choice (biggest).** Replace `qr_race_wechat/src/backend.rs` with
   ORT (Android `libonnxruntime.so`) and run the same 384/768 benchmark. Compare
   against tract; keep the `Net` trait. Candle is the fallback pure-Rust option;
   st-zrt wraps ORT; lele has no ONNX frontend and is not a candidate here.
2. **Far-code tuning.** Add a confidence/tile policy and a repeatable
   far/small fixture; record detection rate, not just latency. Current live
   small-QR test did not reliably decode, so do not claim the rate gain yet.
3. **Apple capture.** `qr_race_capture_apple` + AVFoundation, then the same
   frame-never-in-Dart contract.
4. **RustPython execution.** Finish VM initialization/execution, then run a
   sample module with an admin account.
5. **30 fps proof.** Produce a device trace with p95 scan latency and dropped
   frames for fast and CNN tiers; the 768 path is explicitly a lower-fps
   fallback, not a 30 fps claim.

## Four-layer closure view

- **Algorithm layer:** rxing fast path + own WeChat CNN/SR tiered pipeline.
- **Platform layer:** Android/desktop capture and FFI; Web partial; Apple open.
- **Service layer:** rooms/roles/relay/auth/TLS/WS; token-only login.
- **Product layer:** Flutter UI, auto-publish, invites, deep links, deployment;
  module execution remains the main product gap.

