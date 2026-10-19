//! QR generation with rxing, aligned with rcrm's bridge:
//!
//! - nominal **2048×2048** (2K) image, error correction **M**, UTF-8 payloads;
//! - **lossless WebP (VP8L)**, quality 90 / method 4, L8 luma;
//! - 0 = black module, 255 = white/quiet zone; rxing pads the quiet zone.
//!
//! The encoder runs in Rust and the client only displays bytes, so no Dart QR
//! package is needed.

use rxing::{BarcodeFormat, EncodeHints, MultiFormatWriter, Writer};
use thiserror::Error;
use zenwebp::{EncodeRequest, LosslessConfig, PixelLayout};

/// rcrm's nominal generated resolution: 2048×2048 (2K). The client scales it
/// down; two-tone WebP keeps it a few KB.
pub const QR_SIZE: u32 = 2048;

const WEBP_QUALITY: f32 = 90.0;
const WEBP_METHOD: u8 = 4;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QrError {
    #[error("payload is empty")]
    EmptyPayload,

    #[error("could not encode the payload: {0}")]
    Encode(String),

    #[error("could not encode the WebP: {0}")]
    Webp(String),

    #[error("could not decode the WebP: {0}")]
    Decode(String),
}

/// Encodes `text` as a 2048×2048 lossless WebP QR code.
pub fn qr_webp(text: &str) -> Result<Vec<u8>, QrError> {
    if text.trim().is_empty() {
        return Err(QrError::EmptyPayload);
    }

    let hints = EncodeHints {
        ErrorCorrection: Some("M".to_string()),
        ..EncodeHints::default()
    };
    let matrix = MultiFormatWriter
        .encode_with_hints(
            text,
            &BarcodeFormat::QR_CODE,
            QR_SIZE as i32,
            QR_SIZE as i32,
            &hints,
        )
        .map_err(|error| QrError::Encode(error.to_string()))?;

    let width = matrix.getWidth();
    let height = matrix.getHeight();
    let mut luma = vec![0xffu8; (width as usize) * (height as usize)];
    // rxing's check_in_bounds has inverted semantics: true means OUT of bounds.
    for y in 0..height {
        for x in 0..width {
            if !matrix.check_in_bounds(x, y) && matrix.get(x, y) {
                luma[(y * width + x) as usize] = 0;
            }
        }
    }

    let config = LosslessConfig::new()
        .with_quality(WEBP_QUALITY)
        .with_method(WEBP_METHOD);
    EncodeRequest::lossless(&config, &luma, PixelLayout::L8, width, height)
        .encode()
        .map_err(|error| QrError::Webp(error.to_string()))
}

/// Decodes WebP bytes back to L8 luma. Used by tests and native tooling; the
/// client never needs it.
pub fn webp_luma(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), QrError> {
    let (rgba, width, height) =
        zenwebp::oneshot::decode_rgba(bytes).map_err(|error| QrError::Decode(error.to_string()))?;
    let mut luma = Vec::with_capacity((width as usize) * (height as usize));
    for pixel in rgba.as_chunks::<4>().0 {
        luma.push(
            ((u16::from(pixel[0]) * 77 + u16::from(pixel[1]) * 150 + u16::from(pixel[2]) * 29) >> 8)
                as u8,
        );
    }
    Ok((luma, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_scan::{GrayFrame, RxingDecoder, ScanEngine};

    fn decode(bytes: &[u8]) -> String {
        let (luma, width, height) = webp_luma(bytes).expect("webp decodes");
        let frame = GrayFrame::new(width, height, luma).expect("frame builds");
        let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
        let outcome = engine.scan(&frame).expect("scan runs");
        assert_eq!(outcome.hits.len(), 1, "exactly one symbol");
        outcome.hits[0].text.clone()
    }

    #[test]
    fn a_generated_webp_is_2k_and_scans_back() {
        let payload = "https://example.test/relay/東京";
        let bytes = qr_webp(payload).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
        assert!(
            bytes.windows(4).any(|window| window == b"VP8L"),
            "must be lossless VP8L"
        );
        let (_, width, height) = webp_luma(&bytes).unwrap();
        assert_eq!((width, height), (QR_SIZE, QR_SIZE));
        assert_eq!(decode(&bytes), payload);
    }

    #[test]
    fn the_image_has_a_quiet_zone() {
        let bytes = qr_webp("margin-check").unwrap();
        let (luma, width, height) = webp_luma(&bytes).unwrap();
        for (x, y) in [
            (0, 0),
            (width - 1, 0),
            (0, height - 1),
            (width - 1, height - 1),
        ] {
            assert_eq!(
                luma[(y * width + x) as usize],
                0xff,
                "corner ({x},{y}) is white"
            );
        }
        let dark = luma.iter().filter(|pixel| **pixel == 0).count();
        assert!(dark > 1000, "modules are drawn: {dark} dark pixels");
    }

    #[test]
    fn empty_inputs_fail() {
        assert_eq!(qr_webp("   "), Err(QrError::EmptyPayload));
    }
}
