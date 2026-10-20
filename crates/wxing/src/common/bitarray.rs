//! Port of `zxing/common/bitarray.{hpp,cpp}`.
//!
//! Uses the same one-byte-per-bit layout as the original, whose
//! `getRowBoolPtr()` reinterprets that memory directly as `bool*`. The
//! next-set/next-unset index tables are carried over as well; they accelerate
//! the OneD scan lines.

use crate::error::{illegal_argument, ZXResult};

pub struct BitArray {
    size: usize,
    bits: Vec<u8>,
    next_sets: Vec<i32>,
    next_unsets: Vec<i32>,
}

impl BitArray {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            bits: vec![0u8; size],
            next_sets: vec![0i32; size],
            next_unsets: vec![0i32; size],
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn get(&self, i: usize) -> bool {
        self.bits[i] != 0
    }

    #[inline]
    pub fn set(&mut self, i: usize) {
        self.bits[i] = 1;
    }

    #[inline]
    pub fn set_unchar(&mut self, i: usize, v: u8) {
        self.bits[i] = v;
    }

    /// Port of `setOneRow`: copy in a whole row.
    pub fn set_one_row(&mut self, row: &[u8]) {
        let n = row.len().min(self.bits.len());
        self.bits[..n].copy_from_slice(&row[..n]);
    }

    #[inline]
    pub fn row(&self) -> &[u8] {
        &self.bits
    }

    #[inline]
    pub fn row_mut(&mut self) -> &mut [u8] {
        &mut self.bits
    }

    pub fn clear(&mut self) {
        self.bits.iter_mut().for_each(|b| *b = 0);
    }

    pub fn reverse(&mut self) {
        self.bits[..self.size].reverse();
    }

    /// Port of `initAllNextSets`: scan backwards once, filling in the nearest
    /// set/unset index at or after each position.
    pub fn init_all_next_sets(&mut self) {
        if self.size == 0 {
            return;
        }
        let last = self.size - 1;
        if self.bits[last] != 0 {
            self.next_sets[last] = last as i32;
            self.next_unsets[last] = self.size as i32;
        } else {
            self.next_unsets[last] = last as i32;
            self.next_sets[last] = self.size as i32;
        }
        for i in (0..last).rev() {
            if self.bits[i] != 0 {
                self.next_sets[i] = i as i32;
                self.next_unsets[i] = self.next_unsets[i + 1];
            } else {
                self.next_unsets[i] = i as i32;
                self.next_sets[i] = self.next_sets[i + 1];
            }
        }
    }

    /// Port of `initAllNextSetsFromCounters`: the fast path used when run
    /// counts are already available.
    pub fn init_all_next_sets_from_counters(&mut self, counters: &[i32]) {
        if self.size == 0 {
            return;
        }
        // Note: the original reads rowBits[0] as is, without negating it, so a
        // first run of true is treated as white.
        let mut is_white = self.bits[0] != 0;
        let mut count = 0i32;
        let mut prev_count = 0i32;
        for &curr_count in counters {
            count += curr_count;
            for i in 0..curr_count {
                let offset = (prev_count + i) as usize;
                if offset >= self.size {
                    break;
                }
                if is_white {
                    self.next_sets[offset] = prev_count + i;
                    self.next_unsets[offset] = count;
                } else {
                    self.next_sets[offset] = count;
                    self.next_unsets[offset] = prev_count + i;
                }
            }
            is_white = !is_white;
            prev_count += curr_count;
        }
    }

    pub fn get_next_set(&self, from: usize) -> usize {
        if from >= self.size {
            return self.size;
        }
        self.next_sets[from] as usize
    }

    pub fn get_next_unset(&self, from: usize) -> usize {
        if from >= self.size {
            return self.size;
        }
        self.next_unsets[from] as usize
    }

    pub fn is_range(&self, start: usize, end: usize, value: bool) -> ZXResult<bool> {
        if end < start || end >= self.bits.len() {
            return Err(illegal_argument("isRange"));
        }
        if end == start {
            return Ok(true); // Empty range.
        }
        let start_bool = self.bits[start] != 0;
        let end2 = if start_bool {
            self.get_next_unset(start)
        } else {
            self.get_next_set(start)
        };
        if start_bool == value {
            Ok(end2 >= end)
        } else {
            Ok(false)
        }
    }

    pub fn append_bit(&mut self, value: bool) {
        self.bits.push(if value { 1 } else { 0 });
        self.next_sets.push(0);
        self.next_unsets.push(0);
        self.size += 1;
    }

    pub fn append_bits(&mut self, value: i32, num_bits: i32) -> ZXResult<()> {
        if !(0..=32).contains(&num_bits) {
            return Err(illegal_argument("Number of bits must be between 0 and 32"));
        }
        for shift in (0..num_bits).rev() {
            self.append_bit(((value >> shift) & 0x01) == 1);
        }
        Ok(())
    }

    pub fn append_bit_array(&mut self, other: &BitArray) {
        for i in 0..other.size() {
            self.append_bit(other.get(i));
        }
    }

    pub fn bit_xor(&mut self, other: &BitArray) -> ZXResult<()> {
        if self.size != other.size {
            return Err(illegal_argument("Sizes don't match"));
        }
        for i in 0..self.bits.len() {
            self.bits[i] = if self.bits[i] == other.bits[i] { 0 } else { 1 };
        }
        Ok(())
    }
}
