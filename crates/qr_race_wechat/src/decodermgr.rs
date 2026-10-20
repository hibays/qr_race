//! zxing-cpp decoder adapter for the WeChat candidate/scaling pipeline.
//!
//! The upstream WeChat fork rotates through four binarizers itself. zxing-cpp
//! already performs binarizer selection, rotation, inversion and downscaling
//! internally, so this stage is a single read call over the scaled crop.

use zxingcpp::{BarcodeFormat, ImageFormat, ImageView};

pub struct DecoderMgr;

impl Default for DecoderMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoderMgr {
    pub fn new() -> Self {
        Self
    }

    /// Decodes QR symbols from one scaled crop.
    ///
    /// Returns raw payload bytes and four corners in
    /// [top-left, top-right, bottom-right, bottom-left] order.
    pub fn decode_image(
        &mut self,
        src: &[u8],
        width: usize,
        height: usize,
        _use_nn_detector: bool,
    ) -> Option<Vec<(Vec<u8>, [(f32, f32); 4])>> {
        if width <= 20 || height <= 20 {
            return None;
        }
        let view = ImageView::from_slice(src, width, height, ImageFormat::Lum).ok()?;
        let results = zxingcpp::read()
            .formats([BarcodeFormat::QRCode])
            .try_rotate(true)
            .try_invert(true)
            .try_downscale(true)
            .max_number_of_symbols(8)
            .from(&view)
            .ok()?;
        if results.is_empty() {
            return None;
        }
        let decoded = results
            .into_iter()
            .map(|barcode| {
                let position = barcode.position();
                let quad = [
                    (position.top_left.x as f32, position.top_left.y as f32),
                    (position.top_right.x as f32, position.top_right.y as f32),
                    (
                        position.bottom_right.x as f32,
                        position.bottom_right.y as f32,
                    ),
                    (position.bottom_left.x as f32, position.bottom_left.y as f32),
                ];
                (barcode.bytes(), quad)
            })
            .collect();
        Some(decoded)
    }
}
