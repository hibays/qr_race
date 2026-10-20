//! zxing-cpp-backed decoding. The engine sees only hits; the crate stays here.

use zxingcpp::{Barcode, BarcodeFormat, ImageFormat, ImageView};

use crate::engine::Decoder;
use crate::error::ScanError;
use crate::frame::GrayFrame;

/// One decoded symbol, in the shape the rest of the product needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanHit {
    /// Best-effort charset guess for display. Not authoritative: a QR byte
    /// segment without an ECI can arrive as mojibake. Relay `raw` instead.
    pub text: String,
    pub format: String,
    /// The segment bytes exactly as encoded — what a relay uploads and a
    /// member's device re-renders, so a charset guess can never corrupt it.
    pub raw: Vec<u8>,
    pub points: Vec<[f32; 2]>,
}

impl From<Barcode> for ScanHit {
    fn from(result: Barcode) -> Self {
        let position = result.position();
        let points = [
            position.top_left,
            position.top_right,
            position.bottom_right,
            position.bottom_left,
        ]
        .into_iter()
        .map(|p| [p.x as f32, p.y as f32])
        .collect();
        Self {
            text: result.text(),
            format: format!("{:?}", result.format()),
            raw: result.bytes(),
            points,
        }
    }
}

/// Decodes QR symbols from a whole frame with zxing-cpp.
#[derive(Debug, Clone)]
pub struct ZxingDecoder {
    qr_only: bool,
    try_harder: bool,
}

impl Default for ZxingDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ZxingDecoder {
    pub fn new() -> Self {
        Self {
            qr_only: true,
            try_harder: false,
        }
    }

    /// Restricts the reader to QR (the only format the product relays).
    pub fn qr_only(mut self, qr_only: bool) -> Self {
        self.qr_only = qr_only;
        self
    }

    /// `try_harder` costs a lot of time per frame; off is the 30fps default.
    pub fn try_harder(mut self, try_harder: bool) -> Self {
        self.try_harder = try_harder;
        self
    }

    fn reader(&self) -> zxingcpp::BarcodeReader {
        let reader = zxingcpp::read()
            .try_harder(self.try_harder)
            .try_rotate(false)
            .try_downscale(false)
            .max_number_of_symbols(8);
        if self.qr_only {
            reader.formats([BarcodeFormat::QRCode])
        } else {
            reader
        }
    }

    /// Decodes a tightly packed RGB frame, preserving chroma for the detector.
    pub fn decode_rgb(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<ScanHit>, ScanError> {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(3);
        if data.len() < expected {
            return Err(ScanError::Decode("short RGB frame".into()));
        }
        let view = ImageView::from_slice(&data[..expected], width, height, ImageFormat::RGB)
            .map_err(|error| ScanError::Decode(error.to_string()))?;
        match self.reader().from(&view) {
            Ok(results) => Ok(results.into_iter().map(ScanHit::from).collect()),
            Err(error) => Err(ScanError::Decode(error.to_string())),
        }
    }
}

impl Decoder for ZxingDecoder {
    fn decode(&self, frame: &GrayFrame) -> Result<Vec<ScanHit>, ScanError> {
        let view = ImageView::from_slice(
            frame.data(),
            frame.width(),
            frame.height(),
            ImageFormat::Lum,
        )
        .map_err(|error| ScanError::Decode(error.to_string()))?;
        match self.reader().from(&view) {
            Ok(results) => Ok(results.into_iter().map(ScanHit::from).collect()),
            Err(error) => Err(ScanError::Decode(error.to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "zxing-cpp"
    }
}
