//! Decoding delegated to the host, for formats this library does not carry.
//!
//! The built-in decoders are png, jpeg and gif: what a photo picker writes,
//! and about as much as is worth linking. Everything else a caller might
//! actually be handed — HEIC above all, and AVIF, and a camera's raw files —
//! wants a decoder measured in megabytes, encumbered by patents, or both.
//!
//! Every platform this runs on already has one, and it is better than anything
//! that could be linked here:
//!
//! | | |
//! |---|---|
//! | Apple | `CGImageSource`: HEIC, AVIF, raw, whatever the system knows |
//! | Android | `ImageDecoder`, which has read HEIF since API 28 |
//! | Browser | `createImageBitmap` — used already, from Dart, not through here |
//!
//! So rather than growing this library, a host can lend it one. The decoder is
//! consulted only when the built-in ones have declined, so registering it can
//! never change how a png is read, and a host that registers nothing gets
//! exactly the behaviour it had before.
//!
//! # Ownership
//!
//! `decode` hands back a buffer it still owns, and `release` is called with
//! that same pointer once the pixels have been copied out. Nothing is
//! allocated across the boundary in either direction, which is what makes this
//! safe to implement from a garbage-collected language: the host can pin a
//! byte array for the length of one call and let go of it in `release`.

use std::sync::Mutex;

use crate::c_api::WxScanPixelFormat;

/// A decoder the host lends to this library.
///
/// Both function pointers are required; a struct with either missing is
/// rejected rather than half-installed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WxScanImageDecoder {
    /// Decode `data`, which is an encoded image file.
    ///
    /// Returns 1 on success, having written a pixel buffer and its shape
    /// through the out parameters, and 0 for anything it does not recognise —
    /// which is not an error, only an answer.
    ///
    /// `out_format` is a [`WxScanPixelFormat`]. Rows must be tightly packed.
    /// The orientation recorded in the file is the host's to apply, since the
    /// system decoders do it as a matter of course and this library cannot
    /// tell whether it happened.
    pub decode: Option<
        unsafe extern "C" fn(
            data: *const u8,
            len: usize,
            out_pixels: *mut *const u8,
            out_width: *mut u32,
            out_height: *mut u32,
            out_format: *mut i32,
            ctx: *mut std::ffi::c_void,
        ) -> i32,
    >,

    /// Release a buffer a successful `decode` handed over. Called exactly once
    /// per success, before the scan returns.
    pub release: Option<
        unsafe extern "C" fn(pixels: *const u8, ctx: *mut std::ffi::c_void),
    >,

    /// Passed back to both functions untouched.
    pub ctx: *mut std::ffi::c_void,
}

// The pointers are the host's, and this library only ever passes `ctx` back to
// the host that gave it. Whether it is safe to use from several threads is
// therefore the host's question, not this one's, and the documentation on
// `wxscan_set_image_decoder` puts it to them.
unsafe impl Send for WxScanImageDecoder {}

static DECODER: Mutex<Option<WxScanImageDecoder>> = Mutex::new(None);

/// Lend this library a decoder for formats it does not carry, or pass NULL to
/// take one back.
///
/// It is consulted only after the built-in decoders have declined, so it
/// cannot change how a png, jpeg or gif is read. A caller that registers
/// nothing keeps exactly the previous behaviour: unknown bytes come back as
/// `WxScanStatus::UnsupportedFormat`.
///
/// Register once at start-up. The functions may be called from any thread the
/// caller scans on, and from more than one at a time, so they must be safe to
/// call that way — the system decoders named in this module's documentation
/// all are.
///
/// # Safety
/// `decoder`, when not NULL, must point to a readable [`WxScanImageDecoder`]
/// whose function pointers, and the `ctx` beside them, stay valid **for the
/// life of the process** — not merely until they are replaced or cleared.
///
/// Replacing or clearing a registration does not wait for decodes already
/// under way. A decode that has read the slot but not yet called through it
/// will still call the retired pointers with the retired `ctx`, so a host that
/// frees that context, drops a reference the context holds, or unloads the
/// code behind those pointers once this returns has a use-after-free. There is
/// nowhere to put a wait: the alternative is holding the registration lock
/// across the decode, which would serialise every picture in the process
/// behind every other.
///
/// Registering once at start-up and leaving it, which is what a decoder built
/// into an application does, sidesteps this entirely.
#[no_mangle]
pub unsafe extern "C" fn wxscan_set_image_decoder(decoder: *const WxScanImageDecoder) {
    let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
    *slot = match decoder.as_ref() {
        // Half a decoder is not a decoder; taking it would mean checking for
        // the missing half on every picture instead of once, here.
        Some(d) if d.decode.is_some() && d.release.is_some() => Some(*d),
        _ => None,
    };
}

/// Ask the host to decode `data`, and reduce what comes back to grayscale.
///
/// Returns the image and its dimensions, or None when there is no decoder or
/// the one there is did not recognise the bytes.
pub(crate) fn decode_with_host(data: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let decoder = (*DECODER.lock().unwrap_or_else(|e| e.into_inner()))?;
    let (decode, release) = (decoder.decode?, decoder.release?);

    let mut pixels: *const u8 = std::ptr::null();
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut format: i32 = 0;

    // SAFETY: the pointers are ours and writable, and `data` outlives the call.
    let ok = unsafe {
        decode(
            data.as_ptr(),
            data.len(),
            &mut pixels,
            &mut width,
            &mut height,
            &mut format,
            decoder.ctx,
        )
    };
    if ok == 0 || pixels.is_null() || width == 0 || height == 0 {
        return None;
    }

    // From here the buffer is ours to read and must be released on every path
    // out, including the ones that reject what the host said.
    let result = (|| {
        let format = WxScanPixelFormat::from_raw(format)?;
        let (w, h) = (width as usize, height as usize);
        let len = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(format.bytes_per_pixel()))?;
        // SAFETY: the host has just told us this buffer is w * h * bpp bytes.
        let src = unsafe { std::slice::from_raw_parts(pixels, len) };
        let gray = match format {
            WxScanPixelFormat::Gray => src.to_vec(),
            WxScanPixelFormat::Rgb => cvlite::color::rgb_to_gray(src, w, h),
            WxScanPixelFormat::Rgba => cvlite::color::rgba_to_gray(src, w, h),
            WxScanPixelFormat::Bgr => cvlite::color::bgr_to_gray(src, w, h),
            WxScanPixelFormat::Bgra => cvlite::color::bgra_to_gray(src, w, h),
        };
        Some((gray, w, h))
    })();

    // SAFETY: `pixels` is what a successful decode returned, released once.
    unsafe { release(pixels, decoder.ctx) };
    result
}
