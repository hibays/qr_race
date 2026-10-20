//! Port of `cv::cvtColor(COLOR_BGR2GRAY)`.
//!
//! The OpenCV 8U path is fixed point:
//! `(B*B2Y + G*G2Y + R*R2Y + (1 << (yuv_shift-1))) >> yuv_shift`.
//!
//! These are OpenCV's 15-bit coefficients — `RY15`, `GY15`, `BY15` in
//! `color_yuv.simd.hpp` — and they sum to exactly `1 << 15`, which is what
//! makes a grey pixel come back as itself. They were once paired with a shift
//! of 14, which doubled every luminance and then wrapped it, so mid grey came
//! out black. There is a test for that below now.
//!
//! With the two in step nothing can leave the 0..=255 range, so the clamp in
//! [`luma`] never fires; it is there to keep a future mismatch from wrapping
//! silently the way that one did.

const B2Y: i32 = 3735;
const G2Y: i32 = 19235;
const R2Y: i32 = 9798;
const YUV_SHIFT: i32 = 15;

/// One pixel, the way OpenCV rounds it.
#[inline]
fn luma(b: i32, g: i32, r: i32) -> u8 {
    (((b * B2Y + g * G2Y + r * R2Y + (1 << (YUV_SHIFT - 1))) >> YUV_SHIFT).clamp(0, 255)) as u8
}

/// Interleaved BGR, 3 bytes per pixel, to grayscale.
pub fn bgr_to_gray(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut dst = vec![0u8; width * height];
    for i in 0..width * height {
        let b = src[i * 3] as i32;
        let g = src[i * 3 + 1] as i32;
        let r = src[i * 3 + 2] as i32;
        dst[i] = luma(b, g, r);
    }
    dst
}

/// Interleaved RGB to grayscale, same coefficients with the channel order reversed.
pub fn rgb_to_gray(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut dst = vec![0u8; width * height];
    for i in 0..width * height {
        let r = src[i * 3] as i32;
        let g = src[i * 3 + 1] as i32;
        let b = src[i * 3 + 2] as i32;
        dst[i] = luma(b, g, r);
    }
    dst
}

/// Interleaved RGBA to grayscale.
pub fn rgba_to_gray(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut dst = vec![0u8; width * height];
    for i in 0..width * height {
        let r = src[i * 4] as i32;
        let g = src[i * 4 + 1] as i32;
        let b = src[i * 4 + 2] as i32;
        dst[i] = luma(b, g, r);
    }
    dst
}

/// Interleaved BGRA to grayscale.
///
/// BGR with an ignored alpha, read in one pass rather than by copying the
/// buffer to drop the fourth channel. It lives here rather than in a caller so
/// that all five layouts share one implementation of the arithmetic, and one
/// test of it.
pub fn bgra_to_gray(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut dst = vec![0u8; width * height];
    for (out, px) in dst.iter_mut().zip(src.chunks_exact(4)) {
        *out = luma(px[0] as i32, px[1] as i32, px[2] as i32);
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grey pixel is its own luminance. This is what the coefficients summing
    /// to `1 << YUV_SHIFT` means, and it fails for every level from 1 to 255 if
    /// the two ever drift apart again.
    #[test]
    fn grey_survives_the_round_trip() {
        for level in 0..=255u8 {
            let rgb = [level, level, level];
            assert_eq!(rgb_to_gray(&rgb, 1, 1), vec![level], "level {level}");
            assert_eq!(bgr_to_gray(&rgb, 1, 1), vec![level], "level {level}");
            let rgba = [level, level, level, 255];
            assert_eq!(rgba_to_gray(&rgba, 1, 1), vec![level], "level {level}");
            assert_eq!(bgra_to_gray(&rgba, 1, 1), vec![level], "level {level}");
        }
    }

    /// The coefficients OpenCV uses, against values it produces.
    #[test]
    fn matches_opencv() {
        assert_eq!(B2Y + G2Y + R2Y, 1 << YUV_SHIFT);
        // Pure channels, at OpenCV's rounding.
        assert_eq!(rgb_to_gray(&[255, 0, 0], 1, 1), vec![76]);
        assert_eq!(rgb_to_gray(&[0, 255, 0], 1, 1), vec![150]);
        assert_eq!(rgb_to_gray(&[0, 0, 255], 1, 1), vec![29]);
        // Channel order is the only difference between the two.
        assert_eq!(bgr_to_gray(&[255, 0, 0], 1, 1), vec![29]);
        // Alpha is ignored.
        assert_eq!(rgba_to_gray(&[255, 0, 0, 0], 1, 1), vec![76]);
        assert_eq!(bgra_to_gray(&[255, 0, 0, 0], 1, 1), vec![29]);
    }
}
