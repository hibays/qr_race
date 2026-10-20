# wxing

A Rust port of the ZXing fork used by WeChat's QR code implementation.

The name follows the precedent of [`rxing`](https://crates.io/crates/rxing), the
Rust port of ZXing: this is a port of the **W**eChat fork. That fork differs
from upstream ZXing in several places, among them the N:1:3:1:1 finder pattern
ratio, a rotation over four binarizers, and finder pattern validation by
connected-component area.

Nothing here involves the CNN stages, so this crate decodes QR codes on its own:

- four binarizers (hybrid, fast window, simple adaptive, adaptive threshold)
- finder and alignment pattern detection
- the decoder: format and version information, data masks, Reed-Solomon error
  correction, bit stream parsing

The payload is returned as raw bytes together with the charset the decoder
reports, without conversion, matching the fork's behaviour. HANZI segments are
reported as GB2312 and everything else as UTF-8.

The only dependency is [`cvlite`](https://github.com/wilinz/cvlite), for
adaptive Gaussian thresholding and rounding. The CNN detector and super
resolution stages live in
[`wxscan`](https://github.com/wilinz/wxscan-rs/tree/main/crates/wxscan).

## Usage

```sh
cargo add wxing
```

Or from git. This crate names `cvlite` as an ordinary dependency, so a build
from git has to say where that one is as well:

```toml
[dependencies]
wxing = { git = "https://github.com/wilinz/wxing" }

[patch.crates-io]
cvlite = { git = "https://github.com/wilinz/cvlite" }
```

```rust
use std::sync::Arc;
use wxing::binarizer::{Binarizer, BinarizerKind};
use wxing::common::unicomblock::UnicomBlock;
use wxing::luminance_source::ImgSource;
use wxing::qrcode::qrcode_reader::QRCodeReader;

// `gray` is 8-bit grayscale, one byte per pixel, row after row.
let pixels = Arc::new(gray);
let mut block = UnicomBlock::new(height, width);

for kind in BinarizerKind::ROTATE_ORDER {
    let source = ImgSource::new(Arc::clone(&pixels), width, height);
    let mut binarizer = Binarizer::new(source, kind);
    let mut reader = QRCodeReader::new();
    block.reset();

    let results = reader.decode(&mut binarizer, &mut block, false);
    if let Some(first) = results.first() {
        println!("{}", String::from_utf8_lossy(&first.text));
        break;
    }
}
```

Trying the binarizers in `ROTATE_ORDER` rather than picking one is the fork's
answer to uneven lighting: a symbol that one of them turns into unreadable
black and white, another often gets right. Stopping at the first hit is what
the WeChat implementation does too.

## Tests

```sh
cargo test
```

The suite is in two halves. The unit tests are ports of ZXing's own test cases
— `BitArrayTestCase`, `BitSourceTestCase`, `ReedSolomonTestCase`,
`VersionTestCase`, `FormatInformationTestCase`, `DataMaskTestCase`,
`PerspectiveTransformTestCase` and the rest — because the tables and transforms
they cover are where a transcription slip produces a decoder that reads most
symbols and quietly fails on a few.

`tests/black_box.rs` then decodes real symbols end to end: every content mode,
every error correction level, a version 11 symbol with alignment patterns,
plus inverted and rotated inputs. Its corpus is generated rather than
collected, so it is reproducible:

```sh
python3 -m pip install segno pillow
python3 tools/gen_fixtures.py
```

Part of [wxscan-rs](https://github.com/wilinz/wxscan-rs). Apache-2.0.
