# Backend matrix and crate boundaries

This is the backend plan that the current tree is converging on. It separates
the **capture plane**, the **scan pipeline**, and the **inference backend** so a
target can swap one without changing the lobby/room/relay code.

## Target matrix

| Target | Inference | Capture | Status |
|---|---|---|---|
| WASM (`wasm32-unknown-unknown`) | lele AOT Rust codegen | `getUserMedia` → WebGL texture → WASM memory | Web UI currently uses `zxing-wasm`; lele WASM split still to do |
| Android | ONNX Runtime through `st-zrt`; XNNPACK/CoreML provider hooks | Camera2 YUV → JNI → Rust `&[u8]`; Camera2 `SurfaceTexture` → Flutter Texture | st-zrt/ORT 1.29 and Texture preview work on device |
| iOS/macOS | ONNX Runtime CoreML EP | AVFoundation/CVPixelBuffer → Rust; Metal texture | capture-apple not implemented |
| Windows/Linux/macOS desktop | `ort` rc15 / Candle; lele remains an AOT option | nokhwa (MediaFoundation/V4L2/AVFoundation) | lele is the current desktop default; `ort` rc15 benchmark tool is in `tools/backend_bench/ort` |

The mobile/desktop distinction matters: `st-zrt-sys` and `ort-sys` both declare
`links = "onnxruntime"`. Cargo refuses to resolve a graph containing both, even
when they are optional features. They therefore belong in **separate backend
crates**, not behind two features on the same crate. That is the main reason to
adopt the `detect-*` split below.

## Crate layout: current → target

| Target crate | Current implementation | Action |
|---|---|---|
| `scan-core` | `qr_race_scan::{GrayFrame, Decoder, Detector, ScanHit}` | extract the pure data/traits; keep `qr_race_scan` as the facade |
| `scan-pipeline` | `qr_race_scan::ScanEngine` | move orchestration/NMS/ROI/rayon decode here |
| `decode-zxing` | `qr_race_scan::ZxingDecoder` | keep; wrap C++ zxing-cpp, no platform capture dependency |
| `detect-lele` | `qr_race_wechat::backend::lele` | AOT codegen; suitable for WASM |
| `detect-stzrt` | `qr_race_wechat::backend::st_zrt` | Android ORT 1.29; add XNNPACK/CoreML EP configuration where available |
| `detect-ort` | not in the product crate | new desktop backend using `ort` rc15; benchmark tool exists |
| `detect-candle` | not implemented | optional desktop backend |
| `capture-core` | `qr_race_capture` | frame format + `CaptureSource` trait + pacer |
| `capture-desktop` | `qr_race_capture_desktop` | nokhwa backends |
| `capture-android` | Kotlin `CameraCapture` + JNI in `qr_race_ffi` | extract into a dedicated crate/module; keep the zero-copy YUV path |
| `capture-apple` | absent | AVFoundation/CVPixelBuffer |
| `capture-wasm` | JS `getUserMedia` + `zxing-wasm` bridge | WebGL texture → WASM memory; lele inference |
| `bindings-flutter` | `qr_race_ffi` C ABI + method channel/Texture widgets | keep; expose the new backend/capture capabilities |

The important invariant stays unchanged: **camera frames never enter Dart**.
Native capture owns the buffers; Flutter receives only a texture id (preview)
and scan state (hits/timings). The C ABI remains the compatibility boundary.

## ORT rc15 vs st-zrt 0.4.1 benchmark

Both tools run the same ONNX Runtime 1.29 x64 DLL, zero-filled `[1,1,384,384]`
detect input and `[1,1,224,224]` SR input, single thread, 20 warmup + 200
measured runs. Numbers are raw model forward only, no pre/post-processing.

| Backend | detect p50 | detect p95 | SR p50 | SR p95 |
|---|---:|---:|---:|---:|
| `ort` 2.0.0-rc.15 | 1.528 ms | 2.680 ms | 18.902 ms | 20.830 ms |
| `st-zrt` 0.4.1 | 1.496 ms | 2.248 ms | 18.111 ms | 19.591 ms |

The two wrappers are within noise on these models: ORT owns the kernels, and
the wrapper cost is small relative to the 384 detector / 224 SR graph. st-zrt's
prepared-lane API can remove more per-run marshaling in a serving loop; this
benchmark uses the simple `Session::run` path on both sides so the comparison is
apples-to-apples.

Reproduce:

```powershell
pwsh -File scripts/bench_backends.ps1
```

The tools live outside the main workspace because of the `links = "onnxruntime"`
conflict described above:

```text
tools/backend_bench/ort
tools/backend_bench/stzrt
```

## Proposed migration order

1. Move `Net`/`NetOutput` and the scan traits into a dependency-free `scan-core`.
2. Split `qr_race_scan` into `scan-pipeline` + `decode-zxing`, keeping
   `qr_race_scan` as a re-export facade so callers do not change.
3. Create one `detect-*` crate per runtime. This is the only way to let ORT,
   st-zrt, lele and Candle coexist in the workspace without Cargo's `links`
   collision.
4. Move Android capture out of `qr_race_ffi` into `capture-android`; keep the
   Kotlin `SurfaceTexture`/YUV bridge as its native implementation.
5. Add `capture-apple`/`capture-wasm` behind the same `CaptureSource` contract.
6. Keep `bindings-flutter` thin: method channel + texture ids + JSON state only.

Until steps 1–3 land, the product continues to use `lele` on desktop and
`st-zrt` on Android; the `ort` rc15 path is benchmarked but not the default.
