#!/usr/bin/env python3
"""Independent NumPy forward for the WeChat super-resolution Caffe model.

OpenCV 5 removed the Caffe importer, so ONNX parity cannot be checked with
cv2.dnn. This script re-implements the small SR graph (conv, depthwise conv,
transposed conv, leaky ReLU, concat, crop, add) directly from the Caffe blobs
and compares its output with ONNX Runtime.

    python tools/verify_sr_numpy.py models/sr.onnx \
      .refs/opencv_3rdparty/sr.prototxt .refs/opencv_3rdparty/sr.caffemodel
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from google.protobuf import text_format

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".refs"))
import caffe_wechat_pb2 as caffe_pb2  # noqa: E402


def blob_array(blob) -> np.ndarray:
    dims = [int(dim) for dim in blob.shape.dim]
    if not dims:
        dims = [blob.num, blob.channels, blob.height, blob.width]
    values = np.asarray(blob.data or blob.double_data, dtype=np.float32)
    return values.reshape(dims)


def pairs(param, name: str, default: int) -> tuple[int, int]:
    values = [int(value) for value in getattr(param, name)]
    if len(values) == 1:
        return values[0], values[0]
    if len(values) >= 2:
        return values[0], values[1]
    return default, default


def conv2d(x, weight, bias, stride, pad, group):
    n, c, h, w = x.shape
    out_c, cin_g, kh, kw = weight.shape
    padded = np.pad(x, ((0, 0), (0, 0), (pad, pad), (pad, pad)))
    out_h = (h + 2 * pad - kh) // stride + 1
    out_w = (w + 2 * pad - kw) // stride + 1
    result = np.empty((n, out_c, out_h, out_w), np.float32)
    out_g = out_c // group
    for g in range(group):
        subset = padded[:, g * cin_g : (g + 1) * cin_g]
        acc = np.zeros((n, out_g, out_h, out_w), np.float32)
        for ky in range(kh):
            for kx in range(kw):
                patch = subset[
                    :, :, ky : ky + stride * out_h : stride, kx : kx + stride * out_w : stride
                ]
                acc += np.einsum(
                    "nchw,oc->nohw", patch, weight[g * out_g : (g + 1) * out_g, :, ky, kx]
                )
        result[:, g * out_g : (g + 1) * out_g] = acc
    if bias is not None:
        result += bias.reshape(1, -1, 1, 1)
    return result


def conv_transpose(x, weight, bias, stride, pad, group):
    n, c, h, w = x.shape
    out_c, cout_g, kh, kw = weight.shape
    out_h = (h - 1) * stride - 2 * pad + kh
    out_w = (w - 1) * stride - 2 * pad + kw
    result = np.zeros((n, out_c, out_h, out_w), np.float32)
    in_g = c // group
    for g in range(group):
        for ci in range(in_g):
            channel = x[:, g * in_g + ci]
            for ky in range(kh):
                ys = np.arange(h) * stride - pad + ky
                valid_y = (ys >= 0) & (ys < out_h)
                for kx in range(kw):
                    xs = np.arange(w) * stride - pad + kx
                    valid_x = (xs >= 0) & (xs < out_w)
                    if not valid_y.any() or not valid_x.any():
                        continue
                    tile = channel[:, valid_y][:, :, valid_x]
                    for co in range(cout_g):
                        target = result[:, g * cout_g + co]
                        target[
                            :,
                            ys[valid_y][:, None],
                            xs[valid_x][None, :],
                        ] += tile * weight[g * in_g + ci, co, ky, kx]
    if bias is not None:
        result += bias.reshape(1, -1, 1, 1)
    return result


def leaky(x, slope):
    return np.where(x < 0, x * slope, x)


def forward(prototxt: Path, caffemodel: Path, sample: np.ndarray) -> np.ndarray:
    net = caffe_pb2.NetParameter()
    text_format.Parse(prototxt.read_text(encoding="utf-8"), net)
    model = caffe_pb2.NetParameter()
    model.ParseFromString(caffemodel.read_bytes())
    weights = {layer.name: [blob_array(blob) for blob in layer.blobs] for layer in model.layer}

    blobs: dict[str, np.ndarray] = {}
    for layer in net.layer:
        if layer.type == "Input":
            blobs[layer.top[0]] = sample
            continue
        source = blobs[layer.bottom[0]]
        if layer.type == "Convolution":
            params = layer.convolution_param
            kernel = pairs(params, "kernel_size", 0)
            stride = pairs(params, "stride", 1)
            pad = pairs(params, "pad", 0)
            out = conv2d(
                source,
                weights[layer.name][0],
                weights[layer.name][1] if len(weights[layer.name]) > 1 else None,
                stride[0],
                pad[0],
                int(params.group) or 1,
            )
            blobs[layer.top[0]] = out
        elif layer.type == "Deconvolution":
            params = layer.convolution_param
            kernel = pairs(params, "kernel_size", 0)
            stride = pairs(params, "stride", 1)
            pad = pairs(params, "pad", 0)
            out = conv_transpose(
                source,
                weights[layer.name][0],
                weights[layer.name][1] if len(weights[layer.name]) > 1 else None,
                stride[0],
                pad[0],
                int(params.group) or 1,
            )
            blobs[layer.top[0]] = out
        elif layer.type == "ReLU":
            slope = float(layer.relu_param.negative_slope)
            blobs[layer.top[0]] = leaky(source, slope)
        elif layer.type == "Concat":
            blobs[layer.top[0]] = np.concatenate(
                [blobs[name] for name in layer.bottom], axis=int(layer.concat_param.axis or 1)
            )
        elif layer.type == "Crop":
            reference = blobs[layer.bottom[1]]
            offsets = [int(value) for value in layer.crop_param.offset] or [0, 0]
            if len(offsets) == 1:
                offsets *= 2
            target_h, target_w = reference.shape[2], reference.shape[3]
            start_y = offsets[0]
            start_x = offsets[1]
            blobs[layer.top[0]] = source[
                :, :, start_y : start_y + target_h, start_x : start_x + target_w
            ]
        elif layer.type == "Eltwise":
            total = blobs[layer.bottom[0]].copy()
            for name in layer.bottom[1:]:
                total += blobs[name]
            blobs[layer.top[0]] = total
        else:
            raise SystemExit(f"unsupported layer {layer.name} ({layer.type})")

    return blobs[net.layer[-1].top[0]]


def main() -> None:
    model_path, prototxt, caffemodel = sys.argv[1:4]
    session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
    shape = [1, 1, 224, 224]
    sample = np.random.default_rng(7).standard_normal(shape).astype(np.float32)
    onnx_output = session.run(None, {session.get_inputs()[0].name: sample})[0]
    reference = forward(Path(prototxt), Path(caffemodel), sample)
    diff = float(np.max(np.abs(onnx_output - reference)))
    print(f"onnx {onnx_output.shape}, numpy {reference.shape}, max diff {diff:.6g}")
    if diff > 1e-4:
        raise SystemExit(f"numeric mismatch: {diff}")


if __name__ == "__main__":
    main()

