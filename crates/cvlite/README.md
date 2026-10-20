# cvlite

A Rust port of a few OpenCV `imgproc` functions, with no dependencies.

The selection covers what a QR pipeline needs, but nothing here is specific to
QR codes: it is usable wherever a small subset of OpenCV would otherwise pull in
the whole library.

## Usage

```sh
cargo add cvlite
```

Or from git, which follows the default branch until a `tag`, `branch` or `rev`
pins it:

```toml
[dependencies]
cvlite = { git = "https://github.com/wilinz/cvlite" }
```

```rust
use cvlite::{resize, Interpolation};
use cvlite::threshold::adaptive_threshold_gaussian;

// `gray` is 8-bit grayscale, one byte per pixel, row after row.
let half = resize(&gray, width, height, width / 2, height / 2, Interpolation::Area);
let mask = adaptive_threshold_gaussian(&gray, width, height, 255, 25, 5.0);
```

Every function takes a slice and its dimensions and returns a new buffer. There
is no image type to build first, and nothing to link.

## What is here

- `resize` — INTER_CUBIC, INTER_AREA, INTER_LINEAR, single-channel 8U
- `threshold::adaptive_threshold_gaussian` — the `ADAPTIVE_THRESH_GAUSSIAN_C`
  path, including the separable Gaussian it is built on
- `color` — BGR, RGB and RGBA to grayscale
- `blob` — `dnn::blobFromImage` for the shapes a detector uses

On aarch64 the resize and threshold paths use NEON and produce bit-identical
output to the scalar ones, which the tests check directly.

Numeric behaviour follows OpenCV: `cvRound` rounds halfway cases to even, and
`saturate_cast<uchar>` uses the same rounding before clamping. Known deviations
from upstream are documented on the individual functions; the one that shows up
in practice is the Gaussian blur, where OpenCV uses a fixed-point separable
filter for 8U images while this port accumulates in f32, occasionally differing
by one gray level.

Part of [wxscan-rs](https://github.com/wilinz/wxscan-rs). Apache-2.0.
