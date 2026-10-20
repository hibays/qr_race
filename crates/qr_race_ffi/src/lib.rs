//! C ABI for the Flutter client.
//!
//! Desktop builds include the camera scanner (frames never cross the boundary;
//! Dart polls hits, stats and a low-rate JPEG preview) and native QR rendering.
//! Android builds currently include QR rendering plus a scanner stub; the
//! native camera backend for mobile is still pending.

#[cfg(feature = "desktop-camera")]
mod camera;
#[cfg(all(not(feature = "desktop-camera"), target_os = "android"))]
mod camera_mobile;
#[cfg(all(not(feature = "desktop-camera"), not(target_os = "android")))]
mod camera_stub;
#[cfg(not(target_os = "android"))]
mod desktop_server;
#[cfg(feature = "desktop-camera")]
mod scanner;

#[cfg(feature = "desktop-camera")]
pub use camera::{
    qr_scanner_preview, qr_scanner_start, qr_scanner_start_backend, qr_scanner_state,
    qr_scanner_stop,
};
#[cfg(all(not(feature = "desktop-camera"), target_os = "android"))]
pub use camera_mobile::{
    qr_scanner_preview, qr_scanner_start, qr_scanner_start_backend, qr_scanner_state,
    qr_scanner_stop,
};
#[cfg(all(not(feature = "desktop-camera"), not(target_os = "android")))]
pub use camera_stub::{qr_scanner_preview, qr_scanner_start, qr_scanner_state, qr_scanner_stop};

use std::path::PathBuf;
use std::sync::Mutex;

use qr_race_embedded::TlsMode;

#[cfg(not(target_os = "android"))]
/// Starts the desktop server through the bridge with Windows job-object
/// ownership. Returns 0 on success.
///
/// # Safety
/// `binary`/`config` must point to their stated readable UTF-8 byte lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_desktop_server_start(
    binary: *const u8,
    binary_len: usize,
    config: *const u8,
    config_len: usize,
) -> i32 {
    let Some(binary) = (unsafe { read_utf8(binary, binary_len) }) else {
        return -1;
    };
    let Some(config) = (unsafe { read_utf8(config, config_len) }) else {
        return -1;
    };
    match desktop_server::start(std::path::Path::new(binary), std::path::Path::new(config)) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("qr_desktop_server_start: {error}");
            -1
        }
    }
}

#[cfg(not(target_os = "android"))]
#[unsafe(no_mangle)]
pub extern "C" fn qr_desktop_server_stop() {
    desktop_server::stop();
}

#[cfg(not(target_os = "android"))]
#[unsafe(no_mangle)]
pub extern "C" fn qr_desktop_server_status() -> i32 {
    desktop_server::status()
}

#[cfg(not(target_os = "android"))]
/// Writes the SHA-256 fingerprint of a PEM certificate to `out`.
///
/// # Safety
/// `path` must point to `path_len` readable bytes; `out` to `capacity` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_cert_fingerprint(
    path: *const u8,
    path_len: usize,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    let Some(path) = (unsafe { read_utf8(path, path_len) }) else {
        return -1;
    };
    match qr_race_tls::fingerprint_pem_file(std::path::Path::new(path)) {
        Some(fingerprint) => write_out(fingerprint.as_bytes(), out, capacity),
        None => -1,
    }
}

#[cfg(not(target_os = "android"))]
unsafe fn read_utf8<'a>(pointer: *const u8, length: usize) -> Option<&'a str> {
    if pointer.is_null() || length == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
    std::str::from_utf8(bytes).ok()
}

static EMBEDDED_SERVER: Mutex<Option<qr_race_embedded::EmbeddedServer>> = Mutex::new(None);
static LAST_SERVER_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Last embedded-server startup error as UTF-8. Returns its length,
/// `-(needed)` when the buffer is too small, or -1 when there is no error.
///
/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_server_last_error(out: *mut u8, capacity: usize) -> i32 {
    let message = match LAST_SERVER_ERROR.lock() {
        Ok(slot) => slot.clone(),
        Err(_) => None,
    };
    match message {
        Some(message) => write_out(message.as_bytes(), out, capacity),
        None => -1,
    }
}

#[derive(serde::Deserialize)]
struct ServerLaunch {
    #[serde(default = "default_embedded_port")]
    port: u16,
    #[serde(default)]
    tls: Option<ServerTlsConfig>,
}

#[derive(serde::Deserialize)]
struct ServerTlsConfig {
    mode: String,
    #[serde(default)]
    cert: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    dir: Option<String>,
}

fn default_embedded_port() -> u16 {
    8443
}

/// Starts the in-process server on 127.0.0.1:`port` (0 = OS-assigned) with
/// plain HTTP. Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn qr_server_start(port: u32) -> i32 {
    let port = u16::try_from(port).unwrap_or(0);
    start_embedded(port, TlsMode::None)
}

/// Starts the embedded server from a JSON config:
/// `{"port":8443,"tls":{"mode":"none|self-signed|files","cert":"...","key":"...","dir":"..."}}`.
/// Returns 0 on success.
///
/// # Safety
/// `config` must point to `config_len` readable UTF-8 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_server_start_config(config: *const u8, config_len: usize) -> i32 {
    if config.is_null() || config_len == 0 {
        return -1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(config, config_len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return -1;
    };
    let Ok(launch) = serde_json::from_str::<ServerLaunch>(text) else {
        return -1;
    };
    let tls = match launch.tls {
        None => TlsMode::None,
        Some(tls) if tls.mode == "none" => TlsMode::None,
        Some(tls) if tls.mode == "self-signed" || tls.mode == "selfsigned" => TlsMode::SelfSigned {
            directory: PathBuf::from(tls.dir.unwrap_or_else(|| "qr_race_selfhost".to_string())),
        },
        Some(tls) if tls.mode == "files" => {
            let (Some(cert), Some(key)) = (tls.cert, tls.key) else {
                return -1;
            };
            TlsMode::Files {
                cert: PathBuf::from(cert),
                key: PathBuf::from(key),
            }
        }
        Some(_) => return -1,
    };
    start_embedded(launch.port, tls)
}

fn start_embedded(port: u16, tls: TlsMode) -> i32 {
    qr_server_stop();
    match qr_race_embedded::EmbeddedServer::start(port, tls) {
        Ok(server) => {
            if let Ok(mut slot) = EMBEDDED_SERVER.lock() {
                *slot = Some(server);
            }
            if let Ok(mut slot) = LAST_SERVER_ERROR.lock() {
                *slot = None;
            }
            0
        }
        Err(error) => {
            eprintln!("qr_server_start: {error}");
            if let Ok(mut slot) = LAST_SERVER_ERROR.lock() {
                *slot = Some(error);
            }
            -1
        }
    }
}

/// Writes `{url, fingerprint, host_token, host_secret, member_token,
/// member_secret}` JSON. Returns the byte length, or `-(needed)` when the
/// buffer is too small.
///
/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_server_info(out: *mut u8, capacity: usize) -> i32 {
    let json = match EMBEDDED_SERVER.lock() {
        Ok(slot) => slot.as_ref().and_then(|server| {
            let info = server.info();
            serde_json::to_vec(&serde_json::json!({
                "url": info.url,
                "fingerprint": info.fingerprint,
                "host_token": info.host_token,
                "host_secret": info.host_secret,
                "member_token": info.member_token,
                "member_secret": info.member_secret,
            }))
            .ok()
        }),
        Err(_) => None,
    };
    match json {
        Some(bytes) => write_out(&bytes, out, capacity),
        None => -1,
    }
}

/// Stops the embedded server and waits for its thread.
#[unsafe(no_mangle)]
pub extern "C" fn qr_server_stop() {
    let server = EMBEDDED_SERVER.lock().ok().and_then(|mut slot| slot.take());
    if let Some(server) = server {
        server.stop();
    }
}

/// Renders a 2048×2048 lossless WebP QR with qrcode. Returns the WebP length
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
