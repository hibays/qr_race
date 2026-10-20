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

1. **Rust inference.** Add an ONNX Runtime backend (`ort`) implementing the
   `Detector` and `SuperResolver` traits over these models. The engine is
   already trait-based; only the backends are missing.
2. **Detector postprocess.** Decode `mbox_loc` against `mbox_priorbox`
   (CENTER_SIZE/variance), threshold `mbox_conf`, run NMS, and map boxes back to
   frame coordinates.
3. **SR integration.** Run `sr.onnx` only on small detected crops (the
   on-demand stage), never per frame.
4. **Platform packaging.** Ship the two ONNX files with the desktop/mobile
   builds and expose them through the existing FFI/wasm layers.

## License

Weights: © 2021 THL A29 Limited (Tencent), Apache-2.0. The license text is in
`models/WeChat-QRCode-LICENSE.txt`. The ONNX files are format conversions of
those weights.
