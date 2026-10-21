# Models and the CNN stages

## Why we convert Caffe ourselves

The brief asks for the WeChat-style backend: a CNN detector, an on-demand CNN
super-resolver, then zxing-cpp decode. The weights available to us are Caffe
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

- **Backend chosen:** Android and desktop use ort rc15 + ONNX Runtime 1.29
  as fallback. Real 1920×1440 run: fast p50 8.3 ms / p95 21.1 ms, CNN 9.5 ms.
  `ort` rc15 loads ONNX Runtime 1.29 dynamically. Desktop: detect 384 p50
  6.3 ms, 2×2 tiled far p50 19.1 ms. `st-zrt` is benchmark-only; lele is the
  WASM AOT direction. `OrtNet` and `LeleNet` implement the
  same `Net` trait.
- **Far/small:** a live 11-vmin PC-screen QR decoded
  `https://example.test/far-small-live` at 1280×720 through the tiled path.
  This is one sample; a detection-rate/p95 sweep remains future work.
- **Detector numeric parity** is still open (NumPy reference or OpenCV 4).
- **Wasm/Apple packaging** is not wired; model bytes compile into the native
  FFI library.

## Implementation

- `crates/qr_race_wechat/src/pipeline` equivalent modules: detector, scale,
  decodermgr, wechat_qrcode.
- `src/backend/ort.rs` (native); `src/backend/lele.rs` (WASM direction).
- Android packages `libonnxruntime.so` per ABI under `jniLibs/<abi>/`; desktop
  uses AOT-generated Rust so no ONNX runtime DLL is shipped.

## License

Weights: © 2021 THL A29 Limited (Tencent), Apache-2.0. The license text is in
`models/WeChat-QRCode-LICENSE.txt`. The ONNX files are format conversions of
those weights.

ONNX Runtime 1.29 (MIT) is not vendored; the Android
build packages `jniLibs/<abi>/libonnxruntime.so` from
`com.microsoft.onnxruntime:onnxruntime-android:1.29.0`; `ort` loads it
dynamically. Desktop packages the same ORT 1.29 DLL next to the executable.
















