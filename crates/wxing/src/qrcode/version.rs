//! Port of `zxing/qrcode/version.{hpp,cpp}`: the version table, the error
//! correction block table, and the function pattern templates.
//!
//! The VERSIONS table is a mechanical transcription of `buildVersions()` from
//! the C++ source: 40 versions x 4 error correction levels.

use crate::error::{format, reader, ZXResult};

use super::error_correction_level::ErrorCorrectionLevel;
use super::format_information::FormatInformation;
use crate::common::bitmatrix::BitMatrix;

/// Table of 18-bit BCH codewords for the version information (versions 7 to 40).
const VERSION_DECODE_INFO: [u32; 34] = [
    0x07C94, 0x085BC, 0x09A99, 0x0A4D3, 0x0BBF6, 0x0C762, 0x0D847, 0x0E60D, 0x0F928, 0x10B78,
    0x1145D, 0x12A17, 0x13532, 0x149A6, 0x15683, 0x168C9, 0x177EC, 0x18EC4, 0x191E1, 0x1AFAB,
    0x1B08E, 0x1CC1A, 0x1D33F, 0x1ED75, 0x1F250, 0x209D5, 0x216F0, 0x228BA, 0x2379F, 0x24B0B,
    0x2542E, 0x26A64, 0x27541, 0x28C69,
];

#[derive(Clone, Copy, Debug)]
pub struct ECB {
    pub count: i32,
    pub data_codewords: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct ECBlocks {
    pub ec_codewords: i32,
    pub ec_blocks: &'static [ECB],
}

impl ECBlocks {
    pub fn total_ecb_count(&self) -> i32 {
        self.ec_blocks.iter().map(|b| b.count).sum()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Version {
    pub version_number: i32,
    pub alignment_pattern_centers: &'static [i32],
    /// Ordered L, M, Q, H (matching `ErrorCorrectionLevel::ordinal()`)
    pub ec_blocks: [ECBlocks; 4],
}

impl Version {
    pub fn version_number(&self) -> i32 {
        self.version_number
    }

    pub fn alignment_pattern_centers(&self) -> &'static [i32] {
        self.alignment_pattern_centers
    }

    pub fn ec_blocks_for_level(&self, level: ErrorCorrectionLevel) -> &ECBlocks {
        &self.ec_blocks[level.ordinal()]
    }

    /// Total codeword count: the sum over all blocks of data plus error
    /// correction codewords.
    pub fn total_codewords(&self) -> i32 {
        let b = &self.ec_blocks[0];
        b.ec_blocks
            .iter()
            .map(|e| e.count * (e.data_codewords + b.ec_codewords))
            .sum()
    }

    pub fn dimension_for_version(&self) -> ZXResult<usize> {
        if self.version_number < 1 || self.version_number > 40 {
            return Err(reader("versionNumber must be between 1 and 40"));
        }
        Ok((17 + 4 * self.version_number) as usize)
    }

    pub fn for_number(version_number: i32) -> ZXResult<&'static Version> {
        if version_number < 1 || version_number > 40 {
            return Err(reader("versionNumber must be between 1 and 40"));
        }
        Ok(&VERSIONS[version_number as usize - 1])
    }

    /// Derives the version from the matrix side length (the provisional
    /// version used before the version information has been read).
    pub fn provisional_for_dimension(dimension: usize) -> ZXResult<&'static Version> {
        if dimension % 4 != 1 {
            return Err(format("dimension % 4 != 1"));
        }
        Version::for_number(((dimension as i32) - 17) >> 2).map_err(|_| format("err format"))
    }

    /// BCH decoding of the 18-bit version information (accepted only at a
    /// Hamming distance of 3 or less).
    pub fn decode_version_information(version_bits: u32) -> Option<&'static Version> {
        let mut best_difference = u32::MAX;
        let mut best_version = 0usize;
        for (i, &target_version) in VERSION_DECODE_INFO.iter().enumerate() {
            if target_version == version_bits {
                return Version::for_number(i as i32 + 7).ok();
            }
            let bits_difference =
                FormatInformation::num_bits_differing(version_bits as i32, target_version as i32);
            if bits_difference < best_difference {
                best_version = i + 7;
                best_difference = bits_difference;
            }
        }
        if best_difference <= 3 {
            return Version::for_number(best_version as i32).ok();
        }
        None
    }

    /// Port of `buildFunctionPattern`: marks every non-data area (finder,
    /// alignment, timing, format and version information).
    pub fn build_function_pattern(&self) -> ZXResult<BitMatrix> {
        let dimension = self.dimension_for_version()?;
        let d = dimension as i32;
        let mut fp = BitMatrix::square(dimension)?;

        // The three finder patterns and their separators
        fp.set_region(0, 0, 9, 9)?;
        fp.set_region(d - 8, 0, 8, 9)?;
        fp.set_region(0, d - 8, 9, 8)?;

        // Alignment patterns
        let centers = self.alignment_pattern_centers;
        let max = centers.len();
        for x in 0..max {
            let i = centers[x] - 2;
            for y in 0..max {
                if (x == 0 && (y == 0 || y == max - 1)) || (x == max - 1 && y == 0) {
                    continue;
                }
                fp.set_region(centers[y] - 2, i, 5, 5)?;
            }
        }

        // Timing patterns
        fp.set_region(6, 9, 1, d - 17)?;
        fp.set_region(9, 6, d - 17, 1)?;

        // Version information area (version 7 and up)
        if self.version_number > 6 {
            fp.set_region(d - 11, 0, 3, 6)?;
            fp.set_region(0, d - 11, 6, 3)?;
        }
        Ok(fp)
    }

    /// Port of `buildFixedPatternValue`: the expected values of the fixed
    /// patterns. The fork uses it to compute a template matching score.
    pub fn build_fixed_pattern_value(&self) -> ZXResult<BitMatrix> {
        let dimension = self.dimension_for_version()?;
        let d = dimension as i32;
        let mut m = BitMatrix::square(dimension)?;

        // Timing pattern: set every other module
        let mut i = 0;
        while i < dimension {
            m.set(i, 6);
            i += 2;
        }
        let mut i = 0;
        while i < dimension {
            m.set(6, i);
            i += 2;
        }

        // The concentric rings of the three finder patterns
        for (ox, oy) in [(0, 0), (d - 8, 0), (0, d - 8)] {
            m.set_region(ox, oy, 8, 8)?;
            m.flip_region(ox, oy, 8, 8)?;
            let (fx, fy) = match (ox, oy) {
                (0, 0) => (0, 0),
                (x, 0) => (x + 1, 0),
                (0, y) => (0, y + 1),
                _ => unreachable!(),
            };
            m.flip_region(fx, fy, 7, 7)?;
            m.flip_region(fx + 1, fy + 1, 5, 5)?;
            m.flip_region(fx + 2, fy + 2, 3, 3)?;
        }

        // The concentric rings of the alignment patterns
        let centers = self.alignment_pattern_centers;
        let max = centers.len();
        for x in 0..max {
            let i = centers[x] - 2;
            for y in 0..max {
                if (x == 0 && (y == 0 || y == max - 1)) || (x == max - 1 && y == 0) {
                    continue;
                }
                m.set_region(centers[y] - 2, i, 5, 5)?;
                m.flip_region(centers[y] - 1, i + 1, 3, 3)?;
                m.flip_region(centers[y], i + 2, 1, 1)?;
            }
        }
        Ok(m)
    }

    /// Port of `buildFixedPatternTemplate`: the position mask of the fixed
    /// patterns.
    pub fn build_fixed_pattern_template(&self) -> ZXResult<BitMatrix> {
        let dimension = self.dimension_for_version()?;
        let d = dimension as i32;
        let mut m = BitMatrix::square(dimension)?;
        m.set_region(0, 0, 8, 8)?;
        m.set_region(d - 8, 0, 8, 8)?;
        m.set_region(0, d - 8, 8, 8)?;

        let centers = self.alignment_pattern_centers;
        let max = centers.len();
        for x in 0..max {
            let i = centers[x] - 2;
            for y in 0..max {
                if (x == 0 && (y == 0 || y == max - 1)) || (x == max - 1 && y == 0) {
                    continue;
                }
                m.set_region(centers[y] - 2, i, 5, 5)?;
            }
        }
        m.set_region(6, 8, 1, d - 16)?;
        m.set_region(8, 6, d - 16, 1)?;
        Ok(m)
    }
}

/// Parameter table for the 40 versions (transcribed from the C++
/// `buildVersions()`).
pub static VERSIONS: [Version; 40] = [
    Version {
        version_number: 1,
        alignment_pattern_centers: &[],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 7,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 19,
                }],
            },
            ECBlocks {
                ec_codewords: 10,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 16,
                }],
            },
            ECBlocks {
                ec_codewords: 13,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 13,
                }],
            },
            ECBlocks {
                ec_codewords: 17,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 9,
                }],
            },
        ],
    },
    Version {
        version_number: 2,
        alignment_pattern_centers: &[6, 18],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 10,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 34,
                }],
            },
            ECBlocks {
                ec_codewords: 16,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 28,
                }],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 22,
                }],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 16,
                }],
            },
        ],
    },
    Version {
        version_number: 3,
        alignment_pattern_centers: &[6, 22],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 15,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 55,
                }],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 44,
                }],
            },
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 17,
                }],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 13,
                }],
            },
        ],
    },
    Version {
        version_number: 4,
        alignment_pattern_centers: &[6, 26],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 20,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 80,
                }],
            },
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 32,
                }],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 24,
                }],
            },
            ECBlocks {
                ec_codewords: 16,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 9,
                }],
            },
        ],
    },
    Version {
        version_number: 5,
        alignment_pattern_centers: &[6, 30],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[ECB {
                    count: 1,
                    data_codewords: 108,
                }],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 43,
                }],
            },
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 16,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 11,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 12,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 6,
        alignment_pattern_centers: &[6, 34],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 68,
                }],
            },
            ECBlocks {
                ec_codewords: 16,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 27,
                }],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 19,
                }],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 15,
                }],
            },
        ],
    },
    Version {
        version_number: 7,
        alignment_pattern_centers: &[6, 22, 38],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 20,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 78,
                }],
            },
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 31,
                }],
            },
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 14,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 15,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 13,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 14,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 8,
        alignment_pattern_centers: &[6, 24, 42],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 97,
                }],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 38,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 39,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 18,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 19,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 14,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 15,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 9,
        alignment_pattern_centers: &[6, 26, 46],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[ECB {
                    count: 2,
                    data_codewords: 116,
                }],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 36,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 37,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 20,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 17,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 12,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 13,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 10,
        alignment_pattern_centers: &[6, 28, 50],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 18,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 68,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 69,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 43,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 44,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 19,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 20,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 11,
        alignment_pattern_centers: &[6, 30, 54],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 20,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 81,
                }],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 1,
                        data_codewords: 50,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 51,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 22,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 23,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 12,
                    },
                    ECB {
                        count: 8,
                        data_codewords: 13,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 12,
        alignment_pattern_centers: &[6, 32, 58],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 92,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 93,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 36,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 37,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 20,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 21,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 7,
                        data_codewords: 14,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 15,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 13,
        alignment_pattern_centers: &[6, 34, 62],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[ECB {
                    count: 4,
                    data_codewords: 107,
                }],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 37,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 38,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 20,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 21,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 12,
                        data_codewords: 11,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 12,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 14,
        alignment_pattern_centers: &[6, 26, 46, 66],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 115,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 116,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 40,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 41,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 20,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 17,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 12,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 13,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 15,
        alignment_pattern_centers: &[6, 26, 48, 70],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 22,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 87,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 88,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 41,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 42,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 12,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 13,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 16,
        alignment_pattern_centers: &[6, 26, 50, 74],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 98,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 99,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 7,
                        data_codewords: 45,
                    },
                    ECB {
                        count: 3,
                        data_codewords: 46,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[
                    ECB {
                        count: 15,
                        data_codewords: 19,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 20,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 13,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 17,
        alignment_pattern_centers: &[6, 30, 54, 78],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 1,
                        data_codewords: 107,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 108,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 10,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 1,
                        data_codewords: 22,
                    },
                    ECB {
                        count: 15,
                        data_codewords: 23,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 14,
                    },
                    ECB {
                        count: 17,
                        data_codewords: 15,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 18,
        alignment_pattern_centers: &[6, 30, 56, 82],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 120,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 121,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 9,
                        data_codewords: 43,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 44,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 17,
                        data_codewords: 22,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 23,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 14,
                    },
                    ECB {
                        count: 19,
                        data_codewords: 15,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 19,
        alignment_pattern_centers: &[6, 30, 58, 86],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 113,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 114,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 44,
                    },
                    ECB {
                        count: 11,
                        data_codewords: 45,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 17,
                        data_codewords: 21,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 22,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 9,
                        data_codewords: 13,
                    },
                    ECB {
                        count: 16,
                        data_codewords: 14,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 20,
        alignment_pattern_centers: &[6, 34, 62, 90],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 107,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 108,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 41,
                    },
                    ECB {
                        count: 13,
                        data_codewords: 42,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 15,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 15,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 21,
        alignment_pattern_centers: &[6, 28, 50, 72, 94],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 116,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 117,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[ECB {
                    count: 17,
                    data_codewords: 42,
                }],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 17,
                        data_codewords: 22,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 23,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 17,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 22,
        alignment_pattern_centers: &[6, 26, 50, 74, 98],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 111,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 112,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[ECB {
                    count: 17,
                    data_codewords: 46,
                }],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 7,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 16,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 24,
                ec_blocks: &[ECB {
                    count: 34,
                    data_codewords: 13,
                }],
            },
        ],
    },
    Version {
        version_number: 23,
        alignment_pattern_centers: &[6, 30, 54, 78, 102],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 121,
                    },
                    ECB {
                        count: 5,
                        data_codewords: 122,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 16,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 24,
        alignment_pattern_centers: &[6, 28, 54, 80, 106],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 117,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 118,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 45,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 46,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 16,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 30,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 17,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 25,
        alignment_pattern_centers: &[6, 32, 58, 84, 110],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 26,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 106,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 107,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 13,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 7,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 22,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 22,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 13,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 26,
        alignment_pattern_centers: &[6, 30, 58, 86, 114],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 10,
                        data_codewords: 114,
                    },
                    ECB {
                        count: 2,
                        data_codewords: 115,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 28,
                        data_codewords: 22,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 23,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 33,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 17,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 27,
        alignment_pattern_centers: &[6, 34, 62, 90, 118],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 122,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 123,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 22,
                        data_codewords: 45,
                    },
                    ECB {
                        count: 3,
                        data_codewords: 46,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 8,
                        data_codewords: 23,
                    },
                    ECB {
                        count: 26,
                        data_codewords: 24,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 12,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 28,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 28,
        alignment_pattern_centers: &[6, 26, 50, 74, 98, 122],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 117,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 118,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 3,
                        data_codewords: 45,
                    },
                    ECB {
                        count: 23,
                        data_codewords: 46,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 31,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 31,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 29,
        alignment_pattern_centers: &[6, 30, 54, 78, 102, 126],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 7,
                        data_codewords: 116,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 117,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 21,
                        data_codewords: 45,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 46,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 1,
                        data_codewords: 23,
                    },
                    ECB {
                        count: 37,
                        data_codewords: 24,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 26,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 30,
        alignment_pattern_centers: &[6, 26, 52, 78, 104, 130],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 5,
                        data_codewords: 115,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 116,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 15,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 25,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 23,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 25,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 31,
        alignment_pattern_centers: &[6, 30, 56, 82, 108, 134],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 13,
                        data_codewords: 115,
                    },
                    ECB {
                        count: 3,
                        data_codewords: 116,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 29,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 42,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 23,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 28,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 32,
        alignment_pattern_centers: &[6, 34, 60, 86, 112, 138],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[ECB {
                    count: 17,
                    data_codewords: 115,
                }],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 10,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 23,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 10,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 35,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 35,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 33,
        alignment_pattern_centers: &[6, 30, 58, 86, 114, 142],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 17,
                        data_codewords: 115,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 116,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 14,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 21,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 29,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 19,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 11,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 46,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 34,
        alignment_pattern_centers: &[6, 34, 62, 90, 118, 146],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 13,
                        data_codewords: 115,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 116,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 14,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 23,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 44,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 59,
                        data_codewords: 16,
                    },
                    ECB {
                        count: 1,
                        data_codewords: 17,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 35,
        alignment_pattern_centers: &[6, 30, 54, 78, 102, 126, 150],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 12,
                        data_codewords: 121,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 122,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 12,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 26,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 39,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 22,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 41,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 36,
        alignment_pattern_centers: &[6, 24, 50, 76, 102, 128, 154],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 121,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 122,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 6,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 34,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 46,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 2,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 64,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 37,
        alignment_pattern_centers: &[6, 28, 54, 80, 106, 132, 158],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 17,
                        data_codewords: 122,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 123,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 29,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 49,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 10,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 24,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 46,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 38,
        alignment_pattern_centers: &[6, 32, 58, 84, 110, 136, 162],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 4,
                        data_codewords: 122,
                    },
                    ECB {
                        count: 18,
                        data_codewords: 123,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 13,
                        data_codewords: 46,
                    },
                    ECB {
                        count: 32,
                        data_codewords: 47,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 48,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 14,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 42,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 32,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 39,
        alignment_pattern_centers: &[6, 26, 54, 82, 110, 138, 166],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 20,
                        data_codewords: 117,
                    },
                    ECB {
                        count: 4,
                        data_codewords: 118,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 40,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 7,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 43,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 22,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 10,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 67,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
    Version {
        version_number: 40,
        alignment_pattern_centers: &[6, 30, 58, 86, 114, 142, 170],
        ec_blocks: [
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 19,
                        data_codewords: 118,
                    },
                    ECB {
                        count: 6,
                        data_codewords: 119,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 28,
                ec_blocks: &[
                    ECB {
                        count: 18,
                        data_codewords: 47,
                    },
                    ECB {
                        count: 31,
                        data_codewords: 48,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 34,
                        data_codewords: 24,
                    },
                    ECB {
                        count: 34,
                        data_codewords: 25,
                    },
                ],
            },
            ECBlocks {
                ec_codewords: 30,
                ec_blocks: &[
                    ECB {
                        count: 20,
                        data_codewords: 15,
                    },
                    ECB {
                        count: 61,
                        data_codewords: 16,
                    },
                ],
            },
        ],
    },
];
