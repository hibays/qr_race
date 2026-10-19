# Verification

Audit date: 2026-10-20, Windows x64. This is the evidence behind the README's
claims. It also lists what is **not** verified, so nothing here is mistaken for a
finished product.

## Gates

| gate | command | result |
|---|---|---|
| Rust format | `cargo fmt --all -- --check` | clean |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| Rust tests | `cargo test --workspace` | **125 passed** |
| Flutter analyze | `flutter analyze` | no issues |
| Flutter tests | `flutter test` | **23 passed** |
| Windows build | `flutter build windows --release` | built |
| Web build | `pwsh -File scripts/build-wasm.ps1` then `flutter build web --release` | built |
| Android build | `flutter build apk --debug` | built (151 MB debug APK) |

### QR generation

- rxing encodes room/relay codes as 2048×2048 **lossless WebP** (error
  correction M, quality 90 / method 4, L8), the same parameters as rcrm's
  bridge. `qr_race_qr` tests decode the WebP back and scan it with the pipeline.
- Native FFI: `dart run tool/qr_probe.dart` against the release DLL produced a
  2452-byte RIFF/VP8L file that Pillow reads as 2048×2048.
- Web: the wasm export `qr_webp` produced the same 2048×2048 WebP in Node; the
  Flutter web build uses it through `camera_scan.js`.
- The Flutter UI renders these bytes with `Image.memory` and
  `FilterQuality.none`; `qr_flutter` was removed.

## Feature evidence

### Scan pipeline

- `docs/PERF.md`: whole-frame 1280×720 p50 4.629 ms; pyramid detect 2.331 ms with
  decode 0.127 ms; pyramid + on-demand bilinear super-res 4.037 ms (super-res
  1.319 ms). Stage timings come from the engine itself.
- Multi-code: generated 1280×720 scene with two symbols, pyramid pipeline returns
  both, byte-for-byte.
- wasm: `node tools/wasm_scan_probe.cjs` reads a dumped frame, scans 1280×720 in
  18–22 ms wall and prints the exact payload.
- Browser: `pwsh -File scripts/browser-scan-probe.ps1` runs Edge headless with a
  fake camera (Y4M of a QR page) and reports
  `HIT:https://example.test/relay`, 640×480, `scan_ms` ≈ 1.8.
- Real camera: camera access was enabled and the CLI scanned textured frames at
  1920×1080 (6.01 ms average, ~24 fps input) and 1280×720 (2.89 ms average,
  ~28 fps input); no QR was in view, so a live symbol was not decoded.

### Auth and rooms

- TOTP against RFC 6238 vectors; replay of the same step refused.
- Lockout: five bad codes → `429`; verified by hub/API tests and a live process
  (five `401`s then a valid code returned `429`).
- Room permissions: dark-room entry, host-only light/promote/demote/modules,
  host/co-host-only publish — unit tests plus an HTTP end-to-end test.
- WebSocket: real socket test with a single-use ticket.

### Persistence

- SQLite store tests cover rooms, relays, modules, trimming, and reopening the
  file.
- Real-process restart: create/light/publish → stop → start → `GET /v1/room`
  returns `lit=true` and the same relay bytes; module uploads and room selections
  also survive.

### Transport security

- Generated self-signed certificate is reused on restart and its SHA-256 is
  printed.
- TLS integration test performs a real handshake with the certificate trusted by
  the client (no `danger_accept_invalid_certs`).
- Live checks: `curl -k` 200, `curl --cacert` 200, Python with verified TLS
  authenticates, Dart `tool/tls_probe.dart` authenticates with fingerprint
  pinning; a wrong fingerprint fails the handshake.
- CLI integration test publishes through HTTPS using `--cacert`.

### Modules

- Admin upload/list/source endpoints; members get `403`.
- Room selection only accepts uploaded ids (`400` otherwise).
- Uploads persist across restart.
- Flutter: room module strip, host picker dialog, admin upload dialog; widget and
  API tests cover them.

### Clients

- CLI: image scan + publish, camera scan/publish, `--list-cameras`, `--probe`,
  `--cacert`, tested against a live server.
- Desktop FFI: C ABI state/preview and Dart binding; widget tests for the scan
  page.
- Flutter: connect (including certificate fingerprint), lobby, room, host
  actions, module panels, scan page; 23 tests at desktop and phone sizes.

## Known gaps

1. **CNN detector and CNN super-resolution are not implemented.** The WeChat
   Caffe weights are not ONNX, no ONNX QR-detector model is available offline,
   and no inference runtime was added. The classical pyramid detector and
   bilinear super-res stand in behind the same traits.
2. **Module execution is blocked.** RustPython 0.6.0 panics during VM
   initialization on this Windows host (`essential initialization failed`, or a
   `codecs` recursion with the frozen stdlib); 0.5 is incompatible with the 0.6
   stdlib. Distribution works; execution does not. See `docs/MODULES.md`.
3. **Mobile/Apple capture does not exist.** The Android APK builds but contains
   no native camera or Rust scanner; iOS/macOS cannot be built or verified on
   Windows.
4. **No live QR symbol in view.** Real textured frames were captured and scanned
   (1080p/720p numbers above), but no QR was in front of the camera, so a live
   symbol was never decoded. The web probe uses a fake camera file.
5. **No browser UI run.** Edge headless drove the scan probe page, but the Flutter
   web UI itself was not manually exercised in a browser.
6. **No remote or CI.** History is the initial scaffold plus one implementation
   commit from this session; nothing has been pushed and no CI ran.

## Reproduce

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

pwsh -File scripts/build-wasm.ps1
node tools/wasm_scan_probe.cjs
pwsh -File scripts/browser-scan-probe.ps1

cd crates/qr_race_gui
flutter analyze
flutter test
flutter build web --release
flutter build windows --release
flutter build apk --debug
```
