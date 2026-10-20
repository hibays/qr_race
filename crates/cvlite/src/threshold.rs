//! Port of `cv::GaussianBlur` and `cv::adaptiveThreshold`.
//!
//! Only `adaptiveThreshold(src, dst, 255, ADAPTIVE_THRESH_GAUSSIAN_C,
//! THRESH_BINARY, blockSize, C)` is used, matching OpenCV
//! `thresh.cpp::adaptiveThreshold`: a `GaussianBlur(...,
//! BORDER_REPLICATE|BORDER_ISOLATED)` produces the local mean, then the result
//! is looked up as `dst = tab[src - mean + 255]`.
//!
//! Known deviation: OpenCV uses a fixed-point separable filter for 8U images,
//! while this port accumulates in f32 and applies `saturate_cast<uchar>`. The
//! means occasionally differ by one gray level, which changes the output only
//! where a pixel sits exactly on the threshold.

use super::{cv_ceil, saturate_cast_u8_f32};

/// `dst[i] += k * src[i]`.
///
/// Written as a zip over fixed-length slices so LLVM vectorizes it into NEON or
/// AVX FMAs. Accumulation runs along the kernel tap dimension, so the summation
/// order per output element matches the straightforward per-pixel form.
#[inline(always)]
fn axpy(dst: &mut [f32], src: &[f32], k: f32) {
    debug_assert_eq!(dst.len(), src.len());
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d += k * *s;
    }
}

#[inline(always)]
fn store_saturated(dst: &mut [u8], src: &[f32]) {
    debug_assert_eq!(dst.len(), src.len());
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d = saturate_cast_u8_f32(*s);
    }
}

const SMALL_GAUSSIAN_TAB: [&[f32]; 4] = [
    &[1.0],
    &[0.25, 0.5, 0.25],
    &[0.0625, 0.25, 0.375, 0.25, 0.0625],
    &[
        0.03125, 0.109375, 0.21875, 0.3125, 0.21875, 0.109375, 0.03125,
    ],
];

/// `cv::getGaussianKernel(n, sigma, CV_32F)`.
pub fn gaussian_kernel(n: usize, sigma: f64) -> Vec<f32> {
    let fixed: Option<&[f32]> = if n % 2 == 1 && n <= 7 && sigma <= 0.0 {
        Some(SMALL_GAUSSIAN_TAB[n >> 1])
    } else {
        None
    };
    let sigma_x = if sigma > 0.0 {
        sigma
    } else {
        ((n as f64 - 1.0) * 0.5 - 1.0) * 0.3 + 0.8
    };
    let scale2x = -0.5 / (sigma_x * sigma_x);

    let mut kernel = vec![0f32; n];
    let mut sum = 0f64;
    for i in 0..n {
        let x = i as f64 - (n as f64 - 1.0) * 0.5;
        let t = match fixed {
            Some(tab) => tab[i] as f64,
            None => (scale2x * x * x).exp(),
        };
        kernel[i] = t as f32;
        sum += t;
    }
    let inv = 1.0 / sum;
    for k in kernel.iter_mut() {
        *k = (*k as f64 * inv) as f32;
    }
    kernel
}

/// Output rows computed together in the vertical pass. Each intermediate row
/// read is reused ROWS times, cutting memory traffic to 1/ROWS.
const ROWS: usize = 4;
/// Column strip width, chosen so a working set of ksize rows by STRIP columns
/// stays in cache.
const STRIP: usize = 256;

/// Thread count to use. The Gaussian pass dominates the pipeline cost, so it is
/// worth parallelizing.
fn worker_count(work: usize) -> usize {
    // Threading small images costs more than it saves
    if work < 1 << 18 {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 4)
}

/// Horizontal pass over a contiguous range of rows.
fn horizontal_band(src: &[u8], out: &mut [f32], width: usize, kernel: &[f32], anchor: usize) {
    let mut padded = vec![0f32; width + 2 * anchor];
    for (row, orow) in src.chunks_exact(width).zip(out.chunks_exact_mut(width)) {
        // Expand into a bordered buffer per BORDER_REPLICATE first, so the inner
        // loop is purely sequential and needs no per-tap boundary check
        for p in padded.iter_mut().take(anchor) {
            *p = row[0] as f32;
        }
        for (i, &v) in row.iter().enumerate() {
            padded[anchor + i] = v as f32;
        }
        for p in padded.iter_mut().skip(anchor + width) {
            *p = row[width - 1] as f32;
        }

        orow.iter_mut().for_each(|v| *v = 0.0);
        for (k, &kv) in kernel.iter().enumerate() {
            axpy(orow, &padded[k..k + width], kv);
        }
    }
}

/// Vertical pass, producing rows `[y_start, y_start + dst.len()/width)`.
#[allow(clippy::too_many_arguments)]
fn vertical_band(
    tmp: &[f32],
    dst: &mut [u8],
    y_start: usize,
    width: usize,
    height: usize,
    kernel: &[f32],
    anchor: i32,
) {
    let band_rows = dst.len() / width;
    let h = height as i32;
    let mut accbuf = vec![0f32; STRIP * ROWS];

    let mut x0 = 0usize;
    while x0 < width {
        let cols = STRIP.min(width - x0);
        let mut yb = 0usize;
        while yb < band_rows {
            let rows = ROWS.min(band_rows - yb);
            let y0 = y_start + yb;
            for r in 0..rows {
                accbuf[r * STRIP..r * STRIP + cols]
                    .iter_mut()
                    .for_each(|v| *v = 0.0);
            }

            // Range of intermediate rows these output rows depend on
            let first = y0 as i32 - anchor;
            let last = (y0 + rows - 1) as i32 + anchor;
            for yy in first..=last {
                let src_y = yy.clamp(0, h - 1) as usize;
                let trow = &tmp[src_y * width + x0..src_y * width + x0 + cols];
                for r in 0..rows {
                    let k = yy - (y0 + r) as i32 + anchor;
                    if k < 0 || k >= kernel.len() as i32 {
                        continue;
                    }
                    axpy(
                        &mut accbuf[r * STRIP..r * STRIP + cols],
                        trow,
                        kernel[k as usize],
                    );
                }
            }

            for r in 0..rows {
                let off = (yb + r) * width + x0;
                store_saturated(
                    &mut dst[off..off + cols],
                    &accbuf[r * STRIP..r * STRIP + cols],
                );
            }
            yb += rows;
        }
        x0 += cols;
    }
}

/// `cv::GaussianBlur(src, dst, Size(ksize,ksize), 0, 0, BORDER_REPLICATE)`, 8UC1.
///
/// Separable implementation: horizontal pass followed by vertical pass. This is
/// the dominant cost of the pipeline. blockSize is one tenth of the image
/// width, so a 1920 pixel wide image gives 191 taps and, even separated, on the
/// order of 8e8 multiply-adds. Three things address that:
///   1. the inner loops accumulate contiguously at fixed offsets, which LLVM
///      turns into NEON or AVX FMAs;
///   2. the vertical pass is blocked into column strips of four rows at a time,
///      keeping the working set in cache and reusing rows already read;
///   3. both passes are split into row bands across threads.
///
/// None of the three changes the summation order of any output element, so the
/// result is bit-identical to the naive per-pixel form.
pub fn gaussian_blur_8u(src: &[u8], width: usize, height: usize, ksize: usize) -> Vec<u8> {
    let kernel = gaussian_kernel(ksize, 0.0);
    let anchor = ksize / 2;
    let mut tmp = vec![0f32; width * height];
    let mut dst = vec![0u8; width * height];

    let threads = worker_count(width * height * ksize);
    if threads <= 1 {
        horizontal_band(src, &mut tmp, width, &kernel, anchor);
        vertical_band(&tmp, &mut dst, 0, width, height, &kernel, anchor as i32);
        return dst;
    }

    let band = height.div_ceil(threads);

    // Horizontal: rows are independent, so split directly
    std::thread::scope(|scope| {
        let kernel = &kernel;
        for (i, (out_band, src_band)) in tmp
            .chunks_mut(band * width)
            .zip(src.chunks(band * width))
            .enumerate()
        {
            let _ = i;
            scope.spawn(move || horizontal_band(src_band, out_band, width, kernel, anchor));
        }
    });

    // Vertical: split by output row band with tmp read-only. Bands read rows
    // belonging to their neighbours, so this runs after the horizontal pass completes
    std::thread::scope(|scope| {
        let kernel = &kernel;
        let tmp = &tmp;
        for (i, dst_band) in dst.chunks_mut(band * width).enumerate() {
            scope.spawn(move || {
                vertical_band(
                    tmp,
                    dst_band,
                    i * band,
                    width,
                    height,
                    kernel,
                    anchor as i32,
                )
            });
        }
    });

    dst
}

/// `cv::adaptiveThreshold(..., ADAPTIVE_THRESH_GAUSSIAN_C, THRESH_BINARY, block_size, delta)`.
///
/// Returns a binary image of 0 and `max_value`.
pub fn adaptive_threshold_gaussian(
    src: &[u8],
    width: usize,
    height: usize,
    max_value: u8,
    block_size: usize,
    delta: f64,
) -> Vec<u8> {
    assert!(
        block_size % 2 == 1 && block_size > 1,
        "block_size must be odd and > 1"
    );
    let mean = gaussian_blur_8u(src, width, height, block_size);

    // OpenCV uses cvCeil(delta) for THRESH_BINARY
    let idelta = cv_ceil(delta);
    // tab[i] = (i - 255 > -idelta) ? maxval : 0, i ∈ [0, 768)
    let mut tab = [0u8; 768];
    for (i, t) in tab.iter_mut().enumerate() {
        *t = if (i as i32 - 255) > -idelta {
            max_value
        } else {
            0
        };
    }

    let mut dst = vec![0u8; width * height];
    for i in 0..width * height {
        dst[i] = tab[(src[i] as i32 - mean[i] as i32 + 255) as usize];
    }
    dst
}
