# ONNX models

The scan backend's CNN stages start from the WeChat QRCode Caffe weights shipped
in OpenCV's `3rdparty/wechat_qrcode` (Tencent, Apache-2.0). We convert them
ourselves with `tools/caffe_to_onnx.py`; no third-party conversion service or
prebuilt ONNX download is involved.

## Files

| file | input | outputs | status |
|---|---|---|---|
| `sr.onnx` (24 KB) | `1×1×224×224` | `1×1×447×447` | **numerically verified** against an independent NumPy forward: max abs diff **5.5e-06** |
| `detect.onnx` (1.19 MB) | `1×1×384×384` | `mbox_loc [1,27648]`, `mbox_conf_flatten [1,13824]`, `mbox_priorbox [1,2,27648]` (6912 priors) | loads and runs in ONNX Runtime; softmax rows sum to 1; prior boxes sane. `DetectionOutput` (decode + NMS) is **not** part of the graph and still has to be implemented. |

`DetectionOutput` has no standard ONNX op, so the converter stops before it and
exposes the three tensors it consumes. The next step is a Rust postprocess:
decode `mbox_loc` against `mbox_priorbox`, threshold/softmax `mbox_conf`, NMS.

Numeric parity for the detector is still unverified: OpenCV 5 removed
`cv2.dnn.readNetFromCaffe`, and no Caffe runtime is available here. The SR model
was verified with a NumPy reimplementation of its exact layer graph.

## Regenerate

```powershell
# Caffe protobuf bindings (the file adds Permute/PriorBox/DetectionOutput fields
# that BVLC's caffe.proto does not have)
python -m grpc_tools.protoc -I tools --python_out=.refs tools/caffe_wechat.proto

python tools/caffe_to_onnx.py sr `
  .refs/opencv_3rdparty/sr.prototxt .refs/opencv_3rdparty/sr.caffemodel `
  models/sr.onnx
python tools/verify_sr_numpy.py models/sr.onnx `
  .refs/opencv_3rdparty/sr.prototxt .refs/opencv_3rdparty/sr.caffemodel

python tools/caffe_to_onnx.py detect `
  .refs/opencv_3rdparty/detect.prototxt .refs/opencv_3rdparty/detect.caffemodel `
  models/detect.onnx
```

The `.refs/opencv_3rdparty` checkout is gitignored; the source models are not
vendored in this repository.

## Layer mapping

| Caffe | ONNX |
|---|---|
| Convolution (incl. groups, dilation) | `Conv` |
| Deconvolution | `ConvTranspose` |
| ReLU (`negative_slope > 0`) | `LeakyRelu` / `Relu` |
| BatchNorm + Scale (inference) | `Sub`/`Div` folded with `Mul`/`Add` |
| Pooling | `MaxPool` / `AveragePool` |
| Concat / Eltwise | `Concat` / `Add`, `Mul`, `Max` |
| Permute / Flatten / Reshape / Softmax | `Transpose` / `Flatten` / `Reshape` / `Softmax` |
| Crop | `Slice` (offset-aware) |
| PriorBox | precomputed `Constant` (Caffe's box/variance layout) |
| DetectionOutput | omitted; raw tensors are the graph outputs |

## License

The model weights are Apache-2.0, © 2021 THL A29 Limited (Tencent). The license
text is copied to `models/WeChat-QRCode-LICENSE.txt`; these ONNX files are
modifications (format conversion) of those weights.
