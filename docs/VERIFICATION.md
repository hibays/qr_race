# Verification

Audit date: 2026-10-20, Windows x64. This is the evidence behind the README's
claims. It also lists what is **not** verified, so nothing here is mistaken for a
finished product.

## Gates

| gate | command | result |
|---|---|---|
| Rust format | `cargo fmt --all -- --check` | clean |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| Rust tests | `cargo test --workspace` | **129 passed** |
| Flutter analyze | `flutter analyze` | no issues |
| Flutter tests | `flutter test` | **28 passed** |
| Windows build | `flutter build windows --release` | built |
| Web build | `pwsh -File scripts/build-wasm.ps1` then `flutter build web --release` | built |
| Android build | `flutter build apk --debug` | built (arm64 Rust lib + Camera2 scanning + embedded server) |

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

### Perturbed fixtures

- `qr_race_fixtures` generates ten deterministic scenes (texture, noise, blur,
  low contrast, gradient, rotation, perspective, small, multi-code, camera-like)
  from rxing codes; tests assert every expected payload decodes.
- `qr_bench --suite` reports per-scene timings; whole-frame multi-code is 44 ms
  (over the 33 ms budget) while the pyramid pipeline is 7.6 ms. Table in
  `docs/PERF.md`.

### Model conversion

- `tools/caffe_to_onnx.py` parses the WeChat Caffe prototxt/caffemodel with
  generated bindings and writes ONNX directly (no conversion service). The SSD
  extension fields are in `tools/caffe_wechat.proto`.
- SR: `models/sr.onnx` (24 KB) is numerically verified against an independent
  NumPy forward of the same weights, max abs diff **5.5e-06**, shape
  `1×1×447×447` from `1×1×224×224`.
- Detector: `models/detect.onnx` (1.19 MB) loads in ONNX Runtime and returns
  `mbox_loc [1,27648]`, `mbox_conf_flatten [1,13824]`,
  `mbox_priorbox [1,2,27648]`; softmax sums to 1. `DetectionOutput` is omitted;
  decode/NMS is the next step. No Caffe runtime was available for detector
  numeric parity (OpenCV 5 removed `readNetFromCaffe`).

### Self-host and Android device

- Desktop self-host (`SelfHostScreen` + `ProcessServerLauncher`) provisions two
  accounts, writes TLS/SQLite config, starts the real `qr_race_server`, reads the
  printed certificate fingerprint, and authenticates with a computed TOTP.
  `dart run tool/selfhost_probe.dart` completed this against the release binary:
  `https://<lan>:18470`, certificate fingerprint `b258ac…580f`, authenticated as
  `host`, empty lobby, then stopped the server.
- Real Android device (OnePlus PLF110, `3B163Q00LSK00000`): debug APK installed
  via adb. With `adb reverse tcp:18471 tcp:18471`, the app connected to a server
  started by `scripts/deploy-server.ps1` using
  `https://127.0.0.1:18471`, the printed certificate fingerprint, a host token,
  and a TOTP code; the lobby rendered as `host`.
- Android self-host: `qr_race_embedded` is compiled into
  `libqr_race_ffi.so`. Starting from the UI on port 18443 generated a
  self-signed certificate, reported `https://127.0.0.1:18443` plus fingerprint,
  and “用此账号连接” authenticated over pinned TLS into the lobby. 8443 was
  already in use on this device, which is why the UI exposes the port.
- Android camera: Camera2 YUV frames are rotated and pushed through JNI into
  Rust; the preview is 720×1280, scan time ~6.2 ms on device, and a live QR
  (`https://example.test/camera-hit`) was decoded from the screen. The preview
  keeps the previous frame between updates, removing the black flicker.
- Permission and links: CAMERA runtime dialog with Settings fallback;
  `MANAGE_EXTERNAL_STORAGE` request for arbitrary certificate paths; file picker
  copies into app cache. A `qrrace://connect?...` deep link filled server,
  fingerprint and token in the connect form.
- Auto-publish: scan hits are published without a button; payloads with the same
  prefix (`A:...`, `cam1|...`) update one room slot, different prefixes coexist.
  External links use the Android chooser (WeChat/WeCom/browser) with a copy
  fallback.

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

1. **CNN inference is not wired yet.** The ONNX models exist (SR verified,
   detector structure-verified), but no ONNX Runtime backend implements the
   `Detector`/`SuperResolver` traits and detector decode/NMS is pending. The
   classical pyramid detector and bilinear super-res stand in behind the same
   traits.
2. **Module execution is blocked.** RustPython 0.6.0 panics during VM
   initialization on this Windows host (`essential initialization failed`, or a
   `codecs` recursion with the frozen stdlib); 0.5 is incompatible with the 0.6
   stdlib. Distribution works; execution does not. See `docs/MODULES.md`.
3. **Apple capture does not exist.** iOS/macOS cannot be built or verified on
   Windows; Android now has its own Camera2 capture and in-process server.
4. **No browser UI run.** Edge headless drove the scan probe page, but the Flutter
   web UI itself was not manually exercised in a browser.
5. **No remote or CI.** History is the initial scaffold plus implementation
   commits from this session; nothing has been pushed and no CI ran.

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

# self-host launcher (starts a real server, authenticates, stops it)
dart run tool/selfhost_probe.dart

# Android device (builds, adb reverse, installs, launches)
pwsh -File scripts/android-deploy.ps1 -Port 8443
```
