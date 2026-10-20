#![allow(clippy::all)]
//! Rust port of a few OpenCV `imgproc` functions, with no dependencies.
//!
//! The selection covers what the wechat_qrcode pipeline uses. Nothing here is
//! specific to QR codes, so the crate is usable wherever a small subset of
//! OpenCV is needed without the full dependency. On aarch64 `resize` uses NEON
//! and produces bit-identical output to the scalar path; see the comparison
//! tests in the `resize` module.
//!
//! Only the call shapes used by the original C++ are covered:
//!   - `cvtColor(BGR2GRAY)`          → [`color::bgr_to_gray`]
//!   - `resize(INTER_CUBIC/AREA/LINEAR)` → [`resize`]
//!   - `adaptiveThreshold(ADAPTIVE_THRESH_GAUSSIAN_C, THRESH_BINARY)` → [`threshold::adaptive_threshold_gaussian`]
//!   - `dnn::blobFromImage(1/255, no-mean, no-swapRB)` → [`blob::blob_from_image`]
//!
//! Numeric behavior follows OpenCV: `cvRound` rounds halfway cases to even and
//! `saturate_cast<uchar>` uses the same rounding before clamping. Known
//! deviations are documented on the individual functions.

pub mod blob;
pub mod color;
pub mod resize;
pub mod threshold;

pub use resize::{resize, Interpolation};

/// OpenCV `cvRound`: rounds halfway cases to even, matching SSE `cvtsd2si`.
///
/// Implemented with `round_ties_even` from the standard library, which has the
/// same semantics and compiles to a single rounding instruction (`frintn` on
/// ARM, `roundsd` on x86). A branch-based implementation is slower and blocks
/// auto-vectorization of the enclosing loops.
#[inline]
pub fn cv_round(value: f64) -> i32 {
    value.round_ties_even() as i32
}

#[inline]
pub fn cv_floor(value: f64) -> i32 {
    value.floor() as i32
}

#[inline]
pub fn cv_ceil(value: f64) -> i32 {
    value.ceil() as i32
}

/// `saturate_cast<uchar>(float)`
#[inline]
pub fn saturate_cast_u8_f32(v: f32) -> u8 {
    let i = cv_round(v as f64);
    i.clamp(0, 255) as u8
}

/// `saturate_cast<uchar>(int)`
#[inline]
pub fn saturate_cast_u8_i32(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// The `BORDER_REPLICATE` case of OpenCV `borderInterpolate`.
#[inline]
pub fn border_replicate(p: i32, len: i32) -> usize {
    p.clamp(0, len - 1) as usize
}

/// The `BORDER_REFLECT_101` case of OpenCV `borderInterpolate`, the default for resize.
pub fn border_reflect101(mut p: i32, len: i32) -> usize {
    if len == 1 {
        return 0;
    }
    loop {
        if p < 0 {
            p = -p;
        } else if p >= len {
            p = 2 * (len - 1) - p;
        } else {
            return p as usize;
        }
    }
}
