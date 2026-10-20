//! Scanner ABI for platforms without the desktop camera backend (Android,
//! for now). QR rendering remains available; the scan page reports the error.

use crate::write_out;

const STATE: &str = r#"{"running":false,"error":"安卓相机采集尚未接入；请在 Windows/Web 版扫码","stats":{"frames":0,"scan_ms_last":0.0,"frames_in":0,"frames_out":0,"dropped":0},"hits":[],"preview_seq":0,"source":null,"width":0,"height":0}"#;

#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_start(_camera: u32, _width: u32, _height: u32, _fps: u32) -> i32 {
    // Start succeeds so the Dart session polls the explanatory state instead of
    // showing a low-level ABI return code.
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_stop() {}

/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_state(out: *mut u8, capacity: usize) -> i32 {
    write_out(STATE.as_bytes(), out, capacity)
}

/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_preview(_out: *mut u8, _capacity: usize) -> i32 {
    -1
}
