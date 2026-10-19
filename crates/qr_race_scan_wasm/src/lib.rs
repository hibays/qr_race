//! Browser build of the scan core over `wasm32-unknown-unknown`.
//!
//! Two entry points share one implementation: the C ABI (`qr_wasm_*`, for
//! hand-rolled JS or other hosts) and the wasm-bindgen export (`scan_gray`,
//! which handles the frame copy and returns JSON). The wasm-bindgen path is what
//! the generated glue and the Flutter web app use.

use std::sync::Mutex;

use data_encoding::BASE64;
use qr_race_scan::{GrayFrame, RxingDecoder, ScanEngine};
use serde::Serialize;
use wasm_bindgen::prelude::*;

static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

#[derive(Serialize)]
struct HitDto {
    text: String,
    raw_b64: String,
}

#[derive(Serialize)]
struct ScanDto {
    width: u32,
    height: u32,
    scan_ms: f64,
    hits: Vec<HitDto>,
    error: Option<String>,
}

impl ScanDto {
    fn error(width: u32, height: u32, message: String) -> Self {
        Self {
            width,
            height,
            scan_ms: 0.0,
            hits: Vec::new(),
            error: Some(message),
        }
    }
}

fn scan_frame(width: u32, height: u32, data: Vec<u8>) -> ScanDto {
    match GrayFrame::new(width, height, data) {
        Ok(frame) => {
            let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
            match engine.scan(&frame) {
                Ok(outcome) => ScanDto {
                    width,
                    height,
                    scan_ms: outcome.timings.total.as_secs_f64() * 1000.0,
                    hits: outcome
                        .hits
                        .into_iter()
                        .map(|hit| HitDto {
                            text: hit.text,
                            raw_b64: BASE64.encode(&hit.raw),
                        })
                        .collect(),
                    error: None,
                },
                Err(error) => ScanDto::error(width, height, error.to_string()),
            }
        }
        Err(error) => ScanDto::error(width, height, error.to_string()),
    }
}

/// wasm-bindgen entry point. Copies the grayscale frame in and returns the JSON
/// result; the generated glue handles both directions of the copy.
#[wasm_bindgen]
pub fn scan_gray(width: u32, height: u32, data: &[u8]) -> String {
    let report = scan_frame(width, height, data.to_vec());
    serde_json::to_string(&report)
        .unwrap_or_else(|error| serde_json::json!({ "error": error.to_string() }).to_string())
}

#[wasm_bindgen]
pub fn version() -> u32 {
    1
}

/// Allocates `len` bytes inside wasm memory for the caller to fill.
#[unsafe(no_mangle)]
pub extern "C" fn qr_wasm_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Frees a buffer obtained from `qr_wasm_alloc`; `len` must be the same value.
///
/// # Safety
/// `ptr` must come from `qr_wasm_alloc(len)` and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_wasm_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller contract above matches how the buffer was allocated.
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

/// Scans a grayscale frame of `width * height` bytes at `ptr`. Returns the JSON
/// result length; read it with `qr_wasm_result_ptr` immediately after.
///
/// # Safety
/// `ptr` must point to at least `width * height` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_wasm_scan_gray(ptr: *const u8, width: u32, height: u32) -> i32 {
    let length = width as usize * height as usize;
    let data = if ptr.is_null() || length == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller passes a buffer of width*height bytes.
        unsafe { std::slice::from_raw_parts(ptr, length) }.to_vec()
    };

    let report = scan_frame(width, height, data);

    let bytes = serde_json::to_vec(&report).unwrap_or_default();
    match RESULT.lock() {
        Ok(mut result) => {
            *result = bytes;
            result.len() as i32
        }
        Err(_) => -1,
    }
}

/// Pointer to the last JSON result; valid until the next `qr_wasm_scan_gray`.
#[unsafe(no_mangle)]
pub extern "C" fn qr_wasm_result_ptr() -> *const u8 {
    match RESULT.lock() {
        Ok(result) => result.as_ptr(),
        Err(_) => std::ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn qr_wasm_version() -> u32 {
    1
}
