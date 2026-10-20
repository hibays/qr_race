//! Port of `zxing/binarizer.{hpp,cpp}` plus the five binarizers under
//! `zxing/common/binarizer/*`, and the WeChat fork's `binarizermgr.{hpp,cpp}`.
//!
//! The original is an inheritance hierarchy: a `Binarizer` base class with four
//! subclasses, each lazily computing its own `matrix0_`. This port flattens that
//! into a single [`Binarizer`] holding a kind plus a lazy cache, with the
//! algorithms as free functions. Behavior corresponds one to one.

use crate::error::{not_found, ZXResult};
use cvlite::threshold::adaptive_threshold_gaussian;

use super::common::bitmatrix::BitMatrix;
use super::common::bytematrix::ByteMatrix;
use super::luminance_source::ImgSource;

/// Port of `BinarizerMgr::BINARIZER`; the declaration order is the rotation
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinarizerKind {
    Hybrid = 0,
    FastWindow = 1,
    SimpleAdaptive = 2,
    AdaptiveThreshold = 3,
}

impl BinarizerKind {
    pub const ROTATE_ORDER: [BinarizerKind; 4] = [
        BinarizerKind::Hybrid,
        BinarizerKind::FastWindow,
        BinarizerKind::SimpleAdaptive,
        BinarizerKind::AdaptiveThreshold,
    ];
}

pub struct Binarizer {
    source: ImgSource,
    kind: BinarizerKind,
    width: usize,
    height: usize,
    /// Corresponds to `matrix0_`.
    matrix: Option<BitMatrix>,
    /// Corresponds to `matrixInverted_`.
    matrix_inverted: Option<BitMatrix>,
}

impl Binarizer {
    pub fn new(source: ImgSource, kind: BinarizerKind) -> Self {
        let width = source.width();
        let height = source.height();
        Self {
            source,
            kind,
            width,
            height,
            matrix: None,
            matrix_inverted: None,
        }
    }

    pub fn source(&self) -> &ImgSource {
        &self.source
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn kind(&self) -> BinarizerKind {
        self.kind
    }

    fn ensure_matrix(&mut self) -> ZXResult<()> {
        if self.matrix.is_some() {
            return Ok(());
        }
        let m = match self.kind {
            BinarizerKind::Hybrid => binarize_hybrid(&self.source)?,
            BinarizerKind::FastWindow => binarize_fast_window(&self.source)?,
            BinarizerKind::SimpleAdaptive => binarize_simple_adaptive(&self.source)?,
            BinarizerKind::AdaptiveThreshold => binarize_adaptive_threshold_mean(&self.source)?,
        };
        self.matrix = Some(m);
        Ok(())
    }

    pub fn black_matrix(&mut self) -> ZXResult<&mut BitMatrix> {
        self.ensure_matrix()?;
        Ok(self.matrix.as_mut().unwrap())
    }

    /// Port of `getInvertedMatrix`: a lazily built black/white inverted copy.
    pub fn inverted_matrix(&mut self) -> ZXResult<&mut BitMatrix> {
        self.ensure_matrix()?;
        if self.matrix_inverted.is_none() {
            let mut inv = BitMatrix::new(self.width, self.height)?;
            inv.copy_of(self.matrix.as_ref().unwrap())?;
            inv.flip_all();
            self.matrix_inverted = Some(inv);
        }
        Ok(self.matrix_inverted.as_mut().unwrap())
    }
}

// ── GlobalHistogramBinarizer ───────────────────────────────────────────────

const LUMINANCE_BITS: usize = 5;
const LUMINANCE_SHIFT: usize = 8 - LUMINANCE_BITS;
const LUMINANCE_BUCKETS: usize = 1 << LUMINANCE_BITS;

/// Port of `GlobalHistogramBinarizer::estimateBlackPoint`: find the deepest
/// valley between the two histogram peaks.
pub fn estimate_black_point(buckets: &[i32]) -> ZXResult<i32> {
    let num_buckets = buckets.len();
    let mut max_bucket_count = 0;
    let mut first_peak = 0usize;
    let mut first_peak_size = 0;
    for x in 0..num_buckets {
        if buckets[x] > first_peak_size {
            first_peak = x;
            first_peak_size = buckets[x];
        }
        if buckets[x] > max_bucket_count {
            max_bucket_count = buckets[x];
        }
    }

    let mut second_peak = 0usize;
    let mut second_peak_score = 0i64;
    for x in 0..num_buckets {
        let distance_to_biggest = x as i64 - first_peak as i64;
        let score = buckets[x] as i64 * distance_to_biggest * distance_to_biggest;
        if score > second_peak_score {
            second_peak = x;
            second_peak_score = score;
        }
    }

    let (first_peak, second_peak) = if first_peak > second_peak {
        (second_peak, first_peak)
    } else {
        (first_peak, second_peak)
    };

    if second_peak - first_peak <= num_buckets >> 4 {
        return Err(not_found("NotFound GlobalHistogramBinarizer"));
    }

    let mut best_valley = second_peak as i64 - 1;
    let mut best_valley_score = -1i64;
    let mut x = second_peak as i64 - 1;
    while x > first_peak as i64 {
        let from_first = x - first_peak as i64;
        let score = from_first
            * from_first
            * (second_peak as i64 - x)
            * (max_bucket_count as i64 - buckets[x as usize] as i64);
        if score > best_valley_score {
            best_valley = x;
            best_valley_score = score;
        }
        x -= 1;
    }
    Ok((best_valley << LUMINANCE_SHIFT) as i32)
}

/// Port of `GlobalHistogramBinarizer::binarizeImage0`: build a histogram from
/// the middle 3/5 of the width on four evenly spaced rows, then threshold the
/// whole image with the resulting black point.
pub fn binarize_global_histogram(source: &ImgSource) -> ZXResult<BitMatrix> {
    let width = source.width();
    let height = source.height();
    let mut matrix = BitMatrix::new(width, height)?;

    let mut buckets = vec![0i32; LUMINANCE_BUCKETS];
    for y in 1..5 {
        let row_idx = height * y / 5;
        let row = source.row(row_idx)?;
        let right = (width << 2) / 5;
        for x in width / 5..right {
            let pixel = row[x] as usize;
            buckets[pixel >> LUMINANCE_SHIFT] += 1;
        }
    }

    let black_point = estimate_black_point(&buckets)?;
    let lum = source.matrix();
    for y in 0..height {
        let offset = y * width;
        for x in 0..width {
            if (lum[offset + x] as i32) < black_point {
                matrix.set(x, y);
            }
        }
    }
    Ok(matrix)
}

// ── HybridBinarizer ────────────────────────────────────────────────────────

const BLOCK_SIZE_POWER: usize = 3;
const BLOCK_SIZE: usize = 1 << BLOCK_SIZE_POWER; // 8
const BLOCK_SIZE_MASK: usize = BLOCK_SIZE - 1;
const MINIMUM_DIMENSION: usize = BLOCK_SIZE * 5;
const MIN_DYNAMIC_RANGE: i32 = 24;
const THRES_BLOCKSIZE: i32 = 2;

#[derive(Clone, Copy, Default)]
struct BinarizerBlock {
    #[allow(dead_code)]
    sum: i32,
    #[allow(dead_code)]
    min: i32,
    #[allow(dead_code)]
    max: i32,
    threshold: i32,
}

#[inline]
fn cap(value: i32, min: i32, max: i32) -> i32 {
    value.clamp(min, max)
}

/// Port of `HybridBinarizer`: compute a threshold per 8x8 block, average it
/// over a 5x5 block neighborhood using an integral image, then threshold each
/// block.
pub fn binarize_hybrid(source: &ImgSource) -> ZXResult<BitMatrix> {
    let width = source.width();
    let height = source.height();
    if width < MINIMUM_DIMENSION || height < MINIMUM_DIMENSION {
        return binarize_global_histogram(source);
    }

    let grey = source.byte_matrix();
    let mut sub_width = width >> BLOCK_SIZE_POWER;
    if (width & BLOCK_SIZE_MASK) != 0 {
        sub_width += 1;
    }
    let mut sub_height = height >> BLOCK_SIZE_POWER;
    if (height & BLOCK_SIZE_MASK) != 0 {
        sub_height += 1;
    }

    let blocks = init_blocks(&grey, sub_width, sub_height, width, height);
    let (integral, integral_width) = init_block_integral(&blocks, sub_width, sub_height);

    let mut matrix = BitMatrix::new(width, height)?;
    let max_y_offset = height - BLOCK_SIZE;
    let max_x_offset = width - BLOCK_SIZE;
    let block_area = ((2 * THRES_BLOCKSIZE + 1) * (2 * THRES_BLOCKSIZE + 1)) as i32;

    for y in 0..sub_height {
        let mut yoffset = y << BLOCK_SIZE_POWER;
        if yoffset > max_y_offset {
            yoffset = max_y_offset;
        }
        for x in 0..sub_width {
            let mut xoffset = x << BLOCK_SIZE_POWER;
            if xoffset > max_x_offset {
                xoffset = max_x_offset;
            }
            let left = cap(
                x as i32,
                THRES_BLOCKSIZE,
                sub_width as i32 - THRES_BLOCKSIZE - 1,
            );
            let top = cap(
                y as i32,
                THRES_BLOCKSIZE,
                sub_height as i32 - THRES_BLOCKSIZE - 1,
            );
            let offset1 =
                ((top - THRES_BLOCKSIZE) * integral_width as i32 + left - THRES_BLOCKSIZE) as usize;
            let offset2 = ((top + THRES_BLOCKSIZE + 1) * integral_width as i32 + left
                - THRES_BLOCKSIZE) as usize;
            let blocksize = (THRES_BLOCKSIZE * 2 + 1) as usize;
            let sum = integral[offset1] - integral[offset1 + blocksize] - integral[offset2]
                + integral[offset2 + blocksize];
            let average = sum / block_area;
            threshold_block(&grey, xoffset, yoffset, average, &mut matrix);
        }
    }
    Ok(matrix)
}

/// Port of `HybridBinarizer::initBlocks`.
fn init_blocks(
    grey: &ByteMatrix,
    sub_width: usize,
    sub_height: usize,
    width: usize,
    height: usize,
) -> Vec<BinarizerBlock> {
    let mut blocks = vec![
        BinarizerBlock {
            sum: 0,
            min: 0xFF,
            max: 0,
            threshold: 0
        };
        sub_width * sub_height
    ];
    let bytes = grey.as_slice();

    for y in 0..sub_height {
        let mut yoffset = y << BLOCK_SIZE_POWER;
        let max_y_offset = height - BLOCK_SIZE;
        if yoffset > max_y_offset {
            yoffset = max_y_offset;
        }
        for x in 0..sub_width {
            let mut xoffset = x << BLOCK_SIZE_POWER;
            let max_x_offset = width - BLOCK_SIZE;
            if xoffset > max_x_offset {
                xoffset = max_x_offset;
            }
            let mut sum = 0i32;
            let mut min = 0xFFi32;
            let mut max = 0i32;

            let mut yy = 0usize;
            let mut offset = yoffset * width + xoffset;
            while yy < BLOCK_SIZE {
                for xx in 0..BLOCK_SIZE {
                    let pixel = bytes[offset + xx] as i32;
                    sum += pixel;
                    if pixel < min {
                        min = pixel;
                    }
                    if pixel > max {
                        max = pixel;
                    }
                }
                // Once the dynamic range is wide enough, the remaining rows
                // only accumulate the sum and stop updating min/max (the
                // early-exit optimization from the original).
                if max - min > MIN_DYNAMIC_RANGE {
                    yy += 1;
                    offset += width;
                    while yy < BLOCK_SIZE {
                        let mut xx = 0;
                        while xx < BLOCK_SIZE {
                            sum += bytes[offset + xx] as i32;
                            sum += bytes[offset + xx + 1] as i32;
                            xx += 2;
                        }
                        yy += 1;
                        offset += width;
                    }
                    break;
                }
                yy += 1;
                offset += width;
            }

            let threshold =
                get_block_threshold(&blocks, x, y, sub_width, sum, min, max, MIN_DYNAMIC_RANGE);
            let b = &mut blocks[y * sub_width + x];
            b.min = min;
            b.max = max;
            b.sum = sum;
            b.threshold = threshold;
        }
    }
    blocks
}

/// Port of `getBlackPointFromNeighbors`.
#[inline]
fn black_point_from_neighbors(
    blocks: &[BinarizerBlock],
    sub_width: usize,
    x: usize,
    y: usize,
) -> i32 {
    (blocks[(y - 1) * sub_width + x].threshold
        + 2 * blocks[y * sub_width + x - 1].threshold
        + blocks[(y - 1) * sub_width + x - 1].threshold)
        >> 2
}

#[allow(clippy::too_many_arguments)]
fn get_block_threshold(
    blocks: &[BinarizerBlock],
    x: usize,
    y: usize,
    sub_width: usize,
    sum: i32,
    min: i32,
    max: i32,
    min_dynamic_range: i32,
) -> i32 {
    let mut average = sum >> (BLOCK_SIZE_POWER * 2);
    if max - min <= min_dynamic_range {
        // The block is nearly uniform: treat it as all white, take min/2 as
        // the threshold, and fall back to the upper-left neighbors if needed.
        average = min >> 1;
        if y > 0 && x > 0 {
            let bp = black_point_from_neighbors(blocks, sub_width, x, y);
            if min < bp {
                average = bp;
            }
        }
    }
    average
}

/// Port of `HybridBinarizer::initBlockIntegral`: a 2D prefix sum over the
/// per-block thresholds.
fn init_block_integral(
    blocks: &[BinarizerBlock],
    sub_width: usize,
    sub_height: usize,
) -> (Vec<i32>, usize) {
    let iw = sub_width + 1;
    let ih = sub_height + 1;
    let mut integral = vec![0i32; iw * ih];
    for i in 0..sub_height {
        let offset_block = i * sub_width;
        let offset_integral = (i + 1) * iw;
        let mut rs = 0i32;
        for j in 0..sub_width {
            rs += blocks[offset_block + j].threshold;
            integral[offset_integral + j + 1] = rs + integral[offset_integral - iw + j + 1];
        }
    }
    (integral, iw)
}

/// Port of `HybridBinarizer::thresholdBlock`: write one 8x8 block into the bit
/// matrix using the given threshold.
fn threshold_block(
    grey: &ByteMatrix,
    xoffset: usize,
    yoffset: usize,
    threshold: i32,
    matrix: &mut BitMatrix,
) {
    for y in 0..BLOCK_SIZE {
        let row = grey.row(yoffset + y);
        let out = matrix.row_bits_mut(yoffset + y);
        for x in 0..BLOCK_SIZE {
            out[xoffset + x] = u8::from((row[xoffset + x] as i32) <= threshold);
        }
    }
}

// ── FastWindowBinarizer ────────────────────────────────────────────────────

const FW_BLOCK_SIZE: usize = 6;
const WINDOW_FRACTION: f32 = 0.13;

/// Port of `FastWindowBinarizer::binarizeImage1` plus `fastWindow`.
///
/// Note one behavior of the original: when the window is larger than the image,
/// `fastWindow` only stores its result into `matrix_`, while `binarizeImage1`
/// still assigns the all-zero empty bit matrix to `matrix0_`, so callers get an
/// empty matrix. This port keeps that behavior (it returns an all-zero matrix);
/// otherwise the decision order across the four binarization rounds would not
/// match the original.
pub fn binarize_fast_window(source: &ImgSource) -> ZXResult<BitMatrix> {
    let width = source.width();
    let height = source.height();
    let mut matrix = BitMatrix::new(width, height)?;

    let r =
        (width.min(height) as f32 * WINDOW_FRACTION / FW_BLOCK_SIZE as f32 / 2.0 + 1.0) as usize;
    let newh_block_size = FW_BLOCK_SIZE * r;
    if height < newh_block_size || width < newh_block_size {
        return Ok(matrix); // See the note above: the original yields an empty matrix here.
    }

    let src = source.matrix();
    let integral = fast_integral(&src, width, height);
    let aw = width / FW_BLOCK_SIZE;
    let ah = height / FW_BLOCK_SIZE;

    for ai in 0..ah {
        let top = ((ai as i32 - r as i32 + 1) * FW_BLOCK_SIZE as i32).max(0) as usize;
        let bottom = ((ai + r) * FW_BLOCK_SIZE).min(height);
        for aj in 0..aw {
            let left = ((aj as i32 - r as i32 + 1) * FW_BLOCK_SIZE as i32).max(0) as usize;
            let right = ((aj + r) * FW_BLOCK_SIZE).min(width);

            let pt = top * (width + 1);
            let pb = bottom * (width + 1);
            let block = integral[pb + right]
                .wrapping_add(integral[pt + left])
                .wrapping_sub(integral[pt + right])
                .wrapping_sub(integral[pb + left]);
            let pixels = (bottom - top) * (right - left);
            let avg = (block / pixels as u32) as i32;

            let bi_end = ((ai + 1) * FW_BLOCK_SIZE).min(height);
            for bi in ai * FW_BLOCK_SIZE..bi_end {
                let bj_end = ((aj + 1) * FW_BLOCK_SIZE).min(width);
                for bj in aj * FW_BLOCK_SIZE..bj_end {
                    let v = u8::from((src[bi * width + bj] as i32) < avg);
                    matrix.set_value(bj, bi, v);
                }
            }
        }
    }
    Ok(matrix)
}

/// Port of `FastWindowBinarizer::fastIntegral`: a (w+1) x (h+1) integral
/// image.
fn fast_integral(src: &[u8], width: usize, height: usize) -> Vec<u32> {
    let iw = width + 1;
    let mut out = vec![0u32; iw * (height + 1)];
    for i in 1..height + 1 {
        let s = &src[(i - 1) * width..i * width];
        let mut row_sum = 0u32;
        for j in 0..width {
            row_sum += s[j] as u32;
            out[i * iw + j + 1] = out[(i - 1) * iw + j + 1] + row_sum;
        }
    }
    out
}

// ── SimpleAdaptiveBinarizer ────────────────────────────────────────────────

/// Port of `SimpleAdaptiveBinarizer::qrBinarize`: an adaptive threshold from a
/// sliding mean over a large 2^logw x 2^logh window, with T = m/n - 3.
pub fn binarize_simple_adaptive(source: &ImgSource) -> ZXResult<BitMatrix> {
    let width = source.width();
    let height = source.height();
    let mut matrix = BitMatrix::new(width, height)?;
    if width == 0 || height == 0 {
        return Ok(matrix);
    }
    let src = source.matrix();

    // Make the window large enough that it cannot fall entirely inside the
    // center of a finder pattern of a version-1 symbol.
    let mut logwindw = 4usize;
    while logwindw < 8 && (1 << logwindw) < ((width + 7) >> 3) {
        logwindw += 1;
    }
    let mut logwindh = 4usize;
    while logwindh < 8 && (1 << logwindh) < ((height + 7) >> 3) {
        logwindh += 1;
    }
    let windw = 1usize << logwindw;
    let windh = 1usize << logwindh;
    let logwinds = logwindw + logwindh;

    let mut col_sums = vec![0u32; width];
    for x in 0..width {
        let g = src[x] as u32;
        col_sums[x] = (g << (logwindh - 1)) + g;
    }
    for y in 1..(windh >> 1) {
        let y1offs = y.min(height - 1) * width;
        for x in 0..width {
            col_sums[x] += src[y1offs + x] as u32;
        }
    }

    for y in 0..height {
        let mut m = (col_sums[0] << (logwindw - 1)) + col_sums[0];
        for x in 1..(windw >> 1) {
            m += col_sums[x.min(width - 1)];
        }
        let offset = y * width;
        for x in 0..width {
            let g = src[offset + x] as u32;
            let bit = u8::from(((g + 3) << logwinds) < m);
            matrix.set_value(x, y, bit);
            if x + 1 < width {
                let x0 = x.saturating_sub(windw >> 1);
                let x1 = (x + (windw >> 1)).min(width - 1);
                m = m + col_sums[x1] - col_sums[x0];
            }
        }
        if y + 1 < height {
            let y0offs = y.saturating_sub(windh >> 1) * width;
            let y1offs = (y + (windh >> 1)).min(height - 1) * width;
            for x in 0..width {
                col_sums[x] = col_sums[x] - src[y0offs + x] as u32 + src[y1offs + x] as u32;
            }
        }
    }
    Ok(matrix)
}

// ── AdaptiveThresholdMeanBinarizer ─────────────────────────────────────────

const ATM_BLOCK_SIZE: usize = 25;
const ATM_BIAS: f64 = 10.0;

/// Port of `AdaptiveThresholdMeanBinarizer::binarizeImage`:
/// `cv::adaptiveThreshold(GAUSSIAN_C, THRESH_BINARY, bs = width/10 rounded to
/// odd, C = 10)`, then inverted so that black is 1.
///
/// The original flips the image vertically before and after the call
/// (TransBufferToMat / TransMatToBuffer). The two flips cancel out, because the
/// REPLICATE border used by the Gaussian filter is symmetric under a vertical
/// flip, so both are omitted here.
pub fn binarize_adaptive_threshold_mean(source: &ImgSource) -> ZXResult<BitMatrix> {
    let width = source.width();
    let height = source.height();
    if width < ATM_BLOCK_SIZE || height < ATM_BLOCK_SIZE {
        return binarize_global_histogram(source);
    }

    let mut bs = width / 10;
    bs = bs + bs % 2 - 1;
    if !(bs % 2 == 1 && bs > 1) {
        // The original returns -1 here and leaves matrix0_ empty, so the
        // caller receives an empty bit matrix.
        return BitMatrix::new(width, height);
    }

    let src = source.matrix();
    let th = adaptive_threshold_gaussian(&src, width, height, 255, bs, ATM_BIAS);

    let mut matrix = BitMatrix::new(width, height)?;
    for y in 0..height {
        let out = matrix.row_bits_mut(y);
        for x in 0..width {
            // The original criterion: value > 120 gives 0 (white), otherwise 1 (black).
            out[x] = u8::from(th[y * width + x] <= 120);
        }
    }
    Ok(matrix)
}
