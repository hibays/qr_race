//! Port of `zxing/qrcode/decoder/bitmatrixparser.{hpp,cpp}`: reads the format
//! information and version information out of the module matrix, then removes
//! the data mask and extracts the codewords in zigzag order.

use crate::common::bitmatrix::BitMatrix;
use crate::error::{reader, ZXResult};
use crate::qrcode::format_information::FormatInformation;
use crate::qrcode::version::Version;

use super::datamask::unmask_bit_matrix;

pub struct BitMatrixParser<'a> {
    bits: &'a mut BitMatrix,
    mirror: bool,
    parsed_version: Option<&'static Version>,
    parsed_format_info: Option<FormatInformation>,
}

impl<'a> BitMatrixParser<'a> {
    pub fn new(bits: &'a mut BitMatrix) -> ZXResult<Self> {
        let dimension = bits.height();
        if dimension < 21 || (dimension & 0x03) != 1 {
            return Err(reader("Dimension must be 1 mod 4 and >= 21"));
        }
        Ok(Self {
            bits,
            mirror: false,
            parsed_version: None,
            parsed_format_info: None,
        })
    }

    /// Exposes the current matrix so Decoder can compute the fixed pattern
    /// score.
    pub fn bits_ref(&self) -> &BitMatrix {
        self.bits
    }

    #[inline]
    fn copy_bit(&self, x: usize, y: usize, version_bits: i32) -> i32 {
        let bit = if self.mirror {
            self.bits.get(y, x)
        } else {
            self.bits.get(x, y)
        } != 0;
        if bit {
            (version_bits << 1) | 0x1
        } else {
            version_bits << 1
        }
    }

    pub fn read_format_information(&mut self) -> ZXResult<FormatInformation> {
        if let Some(fi) = self.parsed_format_info {
            return Ok(fi);
        }
        // First copy: the L shape in the top-left corner
        let mut bits1 = 0i32;
        for i in 0..6 {
            bits1 = self.copy_bit(i, 8, bits1);
        }
        bits1 = self.copy_bit(7, 8, bits1);
        bits1 = self.copy_bit(8, 8, bits1);
        bits1 = self.copy_bit(8, 7, bits1);
        for j in (0..6).rev() {
            bits1 = self.copy_bit(8, j, bits1);
        }

        // Second copy: top-right plus bottom-left
        let dimension = self.bits.height();
        let mut bits2 = 0i32;
        let j_min = dimension - 7;
        for j in (j_min..dimension).rev() {
            bits2 = self.copy_bit(8, j, bits2);
        }
        for i in dimension - 8..dimension {
            bits2 = self.copy_bit(i, 8, bits2);
        }

        match FormatInformation::decode(bits1, bits2) {
            Some(fi) => {
                self.parsed_format_info = Some(fi);
                Ok(fi)
            }
            None => Err(reader("Could not decode format information")),
        }
    }

    pub fn read_version(&mut self) -> ZXResult<&'static Version> {
        if let Some(v) = self.parsed_version {
            return Ok(v);
        }
        let dimension = self.bits.height();
        let provisional_version = ((dimension as i32) - 17) >> 2;
        if provisional_version <= 6 {
            return Version::for_number(provisional_version);
        }

        // 6 x 3 version information in the top-right corner
        let mut version_bits = 0i32;
        for y in (0..6).rev() {
            let x_min = dimension - 11;
            for x in (x_min..dimension - 8).rev() {
                version_bits = self.copy_bit(x, y, version_bits);
            }
        }
        if let Some(v) = Version::decode_version_information(version_bits as u32) {
            if v.dimension_for_version()? == dimension {
                self.parsed_version = Some(v);
                return Ok(v);
            }
        }

        // 3 x 6 version information in the bottom-left corner
        let mut version_bits = 0i32;
        for x in (0..6).rev() {
            let y_min = dimension - 11;
            for y in (y_min..dimension - 8).rev() {
                version_bits = self.copy_bit(x, y, version_bits);
            }
        }
        match Version::decode_version_information(version_bits as u32) {
            Some(v) if v.dimension_for_version()? == dimension => {
                self.parsed_version = Some(v);
                Ok(v)
            }
            _ => Err(reader("Could not decode version")),
        }
    }

    /// Port of `readCodewords`: removes the data mask, then reads out every
    /// codeword in the standard two-column zigzag order.
    pub fn read_codewords(&mut self) -> ZXResult<Vec<u8>> {
        let format_info = self.read_format_information()?;
        let version = self.read_version()?;
        let dimension = self.bits.height();
        unmask_bit_matrix(format_info.data_mask(), self.bits, dimension)?;

        let function_pattern = version.build_function_pattern()?;
        let total = version.total_codewords() as usize;
        let mut result = vec![0u8; total];
        let mut reading_up = true;
        let mut result_offset = 0usize;
        let mut current_byte = 0i32;
        let mut bits_read = 0i32;

        let mut x = dimension as i32 - 1;
        while x > 0 {
            if x == 6 {
                x -= 1; // Skip the column holding the vertical timing pattern
            }
            for counter in 0..dimension {
                let y = if reading_up {
                    dimension - 1 - counter
                } else {
                    counter
                };
                for col in 0..2 {
                    let xx = (x - col) as usize;
                    if function_pattern.get(xx, y) == 0 {
                        bits_read += 1;
                        current_byte <<= 1;
                        if self.bits.get(xx, y) != 0 {
                            current_byte |= 1;
                        }
                        if bits_read == 8 {
                            if result_offset >= total {
                                return Err(reader("Did not read all codewords"));
                            }
                            result[result_offset] = current_byte as u8;
                            result_offset += 1;
                            bits_read = 0;
                            current_byte = 0;
                        }
                    }
                }
            }
            reading_up = !reading_up;
            x -= 2;
        }

        if result_offset != total {
            return Err(reader("Did not read all codewords"));
        }
        Ok(result)
    }

    /// Port of `remask`: reapplies the mask that was removed earlier (the mask
    /// is self-inverse).
    pub fn remask(&mut self) {
        if let Some(fi) = self.parsed_format_info {
            let dimension = self.bits.height();
            let _ = unmask_bit_matrix(fi.data_mask(), self.bits, dimension);
        }
    }

    /// Port of `setMirror`: affects only the sampling direction of subsequent
    /// format and version information reads.
    pub fn set_mirror(&mut self, mirror: bool) {
        self.parsed_version = None;
        self.parsed_format_info = None;
        self.mirror = mirror;
    }

    /// Port of `mirror()`: transposes the whole matrix along the main diagonal.
    pub fn mirror_matrix(&mut self) {
        for x in 0..self.bits.width() {
            for y in x + 1..self.bits.height() {
                if self.bits.get(x, y) != self.bits.get(y, x) {
                    self.bits.flip(y, x);
                    self.bits.flip(x, y);
                }
            }
        }
    }
}
