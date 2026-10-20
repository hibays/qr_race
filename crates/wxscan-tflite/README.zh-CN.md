# wxscan-tflite

[English](README.md) · **简体中文**

libtensorflowlite_c（LiteRT 的 C API）的一层最小 FFI 绑定。它是
[`wxscan`](https://github.com/wilinz/wxscan-rs/tree/main/crates/wxscan) 的默认推理后端，也可以单独拿来当一个小巧的 tflite 绑定用。

<img src="https://raw.githubusercontent.com/wilinz/wxscan/main/docs/demo.webp" width="300"
     alt="一帧里两个二维码都被框出，点开其中一个显示解出的中文文本，按 UTF-8 读取。">

*录像里每一个被找到的码，都是某个 CNN 通过这个 crate 跑出来的——它就夹在检测器和
libtensorflowlite_c 中间。*

```rust
let net = wxscan_tflite::TfliteNet::from_bytes(model_bytes)?;
// 维度用模型自己的布局，这两个模型是 NHWC
let outputs = net.run(&input, &[1, height, width, 1])?;
```

解释器按输入形状缓存，形状变了就重建。XNNPACK 构建的是静态图，delegate 生效之后再
resize 会失败。

本 crate 说的是 tflite 的规矩：张量布局是 NHWC，形状就是模型声明的那样。这里不做任何
转换，是有意的。适配到某个推理抽象上，是定义那个抽象的那一层该干的事。`wxscan` 在它的
`net::Net` trait 后面做了这件事，所以算法的哪一部分都不依赖本 crate。

## TFLite 库

不内置任何二进制。把 `TFLITE_LIB_DIR` 指向一个装着该库的目录，或者把这些符号留给最终
的链接步骤去解析，Apple 平台通常这么做。名字随分发渠道变：C API 的构建在每个平台上都叫
`libtensorflowlite_c`，Google 面向 Android 的 LiteRT AAR 把同一套 API 叫 `libLiteRt`。
Android 默认链前者，`litert` feature 换成后者。

[wxscan-rs](https://github.com/wilinz/wxscan-rs) 的一部分。Apache-2.0。
