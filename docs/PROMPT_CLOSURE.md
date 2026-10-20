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
| 5 | WeChat detector + SR + decode | Done (own pipeline) | `qr_race_wechat`, ORT Android / tract desktop |
| 6 | Far/small-code scan-rate gain | Done (sample) | live 11-vmin QR decoded via 2×2 tiled pass; formal rate sweep pending |
| 7 | CNN backend speed selection | Done (tested: ORT/tract) | ORT 6–48 ms beats tract; st-zrt wraps ORT, lele N/A, Candle untested |
| 8 | Camera capture: Android | Done | Camera2 Y plane → JNI → Rust; no frames in Dart |
| 9 | Camera capture: desktop | Done | nokhwa backend |
| 10 | Camera capture: Web | Partial | wasm + getUserMedia; browser matrix not exhaustive |
| 11 | Camera capture: Apple | Not done | no iOS/macOS host on this machine |
| 12 | Server: rooms, roles, relays, lobby | Done | hub/API tests |
| 13 | Auth | Done (token-only) | TOTP removed from product flow; legacy optional code |
| 14 | Self-signed HTTPS + fingerprint | Done | Windows bridge, Linux WSL smoke, fingerprint CLI |
| 15 | Transport HTTP + WS | Done | REST/WS tests, tickets |
| 16 | Flutter multi-platform UI | Done Android/Windows/Web | 30 widget/unit tests |
| 17 | Python mod runtime | Removed | no RustPython dependency/UI; Rust extension seams remain |
| 18 | Admin-only module upload | Removed | admin is governance only; no upload UI |
| 19 | Auto-publish / dynamic prefix slots | Done | client grouping, published-text dedup |
| 20 | Guest/invite identities | Done | per-room guest endpoint, invite links |
| 21 | External app scheme jump | Done | chooser/clipboard fallback |
| 22 | QR generation via rxing WebP | Done | lossless VP8L, rcrm-aligned params |
| 23 | Test infrastructure | Done | 142 Rust / 29 Flutter at last count; fuzz/smoke suites |
| 24 | Deployment docs | Done | Ubuntu/Windows/Android, self-host, TLS |
| 25 | WeChat CNN budget | Done (60 ms) | fast 2.9–5.7 ms; CNN 6.3–48.3 ms; fast path remains 30 fps target |

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

1. **Formal p95/drop trace.** Fast path 2.9–5.7 ms, CNN 6.3–48.3 ms on
   device; collect a repeatable trace and a far-code detection-rate sweep.
2. **Apple capture.** `qr_race_capture_apple` + AVFoundation, then the same
   frame-never-in-Dart contract.
3. **Optional follow-ups.** st-zrt is only an ORT wrapper; lele is not an
   ONNX frontend; Candle was not benchmarked because ORT already met the
   budget. Keep the `Net` trait for future backends.

## Four-layer closure view

- **Algorithm layer:** rxing fast path + own WeChat CNN/SR tiered pipeline.
- **Platform layer:** Android/desktop capture and FFI; Web partial; Apple open.
- **Service layer:** rooms/roles/relay/auth/TLS/WS; token-only login.
- **Product layer:** Flutter UI, auto-publish, invites, deep links, deployment;
  Python modules are explicitly out of scope, with Rust extension seams kept.




