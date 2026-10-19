# Web / wasm scanning

Status: works end to end. The wasm scan core is verified in Node, and the full
`getUserMedia → canvas → wasm` path is verified in Edge headless with a fake
camera. A physical-camera run is still pending.

## Pieces

| piece | role |
|---|---|
| `crates/qr_race_scan_wasm` | wasm-bindgen export `scan_gray(width, height, Uint8Array) -> JSON`, plus `version()` and a raw C ABI |
| `scripts/build-wasm.ps1` | builds the module and emits `--target web` glue into `crates/qr_race_gui/web/scan/`, `--target nodejs` glue into `.refs/wasm-node/` |
| `crates/qr_race_gui/web/scan/camera_scan.js` | `getUserMedia` → hidden `<video>` → canvas → grayscale → `scan_gray` (~10 Hz) → `onJson`; stops every track on stop |
| `crates/qr_race_gui/lib/native/scanner_backend_web.dart` | `dart:js_interop` wrapper implementing `ScannerBackend`; polls the latest hits |
| `tools/wasm_scan_probe.cjs` | Node verification of the scan core against a dumped frame |

The wasm build enables `qr_race_scan`'s `rxing-wasm` feature (rxing → chrono
`wasmbind`) so result timestamps don't call `SystemTime::now`. That makes the
module depend on wasm-bindgen glue; the glue is generated, not hand-written.

## Verified

- Node probe against a 1280×720 synthetic scene:

  ```
  node tools/wasm_scan_probe.cjs
  1280x720 wall=18.28ms hits=1 error=null
    raw: https://example.test/relay
  wasm scan OK
  ```

  Two runs measured 18.3 ms and 21.7 ms wall, including the JS↔wasm copies.
  This is the scan core only; the browser's RGBA→gray conversion and canvas
  readback are additional.
- **Browser camera → wasm, in Edge headless with a fake camera.** A Y4M built
  from a 640×480 QR page is played as the webcam, `camera_scan.js` grabs frames,
  scans them with the wasm core, and the probe reports through CDP:

  ```
  pwsh -File scripts/browser-scan-probe.ps1
  {'title': 'HIT:https://example.test/relay', 'out': 'HIT:https://example.test/relay',
   'state': {'frames': 3, 'width': 640, 'height': 480, 'scanMs': 1.8, 'hit': ...}}
  ```

  This covers `getUserMedia` → video → canvas → grayscale → wasm → JSON →
  Dart-shaped result, not just the wasm core. The scan itself measured ~1.8 ms.
- `flutter build web --release` succeeds and ships `scan/camera_scan.js`,
  `scan/qr_race_scan_wasm.js` and `scan/qr_race_scan_wasm_bg.wasm`.

## Rebuild

```powershell
pwsh -File scripts/build-wasm.ps1     # cargo build + wasm-bindgen glue
cargo run --release -p qr_race_bench -- --input .refs/scene.png `
  --dump-gray .refs/scene.gray        # fixture for the Node probe
node tools/wasm_scan_probe.cjs

cd crates/qr_race_gui
flutter build web --release
```

Generated glue under `web/scan/qr_race_scan_wasm*` is gitignored; rerun the
script before a clean web build.

## Constraints

- **Secure context.** `getUserMedia` only works on HTTPS or `localhost`. A plain
  HTTP server on a LAN IP will fail in the browser — use the built-in self-signed
  HTTPS (`tls` in the server config) and accept the certificate once.
- **The browser check uses a fake camera.** Edge's fake video capture plays a
  Y4M of a QR page; a physical camera still needs a manual run. The probe needs
  Edge, ffmpeg, and Python with Pillow + websocket-client.
- **No preview on web.** Frames stay out of Dart by design, so the web page has
  no live preview image; it shows the hit list and scan stats.

## Browser probe

`crates/qr_race_gui/web/scan_probe.html` loads `scan/camera_scan.js`, waits for a
hit and writes it into the page title. `scripts/browser-scan-probe.ps1` serves
the web directory on localhost, builds a Y4M from a generated QR image, starts
Edge headless with `--use-fake-device-for-media-stream` and the file, and drives
the page over CDP with `tools/browser_scan_probe.py`.
