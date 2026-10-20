//! Port of `zxing/qrcode/decoder/decoded_bit_stream_parser.{hpp,cpp}`: decodes
//! the error-corrected byte stream segment by segment into text.
//!
//! As in the original, no character set conversion is performed. In the fork
//! `NO_ICONV_INSIDE` always holds at compile time, so `append()` concatenates
//! the bytes into the result unchanged; the encoding is reported to the caller
//! only through the `charset` field (GB2312 or UTF-8).

use crate::common::bitsource::BitSource;
use crate::error::{format, reader, ZXResult};
use crate::qrcode::error_correction_level::ErrorCorrectionLevel;
use crate::qrcode::version::Version;
use crate::result::DecoderResult;

use super::mode::Mode;

const ALPHANUMERIC_CHARS: [u8; 45] = *b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

const GB2312_SUBSET: i32 = 1;

/// Hanzi segment: 13 bits per character, expanded to a GB2312 double byte.
fn decode_hanzi_segment(
    bits: &mut BitSource,
    result: &mut Vec<u8>,
    mut count: i32,
) -> ZXResult<()> {
    if count * 13 > bits.available() {
        return Err(format("decodeKanjiSegment"));
    }
    while count > 0 {
        let two_bytes = bits.read_bits(13)?;
        let mut assembled = ((two_bytes / 0x060) << 8) | (two_bytes % 0x060);
        if assembled < 0x003BF {
            assembled += 0x0A1A1;
        } else {
            assembled += 0x0A6A1;
        }
        result.push(((assembled >> 8) & 0xFF) as u8);
        result.push((assembled & 0xFF) as u8);
        count -= 1;
    }
    Ok(())
}

/// Kana/Kanji segment (Shift_JIS): 13 bits per character, expanded to two bytes.
fn decode_kanji_segment(
    bits: &mut BitSource,
    result: &mut Vec<u8>,
    mut count: i32,
) -> ZXResult<()> {
    while count > 0 {
        let two_bytes = bits.read_bits(13)?;
        let mut assembled = ((two_bytes / 0x0C0) << 8) | (two_bytes % 0x0C0);
        if assembled < 0x01F00 {
            assembled += 0x08140;
        } else {
            assembled += 0x0C140;
        }
        result.push(((assembled >> 8) & 0xFF) as u8);
        result.push((assembled & 0xFF) as u8);
        count -= 1;
    }
    Ok(())
}

fn decode_byte_segment(
    bits: &mut BitSource,
    result: &mut Vec<u8>,
    mut count: i32,
    byte_segments: &mut Vec<Vec<u8>>,
) -> ZXResult<()> {
    let available = bits.available();
    if count * 8 > available {
        count = (available + 7) / 8;
    }
    if count <= 0 {
        return Ok(());
    }
    let mut bytes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let available = bits.available();
        let read_bits = if available < 8 { available } else { 8 };
        bytes.push(bits.read_bits(read_bits)? as u8);
    }
    result.extend_from_slice(&bytes);
    byte_segments.push(bytes);
    Ok(())
}

fn decode_numeric_segment(
    bits: &mut BitSource,
    result: &mut Vec<u8>,
    mut count: i32,
) -> ZXResult<()> {
    while count >= 3 {
        if bits.available() < 10 {
            return Err(reader("format exception"));
        }
        let three = bits.read_bits(10)?;
        if three >= 1000 {
            return Err(reader(format!("Illegal value for 3-digit unit: {three}")));
        }
        result.push(ALPHANUMERIC_CHARS[(three / 100) as usize]);
        result.push(ALPHANUMERIC_CHARS[((three / 10) % 10) as usize]);
        result.push(ALPHANUMERIC_CHARS[(three % 10) as usize]);
        count -= 3;
    }
    if count == 2 {
        if bits.available() < 7 {
            return Err(reader("format exception"));
        }
        let two = bits.read_bits(7)?;
        if two >= 100 {
            return Err(reader(format!("Illegal value for 2-digit unit: {two}")));
        }
        result.push(ALPHANUMERIC_CHARS[(two / 10) as usize]);
        result.push(ALPHANUMERIC_CHARS[(two % 10) as usize]);
    } else if count == 1 {
        if bits.available() < 4 {
            return Err(reader("format exception"));
        }
        let d = bits.read_bits(4)?;
        if d >= 10 {
            return Err(reader(format!("Illegal value for digit unit: {d}")));
        }
        result.push(ALPHANUMERIC_CHARS[d as usize]);
    }
    Ok(())
}

fn to_alphanumeric_char(value: i32) -> ZXResult<u8> {
    if value < 0 || value as usize >= ALPHANUMERIC_CHARS.len() {
        return Err(format("toAlphaNumericChar"));
    }
    Ok(ALPHANUMERIC_CHARS[value as usize])
}

fn decode_alphanumeric_segment(
    bits: &mut BitSource,
    result: &mut Vec<u8>,
    mut count: i32,
    fc1_in_effect: bool,
) -> ZXResult<()> {
    let mut buf: Vec<u8> = Vec::new();
    while count > 1 {
        if bits.available() < 11 {
            return Err(format("decodeAlphanumericSegment"));
        }
        let next_two = bits.read_bits(11)?;
        buf.push(to_alphanumeric_char(next_two / 45)?);
        buf.push(to_alphanumeric_char(next_two % 45)?);
        count -= 2;
    }
    if count == 1 {
        if bits.available() < 6 {
            return Err(format("decodeAlphanumericSegment"));
        }
        let v = bits.read_bits(6)?;
        buf.push(to_alphanumeric_char(v)?);
    }

    if fc1_in_effect {
        // "%%" becomes "%", a single "%" becomes GS (0x1D)
        let mut out = Vec::with_capacity(buf.len());
        let mut i = 0usize;
        while i < buf.len() {
            if buf[i] != b'%' {
                out.push(buf[i]);
            } else if i + 1 < buf.len() && buf[i + 1] == b'%' {
                out.push(buf[i]);
                i += 1;
            } else {
                out.push(0x1D);
            }
            i += 1;
        }
        buf = out;
    }
    result.extend_from_slice(&buf);
    Ok(())
}

fn parse_eci_value(bits: &mut BitSource) -> ZXResult<i32> {
    let first_byte = bits.read_bits(8)?;
    if (first_byte & 0x80) == 0 {
        return Ok(first_byte & 0x7F);
    }
    if (first_byte & 0xC0) == 0x80 {
        let second_byte = bits.read_bits(8)?;
        return Ok(((first_byte & 0x3F) << 8) | second_byte);
    }
    if (first_byte & 0xE0) == 0xC0 {
        let second_third = bits.read_bits(16)?;
        return Ok(((first_byte & 0x1F) << 16) | second_third);
    }
    Err(format("parseECIValue"))
}

pub fn decode(
    bytes: &[u8],
    version: &Version,
    ec_level: ErrorCorrectionLevel,
    version_number: i32,
) -> ZXResult<DecoderResult> {
    let mut bits = BitSource::new(bytes);
    let mut result: Vec<u8> = Vec::with_capacity(50);
    let mut byte_segments: Vec<Vec<u8>> = Vec::new();
    let mut mode_name = String::new();
    let mut fc1_in_effect = false;
    let mut output_charset = "UTF-8".to_string();

    loop {
        let mode = if bits.available() < 4 {
            Mode::Terminator
        } else {
            Mode::for_bits(bits.read_bits(4)?)?
        };
        if mode == Mode::Terminator {
            break;
        }

        match mode {
            Mode::Fnc1FirstPosition | Mode::Fnc1SecondPosition => {
                fc1_in_effect = true;
            }
            Mode::StructuredAppend => {
                if bits.available() < 16 {
                    return Err(format("decode"));
                }
                bits.read_bits(16)?;
            }
            Mode::Eci => {
                // The fork has no built-in ECI character set table, so the
                // value is only checked for existence once parsed
                let value = parse_eci_value(&mut bits)?;
                if !is_known_eci_value(value) {
                    return Err(format("decode"));
                }
            }
            Mode::Hanzi => {
                let subset = bits.read_bits(4)?;
                let count_hanzi = bits.read_bits(mode.character_count_bits(version))?;
                if subset == GB2312_SUBSET {
                    decode_hanzi_segment(&mut bits, &mut result, count_hanzi)?;
                    output_charset = "GB2312".to_string();
                    mode_name = mode.name().to_string();
                }
            }
            Mode::Numeric => {
                let count = bits.read_bits(mode.character_count_bits(version))?;
                decode_numeric_segment(&mut bits, &mut result, count)
                    .map_err(|_| format("decode"))?;
                mode_name = mode.name().to_string();
            }
            Mode::Alphanumeric => {
                let count = bits.read_bits(mode.character_count_bits(version))?;
                decode_alphanumeric_segment(&mut bits, &mut result, count, fc1_in_effect)?;
                mode_name = mode.name().to_string();
            }
            Mode::Byte => {
                let count = bits.read_bits(mode.character_count_bits(version))?;
                decode_byte_segment(&mut bits, &mut result, count, &mut byte_segments)
                    .map_err(|_| format("decode"))?;
                mode_name = mode.name().to_string();
            }
            Mode::Kanji => {
                let count = bits.read_bits(mode.character_count_bits(version))?;
                decode_kanji_segment(&mut bits, &mut result, count)?;
                mode_name = mode.name().to_string();
            }
            Mode::Terminator => unreachable!(),
        }
    }

    Ok(DecoderResult {
        raw_bytes: bytes.to_vec(),
        text: result,
        byte_segments,
        ec_level: ec_level.name().to_string(),
        charset: output_charset,
        qrcode_version: version_number,
        charset_mode: mode_name,
        other: None,
    })
}

/// Port of `CharacterSetECI::getCharacterSetECIByValueFind(value) != 0`: only
/// checks whether the ECI value denotes a known character set (the table is in
/// `zxing/common/characterseteci.cpp`).
fn is_known_eci_value(value: i32) -> bool {
    // The table really does omit 14 and 19, and any value >= 900 is invalid
    matches!(value, 0..=13 | 15..=18 | 20..=30 | 170)
}
