//! Port of `zxing/common/bitmatrix.{hpp,cpp}`.
//!
//! Uses the same one-byte-per-bit layout. The row run-length cache (row
//! records) is the central acceleration structure of the WeChat fork's
//! FinderPatternFinder; it is filled lazily on demand, with the same semantics
//! kept here. The original also has a column-direction `cols_*` cache that no
//! code in the module calls (and whose implementation samples out of bounds);
//! it is not ported.

use super::bitarray::BitArray;
use crate::error::{illegal_argument, ZXResult};

/// Placeholder matrix, used only by `std::mem::take`. It is 1x1 because the
/// constructor rejects a zero side length.
impl Default for BitMatrix {
    fn default() -> Self {
        Self::new(1, 1).expect("a 1x1 matrix cannot fail")
    }
}

/// The original's `COUNTER_TYPE`, which is `short`.
pub type Counter = i16;

pub struct BitMatrix {
    width: usize,
    height: usize,
    bits: Vec<u8>,

    is_init_row_counters: bool,
    row_counters: Vec<Counter>,
    row_counters_offset: Vec<Counter>,
    row_point_offset: Vec<Counter>,
    row_counter_offset_end: Vec<Counter>,
    row_counters_recorded: Vec<bool>,
}

impl BitMatrix {
    pub fn new(width: usize, height: usize) -> ZXResult<Self> {
        if width < 1 || height < 1 {
            return Err(illegal_argument("Both dimensions must be greater than 0"));
        }
        Ok(Self {
            width,
            height,
            bits: vec![0u8; width * height],
            is_init_row_counters: false,
            row_counters: Vec::new(),
            row_counters_offset: Vec::new(),
            row_point_offset: Vec::new(),
            row_counter_offset_end: Vec::new(),
            row_counters_recorded: Vec::new(),
        })
    }

    pub fn square(dimension: usize) -> ZXResult<Self> {
        Self::new(dimension, dimension)
    }

    pub fn from_bits(width: usize, height: usize, bits_ptr: &[u8]) -> ZXResult<Self> {
        let mut m = Self::new(width, height)?;
        let n = (width * height).min(bits_ptr.len());
        m.bits[..n].copy_from_slice(&bits_ptr[..n]);
        Ok(m)
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.bits[self.width * y + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize) {
        let w = self.width;
        self.bits[w * y + x] = 1;
    }

    #[inline]
    pub fn set_value(&mut self, x: usize, y: usize, value: u8) {
        let w = self.width;
        self.bits[w * y + x] = value;
    }

    #[inline]
    pub fn row_bits(&self, y: usize) -> &[u8] {
        let off = y * self.width;
        &self.bits[off..off + self.width]
    }

    #[inline]
    pub fn row_bits_mut(&mut self, y: usize) -> &mut [u8] {
        let off = y * self.width;
        let w = self.width;
        &mut self.bits[off..off + w]
    }

    pub fn set_row_bool(&mut self, y: usize, row: &[u8]) {
        let w = self.width;
        let off = y * w;
        self.bits[off..off + w].copy_from_slice(&row[..w]);
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bits
    }

    /// Port of `copyOf`: copy the whole matrix, dimensions included.
    pub fn copy_of(&mut self, other: &BitMatrix) -> ZXResult<()> {
        *self = BitMatrix::from_bits(other.width, other.height, &other.bits)?;
        Ok(())
    }

    pub fn xxor(&mut self, other: &BitMatrix) {
        if self.width != other.width || self.height != other.height {
            return;
        }
        for i in 0..self.bits.len() {
            self.bits[i] ^= other.bits[i];
        }
    }

    pub fn flip(&mut self, x: usize, y: usize) {
        let idx = self.width * y + x;
        self.bits[idx] = u8::from(self.bits[idx] == 0);
    }

    pub fn flip_all(&mut self) {
        for b in self.bits.iter_mut() {
            *b = u8::from(*b == 0);
        }
    }

    pub fn clear(&mut self) {
        self.bits.iter_mut().for_each(|b| *b = 0);
    }

    fn check_region(
        &self,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    ) -> ZXResult<(usize, usize)> {
        if top < 0 || left < 0 {
            return Err(illegal_argument("Left and top must be nonnegative"));
        }
        if height < 1 || width < 1 {
            return Err(illegal_argument("Height and width must be at least 1"));
        }
        let right = left + width;
        let bottom = top + height;
        if bottom as usize > self.height || right as usize > self.width {
            return Err(illegal_argument("The region must fit inside the matrix"));
        }
        Ok((right as usize, bottom as usize))
    }

    pub fn set_region(&mut self, left: i32, top: i32, width: i32, height: i32) -> ZXResult<()> {
        let (right, bottom) = self.check_region(left, top, width, height)?;
        for y in top as usize..bottom {
            for x in left as usize..right {
                let w = self.width;
                self.bits[w * y + x] = 1;
            }
        }
        Ok(())
    }

    pub fn flip_region(&mut self, left: i32, top: i32, width: i32, height: i32) -> ZXResult<()> {
        let (right, bottom) = self.check_region(left, top, width, height)?;
        for y in top as usize..bottom {
            for x in left as usize..right {
                let w = self.width;
                self.bits[w * y + x] ^= 1;
            }
        }
        Ok(())
    }

    pub fn get_row(&self, y: usize, row: Option<BitArray>) -> BitArray {
        let mut row = match row {
            Some(r) if r.size() >= self.width => r,
            _ => BitArray::new(self.width),
        };
        row.set_one_row(self.row_bits(y));
        row
    }

    /// Returns the (x, y) of the first set bit.
    pub fn top_left_on_bit(&self) -> Option<(usize, usize)> {
        self.bits
            .iter()
            .position(|&b| b != 0)
            .map(|off| (off % self.width, off / self.width))
    }

    pub fn bottom_right_on_bit(&self) -> Option<(usize, usize)> {
        self.bits
            .iter()
            .rposition(|&b| b != 0)
            .map(|off| (off % self.width, off / self.width))
    }

    // ── Row run-length cache ───────────────────────────────────────────────
    // initRowCounters must be called before getRowRecords and friends, as in
    // the original.

    pub fn init_row_counters(&mut self) {
        if self.is_init_row_counters {
            return;
        }
        let n = self.width * self.height;
        self.row_counters = vec![0; n];
        self.row_counters_offset = vec![0; n];
        self.row_point_offset = vec![0; n];
        self.row_counter_offset_end = vec![0; self.height];
        self.row_counters_recorded = vec![false; self.height];
        self.is_init_row_counters = true;
    }

    /// Swaps in another bit image of the same size, reusing all buffers of
    /// this matrix.
    ///
    /// Decoding one frame builds more than a dozen BitMatrix instances (four
    /// binarizers x normal/inverted x several scales). Each one otherwise
    /// allocates and zeroes the bit image plus three W x H row run-length
    /// buffers, which adds up to tens of megabytes of allocation per frame.
    /// Reusing them leaves only one copy of the bit image and a clear of a
    /// height-sized flag array.
    pub fn reuse_from(&mut self, width: usize, height: usize, bits: &[u8]) -> ZXResult<()> {
        if bits.len() != width * height {
            return Err(illegal_argument(
                "bits length does not match width x height",
            ));
        }
        if self.width != width || self.height != height {
            *self = Self::new(width, height)?;
        }
        self.bits.copy_from_slice(bits);
        self.init_row_counters();
        // Row records are lazy: clearing the "recorded" flags is enough, the
        // buffers themselves need not be cleared.
        self.row_counters_recorded.fill(false);
        Ok(())
    }

    /// Port of `setRowRecords`: compress one row into alternating black/white
    /// run lengths, the start offset of each run, and the run index each pixel
    /// belongs to.
    fn set_row_records(&mut self, y: usize) {
        let width = self.width;
        let base = y * width;
        let end = width;

        let mut is_white = self.bits[base] == 0;
        let mut counter_position: usize = 0;
        let mut i: usize = 0;
        // Only run 0 is accumulated rather than assigned on first use, so only
        // it needs an initial value of 0. With these two lines the three W x H
        // buffers need no full clear and can be reused across matrices.
        self.row_counters[base] = 0;
        self.row_counters_offset[base] = 0;
        while i < end {
            let bit = self.bits[base + i] != 0;
            if bit ^ is_white {
                // Same color, extend the current run.
                self.row_counters[base + counter_position] += 1;
            } else {
                counter_position += 1;
                if counter_position == end {
                    break;
                }
                self.row_counters[base + counter_position] = 1;
                is_white = !is_white;
                self.row_counters_offset[base + counter_position] = i as Counter;
            }
            self.row_point_offset[base + i] = counter_position as Counter;
            i += 1;
        }

        self.row_counter_offset_end[y] = if counter_position < end {
            (counter_position + 1) as Counter
        } else {
            end as Counter
        };
        self.row_counters_recorded[y] = true;
    }

    #[inline]
    pub fn ensure_row_records(&mut self, y: usize) {
        if !self.row_counters_recorded[y] {
            self.set_row_records(y);
        }
    }

    pub fn row_records(&mut self, y: usize) -> &[Counter] {
        self.ensure_row_records(y);
        let off = y * self.width;
        &self.row_counters[off..off + self.width]
    }

    pub fn row_records_offset(&mut self, y: usize) -> &[Counter] {
        self.ensure_row_records(y);
        let off = y * self.width;
        &self.row_counters_offset[off..off + self.width]
    }

    pub fn row_point_in_records(&mut self, y: usize) -> &[Counter] {
        // The original tests row_point_offset[y] instead of the recorded flag;
        // the effect is the same, since it is 0 when not recorded.
        self.ensure_row_records(y);
        let off = y * self.width;
        &self.row_point_offset[off..off + self.width]
    }

    pub fn row_counter_offset_end(&mut self, y: usize) -> Counter {
        self.ensure_row_records(y);
        self.row_counter_offset_end[y]
    }

    /// The following three read-only accessors require the caller to have
    /// called [`Self::ensure_row_records`] first.
    ///
    /// This lets a caller hold several record slices for one row at the same
    /// time: the `&mut self` variants reborrow the whole matrix on every call,
    /// so taking two slices at once conflicts and would require copying them
    /// out first.
    pub fn row_records_at(&self, y: usize) -> &[Counter] {
        let off = y * self.width;
        &self.row_counters[off..off + self.width]
    }

    pub fn row_records_offset_at(&self, y: usize) -> &[Counter] {
        let off = y * self.width;
        &self.row_counters_offset[off..off + self.width]
    }

    pub fn row_counter_offset_end_at(&self, y: usize) -> Counter {
        self.row_counter_offset_end[y]
    }

    pub fn row_first_is_white(&self, y: usize) -> bool {
        self.get(0, y) == 0
    }

    pub fn row_last_is_white(&self, y: usize) -> bool {
        self.get(self.width - 1, y) == 0
    }
}
