# wxscan

[English](README.md) · **简体中文**

OpenCV contrib 里 `wechat_qrcode` 算法的 Rust 移植：CNN 检测、超分辨率、解码。不依赖
OpenCV。

<img src="https://raw.githubusercontent.com/wilinz/wxscan/main/docs/demo.webp" width="300"
     alt="一帧里两个二维码都被框出，点开其中一个显示解出的中文文本，按 UTF-8 读取。">

*录像里做解码的就是这个 crate。围着它的那部手机是
[wxscan](https://github.com/wilinz/wxscan) 里的 Flutter 绑定。*

```rust
use wxscan::WeChatQRCode;

let detect = wxscan::tflite::TfliteNet::from_bytes(detect_bytes)?;
let sr = wxscan::tflite::TfliteNet::from_bytes(sr_bytes)?;
let scanner = WeChatQRCode::new(Some(detect), Some(sr));

for result in scanner.detect_and_decode_gray(&gray, width, height) {
    // 负载是原始字节；怎么解释它看 `charset`。
    println!("{} ({})", result.text_lossy(), result.charset);
}
```

`detect_and_decode_gray_with_candidates` 会顺带把检测器找到的东西也返回。有候选却没有
结果，说明码定位到了但解不出来，通常是太小或者太糊。调用方可以据此提示用户凑近点，
而不是报一次失败。

## 它由什么组成

不是本算法专有的那些部分，都是独立的 crate：
[`cvlite`](https://github.com/wilinz/cvlite) 提供用到的 OpenCV 函数，
[`wxing`](https://github.com/wilinz/wxing) 提供解码器出自的那个 ZXing 分支。本 crate
装的是专有的部分：SSD 检测器、超分辨率阶段，以及围绕它们的编排。

## 模型

权重不属于本 crate，也不属于其它任何 crate。去
[wxscan-weights](https://github.com/wilinz/wxscan-weights) 取预构建的那些——四个文件都在
`models/` 下，按格式命名，挑跟你的后端对得上的那一对；也可以传你自己的缓冲：

```rust
let detect = std::fs::read("detect.tflite")?;
let sr = std::fs::read("sr.tflite")?;
```

没有模型时，这条流水线**降级**成一个普通解码器，不是失效。普通的码它照样读得出来，
丢掉的是小码和远处的码的检出率。

## 推理后端

CNN 推理藏在 `net::Net` trait 后面，算法的哪一部分都不知道是哪个库在跑它。仓库自带
两个后端，都在 `backend` 模块里：

| Feature | 引擎 | 权重 | C 依赖 |
|---|---|---|---|
| `tflite`（默认） | [`wxscan-tflite`](https://github.com/wilinz/wxscan-rs/tree/main/crates/wxscan-tflite) | `detect.tflite`、`sr.tflite` | libtensorflowlite_c |
| `tract` | [tract](https://crates.io/crates/tract-onnx) | `detect.onnx`、`sr.onnx` | 无 |

布局转换也放在 tflite 适配器里：tflite 用 NHWC，trait 约定的是 NCHW。tract 那边不用转，
ONNX 本来就是 NCHW，跟两种格式共同的来源 Caffe 模型一致；而且 cargo 能到的地方它都能编：

```sh
cargo add wxscan --no-default-features --features tract
```

从 git 引的话：

```toml
wxscan = { git = "https://github.com/wilinz/wxscan-rs", default-features = false, features = ["tract"] }
```

不管开哪些 feature，在 `wasm32-unknown-unknown` 上都有一件事必须做：宿主得在模块
`wxscan` 下提供一个 `wxscan_host_now_us() -> f64` 导入。`std::time::Instant::now()`
在那个目标上会 panic，所以各阶段的计时器改成读宿主的时钟，浏览器回一个
`performance.now() * 1000` 就行。没有时钟可借的宿主返回常数即可，那样每个阶段报出来的
耗时都是零。

两个都关掉，剩下的核心没有推理，也没有 C 依赖，普通的 `cargo build` 就能编：

```sh
cargo add wxscan --no-default-features
```

从 git 引的话：

```toml
wxscan = { git = "https://github.com/wilinz/wxscan-rs", default-features = false }
```

一个后端就是一个方法。trait 就在这里，所以你自己的 crate 可以给自己的类型实现：

```rust
use wxscan::net::{Net, NetOutput};

impl Net for MyNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        // 输入是 NCHW；返回也用 NCHW
    }
}
```

就算开着默认 feature，那个库本身仍然不是内置的。把 `TFLITE_LIB_DIR` 指向一个装着它的
目录，或者把这些符号留给最终的链接步骤去解析，Apple 平台通常这么做。

## Features

| Feature | 默认 | 作用 |
|---|---|---|
| `tflite` | 是 | `net::Net` 的 libtensorflowlite_c 实现。 |
| `profiling` | 否 | 热点路径上的埋点，给 `examples/profile` 用。 |

[wxscan-rs](https://github.com/wilinz/wxscan-rs) 的一部分。Apache-2.0。
