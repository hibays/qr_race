//! C ABI for the Flutter client.
//!
//! Desktop builds include the camera scanner (frames never cross the boundary;
//! Dart polls hits, stats and a low-rate JPEG preview) and rxing QR rendering.
//! Android builds currently include QR rendering plus a scanner stub; the
//! native camera backend for mobile is still pending.

#[cfg(feature = "desktop-camera")]
mod camera;
#[cfg(not(feature = "desktop-camera"))]
mod camera_stub;
#[cfg(feature = "desktop-camera")]
mod scanner;

#[cfg(feature = "desktop-camera")]
pub use camera::{qr_scanner_preview, qr_scanner_start, qr_scanner_state, qr_scanner_stop};
#[cfg(not(feature = "desktop-camera"))]
pub use camera_stub::{qr_scanner_preview, qr_scanner_start, qr_scanner_state, qr_scanner_stop};

/// Renders a 2048×2048 lossless WebP QR with rxing. Returns the WebP length
/// written to `out`, a negative needed length when the buffer is too small, or
/// -1 on bad input.
///
/// # Safety
/// `text` must point to `text_len` readable bytes; `out` must point to
/// `capacity` writable bytes (null is allowed only when `capacity` is zero).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_qr_webp(
    text: *const u8,
    text_len: usize,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    if text.is_null() || text_len == 0 {
        return -1;
    }
    // SAFETY: the caller passes `text_len` readable bytes at `text`.
    let bytes = unsafe { std::slice::from_raw_parts(text, text_len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return -1;
    };
    let webp = match qr_race_qr::qr_webp(text) {
        Ok(webp) => webp,
        Err(_) => return -1,
    };
    write_out(&webp, out, capacity)
}

/// ABI version; bump when the contract changes.
#[unsafe(no_mangle)]
pub extern "C" fn qr_race_ffi_version() -> u32 {
    1
}

pub(crate) fn write_out(data: &[u8], out: *mut u8, capacity: usize) -> i32 {
    if out.is_null() || capacity == 0 {
        return -1;
    }
    if data.len() > capacity {
        return -(data.len() as i32);
    }
    // SAFETY: the caller owns `capacity` writable bytes at `out`; we checked it
    // is non-null and large enough for `data`.
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), out, data.len());
    }
    data.len() as i32
}
