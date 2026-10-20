//! Port of `zxing/qrcode/decoder/mode.{hpp,cpp}`: the data segment mode
//! indicators.

use crate::error::{reader, ZXResult};
use crate::qrcode::version::Version;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Terminator,
    Numeric,
    Alphanumeric,
    StructuredAppend,
    Byte,
    Eci,
    Kanji,
    Fnc1FirstPosition,
    Fnc1SecondPosition,
    /// GB2312 Hanzi mode, specific to WeChat and mainland China
    Hanzi,
}

impl Mode {
    pub fn for_bits(bits: i32) -> ZXResult<Mode> {
        Ok(match bits {
            0x0 => Mode::Terminator,
            0x1 => Mode::Numeric,
            0x2 => Mode::Alphanumeric,
            0x3 => Mode::StructuredAppend,
            0x4 => Mode::Byte,
            0x5 => Mode::Fnc1FirstPosition,
            0x7 => Mode::Eci,
            0x8 => Mode::Kanji,
            0x9 => Mode::Fnc1SecondPosition,
            0xD => Mode::Hanzi,
            _ => return Err(reader(format!("Illegal mode bits: {bits}"))),
        })
    }

    pub fn bits(&self) -> i32 {
        match self {
            Mode::Terminator => 0x00,
            Mode::Numeric => 0x01,
            Mode::Alphanumeric => 0x02,
            Mode::StructuredAppend => 0x03,
            Mode::Byte => 0x04,
            Mode::Fnc1FirstPosition => 0x05,
            Mode::Eci => 0x07,
            Mode::Kanji => 0x08,
            Mode::Fnc1SecondPosition => 0x09,
            Mode::Hanzi => 0x0D,
        }
    }

    /// Bit width of the character count field, which varies by version range.
    pub fn character_count_bits(&self, version: &Version) -> i32 {
        let (a, b, c) = match self {
            Mode::Numeric => (10, 12, 14),
            Mode::Alphanumeric => (9, 11, 13),
            Mode::Byte => (8, 16, 16),
            Mode::Kanji | Mode::Hanzi => (8, 10, 12),
            _ => (0, 0, 0),
        };
        let number = version.version_number();
        if number <= 9 {
            a
        } else if number <= 26 {
            b
        } else {
            c
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Mode::Terminator => "TERMINATOR",
            Mode::Numeric => "NUMERIC",
            Mode::Alphanumeric => "ALPHANUMERIC",
            Mode::StructuredAppend => "STRUCTURED_APPEND",
            Mode::Byte => "BYTE",
            Mode::Eci => "ECI",
            Mode::Kanji => "KANJI",
            Mode::Fnc1FirstPosition => "FNC1_FIRST_POSITION",
            Mode::Fnc1SecondPosition => "FNC1_SECOND_POSITION",
            Mode::Hanzi => "HANZI",
        }
    }
}
