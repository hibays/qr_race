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

## What is still missing

1. **Faster backend.** The current tract backend runs both models on device
   (fast path remains rxing; CNN path 23–34 ms at 384, ~90 ms at 768). ORT /
   st-zrt / Candle comparison is the next step; replace `src/backend.rs` behind
   the `Net` trait.
2. **Far-code tuning.** The tiered pipeline has 384 → full-res crop → SR →
   scheduled 768 detection. Live far/small samples still need a tuning pass
   (confidence threshold, tile/ROI policy) before claiming scan-rate gains.
3. **Detector numeric parity.** A NumPy reference for the detector heads or an
   OpenCV 4 comparison is still open.
4. **Wasm/Apple packaging.** Model bytes compile into the native FFI library;
   wasm and Apple backends are not wired.

## License

Weights: © 2021 THL A29 Limited (Tencent), Apache-2.0. The license text is in
`models/WeChat-QRCode-LICENSE.txt`. The ONNX files are format conversions of
those weights.
