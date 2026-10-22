//! Android scanner session: Kotlin's Camera2 ImageReader pushes YUV_420_888
//! planes through JNI, Rust converts them to RGB (and keeps luma for CNN/preview)
//! and scans on a dedicated thread. Frames never enter Dart; Dart only polls
//! hits, stats and the low-rate JPEG preview.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use data_encoding::BASE64;
use qr_race_capture::GrayFrame;
use qr_race_pipeline::{ScanMode, ScanPipeline};
use serde::Serialize;

const MAX_HITS: usize = 8;

#[derive(Clone, Serialize)]
struct Hit {
    text: String,
    raw_b64: String,
    at_ms: u64,
    /// Sequence of the last frame that contained this hit.
    last_seq: u64,
    /// Four corners in source-frame pixels, [tl, tr, br, bl].
    points: Vec<[f32; 2]>,
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
    latest: Vec<Hit>,
    latest_seq: u64,
    detections: Vec<[(f32, f32); 4]>,
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

pub fn start(width: u32, height: u32, backend: u32) -> i32 {
    stop();
    DROPPED.store(0, Ordering::Relaxed);
    let mode = backend.min(2);
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
        latest: Vec::new(),
        latest_seq: 0,
        detections: Vec::new(),
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
            let pipeline = match ScanPipeline::new() {
                Ok(pipeline) => pipeline,
                Err(error) => {
                    if let Ok(mut state) = worker_shared.lock() {
                        state.error = Some(format!("scan pipeline: {error}"));
                        state.running = false;
                    }
                    return;
                }
            };
            let scan_mode = match mode {
                1 => ScanMode::Zxing,
                2 => ScanMode::Cnn,
                _ => ScanMode::Hybrid,
            };
            let mut frame_seq: u64 = 0;
            while let Ok(frame) = rx.recv() {
                let Ok(gray) = GrayFrame::new(frame.width, frame.height, frame.data) else {
                    continue;
                };
                let started = Instant::now();
                frame_seq += 1;
                let outcome = pipeline.scan(&gray, frame.color.as_deref(), scan_mode, frame_seq);
                let ran_cnn = outcome.tier.starts_with("cnn");
                let detections = outcome.detections.clone();
                let hits: Vec<(String, Vec<u8>, Vec<[f32; 2]>)> = outcome
                    .hits
                    .into_iter()
                    .map(|hit| (hit.text, hit.raw, hit.points))
                    .collect();
                if ran_cnn {
                    if let Ok(mut state) = worker_shared.lock() {
                        state.wechat_runs += 1;
                        state.wechat_ms_last = outcome.detect_ms + outcome.decode_ms;
                        state.last_tier = Some(outcome.tier.to_string());
                    }
                }
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
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
                    if ran_cnn {
                        state.detections = detections;
                    } else {
                        state.detections.clear();
                    }
                    let at_ms = now_ms();
                    state.latest_seq += 1;
                    state.latest = hits
                        .iter()
                        .map(|(text, raw, points)| Hit {
                            text: text.clone(),
                            raw_b64: BASE64.encode(raw),
                            at_ms,
                            last_seq: state.latest_seq,
                            points: points.clone(),
                        })
                        .collect();
                    let hit_seq = state.latest_seq;
                    for (text, raw, points) in hits {
                        let raw_b64 = BASE64.encode(&raw);
                        if let Some(existing) = state
                            .hits
                            .iter_mut()
                            .find(|hit| hit.raw_b64 == raw_b64 || hit.text == text)
                        {
                            existing.at_ms = at_ms;
                            existing.last_seq = hit_seq;
                            existing.points = points;
                            continue;
                        }
                        state.hits.insert(
                            0,
                            Hit {
                                text,
                                raw_b64,
                                at_ms,
                                last_seq: hit_seq,
                                points,
                            },
                        );
                        state.hits.truncate(MAX_HITS);
                        state.hit_count += 1;
                    }
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
                        "latest": state.latest,
                        "latest_seq": state.latest_seq,
                        "detections": state.detections,
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
        "latest": [],
        "latest_seq": 0,
        "detections": [],
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

/// Pushes one Camera2 YUV_420_888 frame. The U/V planes are tightly packed
/// half-resolution chroma; Rust converts to RGB for zxing-cpp and keeps luma
/// for the CNN tier and the preview.
/// Converts tightly packed YUV_420_888 planes to RGB with imazen `zenyuv`'s
/// SIMD kernels (AVX2 / NEON / WASM dispatch).
fn yuv_to_rgb(y: &[u8], u: &[u8], v: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgb = vec![0u8; width * height * 3];
    let image = yuv::YuvPlanarImage {
        y_plane: y,
        y_stride: width as u32,
        u_plane: u,
        u_stride: (width / 2) as u32,
        v_plane: v,
        v_stride: (width / 2) as u32,
        width: width as u32,
        height: height as u32,
    };
    let _ = yuv::yuv420_to_rgb(
        &image,
        &mut rgb,
        (width * 3) as u32,
        yuv::YuvRange::Limited,
        yuv::YuvStandardMatrix::Bt601,
    );
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

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_qrrace_qr_1race_1gui_CameraCapture_nativePushFrame(
    mut env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    width: jni::sys::jint,
    height: jni::sys::jint,
    rotation: jni::sys::jint,
    y: jni::objects::JByteArray,
    u: jni::objects::JByteArray,
    v: jni::objects::JByteArray,
) {
    let outcome = env.with_env(|env| -> jni::errors::Result<()> {
        let y = env.convert_byte_array(y)?;
        let u = env.convert_byte_array(u)?;
        let v = env.convert_byte_array(v)?;
        let _ = push_yuv(
            width.max(0) as u32,
            height.max(0) as u32,
            rotation,
            &y,
            &u,
            &v,
        );
        Ok(())
    });
    outcome.resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_qrrace_qr_1race_1gui_CameraCapture_nativeFrameDropped(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    DROPPED.fetch_add(1, Ordering::Relaxed);
}

/// Starts the mobile scanner worker; camera frames arrive via JNI.
#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_start(_camera: u32, width: u32, height: u32, _fps: u32) -> i32 {
    start(width, height, 0)
}

/// Starts with an explicit backend: 0 = hybrid, 1 = zxing-cpp only,
/// 2 = WeChat CNN only.
#[unsafe(no_mangle)]
pub extern "C" fn qr_scanner_start_backend(
    _camera: u32,
    width: u32,
    height: u32,
    backend: u32,
) -> i32 {
    start(width, height, backend)
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

