//! Port of `zxing/qrcode/format_information.{hpp,cpp}`: BCH error correction
//! decoding of the 15-bit format information.

use super::error_correction_level::ErrorCorrectionLevel;

const FORMAT_INFO_MASK_QR: i32 = 0x5412;

/// (masked 15-bit codeword, corresponding 5-bit format value)
const FORMAT_INFO_DECODE_LOOKUP: [(i32, i32); 32] = [
    (0x5412, 0x00),
    (0x5125, 0x01),
    (0x5E7C, 0x02),
    (0x5B4B, 0x03),
    (0x45F9, 0x04),
    (0x40CE, 0x05),
    (0x4F97, 0x06),
    (0x4AA0, 0x07),
    (0x77C4, 0x08),
    (0x72F3, 0x09),
    (0x7DAA, 0x0A),
    (0x789D, 0x0B),
    (0x662F, 0x0C),
    (0x6318, 0x0D),
    (0x6C41, 0x0E),
    (0x6976, 0x0F),
    (0x1689, 0x10),
    (0x13BE, 0x11),
    (0x1CE7, 0x12),
    (0x19D0, 0x13),
    (0x0762, 0x14),
    (0x0255, 0x15),
    (0x0D0C, 0x16),
    (0x083B, 0x17),
    (0x355F, 0x18),
    (0x3068, 0x19),
    (0x3F31, 0x1A),
    (0x3A06, 0x1B),
    (0x24B4, 0x1C),
    (0x2183, 0x1D),
    (0x2EDA, 0x1E),
    (0x2BED, 0x1F),
];

#[derive(Clone, Copy, Debug)]
pub struct FormatInformation {
    error_correction_level: ErrorCorrectionLevel,
    data_mask: u8,
    /// How well the two copies of the format information agree; the fork uses
    /// it as a confidence value.
    possiable_fix: f32,
}

impl FormatInformation {
    fn new(format_info: i32, possiable_fix: f32) -> Option<Self> {
        let ecl = ErrorCorrectionLevel::for_bits((format_info >> 3) & 0x03).ok()?;
        Some(Self {
            error_correction_level: ecl,
            data_mask: (format_info & 0x07) as u8,
            possiable_fix,
        })
    }

    pub fn error_correction_level(&self) -> ErrorCorrectionLevel {
        self.error_correction_level
    }

    pub fn data_mask(&self) -> u8 {
        self.data_mask
    }

    pub fn possiable_fix(&self) -> f32 {
        self.possiable_fix
    }

    pub fn num_bits_differing(a: i32, b: i32) -> u32 {
        ((a ^ b) as u32).count_ones()
    }

    pub fn decode(masked_format_info1: i32, masked_format_info2: i32) -> Option<FormatInformation> {
        Self::do_decode(masked_format_info1, masked_format_info2).or_else(|| {
            Self::do_decode(
                masked_format_info1 ^ FORMAT_INFO_MASK_QR,
                masked_format_info2 ^ FORMAT_INFO_MASK_QR,
            )
        })
    }

    fn do_decode(masked_format_info1: i32, masked_format_info2: i32) -> Option<FormatInformation> {
        let distance = Self::num_bits_differing(masked_format_info1, masked_format_info2) as i32;
        let possiable_fix = ((16.0 - (distance.min(16)) as f64) / 16.0) as f32;

        let mut best_difference = u32::MAX;
        let mut best_format_info = 0;
        for &(target_info, format_value) in FORMAT_INFO_DECODE_LOOKUP.iter() {
            if target_info == masked_format_info1 || target_info == masked_format_info2 {
                return FormatInformation::new(format_value, possiable_fix);
            }
            let mut bits_difference = Self::num_bits_differing(masked_format_info1, target_info);
            if bits_difference < best_difference {
                best_format_info = format_value;
                best_difference = bits_difference;
            }
            if masked_format_info1 != masked_format_info2 {
                bits_difference = Self::num_bits_differing(masked_format_info2, target_info);
                if bits_difference < best_difference {
                    best_format_info = format_value;
                    best_difference = bits_difference;
                }
            }
        }
        // Accepted only at a Hamming distance of 3 or less
        if best_difference <= 3 {
            return FormatInformation::new(best_format_info, possiable_fix);
        }
        None
    }
}

impl PartialEq for FormatInformation {
    fn eq(&self, other: &Self) -> bool {
        self.error_correction_level == other.error_correction_level
            && self.data_mask == other.data_mask
    }
}
