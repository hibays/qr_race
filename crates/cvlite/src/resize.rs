//! Port of `cv::resize` for single-channel 8U images, covering INTER_CUBIC,
//! INTER_AREA and INTER_LINEAR.
//!
//! The structure follows `resize()` in OpenCV `imgproc/src/resize.cpp`:
//!   - INTER_AREA with scale_x/scale_y >= 1 uses `resizeAreaFast_` for integer
//!     ratios and `resizeArea_` with a decimation table otherwise;
//!   - everything else uses the generic path `resizeGeneric_`: a horizontal
//!     pass (HResize) then a vertical pass (VResize), with fixed-point
//!     coefficients for 8U (INTER_RESIZE_COEF_BITS = 11) and a final
//!     `FixedPtCast<int,uchar,22>`.
//!
//! Only grayscale input is needed, so the channel count is fixed at 1.

use super::{cv_ceil, cv_floor, cv_round, saturate_cast_u8_f32, saturate_cast_u8_i32};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Cubic,
    Area,
}

const INTER_RESIZE_COEF_BITS: i32 = 11;
const INTER_RESIZE_COEF_SCALE: f32 = (1 << INTER_RESIZE_COEF_BITS) as f32; // 2048

/// `interpolateCubic`, A = -0.75.
#[inline]
fn interpolate_cubic(x: f32) -> [f32; 4] {
    const A: f32 = -0.75;
    let mut c = [0f32; 4];
    c[0] = ((A * (x + 1.0) - 5.0 * A) * (x + 1.0) + 8.0 * A) * (x + 1.0) - 4.0 * A;
    c[1] = ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0;
    c[2] = ((A + 2.0) * (1.0 - x) - (A + 3.0)) * (1.0 - x) * (1.0 - x) + 1.0;
    c[3] = 1.0 - c[0] - c[1] - c[2];
    c
}

/// `saturate_cast<short>`
#[inline]
fn saturate_cast_i16(v: f32) -> i16 {
    cv_round(v as f64).clamp(-32768, 32767) as i16
}

/// OpenCV `resize.cpp::clip(x, a, b)`
#[inline]
fn clip(x: i32, a: i32, b: i32) -> i32 {
    if x >= a {
        if x < b {
            x
        } else {
            b - 1
        }
    } else {
        a
    }
}

/// `cv::resize(src, dst, dsize, 0, 0, interpolation)`, 8UC1.
pub fn resize(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    interpolation: Interpolation,
) -> Vec<u8> {
    assert!(src_width > 0 && src_height > 0 && dst_width > 0 && dst_height > 0);
    if src_width == dst_width && src_height == dst_height {
        return src.to_vec();
    }

    let inv_scale_x = dst_width as f64 / src_width as f64;
    let inv_scale_y = dst_height as f64 / src_height as f64;
    let scale_x = 1.0 / inv_scale_x;
    let scale_y = 1.0 / inv_scale_y;

    if interpolation == Interpolation::Area && scale_x >= 1.0 && scale_y >= 1.0 {
        let iscale_x = scale_x.round() as i32; // saturate_cast<int> = cvRound
        let iscale_y = scale_y.round() as i32;
        let is_area_fast = (scale_x - iscale_x as f64).abs() < f64::EPSILON
            && (scale_y - iscale_y as f64).abs() < f64::EPSILON;
        if is_area_fast {
            return resize_area_fast(
                src,
                src_width,
                src_height,
                dst_width,
                dst_height,
                iscale_x as usize,
                iscale_y as usize,
            );
        }
        return resize_area(
            src, src_width, src_height, dst_width, dst_height, scale_x, scale_y,
        );
    }

    resize_generic(
        src,
        src_width,
        src_height,
        dst_width,
        dst_height,
        interpolation,
        scale_x,
        scale_y,
        inv_scale_x,
        inv_scale_y,
    )
}

/// Port of `resizeAreaFast_`: integer-ratio downscale, block sum times 1/area.
fn resize_area_fast(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    scale_x: usize,
    scale_y: usize,
) -> Vec<u8> {
    let area = scale_x * scale_y;
    let scale = 1.0f32 / area as f32;
    let dwidth1 = src_width / scale_x;
    let mut dst = vec![0u8; dst_width * dst_height];

    for dy in 0..dst_height {
        let sy0 = dy * scale_y;
        // Fast path only when the full block fits
        let w = if sy0 + scale_y <= src_height {
            dwidth1.min(dst_width)
        } else {
            0
        };
        if sy0 >= src_height {
            continue; // fill this row with zeros
        }
        // Only the NEON path below writes to it, so on other targets it never
        // needs to be mutable.
        #[allow(unused_mut)]
        let mut dx0 = 0usize;
        // 2x2 is the form the pipeline uses (SuperScale downscale), so it is vectorized
        #[cfg(target_arch = "aarch64")]
        if scale_x == 2 && scale_y == 2 && w > 0 {
            let r0 = &src[sy0 * src_width..sy0 * src_width + src_width];
            let r1 = &src[(sy0 + 1) * src_width..(sy0 + 1) * src_width + src_width];
            let out = &mut dst[dy * dst_width..dy * dst_width + w];
            dx0 = unsafe { area_fast_2x2_neon(r0, r1, out) };
        }
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        if scale_x == 2 && scale_y == 2 && w > 0 {
            let r0 = &src[sy0 * src_width..sy0 * src_width + src_width];
            let r1 = &src[(sy0 + 1) * src_width..(sy0 + 1) * src_width + src_width];
            let out = &mut dst[dy * dst_width..dy * dst_width + w];
            dx0 = area_fast_2x2_simd128(r0, r1, out);
        }
        for dx in dx0..w {
            let sx0 = dx * scale_x;
            let mut sum: i32 = 0;
            for ky in 0..scale_y {
                let row = (sy0 + ky) * src_width;
                for kx in 0..scale_x {
                    sum += src[row + sx0 + kx] as i32;
                }
            }
            dst[dy * dst_width + dx] = saturate_cast_u8_f32(sum as f32 * scale);
        }
        // Tail: the block is clipped by the image edge, so average over the pixels present
        for dx in w..dst_width {
            let sx0 = dx * scale_x;
            if sx0 >= src_width {
                continue;
            }
            let mut sum: i32 = 0;
            let mut count = 0i32;
            for sy in 0..scale_y {
                if sy0 + sy >= src_height {
                    break;
                }
                let row = (sy0 + sy) * src_width;
                for sx in 0..scale_x {
                    if sx0 + sx >= src_width {
                        break;
                    }
                    sum += src[row + sx0 + sx] as i32;
                    count += 1;
                }
            }
            if count > 0 {
                dst[dy * dst_width + dx] = saturate_cast_u8_f32(sum as f32 / count as f32);
            }
        }
    }
    dst
}

#[derive(Clone, Copy)]
struct DecimateAlpha {
    si: usize,
    di: usize,
    alpha: f32,
}

/// Port of `computeResizeAreaTab` for cn = 1.
fn compute_resize_area_tab(ssize: usize, dsize: usize, scale: f64) -> Vec<DecimateAlpha> {
    let mut tab = Vec::with_capacity(ssize * 2);
    for dx in 0..dsize {
        let fsx1 = dx as f64 * scale;
        let fsx2 = fsx1 + scale;
        let cell_width = scale.min(ssize as f64 - fsx1);

        let mut sx1 = cv_ceil(fsx1);
        let mut sx2 = cv_floor(fsx2);
        sx2 = sx2.min(ssize as i32 - 1);
        sx1 = sx1.min(sx2);

        if sx1 as f64 - fsx1 > 1e-3 {
            tab.push(DecimateAlpha {
                di: dx,
                si: (sx1 - 1).max(0) as usize,
                alpha: ((sx1 as f64 - fsx1) / cell_width) as f32,
            });
        }
        for sx in sx1..sx2 {
            tab.push(DecimateAlpha {
                di: dx,
                si: sx.max(0) as usize,
                alpha: (1.0 / cell_width) as f32,
            });
        }
        if fsx2 - sx2 as f64 > 1e-3 {
            tab.push(DecimateAlpha {
                di: dx,
                si: sx2.max(0) as usize,
                alpha: (((fsx2 - sx2 as f64).min(1.0)).min(cell_width) / cell_width) as f32,
            });
        }
    }
    tab
}

/// Port of `resizeArea_` / `ResizeArea_Invoker` for cn = 1 and WT = float.
fn resize_area(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    scale_x: f64,
    scale_y: f64,
) -> Vec<u8> {
    let xtab = compute_resize_area_tab(src_width, dst_width, scale_x);
    let ytab = compute_resize_area_tab(src_height, dst_height, scale_y);

    let mut dst = vec![0u8; dst_width * dst_height];
    let mut buf = vec![0f32; dst_width];
    let mut sum = vec![0f32; dst_width];

    if ytab.is_empty() {
        return dst;
    }
    let mut prev_dy = ytab[0].di;

    for t in ytab.iter() {
        let beta = t.alpha;
        let dy = t.di;
        let sy = t.si;

        buf.iter_mut().for_each(|v| *v = 0.0);
        let s = &src[sy * src_width..(sy + 1) * src_width];
        for x in xtab.iter() {
            buf[x.di] += s[x.si] as f32 * x.alpha;
        }

        if dy != prev_dy {
            for dx in 0..dst_width {
                dst[prev_dy * dst_width + dx] = saturate_cast_u8_f32(sum[dx]);
                sum[dx] = beta * buf[dx];
            }
            prev_dy = dy;
        } else {
            for dx in 0..dst_width {
                sum[dx] += beta * buf[dx];
            }
        }
    }
    for dx in 0..dst_width {
        dst[prev_dy * dst_width + dx] = saturate_cast_u8_f32(sum[dx]);
    }
    dst
}

/// Port of `resizeGeneric_`: separable resampling with 8U fixed-point coefficients.
#[allow(clippy::too_many_arguments)]
fn resize_generic(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    interpolation: Interpolation,
    scale_x: f64,
    scale_y: f64,
    inv_scale_x: f64,
    inv_scale_y: f64,
) -> Vec<u8> {
    let area_mode = interpolation == Interpolation::Area;
    let ksize: usize = if interpolation == Interpolation::Cubic {
        4
    } else {
        2
    };
    let ksize2 = (ksize / 2) as i32;

    let mut xofs = vec![0i32; dst_width];
    let mut ialpha = vec![0i16; dst_width * ksize];
    let mut xmin = 0usize;
    let mut xmax = dst_width;

    for dx in 0..dst_width {
        let (mut sx, mut fx);
        if !area_mode {
            let f = ((dx as f64 + 0.5) * scale_x - 0.5) as f32;
            sx = cv_floor(f as f64);
            fx = f - sx as f32;
        } else {
            sx = cv_floor(dx as f64 * scale_x);
            let f = ((dx as f64 + 1.0) - (sx as f64 + 1.0) * inv_scale_x) as f32;
            fx = if f <= 0.0 {
                0.0
            } else {
                f - cv_floor(f as f64) as f32
            };
        }

        if sx < ksize2 - 1 {
            xmin = dx + 1;
            if sx < 0 && interpolation != Interpolation::Cubic {
                fx = 0.0;
                sx = 0;
            }
        }
        if sx + ksize2 >= src_width as i32 {
            xmax = xmax.min(dx);
            if sx >= src_width as i32 - 1 && interpolation != Interpolation::Cubic {
                fx = 0.0;
                sx = src_width as i32 - 1;
            }
        }

        xofs[dx] = sx;
        let cbuf: [f32; 4] = if interpolation == Interpolation::Cubic {
            interpolate_cubic(fx)
        } else {
            [1.0 - fx, fx, 0.0, 0.0]
        };
        for k in 0..ksize {
            ialpha[dx * ksize + k] = saturate_cast_i16(cbuf[k] * INTER_RESIZE_COEF_SCALE);
        }
    }

    let mut yofs = vec![0i32; dst_height];
    let mut ibeta = vec![0i16; dst_height * ksize];
    for dy in 0..dst_height {
        let (sy, fy);
        if !area_mode {
            let f = ((dy as f64 + 0.5) * scale_y - 0.5) as f32;
            sy = cv_floor(f as f64);
            fy = f - sy as f32;
        } else {
            sy = cv_floor(dy as f64 * scale_y);
            let f = ((dy as f64 + 1.0) - (sy as f64 + 1.0) * inv_scale_y) as f32;
            fy = if f <= 0.0 {
                0.0
            } else {
                f - cv_floor(f as f64) as f32
            };
        }
        yofs[dy] = sy;
        let cbuf: [f32; 4] = if interpolation == Interpolation::Cubic {
            interpolate_cubic(fy)
        } else {
            [1.0 - fy, fy, 0.0, 0.0]
        };
        for k in 0..ksize {
            ibeta[dy * ksize + k] = saturate_cast_i16(cbuf[k] * INTER_RESIZE_COEF_SCALE);
        }
    }

    // Horizontal filter: reduce one source row to dst_width i32 values using
    // xofs/ialpha. Boundary handling differs between cubic and linear in OpenCV:
    //   HResizeCubic: [0,xmin) and [xmax,dwidth) clamp the index and still
    //                  apply the usual four-tap weighting
    //   HResizeLinear: [xmax,dwidth) takes S[sx]*ONE, i.e. nearest neighbour,
    //                  without alpha
    let cubic = interpolation == Interpolation::Cubic;
    let hresize_row = |sy: usize, out: &mut [i32]| {
        let s = &src[sy * src_width..(sy + 1) * src_width];
        hresize(s, out, &xofs, &ialpha, ksize, cubic, xmin, xmax, src_width);
    };

    let mut dst = vec![0u8; dst_width * dst_height];
    let mut rows: Vec<Vec<i32>> = vec![vec![0i32; dst_width]; ksize];
    let mut prev_sy: Vec<i32> = vec![-1; ksize];

    let mut want = vec![0i32; ksize];
    for dy in 0..dst_height {
        let sy0 = yofs[dy];
        // The ksize source rows needed here. Sampling windows of adjacent
        // output rows overlap heavily, so rows already computed are swapped
        // into place rather than cloned; cloning would copy four dst_width i32
        // buffers per output row in the cubic case.
        for k in 0..ksize {
            want[k] = clip(sy0 - ksize2 + 1 + k as i32, 0, src_height as i32);
        }
        for k in 0..ksize {
            if prev_sy[k] == want[k] {
                continue;
            }
            match (k + 1..ksize).find(|&i| prev_sy[i] == want[k]) {
                Some(idx) => {
                    rows.swap(k, idx);
                    prev_sy.swap(k, idx);
                }
                None => {
                    hresize_row(want[k] as usize, &mut rows[k]);
                    prev_sy[k] = want[k];
                }
            }
        }

        // Vertical filter plus FixedPtCast<int, uchar, 22>
        let beta = &ibeta[dy * ksize..dy * ksize + ksize];
        let out = &mut dst[dy * dst_width..(dy + 1) * dst_width];
        vresize(&rows, ksize, beta, out);
    }
    dst
}

/// NEON implementation of the 2x2 downscale in `resize_area_fast`, producing 8
/// pixels per iteration and returning the dx it reached.
///
/// Bit-identical output hinges on rounding. The scalar path computes
/// `cvRound(sum / 4.0)`, which rounds halfway cases to even. The sum of four u8
/// values is at most 1020 and dividing by 4 can only land on .0, .25, .5 or
/// .75, so integer arithmetic reproduces it exactly. With `q = sum >> 2` and
/// `r = sum & 3`:
///   r = 0 or 1 -> q      (.0 and .25 round down)
///   r = 2      -> exactly .5, round to even: keep q if even, otherwise q + 1
///   r = 3      -> q + 1  (.75 rounds up)
/// The result is at most 255, so saturation never triggers.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn area_fast_2x2_neon(r0: &[u8], r1: &[u8], out: &mut [u8]) -> usize {
    use std::arch::aarch64::*;

    let n = out.len();
    let one = vdupq_n_u16(1);
    let three = vdupq_n_u16(3);
    let two = vdupq_n_u16(2);
    let mut dx = 0usize;

    while dx + 8 <= n {
        // Read 16 bytes per row and pairwise-add into 8 u16 values; adding the
        // two rows gives the 2x2 block sums
        let a = vpaddlq_u8(vld1q_u8(r0.as_ptr().add(dx * 2)));
        let b = vpaddlq_u8(vld1q_u8(r1.as_ptr().add(dx * 2)));
        let sum = vaddq_u16(a, b);

        let q = vshrq_n_u16::<2>(sum);
        let r = vandq_u16(sum, three);
        // r == 3 always rounds up; r == 2 rounds up only when q is odd
        let up3 = vandq_u16(vceqq_u16(r, three), one);
        let up2 = vandq_u16(vandq_u16(vceqq_u16(r, two), vandq_u16(q, one)), one);
        let rounded = vaddq_u16(q, vorrq_u16(up3, up2));

        vst1_u8(out.as_mut_ptr().add(dx), vqmovn_u16(rounded));
        dx += 8;
    }
    dx
}

/// The same 2x2 block average as [`area_fast_2x2_neon`], for WebAssembly.
///
/// wasm has no pairwise add, so the two halves are picked apart with shuffles
/// and added instead; the rounding is the identical integer form, and the
/// tests check both against the scalar path.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn area_fast_2x2_simd128(r0: &[u8], r1: &[u8], out: &mut [u8]) -> usize {
    use core::arch::wasm32::*;

    /// Even lanes of a vector, widened, which is one column of each block.
    #[inline]
    fn even(v: v128) -> v128 {
        u16x8_extend_low_u8x16(u8x16_shuffle::<
            0,
            2,
            4,
            6,
            8,
            10,
            12,
            14,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        >(v, v))
    }

    /// Odd lanes, the other column.
    #[inline]
    fn odd(v: v128) -> v128 {
        u16x8_extend_low_u8x16(u8x16_shuffle::<
            1,
            3,
            5,
            7,
            9,
            11,
            13,
            15,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        >(v, v))
    }

    let n = out.len();
    let mut dx = 0usize;

    while dx + 8 <= n {
        // Sixteen source bytes per row cover eight output pixels. wasm has no
        // pairwise add, so the columns are separated with a shuffle instead.
        // SAFETY: both rows hold at least `dx * 2 + 16` bytes, since `out` is
        // half the width of a row and the loop stops eight outputs short.
        let (a, b) = unsafe {
            (
                v128_load(r0.as_ptr().add(dx * 2) as *const v128),
                v128_load(r1.as_ptr().add(dx * 2) as *const v128),
            )
        };
        let sum = u16x8_add(u16x8_add(even(a), odd(a)), u16x8_add(even(b), odd(b)));

        // The same integer rounding as the NEON path, for the same reason:
        // it reproduces cvRound's round-half-to-even exactly.
        let one = u16x8_splat(1);
        let q = u16x8_shr(sum, 2);
        let r = v128_and(sum, u16x8_splat(3));
        let up3 = v128_and(u16x8_eq(r, u16x8_splat(3)), one);
        let up2 = v128_and(v128_and(u16x8_eq(r, u16x8_splat(2)), v128_and(q, one)), one);
        let rounded = u16x8_add(q, v128_or(up3, up2));

        // Eight u16 down to eight u8. The value is at most 255, so the
        // saturation never bites.
        let bytes: [u8; 16] = unsafe { core::mem::transmute(u8x16_narrow_i16x8(rounded, rounded)) };
        out[dx..dx + 8].copy_from_slice(&bytes[..8]);
        dx += 8;
    }
    dx
}

/// Horizontal filter: reduce one source row to `dst_width` i32 values using
/// `xofs` and `ialpha`.
///
/// Boundary handling follows OpenCV, where cubic and linear differ:
///   `HResizeCubic`: [0,xmin) and [xmax,dwidth) clamp the index and still
///                    apply the usual four-tap weighting
///   `HResizeLinear`: [xmax,dwidth) takes `S[sx]*ONE`, i.e. nearest neighbour,
///                    without alpha
///
/// The middle range ([xmin,xmax) for cubic, [0,xmax) for linear) needs no
/// clamping and its taps are contiguous in memory, so on aarch64 it goes
/// through [`hresize_inner_neon`]. The edges and the tail stay scalar.
#[allow(clippy::too_many_arguments)]
fn hresize(
    s: &[u8],
    out: &mut [i32],
    xofs: &[i32],
    ialpha: &[i16],
    ksize: usize,
    cubic: bool,
    xmin: usize,
    xmax: usize,
    src_width: usize,
) {
    let dst_width = out.len();
    // Middle range: cubic starts at xmin because earlier indices need clamping, linear at 0
    let lo = if cubic { xmin.min(dst_width) } else { 0 };
    let hi = xmax.min(dst_width);

    for dx in 0..lo.min(hi) {
        out[dx] = hresize_px(s, xofs, ialpha, ksize, cubic, src_width, dx, true);
    }

    #[allow(unused_mut)] // as above: reassigned only on the NEON path
    let mut dx = lo;
    #[cfg(target_arch = "aarch64")]
    if hi > lo {
        dx = unsafe { hresize_inner_neon(s, out, xofs, ialpha, ksize, lo, hi) };
    }
    for dx in dx..hi {
        out[dx] = hresize_px(s, xofs, ialpha, ksize, cubic, src_width, dx, false);
    }

    for dx in hi.max(lo)..dst_width {
        out[dx] = hresize_px(s, xofs, ialpha, ksize, cubic, src_width, dx, true);
    }
}

/// Horizontal filter for a single output pixel. `edge` selects the boundary
/// rules, i.e. clamping or nearest neighbour.
#[allow(clippy::too_many_arguments)]
#[inline]
fn hresize_px(
    s: &[u8],
    xofs: &[i32],
    ialpha: &[i16],
    ksize: usize,
    cubic: bool,
    src_width: usize,
    dx: usize,
    edge: bool,
) -> i32 {
    let a = &ialpha[dx * ksize..dx * ksize + ksize];
    let base = xofs[dx];
    if cubic {
        let mut v = 0i32;
        for j in 0..ksize {
            let sxj = if edge {
                (base - 1 + j as i32).clamp(0, src_width as i32 - 1) as usize
            } else {
                (base - 1 + j as i32) as usize
            };
            v += s[sxj] as i32 * a[j] as i32;
        }
        v
    } else if !edge {
        s[base as usize] as i32 * a[0] as i32 + s[base as usize + 1] as i32 * a[1] as i32
    } else {
        s[base as usize] as i32 * (INTER_RESIZE_COEF_SCALE as i32)
    }
}

/// NEON implementation of the middle range of [`hresize`], producing 4 pixels
/// per iteration and returning the dx it reached; the rest stays scalar.
///
/// The four cubic taps (`base-1..base+2`) are contiguous in memory, as are the
/// two linear taps, so each output pixel corresponds to a short run of bytes.
/// The runs for four output pixels are loaded as integers into one vector,
/// widened, multiplied by the contiguous alphas, and reduced with two
/// `vpaddq_s32` instructions, which yields the four results at once.
///
/// The result is bit-identical: pixels are u8 and alphas are i16, so
/// `vmull_s16` gives exact i32 products, and the reduction only reassociates
/// integer additions, which is exact.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn hresize_inner_neon(
    s: &[u8],
    out: &mut [i32],
    xofs: &[i32],
    ialpha: &[i16],
    ksize: usize,
    lo: usize,
    hi: usize,
) -> usize {
    use std::arch::aarch64::*;

    let mut dx = lo;
    if ksize == 4 {
        while dx + 4 <= hi {
            // Four contiguous bytes per pixel, starting at base-1, packed into one 16-byte vector
            let mut g = vdupq_n_u32(0);
            g = vsetq_lane_u32::<0>(read_u32(s, xofs[dx] - 1), g);
            g = vsetq_lane_u32::<1>(read_u32(s, xofs[dx + 1] - 1), g);
            g = vsetq_lane_u32::<2>(read_u32(s, xofs[dx + 2] - 1), g);
            g = vsetq_lane_u32::<3>(read_u32(s, xofs[dx + 3] - 1), g);
            let bytes = vreinterpretq_u8_u32(g);

            // u8 to i16; pixels are at most 255 so this cannot overflow
            let px_lo = vreinterpretq_s16_u16(vmovl_u8(vget_low_u8(bytes)));
            let px_hi = vreinterpretq_s16_u16(vmovl_u8(vget_high_u8(bytes)));

            let ap = ialpha.as_ptr().add(dx * 4);
            let a_lo = vld1q_s16(ap);
            let a_hi = vld1q_s16(ap.add(8));

            // Each int32x4 holds the four tap products of one output pixel
            let v0 = vmull_s16(vget_low_s16(px_lo), vget_low_s16(a_lo));
            let v1 = vmull_high_s16(px_lo, a_lo);
            let v2 = vmull_s16(vget_low_s16(px_hi), vget_low_s16(a_hi));
            let v3 = vmull_high_s16(px_hi, a_hi);

            // Two rounds of pairwise addition reduce each group of taps to one value
            let sum = vpaddq_s32(vpaddq_s32(v0, v1), vpaddq_s32(v2, v3));
            vst1q_s32(out.as_mut_ptr().add(dx), sum);
            dx += 4;
        }
    } else if ksize == 2 {
        while dx + 4 <= hi {
            // Two contiguous bytes per pixel, 8 bytes for four pixels
            let mut g = vdup_n_u16(0);
            g = vset_lane_u16::<0>(read_u16(s, xofs[dx]), g);
            g = vset_lane_u16::<1>(read_u16(s, xofs[dx + 1]), g);
            g = vset_lane_u16::<2>(read_u16(s, xofs[dx + 2]), g);
            g = vset_lane_u16::<3>(read_u16(s, xofs[dx + 3]), g);
            let px = vreinterpretq_s16_u16(vmovl_u8(vreinterpret_u8_u16(g)));

            let a = vld1q_s16(ialpha.as_ptr().add(dx * 2));
            let v0 = vmull_s16(vget_low_s16(px), vget_low_s16(a));
            let v1 = vmull_high_s16(px, a);

            // Two taps per pixel, so one round of pairwise addition suffices
            let sum = vpaddq_s32(v0, v1);
            vst1q_s32(out.as_mut_ptr().add(dx), sum);
            dx += 4;
        }
    }
    dx
}

/// Read 4 bytes little-endian starting at byte `at` of `s`, without alignment requirements.
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn read_u32(s: &[u8], at: i32) -> u32 {
    (s.as_ptr().add(at as usize) as *const u32).read_unaligned()
}

/// As above, for 2 bytes.
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn read_u16(s: &[u8], at: i32) -> u16 {
    (s.as_ptr().add(at as usize) as *const u16).read_unaligned()
}

/// Vertical filter: `dst[dx] = saturate_u8((sum rows[k][dx] * beta[k] + (1<<21)) >> 22)`.
///
/// This is the hottest loop in resize: fully sequential access with ksize
/// multiply-adds per output pixel. aarch64 uses NEON (see [`vresize_neon`]),
/// other targets stay scalar. Both paths produce bit-identical results: the
/// fixed-point multiply-adds wrap on overflow in both, and saturating narrowing
/// clamps to the same range as `saturate_cast<uchar>`. See the comparison tests.
#[inline]
fn vresize(rows: &[Vec<i32>], ksize: usize, beta: &[i16], out: &mut [u8]) {
    #[cfg(target_arch = "aarch64")]
    {
        // NEON is baseline on aarch64, so no runtime detection is needed
        unsafe { vresize_neon(rows, ksize, beta, out) };
        return;
    }
    #[cfg(not(target_arch = "aarch64"))]
    vresize_scalar(rows, ksize, beta, out);
}

#[cfg_attr(target_arch = "aarch64", allow(dead_code))]
fn vresize_scalar(rows: &[Vec<i32>], ksize: usize, beta: &[i16], out: &mut [u8]) {
    for (dx, o) in out.iter_mut().enumerate() {
        let mut v: i32 = 0;
        for k in 0..ksize {
            v = v.wrapping_add(rows[k][dx].wrapping_mul(beta[k] as i32));
        }
        let shifted =
            (v.wrapping_add(1 << (INTER_RESIZE_COEF_BITS * 2 - 1))) >> (INTER_RESIZE_COEF_BITS * 2);
        *o = saturate_cast_u8_i32(shifted);
    }
}

/// NEON implementation of [`vresize`], producing 8 pixels per iteration with a
/// scalar tail.
///
/// Two points make it bit-identical to the scalar path:
///   * `vmulq_n_s32` and `vaddq_s32` wrap on overflow, matching
///     `wrapping_mul` and `wrapping_add`;
///   * saturating narrowing happens in two steps (i32 to i16 to u8), which
///     looks like it clamps twice but is equivalent: a value above 32767
///     saturates to 32767 and then to 255, and a value below -32768 saturates
///     to -32768 and then to 0, the same as clamping directly to [0,255].
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vresize_neon(rows: &[Vec<i32>], ksize: usize, beta: &[i16], out: &mut [u8]) {
    use std::arch::aarch64::*;

    let n = out.len();
    let delta = vdupq_n_s32(1 << (INTER_RESIZE_COEF_BITS * 2 - 1));
    let mut dx = 0usize;

    while dx + 8 <= n {
        let mut acc0 = vdupq_n_s32(0);
        let mut acc1 = vdupq_n_s32(0);
        for k in 0..ksize {
            let b = beta[k] as i32;
            let p = rows[k].as_ptr().add(dx);
            acc0 = vaddq_s32(acc0, vmulq_n_s32(vld1q_s32(p), b));
            acc1 = vaddq_s32(acc1, vmulq_n_s32(vld1q_s32(p.add(4)), b));
        }
        let s0 = vshrq_n_s32::<{ INTER_RESIZE_COEF_BITS * 2 }>(vaddq_s32(acc0, delta));
        let s1 = vshrq_n_s32::<{ INTER_RESIZE_COEF_BITS * 2 }>(vaddq_s32(acc1, delta));
        let narrowed = vcombine_s16(vqmovn_s32(s0), vqmovn_s32(s1));
        vst1_u8(out.as_mut_ptr().add(dx), vqmovun_s16(narrowed));
        dx += 8;
    }

    if dx < n {
        vresize_scalar_from(rows, ksize, beta, out, dx);
    }
}

/// Scalar tail: process from `start` to the end of the row.
#[cfg_attr(not(target_arch = "aarch64"), allow(dead_code))]
fn vresize_scalar_from(
    rows: &[Vec<i32>],
    ksize: usize,
    beta: &[i16],
    out: &mut [u8],
    start: usize,
) {
    for dx in start..out.len() {
        let mut v: i32 = 0;
        for k in 0..ksize {
            v = v.wrapping_add(rows[k][dx].wrapping_mul(beta[k] as i32));
        }
        let shifted =
            (v.wrapping_add(1 << (INTER_RESIZE_COEF_BITS * 2 - 1))) >> (INTER_RESIZE_COEF_BITS * 2);
        out[dx] = saturate_cast_u8_i32(shifted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SIMD 2x2 area downscale, whichever vector path this target has.
    ///
    /// Rounding is the fragile part, since halfway cases must round to even, so
    /// besides random images this constructs input covering every possible
    /// block sum in 0..=1020. It reaches whichever of the NEON and wasm paths
    /// is compiled, having no target of its own — though reaching the wasm one
    /// needs a runner `cargo test` does not have.
    #[test]
    fn area_fast_2x2_simd_matches_scalar() {
        // Build an image whose 2x2 block sums cover every value in 0..=1020
        let blocks = 1021usize;
        let (sw, sh) = (blocks * 2, 2usize);
        let mut src = vec![0u8; sw * sh];
        for (b, total) in (0..=1020usize).enumerate() {
            let mut left = total;
            let mut px = [0u8; 4];
            for p in px.iter_mut() {
                let v = left.min(255);
                *p = v as u8;
                left -= v;
            }
            src[b * 2] = px[0];
            src[b * 2 + 1] = px[1];
            src[sw + b * 2] = px[2];
            src[sw + b * 2 + 1] = px[3];
        }
        let got = resize(&src, sw, sh, blocks, 1, Interpolation::Area);
        for (b, &g) in got.iter().enumerate() {
            let sum = src[b * 2] as i32
                + src[b * 2 + 1] as i32
                + src[sw + b * 2] as i32
                + src[sw + b * 2 + 1] as i32;
            let want = saturate_cast_u8_f32(sum as f32 * 0.25);
            assert_eq!(g, want, "block sum = {sum}");
        }

        // Then compare on random images through the full resize, including the edge tail
        let mut state = 0x1234_5678_9ABC_DEF0u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        // Only exact 2x sizes: a slightly different ratio such as 641 to 320
        // fails is_area_fast and takes the generic area path, which does a real
        // weighted average and is not what this test covers
        for (sw, sh) in [(1280usize, 720usize), (642, 362), (16, 8)] {
            let src: Vec<u8> = (0..sw * sh).map(|_| (next() % 256) as u8).collect();
            let (dw, dh) = (sw / 2, sh / 2);
            let got = resize(&src, sw, sh, dw, dh, Interpolation::Area);
            let mut want = vec![0u8; dw * dh];
            for dy in 0..dh {
                for dx in 0..dw {
                    let sum = src[(dy * 2) * sw + dx * 2] as i32
                        + src[(dy * 2) * sw + dx * 2 + 1] as i32
                        + src[(dy * 2 + 1) * sw + dx * 2] as i32
                        + src[(dy * 2 + 1) * sw + dx * 2 + 1] as i32;
                    want[dy * dw + dx] = saturate_cast_u8_f32(sum as f32 * 0.25);
                }
            }
            assert_eq!(got, want, "{sw}x{sh}");
        }
    }

    /// The SIMD horizontal filter must be bit-identical too. This compares the
    /// output of the full `resize` against a purely scalar reference built from
    /// [`hresize_px`] and [`vresize_scalar`], over a range of scale factors and
    /// widths that exercise the boundary rules.
    #[test]
    fn resize_simd_matches_scalar() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let cases = [
            (1280usize, 720usize, 533usize, 300usize),
            (1280, 720, 640, 360),
            (640, 480, 97, 71),
            (97, 71, 640, 480),
            (33, 17, 9, 5),
            (9, 5, 33, 17),
            (5, 5, 4, 4),
        ];
        for (sw, sh, dw, dh) in cases {
            let src: Vec<u8> = (0..sw * sh).map(|_| (next() % 256) as u8).collect();
            for interp in [
                Interpolation::Cubic,
                Interpolation::Linear,
                Interpolation::Area,
            ] {
                let got = resize(&src, sw, sh, dw, dh, interp);
                let want = resize_reference(&src, sw, sh, dw, dh, interp);
                assert_eq!(got, want, "{sw}x{sh}->{dw}x{dh} {interp:?}");
            }
        }
    }

    /// Scalar reference: forces the two SIMD sites onto their scalar paths and
    /// reuses the setup logic of `resize` unchanged.
    fn resize_reference(
        src: &[u8],
        sw: usize,
        sh: usize,
        dw: usize,
        dh: usize,
        interp: Interpolation,
    ) -> Vec<u8> {
        // The two area fast paths have no SIMD, so they are reused directly
        if interp == Interpolation::Area && dw <= sw && dh <= sh {
            return resize(src, sw, sh, dw, dh, interp);
        }
        let inv_scale_x = dw as f64 / sw as f64;
        let inv_scale_y = dh as f64 / sh as f64;
        let (scale_x, scale_y) = (1.0 / inv_scale_x, 1.0 / inv_scale_y);
        let ksize = if interp == Interpolation::Cubic { 4 } else { 2 };
        let cubic = interp == Interpolation::Cubic;
        let ksize2 = (ksize / 2) as i32;

        // Same coefficient tables as resize_generic; computed here only to obtain
        // xofs/ialpha/yofs/ibeta
        let mut xofs = vec![0i32; dw];
        let mut ialpha = vec![0i16; dw * ksize];
        let (mut xmin, mut xmax) = (0usize, dw);
        let area_mode = interp == Interpolation::Area;
        for dx in 0..dw {
            let (mut sx, mut fx);
            if !area_mode {
                let f = ((dx as f64 + 0.5) * scale_x - 0.5) as f32;
                sx = cv_floor(f as f64);
                fx = f - sx as f32;
            } else {
                sx = cv_floor(dx as f64 * scale_x);
                let f = ((dx as f64 + 1.0) - (sx as f64 + 1.0) * inv_scale_x) as f32;
                fx = if f <= 0.0 {
                    0.0
                } else {
                    f - cv_floor(f as f64) as f32
                };
            }
            if sx < ksize2 - 1 {
                xmin = dx + 1;
                if sx < 0 && !cubic {
                    fx = 0.0;
                    sx = 0;
                }
            }
            if sx + ksize2 >= sw as i32 {
                xmax = xmax.min(dx);
                if sx >= sw as i32 - 1 && !cubic {
                    fx = 0.0;
                    sx = sw as i32 - 1;
                }
            }
            xofs[dx] = sx;
            let cbuf = if cubic {
                interpolate_cubic(fx)
            } else {
                [1.0 - fx, fx, 0.0, 0.0]
            };
            for k in 0..ksize {
                ialpha[dx * ksize + k] = saturate_cast_i16(cbuf[k] * INTER_RESIZE_COEF_SCALE);
            }
        }
        let mut yofs = vec![0i32; dh];
        let mut ibeta = vec![0i16; dh * ksize];
        for dy in 0..dh {
            let (sy, fy);
            if !area_mode {
                let f = ((dy as f64 + 0.5) * scale_y - 0.5) as f32;
                sy = cv_floor(f as f64);
                fy = f - sy as f32;
            } else {
                sy = cv_floor(dy as f64 * scale_y);
                let f = ((dy as f64 + 1.0) - (sy as f64 + 1.0) * inv_scale_y) as f32;
                fy = if f <= 0.0 {
                    0.0
                } else {
                    f - cv_floor(f as f64) as f32
                };
            }
            yofs[dy] = sy;
            let cbuf = if cubic {
                interpolate_cubic(fy)
            } else {
                [1.0 - fy, fy, 0.0, 0.0]
            };
            for k in 0..ksize {
                ibeta[dy * ksize + k] = saturate_cast_i16(cbuf[k] * INTER_RESIZE_COEF_SCALE);
            }
        }

        let mut dst = vec![0u8; dw * dh];
        let mut rows = vec![vec![0i32; dw]; ksize];
        for dy in 0..dh {
            for k in 0..ksize {
                let sy = clip(yofs[dy] - ksize2 + 1 + k as i32, 0, sh as i32) as usize;
                let s = &src[sy * sw..(sy + 1) * sw];
                for dx in 0..dw {
                    let edge = if cubic {
                        dx < xmin || dx >= xmax
                    } else {
                        dx >= xmax
                    };
                    rows[k][dx] = hresize_px(s, &xofs, &ialpha, ksize, cubic, sw, dx, edge);
                }
            }
            let beta = &ibeta[dy * ksize..dy * ksize + ksize];
            vresize_scalar(&rows, ksize, beta, &mut dst[dy * dw..(dy + 1) * dw]);
        }
        dst
    }

    /// The SIMD vertical filter must be bit-identical to the scalar one. It
    /// replaces the hottest part of the pipeline, and a one-LSB difference
    /// changes the detector input.
    #[test]
    fn vresize_simd_matches_scalar() {
        // xorshift pseudo-random data covering positive, negative, near-overflow
        // and saturating values
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for ksize in [2usize, 4] {
            for width in [1usize, 7, 8, 9, 16, 533, 1024] {
                let rows: Vec<Vec<i32>> = (0..ksize)
                    .map(|_| {
                        (0..width)
                            .map(|_| {
                                // Cover the normal range (0..255*2048*ksize) and extremes
                                match next() % 8 {
                                    0 => i32::MAX,
                                    1 => i32::MIN,
                                    2 => 0,
                                    _ => {
                                        (next() % (255 * 2048 * 4)) as i32
                                            * if next() % 4 == 0 { -1 } else { 1 }
                                    }
                                }
                            })
                            .collect()
                    })
                    .collect();
                let beta: Vec<i16> = (0..ksize)
                    .map(|_| (next() % (2 * 2048)) as i16 - 2048)
                    .collect();

                let mut a = vec![0u8; width];
                let mut b = vec![0u8; width];
                vresize(&rows, ksize, &beta, &mut a);
                vresize_scalar(&rows, ksize, &beta, &mut b);
                assert_eq!(a, b, "ksize={ksize} width={width}");
            }
        }
    }
}
