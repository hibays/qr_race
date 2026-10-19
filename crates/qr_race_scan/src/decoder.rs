//! rxing-backed decoding. The engine sees only hits; the crate stays here.

use std::collections::HashSet;

use rxing::{BarcodeFormat, DecodeHints, Exceptions, RXingResult};

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

impl From<RXingResult> for ScanHit {
    fn from(result: RXingResult) -> Self {
        Self {
            text: result.getText().to_string(),
            format: format!("{:?}", result.getBarcodeFormat()),
            raw: result.getRawBytes().to_vec(),
            points: result.getPoints().iter().map(|p| [p.x, p.y]).collect(),
        }
    }
}

/// Decodes QR symbols from a whole frame with rxing's multi-symbol reader.
#[derive(Debug, Clone)]
pub struct RxingDecoder {
    qr_only: bool,
    try_harder: bool,
}

impl Default for RxingDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RxingDecoder {
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

    fn hints(&self) -> DecodeHints {
        DecodeHints {
            TryHarder: Some(self.try_harder),
            PossibleFormats: self
                .qr_only
                .then(|| HashSet::from([BarcodeFormat::QR_CODE])),
            ..DecodeHints::default()
        }
    }
}

impl Decoder for RxingDecoder {
    fn decode(&self, frame: &GrayFrame) -> Result<Vec<ScanHit>, ScanError> {
        let mut hints = self.hints();
        let results = rxing::helpers::detect_multiple_in_luma_with_hints(
            frame.data().to_vec(),
            frame.width(),
            frame.height(),
            &mut hints,
        );
        match results {
            Ok(results) => Ok(results.into_iter().map(ScanHit::from).collect()),
            // "Nothing here" is a normal frame, not a failure.
            Err(Exceptions::NotFoundException(_)) => Ok(Vec::new()),
            Err(err) => Err(ScanError::Decode(err.to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "rxing"
    }
}
