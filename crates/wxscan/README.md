# wxscan

**English** · [简体中文](README.zh-CN.md)

A Rust port of the `wechat_qrcode` algorithm from OpenCV contrib: CNN-based
detection, super resolution, and decoding. No OpenCV dependency.

<img src="https://raw.githubusercontent.com/wilinz/wxscan/main/docs/demo.webp" width="300"
     alt="Two QR codes in one camera frame, each marked; tapping one opens its decoded
     text, a Chinese payload read as UTF-8.">

*This crate is what decodes in the recording. The phone around it is the
Flutter binding in [wxscan](https://github.com/wilinz/wxscan).*

```rust
use wxscan::WeChatQRCode;

let detect = wxscan::tflite::TfliteNet::from_bytes(detect_bytes)?;
let sr = wxscan::tflite::TfliteNet::from_bytes(sr_bytes)?;
let scanner = WeChatQRCode::new(Some(detect), Some(sr));

for result in scanner.detect_and_decode_gray(&gray, width, height) {
    // The payload is raw bytes; `charset` says how to interpret it.
    println!("{} ({})", result.text_lossy(), result.charset);
}
```

`detect_and_decode_gray_with_candidates` additionally returns what the detector
found. Candidates without results mean a symbol was located but could not be
decoded, usually because it is too small or too blurred, which a caller can act
on by zooming in rather than reporting a failure.

## What it is made of

The pieces that are not specific to this algorithm are separate crates:
[`cvlite`](https://github.com/wilinz/cvlite) for the OpenCV functions used, and
[`wxing`](https://github.com/wilinz/wxing) for the ZXing fork the decoder comes
from. This crate holds the parts that are: the SSD detector, the super
resolution stage, and the orchestration around them.

## Models

The weights are not part of this crate, nor of any other. Take the prebuilt ones
from [wxscan-weights](https://github.com/wilinz/wxscan-weights) — all four sit
in `models/`, named by format, so take the pair matching the backend in use —
or pass your own buffers:

```rust
let detect = std::fs::read("detect.tflite")?;
let sr = std::fs::read("sr.tflite")?;
```

Without models the pipeline degrades to a plain decoder rather than failing. It
still reads ordinary codes; what it loses is the detection rate on small or
distant ones.

## Inference backends

CNN inference sits behind the `net::Net` trait, and no part of the algorithm
knows which library runs it. Two backends ship, both in the `backend` module:

| Feature | Engine | Weights | C dependency |
|---|---|---|---|
| `tflite` (default) | [`wxscan-tflite`](https://github.com/wilinz/wxscan-rs/tree/main/crates/wxscan-tflite) | `detect.tflite`, `sr.tflite` | libtensorflowlite_c |
| `tract` | [tract](https://crates.io/crates/tract-onnx) | `detect.onnx`, `sr.onnx` | none |

The tflite adapter is also where layout conversion lives: tflite is NHWC, the
trait contract is NCHW. tract needs none, ONNX being NCHW like the Caffe models
both formats are converted from, and it builds anywhere cargo does:

```sh
cargo add wxscan --no-default-features --features tract
```

Or, from git:

```toml
wxscan = { git = "https://github.com/wilinz/wxscan-rs", default-features = false, features = ["tract"] }
```

One thing is needed on `wasm32-unknown-unknown` whatever the features: the host
must supply a `wxscan_host_now_us() -> f64` import in the module `wxscan`.
`std::time::Instant::now()` panics on that target, so the stage timers read the
host's clock instead — a browser answers `performance.now() * 1000`. A host with
no clock to lend can return a constant, and every stage then reports zero.

Turning both off leaves a core with no inference and no C dependency, which
builds with plain `cargo build`:

```sh
cargo add wxscan --no-default-features
```

Or, from git:

```toml
wxscan = { git = "https://github.com/wilinz/wxscan-rs", default-features = false }
```

A backend is one method. The trait lives here, so a crate of your own can
implement it for its own type:

```rust
use wxscan::net::{Net, NetOutput};

impl Net for MyNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        // input is NCHW; return NCHW
    }
}
```

With the default feature on, the library itself is still not vendored: point
`TFLITE_LIB_DIR` at a directory containing it, or let the final link step
resolve the symbols, which is what Apple platforms normally do.

## Features

| Feature | Default | Effect |
|---|---|---|
| `tflite` | yes | The libtensorflowlite_c implementation of `net::Net`. |
| `profiling` | no | Instrumentation on hot paths, used by `examples/profile`. |

Part of [wxscan-rs](https://github.com/wilinz/wxscan-rs). Apache-2.0.
