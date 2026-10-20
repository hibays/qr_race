#!/usr/bin/env python3
"""Convert the WeChat QRCode Caffe models to ONNX.

The weights come from OpenCV's `opencv_3rdparty/wechat_qrcode` (Apache-2.0,
Tencent). This script parses the prototxt + caffemodel with the generated Caffe
protobuf bindings and writes an ONNX graph. It is deliberately explicit about
layer mappings instead of hiding them in a framework: every op is visible.

Prerequisites (see docs/MODELS.md):

    curl -L -o .refs/caffe.proto \
      https://raw.githubusercontent.com/BVLC/caffe/master/src/caffe/proto/caffe.proto
    python -m grpc_tools.protoc -I .refs --python_out=.refs .refs/caffe.proto
    python -m pip install onnx onnxruntime

Usage:

    python tools/caffe_to_onnx.py sr \
      .refs/opencv_3rdparty/sr.prototxt .refs/opencv_3rdparty/sr.caffemodel \
      models/sr.onnx --verify

    python tools/caffe_to_onnx.py detect \
      .refs/opencv_3rdparty/detect.prototxt .refs/opencv_3rdparty/detect.caffemodel \
      models/detect.onnx --verify
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import numpy as np
import onnx
from google.protobuf import text_format
from onnx import TensorProto, helper, numpy_helper

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".refs"))
import caffe_wechat_pb2 as caffe_pb2  # noqa: E402  (generated from caffe_wechat.proto)


class Converter:
    def __init__(self, prototxt: Path, caffemodel: Path):
        self.net = caffe_pb2.NetParameter()
        text_format.Parse(prototxt.read_text(encoding="utf-8"), self.net)
        self.model = caffe_pb2.NetParameter()
        self.model.ParseFromString(caffemodel.read_bytes())
        self.weights = {layer.name: layer.blobs for layer in self.model.layer}

        self.nodes: list[onnx.NodeProto] = []
        self.initializers: list[onnx.TensorProto] = []
        self.shapes: dict[str, list[int]] = {}
        self.resolved: dict[str, str] = {}
        self.counter = 0
        self.skipped: list[str] = []
        self.input_name = "data"
        self.input_shape: list[int] | None = None

    # -- helpers ---------------------------------------------------------
    def tensor_name(self, layer, top: str) -> str:
        """ONNX requires unique value names; in-place Caffe layers reuse tops."""
        name = self.resolved.get(top, top)
        if top in layer.bottom:
            self.counter += 1
            name = f"{layer.name}/onnx{self.counter}"
        self.resolved[top] = name
        return name

    def source(self, name: str) -> str:
        return self.resolved.get(name, name)

    def blob_values(self, blob) -> np.ndarray:
        dims = [int(dim) for dim in blob.shape.dim]
        if not dims:
            dims = [blob.num, blob.channels, blob.height, blob.width]
        if len(blob.data):
            values = np.asarray(blob.data, dtype=np.float32)
        elif len(blob.double_data):
            values = np.asarray(blob.double_data, dtype=np.float32)
        else:
            values = np.asarray(blob.float_data, dtype=np.float32)
        return values.reshape(dims)

    def initializer(self, name: str, array: np.ndarray) -> str:
        self.initializers.append(numpy_helper.from_array(array, name))
        return name

    def node(self, op: str, inputs, outputs, **attrs) -> None:
        name = f"{op}_{len(self.nodes)}"
        self.nodes.append(helper.make_node(op, inputs, outputs, name=name, **attrs))

    # -- shape tracking --------------------------------------------------
    def conv_out(self, size: int, kernel: int, stride: int, pad: int) -> int:
        return (size + 2 * pad - kernel) // stride + 1

    # -- layer handlers --------------------------------------------------
    def convert_input(self, layer) -> None:
        if self.input_shape is not None:
            return
        shape = layer.input_param.shape[0]
        self.input_shape = [int(dim) for dim in shape.dim]
        self.shapes[layer.top[0]] = self.input_shape
        self.resolved[layer.top[0]] = layer.top[0]
        self.input_name = layer.top[0]

    def convert_convolution(self, layer) -> None:
        params = layer.convolution_param
        blobs = self.weights[layer.name]
        weight = self.blob_values(blobs[0])
        kernel = [int(v) for v in params.kernel_size] or [
            int(params.kernel_h),
            int(params.kernel_w),
        ]
        stride = [int(v) for v in params.stride] or [int(params.stride_h), int(params.stride_w)]
        pad = [int(v) for v in params.pad] or [int(params.pad_h), int(params.pad_w)]
        if len(stride) == 1:
            stride *= 2
        if len(pad) == 1:
            pad *= 2
        if len(kernel) == 1:
            kernel *= 2
        dilation = [int(value) for value in params.dilation] or [1, 1]
        if len(dilation) == 1:
            dilation *= 2
        group = int(params.group) or 1
        inputs = [self.source(layer.bottom[0])]
        inputs.append(self.initializer(f"{layer.name}:w", weight))
        if len(blobs) > 1:
            inputs.append(self.initializer(f"{layer.name}:b", self.blob_values(blobs[1])))
        output = self.tensor_name(layer, layer.top[0])
        self.node(
            "Conv",
            inputs,
            [output],
            group=group,
            pads=[pad[0], pad[1], pad[0], pad[1]],
            strides=[stride[0], stride[1]],
            dilations=[dilation[0], dilation[1]],
        )
        source = self.shapes[self.source(layer.bottom[0])]
        effective = [dilation[0] * (kernel[0] - 1) + 1, dilation[1] * (kernel[1] - 1) + 1]
        self.shapes[output] = [
            source[0],
            int(params.num_output),
            self.conv_out(source[2], effective[0], stride[0], pad[0]),
            self.conv_out(source[3], effective[1], stride[1], pad[1]),
        ]

    def convert_deconvolution(self, layer) -> None:
        params = layer.convolution_param
        blobs = self.weights[layer.name]
        weight = self.blob_values(blobs[0])
        source_shape = self.shapes[self.source(layer.bottom[0])]
        group = int(params.group) or 1
        out_channels = int(params.num_output)
        # ONNX ConvTranspose wants [C_in, C_out/group, kH, kW]; Caffe stores
        # [C_out, C_in/group, kH, kW] for its transposed convolution.
        if weight.shape[0] == out_channels and weight.shape[1] == source_shape[1] // group:
            if weight.shape[0] != source_shape[1] or weight.shape[1] != out_channels // group:
                weight = np.transpose(weight, (1, 0, 2, 3))
        kernel = [int(v) for v in params.kernel_size] or [
            int(params.kernel_h),
            int(params.kernel_w),
        ]
        stride = [int(v) for v in params.stride] or [int(params.stride_h), int(params.stride_w)]
        pad = [int(v) for v in params.pad] or [int(params.pad_h), int(params.pad_w)]
        if len(stride) == 1:
            stride *= 2
        if len(pad) == 1:
            pad *= 2
        if len(kernel) == 1:
            kernel *= 2
        inputs = [self.source(layer.bottom[0]), self.initializer(f"{layer.name}:w", weight)]
        if len(blobs) > 1 and params.bias_term:
            inputs.append(self.initializer(f"{layer.name}:b", self.blob_values(blobs[1])))
        output = self.tensor_name(layer, layer.top[0])
        self.node(
            "ConvTranspose",
            inputs,
            [output],
            group=group,
            pads=[pad[0], pad[1], pad[0], pad[1]],
            strides=[stride[0], stride[1]],
        )
        self.shapes[output] = [
            source_shape[0],
            out_channels,
            (source_shape[2] - 1) * stride[0] - 2 * pad[0] + kernel[0],
            (source_shape[3] - 1) * stride[1] - 2 * pad[1] + kernel[1],
        ]

    def convert_relu(self, layer) -> None:
        slope = float(layer.relu_param.negative_slope) if layer.relu_param else 0.0
        source = self.source(layer.bottom[0])
        source_shape = list(self.shapes[source])
        output = self.tensor_name(layer, layer.top[0])
        if slope > 0.0:
            self.node("LeakyRelu", [source], [output], alpha=slope)
        else:
            self.node("Relu", [source], [output])
        self.shapes[output] = source_shape

    def convert_concat(self, layer) -> None:
        output = self.tensor_name(layer, layer.top[0])
        axis = int(layer.concat_param.axis or 1)
        self.node(
            "Concat",
            [self.source(name) for name in layer.bottom],
            [output],
            axis=axis,
        )
        shapes = [self.shapes[self.source(name)] for name in layer.bottom]
        merged = list(shapes[0])
        merged[axis] = sum(shape[axis] for shape in shapes)
        self.shapes[output] = merged

    def convert_eltwise(self, layer) -> None:
        operation = int(layer.eltwise_param.operation) if layer.eltwise_param else 1
        op = {1: "Add", 2: "Mul", 3: "Max"}.get(operation, "Add")
        output = self.tensor_name(layer, layer.top[0])
        inputs = [self.source(name) for name in layer.bottom]
        if len(inputs) == 2:
            self.node(op, inputs, [output])
        else:
            current = inputs[0]
            for index, extra in enumerate(inputs[1:]):
                merged = f"{layer.name}/onnx_elt{index}"
                self.node(op, [current, extra], [merged])
                current = merged
            self.node("Identity", [current], [output])
        self.shapes[output] = list(self.shapes[self.source(layer.bottom[0])])

    def convert_crop(self, layer) -> None:
        data = self.source(layer.bottom[0])
        reference = self.source(layer.bottom[1])
        output = self.tensor_name(layer, layer.top[0])
        target = list(self.shapes[reference])
        source_shape = self.shapes[data]
        if source_shape == target:
            self.node("Identity", [data], [output])
        else:
            offsets = [int(value) for value in layer.crop_param.offset] or [0, 0]
            if len(offsets) == 1:
                offsets *= 2
            starts = [0, 0, offsets[0], offsets[1]]
            ends = list(target)
            self.node(
                "Slice",
                [
                    data,
                    self.initializer(f"{layer.name}:starts", np.asarray(starts, dtype=np.int64)),
                    self.initializer(f"{layer.name}:ends", np.asarray(ends, dtype=np.int64)),
                    self.initializer(
                        f"{layer.name}:axes", np.asarray([0, 1, 2, 3], dtype=np.int64)
                    ),
                ],
                [output],
            )
        self.shapes[output] = target

    def convert_batchnorm(self, layer) -> None:
        blobs = self.weights[layer.name]
        mean = self.blob_values(blobs[0]).reshape(1, -1, 1, 1).astype(np.float32)
        variance = self.blob_values(blobs[1]).reshape(1, -1, 1, 1).astype(np.float32)
        source = self.source(layer.bottom[0])
        source_shape = list(self.shapes[source])
        output = self.tensor_name(layer, layer.top[0])
        # Inference BatchNorm: (x - mean) / sqrt(var + eps). The following
        # Scale layer supplies gamma/beta, so folding keeps the graph to plain
        # arithmetic and avoids provider quirks with the BatchNorm op.
        shifted = f"{layer.name}/shift"
        self.node(
            "Sub",
            [source, self.initializer(f"{layer.name}:mean", mean)],
            [shifted],
        )
        denominator = np.sqrt(variance + 1e-5).astype(np.float32)
        self.node(
            "Div",
            [shifted, self.initializer(f"{layer.name}:denom", denominator)],
            [output],
        )
        self.shapes[output] = source_shape

    def convert_scale(self, layer) -> None:
        blobs = self.weights[layer.name]
        scale = self.blob_values(blobs[0]).reshape(1, -1, 1, 1).astype(np.float32)
        source = self.source(layer.bottom[0])
        source_shape = list(self.shapes[source])
        output = self.tensor_name(layer, layer.top[0])
        self.node(
            "Mul",
            [source, self.initializer(f"{layer.name}:scale", scale)],
            [output],
        )
        if len(blobs) > 1 and layer.scale_param.bias_term:
            bias = self.blob_values(blobs[1]).reshape(1, -1, 1, 1).astype(np.float32)
            biased = f"{layer.name}/bias"
            self.node(
                "Add",
                [output, self.initializer(f"{layer.name}:bias", bias)],
                [biased],
            )
            self.resolved[layer.top[0]] = biased
            self.shapes[biased] = source_shape
            return
        self.shapes[output] = source_shape

    def convert_pooling(self, layer) -> None:
        params = layer.pooling_param
        kernel = [int(params.kernel_size)] or [int(params.kernel_h), int(params.kernel_w)]
        if len(kernel) == 1:
            kernel *= 2
        stride = [int(params.stride)] if params.stride else [1, 1]
        if len(stride) == 1:
            stride *= 2
        pad = [int(params.pad)] if params.pad else [0, 0]
        if len(pad) == 1:
            pad *= 2
        pool = int(params.pool)
        source = self.source(layer.bottom[0])
        source_shape = self.shapes[source]
        output = self.tensor_name(layer, layer.top[0])
        self.node(
            "MaxPool" if pool == 0 else "AveragePool",
            [source],
            [output],
            kernel_shape=kernel,
            strides=stride,
            pads=[pad[0], pad[1], pad[0], pad[1]],
        )
        self.shapes[output] = [
            source_shape[0],
            source_shape[1],
            (source_shape[2] + 2 * pad[0] - kernel[0]) // stride[0] + 1,
            (source_shape[3] + 2 * pad[1] - kernel[1]) // stride[1] + 1,
        ]

    def convert_permute(self, layer) -> None:
        order = [int(value) for value in layer.permute_param.order]
        source = self.source(layer.bottom[0])
        source_shape = self.shapes[source]
        output = self.tensor_name(layer, layer.top[0])
        self.node("Transpose", [source], [output], perm=order)
        self.shapes[output] = [source_shape[index] for index in order]

    def convert_flatten(self, layer) -> None:
        axis = int(layer.flatten_param.axis or 1)
        source = self.source(layer.bottom[0])
        source_shape = self.shapes[source]
        output = self.tensor_name(layer, layer.top[0])
        self.node("Flatten", [source], [output], axis=axis)
        flat = 1
        for dim in source_shape[axis:]:
            flat *= dim
        self.shapes[output] = [source_shape[0], flat]

    def convert_reshape(self, layer) -> None:
        dims = [int(dim) for dim in layer.reshape_param.shape.dim]
        source = self.source(layer.bottom[0])
        source_shape = self.shapes[source]
        output = self.tensor_name(layer, layer.top[0])
        self.node(
            "Reshape",
            [
                source,
                self.initializer(
                    f"{layer.name}:shape", np.asarray(dims, dtype=np.int64)
                ),
            ],
            [output],
        )
        resolved = []
        known_product = 1
        minus_one = None
        for index, dim in enumerate(dims):
            if dim == 0:
                resolved.append(source_shape[index])
                known_product *= source_shape[index]
            elif dim == -1:
                resolved.append(-1)
                minus_one = index
            else:
                resolved.append(dim)
                known_product *= dim
        if minus_one is not None:
            total = int(np.prod(source_shape))
            resolved[minus_one] = total // known_product
        self.shapes[output] = resolved

    def convert_softmax(self, layer) -> None:
        axis = int(layer.softmax_param.axis or 1)
        source = self.source(layer.bottom[0])
        output = self.tensor_name(layer, layer.top[0])
        self.node("Softmax", [source], [output], axis=axis)
        self.shapes[output] = list(self.shapes[source])

    def convert_priorbox(self, layer) -> None:
        params = layer.prior_box_param
        feature = self.shapes[self.source(layer.bottom[0])]
        image = self.shapes[self.source(layer.bottom[1])]
        feature_h, feature_w = feature[2], feature[3]
        image_h, image_w = image[2], image[3]

        step = float(params.step) or 0.0
        step_h = float(params.step_h) or step
        step_w = float(params.step_w) or step
        offset = float(params.offset) if params.HasField("offset") else 0.5
        min_sizes = [float(value) for value in params.min_size]
        max_sizes = [float(value) for value in params.max_size]
        aspect_ratios = [1.0]
        for ratio in params.aspect_ratio:
            if abs(ratio - 1.0) > 1e-6:
                aspect_ratios.append(float(ratio))
            if params.flip:
                aspect_ratios.append(1.0 / float(ratio))

        boxes = []
        for h in range(feature_h):
            for w in range(feature_w):
                center_x = (w + offset) * step_w
                center_y = (h + offset) * step_h
                for index, min_size in enumerate(min_sizes):
                    boxes.append(
                        (
                            (center_x - min_size / 2) / image_w,
                            (center_y - min_size / 2) / image_h,
                            (center_x + min_size / 2) / image_w,
                            (center_y + min_size / 2) / image_h,
                        )
                    )
                    if index < len(max_sizes):
                        max_size = max_sizes[index]
                        boxes.append(
                            (
                                (center_x - max_size / 2) / image_w,
                                (center_y - max_size / 2) / image_h,
                                (center_x + max_size / 2) / image_w,
                                (center_y + max_size / 2) / image_h,
                            )
                        )
                    for ratio in aspect_ratios[1:]:
                        side = min_size * np.sqrt(ratio)
                        boxes.append(
                            (
                                (center_x - side / 2) / image_w,
                                (center_y - side / 2) / image_h,
                                (center_x + side / 2) / image_w,
                                (center_y + side / 2) / image_h,
                            )
                        )
        if params.clip:
            boxes = [
                (min(max(x1, 0.0), 1.0), min(max(y1, 0.0), 1.0),
                 min(max(x2, 0.0), 1.0), min(max(y2, 0.0), 1.0))
                for x1, y1, x2, y2 in boxes
            ]
        variances = [float(value) for value in params.variance] or [0.1, 0.1, 0.2, 0.2]
        while len(variances) < 4:
            variances.append(variances[-1])
        coordinates = np.asarray(boxes, dtype=np.float32).reshape(1, -1)
        variance_plane = np.tile(np.asarray(variances, dtype=np.float32), len(boxes)).reshape(
            1, -1
        )
        tensor = np.concatenate([coordinates, variance_plane], axis=0).reshape(
            1, 2, -1
        )
        output = self.tensor_name(layer, layer.top[0])
        self.initializer(output, tensor)
        self.shapes[output] = [1, 2, len(boxes) * 4]

    def unsupported(self, layer) -> None:
        self.skipped.append(f"{layer.name} ({layer.type})")

    def convert(self) -> onnx.ModelProto:
        handlers = {
            "Input": self.convert_input,
            "Convolution": self.convert_convolution,
            "Deconvolution": self.convert_deconvolution,
            "ReLU": self.convert_relu,
            "BatchNorm": self.convert_batchnorm,
            "Scale": self.convert_scale,
            "Pooling": self.convert_pooling,
            "Concat": self.convert_concat,
            "Eltwise": self.convert_eltwise,
            "Crop": self.convert_crop,
            "Permute": self.convert_permute,
            "Flatten": self.convert_flatten,
            "Reshape": self.convert_reshape,
            "Softmax": self.convert_softmax,
            "PriorBox": self.convert_priorbox,
        }
        for layer in self.net.layer:
            handler = handlers.get(layer.type)
            if handler is None:
                self.unsupported(layer)
                continue
            handler(layer)

        if self.input_shape is None:
            raise SystemExit("no Input layer found")
        if not self.nodes:
            raise SystemExit("the graph is empty")

        # DetectionOutput has no ONNX equivalent; expose its inputs (raw loc,
        # softmaxed conf, priors) and decode/NMS in the caller instead.
        detection_layers = [layer for layer in self.net.layer if layer.type == "DetectionOutput"]
        if detection_layers:
            output_names = [
                self.resolved[name] for name in detection_layers[0].bottom
            ]
        else:
            output_names = [self.resolved[self.net.layer[-1].top[0]]]

        graph_outputs = [
            helper.make_tensor_value_info(
                name, TensorProto.FLOAT, self.shapes[name]
            )
            for name in output_names
        ]
        graph = helper.make_graph(
            self.nodes,
            "wechat_qrcode",
            [
                helper.make_tensor_value_info(
                    self.input_name, TensorProto.FLOAT, self.input_shape
                )
            ],
            graph_outputs,
            initializer=self.initializers,
        )
        model = helper.make_model(graph, producer_name="qr_race caffe_to_onnx")
        model.opset_import[0].version = 17
        onnx.checker.check_model(model)
        return model


def verify(model_path: Path, prototxt: Path, caffemodel: Path, input_shape) -> None:
    import cv2
    import onnxruntime as ort

    rng = np.random.default_rng(7)
    sample = rng.standard_normal(input_shape).astype(np.float32)

    session = ort.InferenceSession(str(model_path), providers=["CPUExecutionProvider"])
    onnx_output = session.run(None, {session.get_inputs()[0].name: sample})[0]

    net = cv2.dnn.readNetFromCaffe(str(prototxt), str(caffemodel))
    net.setInput(sample)
    reference = net.forward()
    if reference.ndim == 2:
        reference = reference.reshape(onnx_output.shape)

    diff = np.max(np.abs(onnx_output - reference))
    print(f"onnx shape {onnx_output.shape}, caffe shape {reference.shape}, max diff {diff:.6g}")
    if diff > 1e-3:
        raise SystemExit(f"numeric mismatch: {diff}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", choices=["sr", "detect"])
    parser.add_argument("prototxt", type=Path)
    parser.add_argument("caffemodel", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()

    converter = Converter(args.prototxt, args.caffemodel)
    model = converter.convert()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, args.output)
    print(f"wrote {args.output} ({args.output.stat().st_size} bytes)")
    if converter.skipped:
        print("skipped layers:", ", ".join(converter.skipped))
    if args.verify:
        verify(args.output, args.prototxt, args.caffemodel, converter.input_shape)


if __name__ == "__main__":
    main()

