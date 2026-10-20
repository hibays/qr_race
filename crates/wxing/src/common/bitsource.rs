//! Port of `zxing/common/bitsource.{hpp,cpp}`: bit-level reads over a byte
//! stream, most significant bit first.

use crate::error::{illegal_argument, ZXResult};

pub struct BitSource<'a> {
    bytes: &'a [u8],
    byte_offset: usize,
    bit_offset: usize,
}

impl<'a> BitSource<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    #[inline]
    pub fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    #[inline]
    pub fn byte_offset(&self) -> usize {
        self.byte_offset
    }

    pub fn available(&self) -> i32 {
        8 * (self.bytes.len() as i32 - self.byte_offset as i32) - self.bit_offset as i32
    }

    pub fn read_bits(&mut self, num_bits: i32) -> ZXResult<i32> {
        let mut num_bits = num_bits;
        if !(0..=32).contains(&num_bits) || num_bits > self.available() {
            return Err(illegal_argument(num_bits.to_string()));
        }
        let mut result: i32 = 0;

        // Consume the remaining bits of the current byte first
        if self.bit_offset > 0 {
            let bits_left = 8 - self.bit_offset as i32;
            let to_read = num_bits.min(bits_left);
            let bits_to_not_read = bits_left - to_read;
            let mask = (0xFF >> (8 - to_read)) << bits_to_not_read;
            result = (self.bytes[self.byte_offset] as i32 & mask) >> bits_to_not_read;
            num_bits -= to_read;
            self.bit_offset += to_read as usize;
            if self.bit_offset == 8 {
                self.bit_offset = 0;
                self.byte_offset += 1;
            }
        }

        if num_bits > 0 {
            while num_bits >= 8 {
                result = (result << 8) | (self.bytes[self.byte_offset] as i32 & 0xFF);
                self.byte_offset += 1;
                num_bits -= 8;
            }
            if num_bits > 0 {
                let bits_to_not_read = 8 - num_bits;
                let mask = (0xFF >> bits_to_not_read) << bits_to_not_read;
                result = (result << num_bits)
                    | ((self.bytes[self.byte_offset] as i32 & mask) >> bits_to_not_read);
                self.bit_offset += num_bits as usize;
            }
        }
        Ok(result)
    }
}
