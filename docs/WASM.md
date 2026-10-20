# Web / wasm scanning

Status: works end to end with **zxing-wasm** (ZXing-C++ compiled to WebAssembly).
No rxing, no Rust wasm crate, and no server round-trip.

| piece | role |
|---|---|
| `web/scan/camera_scan.js` | `getUserMedia` → canvas → `readBarcodes`; also `qrRaceQrWebp` via `writeBarcode` |
| `web/scan/zxing-wasm/` | vendored zxing-wasm ES modules (reader + writer) |
| `web/scan/zxing_reader.wasm` | zxing-cpp reader wasm |
| `web/scan/zxing_writer.wasm` | zxing-cpp writer wasm |
| `lib/native/scanner_backend_web.dart` | polls the hit JSON, same interface as native |
| `lib/native/qr_renderer_web.dart` | receives WebP bytes from `qrRaceQrWebp` |

The browser path keeps frames in JavaScript; Dart only receives hits/stats.

## Verified

- Deterministic fake camera probe:

  ```text
  pwsh -File scripts/browser-scan-probe.ps1
  {'title': 'HIT:https://example.test/relay', 'state':
   {'frames': 4, 'width': 640, 'height': 480, 'scanMs': 1.7,
    'hit': 'https://example.test/relay', 'error': None}}
  ```

- WebP QR generation through zxing-wasm returned a 13,616-byte RIFF/WebP.
- Physical browser camera: `-RealCamera` acquired 1280×720 and zxing-wasm
  scanned at ~6.9 ms/frame; no QR was in view, so a physical hit is still manual.

## Rebuild

`flutter build web --release` (assets under `web/scan/` are checked in).

## Constraints

- **Secure context.** `getUserMedia` needs HTTPS or localhost.
- The browser backend is zxing-wasm, not the Rust zxing-cpp binding; both wrap
  the same ZXing-C++ core.
- Physical-camera QR hit still needs a manual run.

