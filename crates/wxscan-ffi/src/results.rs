//! Owned result types handed to the C side, and their deallocation.
//!
//! Every pointer inside [`WxScanResults`] is owned by that allocation and stays
//! valid until [`wxscan_results_free`] is called on it. Strings are NUL
//! terminated so they can be used directly by C, while `bytes` keeps the raw
//! payload because QR content is not required to be valid UTF-8.

use std::ffi::{c_char, CString};

use wxscan::QRCodeResult;

/// One decoded symbol.
#[repr(C)]
pub struct WxScanResult {
    /// Raw payload bytes, in the encoding named by `charset`.
    pub bytes: *const u8,
    pub bytes_len: usize,
    /// Payload decoded to UTF-8; GB2312 payloads are converted, everything else
    /// is interpreted as UTF-8 with invalid sequences replaced.
    pub text: *const c_char,
    pub charset: *const c_char,
    /// Corner points in the upright frame, ordered top-left, top-right,
    /// bottom-right, bottom-left, stored as `x0, y0, x1, y1, ...`.
    pub points: [f32; 8],
    pub qrcode_version: i32,
    pub ec_level: *const c_char,
    pub charset_mode: *const c_char,
    pub binary_method: i32,
}

/// Result of scanning one frame.
#[repr(C)]
pub struct WxScanResults {
    pub results: *const WxScanResult,
    pub results_len: usize,
    /// Detector candidates, 8 floats per quadrilateral in the same order as
    /// [`WxScanResult::points`]. A non-empty candidate list with an empty
    /// result list means a symbol was located but not decoded.
    pub candidates: *const f32,
    /// Number of quadrilaterals, not the number of floats.
    pub candidates_len: usize,
    /// Dimensions of the upright frame the coordinates refer to.
    pub width: u32,
    pub height: u32,
}

/// Interpret payload bytes according to the reported charset.
pub(crate) fn decode_text(bytes: &[u8], charset: &str) -> String {
    if charset.eq_ignore_ascii_case("GB2312") {
        let (cow, _, _) = encoding_rs::GBK.decode(bytes);
        return cow.into_owned();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Allocate a C string, replacing interior NUL bytes so the conversion cannot fail.
fn c_string(s: &str) -> *const c_char {
    let cleaned = if s.as_bytes().contains(&0) {
        s.replace('\0', "")
    } else {
        s.to_string()
    };
    CString::new(cleaned).unwrap().into_raw()
}

/// # Safety
/// `p` must come from [`c_string`] and must not be freed twice.
unsafe fn free_c_string(p: *const c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p as *mut c_char));
    }
}

/// Move decoded results and detector candidates into a C-owned allocation.
pub(crate) fn into_c(
    results: Vec<QRCodeResult>,
    candidates: Vec<[(f32, f32); 4]>,
    width: u32,
    height: u32,
    flip_x: Option<f32>,
) -> *mut WxScanResults {
    let flip = |x: f32| match flip_x {
        Some(w) => w - x,
        None => x,
    };

    let items: Vec<WxScanResult> = results
        .into_iter()
        .map(|r| {
            let text = decode_text(&r.bytes, &r.charset);
            let mut points = [0f32; 8];
            for (i, (x, y)) in r.points.iter().enumerate() {
                points[i * 2] = flip(*x);
                points[i * 2 + 1] = *y;
            }
            let bytes = r.bytes.into_boxed_slice();
            let bytes_len = bytes.len();
            WxScanResult {
                bytes: Box::into_raw(bytes) as *const u8,
                bytes_len,
                text: c_string(&text),
                charset: c_string(&r.charset),
                points,
                qrcode_version: r.qrcode_version,
                ec_level: c_string(&r.ec_level),
                charset_mode: c_string(&r.charset_mode),
                binary_method: r.binary_method,
            }
        })
        .collect();

    let mut quads = Vec::with_capacity(candidates.len() * 8);
    for q in &candidates {
        for (x, y) in q.iter() {
            quads.push(flip(*x));
            quads.push(*y);
        }
    }

    let results_len = items.len();
    let candidates_len = candidates.len();
    let items = items.into_boxed_slice();
    let quads = quads.into_boxed_slice();

    Box::into_raw(Box::new(WxScanResults {
        results: Box::into_raw(items) as *const WxScanResult,
        results_len,
        candidates: Box::into_raw(quads) as *const f32,
        candidates_len,
        width,
        height,
    }))
}

/// Free a result set and everything it owns. Passing NULL is a no-op.
///
/// # Safety
/// The pointer must come from a scan function of this library and must be freed
/// at most once.
#[no_mangle]
pub unsafe extern "C" fn wxscan_results_free(r: *mut WxScanResults) {
    if r.is_null() {
        return;
    }
    let r = Box::from_raw(r);

    if !r.results.is_null() {
        let items = Box::from_raw(std::slice::from_raw_parts_mut(
            r.results as *mut WxScanResult,
            r.results_len,
        ));
        for it in items.iter() {
            if !it.bytes.is_null() {
                drop(Box::from_raw(std::slice::from_raw_parts_mut(
                    it.bytes as *mut u8,
                    it.bytes_len,
                )));
            }
            free_c_string(it.text);
            free_c_string(it.charset);
            free_c_string(it.ec_level);
            free_c_string(it.charset_mode);
        }
    }

    if !r.candidates.is_null() {
        drop(Box::from_raw(std::slice::from_raw_parts_mut(
            r.candidates as *mut f32,
            r.candidates_len * 8,
        )));
    }
}
