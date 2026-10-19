//! C ABI for the Flutter client.
//!
//! The scanner owns the camera on a native thread. Dart only polls state (hits
//! with exact bytes, stats, error) and a low-rate JPEG preview; camera frames
//! never cross the boundary.

mod scanner;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use qr_race_capture_desktop::{CameraConfig, CameraSource};
use serde::Serialize;

pub use scanner::{SharedState, preview_jpeg, run_source};

struct Session {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<SharedState>>,
    join: Option<std::thread::JoinHandle<()>>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

#[derive(Serialize)]
struct StateDto<'a> {
    running: bool,
    error: Option<&'a str>,
    stats: &'a scanner::ScanStats,
    hits: &'a [scanner::ScanHitDto],
    preview_seq: u64,
    source: Option<&'a str>,
    width: u32,
    height: u32,
}

/// Starts (or restarts) the native scanner. Returns 0 when the thread started;
/// camera failures surface through `qr_scanner_state`.
#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_start(camera: u32, width: u32, height: u32, fps: u32) -> i32 {
    qr_scanner_stop();

    let shared = Arc::new(Mutex::new(SharedState::new()));
    if let Ok(mut state) = shared.lock() {
        state.running = true;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let thread_shared = shared.clone();
    let thread_stop = stop.clone();

    let join = std::thread::Builder::new()
        .name("qr-scanner".to_string())
        .spawn(move || {
            match CameraSource::open_with(camera, CameraConfig { width, height, fps }) {
                Ok(source) => run_source(
                    source,
                    thread_shared.clone(),
                    thread_stop,
                    width,
                    height,
                    fps,
                ),
                Err(error) => {
                    if let Ok(mut state) = thread_shared.lock() {
                        state.error = Some(error.to_string());
                        state.running = false;
                    }
                }
            }
        });

    match join {
        Ok(join) => {
            if let Ok(mut session) = SESSION.lock() {
                *session = Some(Session {
                    stop,
                    shared,
                    join: Some(join),
                });
            }
            0
        }
        Err(error) => {
            eprintln!("qr_scanner_start: could not spawn thread: {error}");
            1
        }
    }
}

/// Stops the scanner and waits for its thread to finish.
#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_stop() {
    let session = SESSION.lock().ok().and_then(|mut slot| slot.take());
    if let Some(mut session) = session {
        session.stop.store(true, Ordering::SeqCst);
        if let Some(join) = session.join.take() {
            let _ = join.join();
        }
    }
}

/// Writes the current scanner state as UTF-8 JSON. Returns the byte length, or
/// `-(needed)` when the buffer is too small, or -1 for a bad pointer.
///
/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_state(out: *mut u8, capacity: usize) -> i32 {
    let json = match SESSION.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(session) => match session.shared.lock() {
                Ok(state) => serde_json::to_vec(&StateDto {
                    running: state.running,
                    error: state.error.as_deref(),
                    stats: &state.stats,
                    hits: &state.hits,
                    preview_seq: state.preview_seq,
                    source: state.source.as_deref(),
                    width: state.width,
                    height: state.height,
                }),
                Err(_) => return -2,
            },
            None => {
                let empty = SharedState::new();
                serde_json::to_vec(&StateDto {
                    running: false,
                    error: None,
                    stats: &empty.stats,
                    hits: &empty.hits,
                    preview_seq: 0,
                    source: None,
                    width: 0,
                    height: 0,
                })
            }
        },
        Err(_) => return -2,
    };
    match json {
        Ok(bytes) => write_out(&bytes, out, capacity),
        Err(_) => -2,
    }
}

/// Copies the latest JPEG preview into `out`. Returns its length, `-(needed)`
/// if the buffer is too small, or -1 when there is no preview yet.
///
/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_preview(out: *mut u8, capacity: usize) -> i32 {
    let preview = match SESSION.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(session) => match session.shared.lock() {
                Ok(state) => state.preview.clone(),
                Err(_) => return -2,
            },
            None => Vec::new(),
        },
        Err(_) => return -2,
    };
    if preview.is_empty() {
        return -1;
    }
    write_out(&preview, out, capacity)
}

/// ABI version; bump when the contract changes.
#[unsafe(no_mangle)]
pub extern "C" fn qr_race_ffi_version() -> u32 {
    1
}

fn write_out(data: &[u8], out: *mut u8, capacity: usize) -> i32 {
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
