//! QR generation aligned with rcrm's bridge:
//!
//! - nominal **2048×2048** (2K) image, error correction **M**, UTF-8 payloads;
//! - **lossless WebP (VP8L)**, quality 90 / method 4, L8 luma;
//! - 0 = black module, 255 = white/quiet zone; the renderer pads four modules.
//!
//! The encoder runs in Rust and the client only displays bytes, so no Dart QR
//! package is needed.

use qrcode::{EcLevel, QrCode};
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

    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)
        .map_err(|error| QrError::Encode(error.to_string()))?;
    let modules = code.width() as u32;
    let total = modules + 8;
    let scale = (QR_SIZE / total).max(1);
    let side = total * scale;
    let offset = (QR_SIZE - side) / 2;
    let mut luma = vec![0xffu8; (QR_SIZE as usize) * (QR_SIZE as usize)];
    let colors = code.to_colors();
    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let px = offset + (x + 4) * scale + dx;
                    let py = offset + (y + 4) * scale + dy;
                    luma[(py * QR_SIZE + px) as usize] = 0;
                }
            }
        }
    }

    let config = LosslessConfig::new()
        .with_quality(WEBP_QUALITY)
        .with_method(WEBP_METHOD);
    EncodeRequest::lossless(&config, &luma, PixelLayout::L8, QR_SIZE, QR_SIZE)
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
    use qr_race_scan::{GrayFrame, ScanEngine, ZxingDecoder};

    fn decode(bytes: &[u8]) -> String {
        let (luma, width, height) = webp_luma(bytes).expect("webp decodes");
        let frame = GrayFrame::new(width, height, luma).expect("frame builds");
        let engine = ScanEngine::new(Box::new(ZxingDecoder::new()));
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
