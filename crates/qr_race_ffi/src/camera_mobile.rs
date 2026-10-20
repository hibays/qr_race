//! Android scanner session: Kotlin's Camera2 ImageReader pushes YUV_420_888
//! planes through JNI, Rust converts them to RGB (and keeps luma for CNN/preview)
//! and scans on a dedicated thread. Frames never enter Dart; Dart only polls
//! hits, stats and the low-rate JPEG preview.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use data_encoding::BASE64;
use qr_race_capture::GrayFrame;
use qr_race_scan::{ScanEngine, ZxingDecoder};
use qr_race_wechat::TieredScanner;
use serde::Serialize;

const PREVIEW_INTERVAL: Duration = Duration::from_millis(60);
const PREVIEW_MAX_WIDTH: u32 = 480;
const MAX_HITS: usize = 8;

#[derive(Clone, Serialize)]
struct Hit {
    text: String,
    raw_b64: String,
    at_ms: u64,
}

struct Shared {
    running: bool,
    error: Option<String>,
    frames: u64,
    scans: u64,
    hit_count: u64,
    scan_ms_last: f64,
    scan_ms_total: f64,
    wechat_runs: u64,
    wechat_ms_last: f64,
    last_tier: Option<String>,
    latencies: Vec<f64>,
    max_ms: f64,
    hits: Vec<Hit>,
    preview: Vec<u8>,
    preview_seq: u64,
    width: u32,
    height: u32,
    rotation: i32,
}

struct Frame {
    width: u32,
    height: u32,
    data: Vec<u8>,
    /// Tightly packed RGB, kept so zxing-cpp can use chroma as well as luma.
    color: Option<Vec<u8>>,
    rotation: i32,
}

struct Session {
    tx: SyncSender<Frame>,
    shared: Arc<Mutex<Shared>>,
    join: std::thread::JoinHandle<()>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn start(width: u32, height: u32) -> i32 {
    stop();
    DROPPED.store(0, Ordering::Relaxed);
    let shared = Arc::new(Mutex::new(Shared {
        running: true,
        error: None,
        frames: 0,
        scans: 0,
        hit_count: 0,
        scan_ms_last: 0.0,
        scan_ms_total: 0.0,
        wechat_runs: 0,
        wechat_ms_last: 0.0,
        last_tier: None,
        latencies: Vec::new(),
        max_ms: 0.0,
        hits: Vec::new(),
        preview: Vec::new(),
        preview_seq: 0,
        width,
        height,
        rotation: 0,
    }));
    let (tx, rx) = sync_channel::<Frame>(2);
    let worker_shared = shared.clone();
    let join = match std::thread::Builder::new()
        .name("qr-scanner-android".to_string())
        .spawn(move || {
            let decoder = ZxingDecoder::new();
            let hard_decoder = ZxingDecoder::new().try_harder(true);
            let engine = ScanEngine::new(Box::new(decoder.clone()));
            let tiered = TieredScanner::new().ok();
            let mut frame_seq: u64 = 0;
            let mut last_preview: Option<Instant> = None;
            while let Ok(frame) = rx.recv() {
                let Ok(gray) = GrayFrame::new(frame.width, frame.height, frame.data) else {
                    continue;
                };
                let started = Instant::now();
                frame_seq += 1;
                // The fast path gets RGB from Camera2. On a miss, retry the RGB
                // frame with zxing-cpp's harder reader before waking the CNN;
                // this is the main recognition-rate lever for dim/far codes.
                let mut color_hits = frame
                    .color
                    .as_deref()
                    .and_then(|rgb| decoder.decode_rgb(rgb, frame.width, frame.height).ok())
                    .unwrap_or_default();
                if color_hits.is_empty() {
                    color_hits = frame
                        .color
                        .as_deref()
                        .and_then(|rgb| {
                            hard_decoder.decode_rgb(rgb, frame.width, frame.height).ok()
                        })
                        .unwrap_or_default();
                }
                let outcome = if color_hits.is_empty() {
                    engine.scan(&gray)
                } else {
                    Ok(qr_race_scan::ScanOutcome {
                        hits: color_hits,
                        ..Default::default()
                    })
                };
                match outcome {
                    Ok(outcome) => {
                        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                        let mut hits: Vec<(String, Vec<u8>)> = outcome
                            .hits
                            .into_iter()
                            .map(|hit| (hit.text, hit.raw))
                            .collect();
                        // Tiered fallback: zxing-cpp is the 30 fps fast path; only a
                        // miss (about every other frame) wakes the CNN detector.
                        if hits.is_empty() && frame_seq % 2 == 0 {
                            if let Some(tiered) = &tiered {
                                let cnn_started = Instant::now();
                                // Most CNN passes use 384; every fourth pass
                                // spends the 768 detector for far/small codes.
                                let target = if frame_seq % 8 == 0 { 768 } else { 384 };
                                let cnn = tiered.scan_cnn_at(&gray, target);
                                let cnn_ms = cnn_started.elapsed().as_secs_f64() * 1000.0;
                                hits.extend(cnn.hits.into_iter().map(|hit| (hit.text, hit.raw)));
                                if let Ok(mut state) = worker_shared.lock() {
                                    state.wechat_runs += 1;
                                    state.wechat_ms_last = cnn_ms;
                                    state.last_tier = Some(cnn.tier.to_string());
                                }
                            }
                        }
                        if let Ok(mut state) = worker_shared.lock() {
                            state.frames += 1;
                            state.scans += 1;
                            state.scan_ms_last = elapsed_ms;
                            state.scan_ms_total += elapsed_ms;
                            state.max_ms = state.max_ms.max(elapsed_ms);
                            state.latencies.push(elapsed_ms);
                            if state.latencies.len() > 512 {
                                state.latencies.remove(0);
                            }
                            state.width = frame.width;
                            state.height = frame.height;
                            state.rotation = frame.rotation;
                            for (text, raw) in hits {
                                let raw_b64 = BASE64.encode(&raw);
                                if state
                                    .hits
                                    .iter()
                                    .any(|hit| hit.raw_b64 == raw_b64 || hit.text == text)
                                {
                                    continue;
                                }
                                state.hits.insert(
                                    0,
                                    Hit {
                                        text,
                                        raw_b64,
                                        at_ms: now_ms(),
                                    },
                                );
                                state.hits.truncate(MAX_HITS);
                                state.hit_count += 1;
                            }
                        }
                    }
                    Err(error) => {
                        if let Ok(mut state) = worker_shared.lock() {
                            state.error = Some(format!("scan: {error}"));
                            state.running = false;
                        }
                        break;
                    }
                }
                let due = last_preview.is_none_or(|last| last.elapsed() >= PREVIEW_INTERVAL);
                if due {
                    if let Some(jpeg) = preview_jpeg(&gray, PREVIEW_MAX_WIDTH, frame.rotation) {
                        if let Ok(mut state) = worker_shared.lock() {
                            state.preview = jpeg;
                            state.preview_seq += 1;
                        }
                    }
                    last_preview = Some(Instant::now());
                }
            }
            if let Ok(mut state) = worker_shared.lock() {
                state.running = false;
            }
        }) {
        Ok(join) => join,
        Err(error) => {
            eprintln!("qr_scanner_start(android): {error}");
            return -1;
        }
    };
    if let Ok(mut slot) = SESSION.lock() {
        *slot = Some(Session { tx, shared, join });
    }
    0
}

pub fn stop() {
    let session = SESSION.lock().ok().and_then(|mut slot| slot.take());
    if let Some(session) = session {
        drop(session.tx);
        let _ = session.join.join();
    }
}

pub fn state_json() -> Vec<u8> {
    let (json, _) = match SESSION.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(session) => match session.shared.lock() {
                Ok(state) => {
                    let mut sorted = state.latencies.clone();
                    sorted.sort_by(|a, b| a.total_cmp(b));
                    let p95 = if sorted.is_empty() {
                        0.0
                    } else {
                        sorted[((sorted.len() - 1) as f64 * 0.95).round() as usize]
                    };
                    let json = serde_json::json!({
                        "running": state.running,
                        "error": state.error,
                        "stats": {
                            "frames": state.frames,
                            "scans": state.scans,
                            "hits": state.hit_count,
                            "scan_ms_last": state.scan_ms_last,
                            "scan_ms_avg": if state.scans == 0 { 0.0 } else { state.scan_ms_total / state.scans as f64 },
                            "p95_ms": p95,
                            "max_ms": state.max_ms,
                            "dropped": DROPPED.load(Ordering::Relaxed),
                            "wechat_runs": state.wechat_runs,
                            "wechat_ms_last": state.wechat_ms_last,
                            "last_tier": state.last_tier,
                        },
                        "hits": state.hits,
                        "preview_seq": state.preview_seq,
                        "source": "android-camera",
                        "width": state.width,
                        "height": state.height,
                        "rotation": state.rotation,
                    });
                    (serde_json::to_vec(&json).unwrap_or_default(), ())
                }
                Err(_) => (Vec::new(), ()),
            },
            None => (empty_state(), ()),
        },
        Err(_) => (Vec::new(), ()),
    };
    json
}

pub fn preview() -> Vec<u8> {
    match SESSION.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(session) => match session.shared.lock() {
                Ok(state) => state.preview.clone(),
                Err(_) => Vec::new(),
            },
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

fn empty_state() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "running": false,
        "error": null,
        "stats": { "frames": 0, "scans": 0, "hits": 0, "scan_ms_last": 0.0, "scan_ms_avg": 0.0, "wechat_runs": 0, "wechat_ms_last": 0.0, "last_tier": null },
        "hits": [],
        "preview_seq": 0,
        "source": null,
        "width": 0,
        "height": 0,
        "rotation": 0,
    }))
    .unwrap_or_default()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn preview_jpeg(frame: &GrayFrame, max_width: u32, rotation: i32) -> Option<Vec<u8>> {
    let scale = frame.width().div_ceil(max_width).max(1);
    let width = (frame.width() / scale).max(1);
    let height = (frame.height() / scale).max(1);
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0u32;
            let mut count = 0u32;
            for dy in 0..scale {
                for dx in 0..scale {
                    if let Some(pixel) = frame.pixel(x * scale + dx, y * scale + dy) {
                        sum += u32::from(pixel);
                        count += 1;
                    }
                }
            }
            data.push(sum.checked_div(count).unwrap_or(0) as u8);
        }
    }
    let (data, width, height) = rotate_luma(data, width, height, rotation);
    let image = image::GrayImage::from_raw(width, height, data)?;
    let mut buffer = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 60);
    encoder
        .encode_image(&image::DynamicImage::ImageLuma8(image))
        .ok()?;
    Some(buffer)
}

/// Pushes one Camera2 YUV_420_888 frame. The U/V planes are tightly packed
/// half-resolution chroma; Rust converts to RGB for zxing-cpp and keeps luma
/// for the CNN tier and the preview.
/// Converts tightly packed YUV_420_888 planes to RGB. BT.601 full-range
/// integer coefficients keep this cheap enough for a 1280×720 frame.
fn yuv_to_rgb(y: &[u8], u: &[u8], v: &[u8], width: usize, height: usize) -> Vec<u8> {
    let chroma_w = width / 2;
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        let chroma_row = row / 2 * chroma_w;
        for col in 0..width {
            let luma = i32::from(y[row * width + col]);
            let chroma_col = chroma_row + col / 2;
            let u = i32::from(u[chroma_col]) - 128;
            let v = i32::from(v[chroma_col]) - 128;
            let r = (luma + ((91874 * v) >> 16)).clamp(0, 255);
            let g = (luma - ((22554 * u + 46802 * v) >> 16)).clamp(0, 255);
            let b = (luma + ((116130 * u) >> 16)).clamp(0, 255);
            rgb.push(r as u8);
            rgb.push(g as u8);
            rgb.push(b as u8);
        }
    }
    rgb
}

pub fn push_yuv(width: u32, height: u32, rotation: i32, y: &[u8], u: &[u8], v: &[u8]) -> i32 {
    if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
        return -1;
    }
    let luma_len = (width * height) as usize;
    let chroma_w = (width / 2) as usize;
    let chroma_h = (height / 2) as usize;
    let chroma_len = chroma_w * chroma_h;
    if y.len() < luma_len || u.len() < chroma_len || v.len() < chroma_len {
        return -1;
    }
    let rgb = yuv_to_rgb(y, u, v, width as usize, height as usize);
    let session = match SESSION.lock() {
        Ok(slot) => slot.as_ref().map(|session| session.tx.clone()),
        Err(_) => return -2,
    };
    let Some(tx) = session else {
        return -1;
    };
    match tx.try_send(Frame {
        width,
        height,
        data: y[..luma_len].to_vec(),
        color: Some(rgb),
        rotation,
    }) {
        Ok(()) => 0,
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            0
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => -1,
    }
}

fn rotate_luma(data: Vec<u8>, width: u32, height: u32, rotation: i32) -> (Vec<u8>, u32, u32) {
    let normalized = ((rotation % 360) + 360) % 360;
    if normalized == 0 {
        return (data, width, height);
    }
    let mut output = vec![0u8; data.len()];
    match normalized {
        90 => {
            for y in 0..height {
                for x in 0..width {
                    output[(x * height + (height - 1 - y)) as usize] =
                        data[(y * width + x) as usize];
                }
            }
            (output, height, width)
        }
        180 => {
            for y in 0..height {
                for x in 0..width {
                    output[((height - 1 - y) * width + (width - 1 - x)) as usize] =
                        data[(y * width + x) as usize];
                }
            }
            (output, width, height)
        }
        270 => {
            for y in 0..height {
                for x in 0..width {
                    output[((width - 1 - x) * height + y) as usize] =
                        data[(y * width + x) as usize];
                }
            }
            (output, height, width)
        }
        _ => (data, width, height),
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_qrrace_qr_1race_1gui_CameraCapture_nativePushFrame(
    env: jni::JNIEnv,
    _class: jni::objects::JClass,
    width: jni::sys::jint,
    height: jni::sys::jint,
    rotation: jni::sys::jint,
    y: jni::objects::JByteArray,
    u: jni::objects::JByteArray,
    v: jni::objects::JByteArray,
) {
    let (Ok(y), Ok(u), Ok(v)) = (
        env.convert_byte_array(y),
        env.convert_byte_array(u),
        env.convert_byte_array(v),
    ) else {
        return;
    };
    let _ = push_yuv(
        width.max(0) as u32,
        height.max(0) as u32,
        rotation,
        &y,
        &u,
        &v,
    );
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_qrrace_qr_1race_1gui_CameraCapture_nativeFrameDropped(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) {
    DROPPED.fetch_add(1, Ordering::Relaxed);
}

/// Starts the mobile scanner worker; camera frames arrive via JNI.
#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_start(_camera: u32, width: u32, height: u32, _fps: u32) -> i32 {
    start(width, height)
}

#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_stop() {
    stop();
}

/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_state(out: *mut u8, capacity: usize) -> i32 {
    crate::write_out(&state_json(), out, capacity)
}

/// # Safety
/// `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qr_scanner_preview(out: *mut u8, capacity: usize) -> i32 {
    let preview = preview();
    if preview.is_empty() {
        return -1;
    }
    crate::write_out(&preview, out, capacity)
}

#[cfg(test)]
mod tests {
    use super::yuv_to_rgb;

    #[test]
    fn yuv_red_white_and_black_convert_to_rgb() {
        let red = yuv_to_rgb(&[76], &[84], &[255], 2, 2);
        assert!(red[0] > 240, "red channel: {red:?}");
        assert!(red[1] < 24, "green channel: {red:?}");
        assert!(red[2] < 24, "blue channel: {red:?}");

        let white = yuv_to_rgb(&[235], &[128], &[128], 2, 2);
        assert_eq!(&white[..3], &[235, 235, 235]);

        let black = yuv_to_rgb(&[16], &[128], &[128], 2, 2);
        assert_eq!(&black[..3], &[16, 16, 16]);
    }
}
