//! Port of `zxing/qrcode/error_correction_level.{hpp,cpp}`.

use crate::error::{reader, ZXResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCorrectionLevel {
    L,
    M,
    Q,
    H,
}

impl ErrorCorrectionLevel {
    /// The original's `ordinal_`: L=0, M=1, Q=2, H=3 (Version indexes its
    /// ECBlocks by this value).
    pub fn ordinal(&self) -> usize {
        match self {
            ErrorCorrectionLevel::L => 0,
            ErrorCorrectionLevel::M => 1,
            ErrorCorrectionLevel::Q => 2,
            ErrorCorrectionLevel::H => 3,
        }
    }

    /// The 2-bit encoding used in the format information.
    pub fn bits(&self) -> i32 {
        match self {
            ErrorCorrectionLevel::L => 0x01,
            ErrorCorrectionLevel::M => 0x00,
            ErrorCorrectionLevel::Q => 0x03,
            ErrorCorrectionLevel::H => 0x02,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ErrorCorrectionLevel::L => "L",
            ErrorCorrectionLevel::M => "M",
            ErrorCorrectionLevel::Q => "Q",
            ErrorCorrectionLevel::H => "H",
        }
    }

    /// Port of `FOR_BITS[] = {M, L, H, Q}`.
    pub fn for_bits(bits: i32) -> ZXResult<ErrorCorrectionLevel> {
        match bits {
            0 => Ok(ErrorCorrectionLevel::M),
            1 => Ok(ErrorCorrectionLevel::L),
            2 => Ok(ErrorCorrectionLevel::H),
            3 => Ok(ErrorCorrectionLevel::Q),
            _ => Err(reader("Ellegal error correction level bits")),
        }
    }
}
