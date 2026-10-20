# Models and the CNN stages

## Why we convert Caffe ourselves

The brief asks for the WeChat-style backend: a CNN detector, an on-demand CNN
super-resolver, then rxing decode. The weights available to us are Caffe
(`detect.prototxt/.caffemodel`, `sr.prototxt/.caffemodel`, Apache-2.0). There is
no prebuilt ONNX for them, so `tools/caffe_to_onnx.py` parses the prototxt and
caffemodel with generated Caffe protobuf bindings and emits ONNX itself. The
script is explicit: one handler per Caffe layer, no conversion framework.

OpenCV's Caffe schema predates the SSD extensions these models use (`Permute`,
`PriorBox`, `DetectionOutput`), so `tools/caffe_wechat.proto` adds those fields
to BVLC's `caffe.proto`. The converter then runs on the same text format Caffe
uses.

## Verified today

- **Super-resolution:** `models/sr.onnx` produces `1×1×447×447` for
  `1×1×224×224`. `tools/verify_sr_numpy.py` re-implements the graph (conv,
  depthwise conv, transposed conv, leaky ReLU, concat, crop, add) directly from
  the Caffe blobs; the maximum absolute difference against ONNX Runtime is
  **5.5e-06**. This validates weight layout, padding, dilation, transposed
  convolution and the crop.
- **Detector:** `models/detect.onnx` loads and runs in ONNX Runtime. It exposes
  `mbox_loc [1,27648]`, `mbox_conf_flatten [1,13824]` and
  `mbox_priorbox [1,2,27648]` (6912 priors); softmax rows sum to 1 and the
  prior boxes match Caffe's normalized center/size/variance layout.
  `DetectionOutput` is deliberately outside the graph.

Detector numeric parity is still open: OpenCV 5 removed the Caffe importer, and
there is no Caffe runtime on this machine. The next check is a NumPy reference
for the detector heads, or a comparison against an OpenCV 4 environment.

## Current status

- **Backend chosen:** Android uses ORT + ONNX Runtime 1.31; desktop uses tract
  as fallback. Device HUD: fast path 2.9–5.7 ms; CNN 384 6.3–15.9 ms; 2×2 tiled
  far-code pass 48.3 ms, inside the 60 ms relaxed CNN budget. `st-zrt` wraps ORT
  (same kernels), `lele` has no ONNX frontend, Candle was not competitive enough
  to justify replacing ORT. `OrtNet` and `TractNet` implement the same `Net`
  trait.
- **Far/small:** a live 11-vmin PC-screen QR decoded
  `https://example.test/far-small-live` at 1280×720 through the tiled path.
  This is one sample; a detection-rate/p95 sweep remains future work.
- **Detector numeric parity** is still open (NumPy reference or OpenCV 4).
- **Wasm/Apple packaging** is not wired; model bytes compile into the native
  FFI library.

## Implementation

- `crates/qr_race_wechat/src/pipeline` equivalent modules: detector, scale,
  decodermgr, wechat_qrcode.
- `src/backend/ort.rs` (Android) and `src/backend/tract.rs` (desktop).
- Android packages `libonnxruntime.so` in `jniLibs/arm64-v8a/`; desktop uses
  tract so no DLL shipping is required.

## License

Weights: © 2021 THL A29 Limited (Tencent), Apache-2.0. The license text is in
`models/WeChat-QRCode-LICENSE.txt`. The ONNX files are format conversions of
those weights.

ONNX Runtime 1.31 (MIT) is not linked into the Rust static library; the Android
build packages `jniLibs/arm64-v8a/libonnxruntime.so` from
`com.microsoft.onnxruntime:onnxruntime-android:1.31.0`, and `ort` loads it with
`load-dynamic`. Desktop uses tract instead, so no ORT DLL is shipped there.
