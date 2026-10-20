//! The scanner session that runs natively, so no camera frame ever reaches
//! Dart: only decoded hits (with their exact bytes) and a low-rate JPEG preview
//! cross the boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use data_encoding::BASE64;
use qr_race_capture::{CaptureConfig, CaptureSession, CaptureSource, GrayFrame};
use qr_race_scan::{ScanEngine, ZxingDecoder};
use serde::Serialize;

const PREVIEW_INTERVAL: Duration = Duration::from_millis(90);
const PREVIEW_MAX_WIDTH: u32 = 480;
const MAX_HITS: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct ScanHitDto {
    pub text: String,
    pub raw_b64: String,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ScanStats {
    pub frames: u64,
    pub scans: u64,
    pub hits: u64,
    pub scan_ms_last: f64,
    pub scan_ms_avg: f64,
}

pub struct SharedState {
    pub running: bool,
    pub error: Option<String>,
    pub stats: ScanStats,
    pub hits: Vec<ScanHitDto>,
    pub preview: Vec<u8>,
    pub preview_seq: u64,
    /// What the device actually gave us; 0 until the first frame.
    pub source: Option<String>,
    pub width: u32,
    pub height: u32,
    scan_ms_total: f64,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            running: false,
            error: None,
            stats: ScanStats::default(),
            hits: Vec::new(),
            preview: Vec::new(),
            preview_seq: 0,
            source: None,
            width: 0,
            height: 0,
            scan_ms_total: 0.0,
        }
    }

    pub fn record_scan(&mut self, elapsed_ms: f64, hits: Vec<(String, Vec<u8>)>) {
        self.stats.frames += 1;
        self.stats.scans += 1;
        self.stats.scan_ms_last = elapsed_ms;
        self.scan_ms_total += elapsed_ms;
        self.stats.scan_ms_avg = self.scan_ms_total / self.stats.scans as f64;

        for (text, raw) in hits {
            let raw_b64 = BASE64.encode(&raw);
            if self
                .hits
                .iter()
                .any(|hit| hit.raw_b64 == raw_b64 || hit.text == text)
            {
                continue;
            }
            self.hits.insert(
                0,
                ScanHitDto {
                    text,
                    raw_b64,
                    at_ms: now_ms(),
                },
            );
            self.hits.truncate(MAX_HITS);
            self.stats.hits += 1;
        }
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs a capture source to exhaustion or until `stop` is set. The same
/// function serves the real camera and the tests' fake source.
pub fn run_source<S: CaptureSource>(
    source: S,
    shared: Arc<Mutex<SharedState>>,
    stop: Arc<AtomicBool>,
    width: u32,
    height: u32,
    fps: u32,
) {
    let description = source.description().to_string();
    let negotiated = source.resolution();
    let config = match CaptureConfig::new(width.max(1), height.max(1), fps.max(1)) {
        Ok(config) => config,
        Err(error) => {
            fail(&shared, format!("capture config: {error}"));
            return;
        }
    };
    let mut session = match CaptureSession::new(source, config) {
        Ok(session) => session,
        Err(error) => {
            fail(&shared, format!("capture session: {error}"));
            return;
        }
    };
    let engine = ScanEngine::new(Box::new(ZxingDecoder::new()));
    let mut last_preview: Option<Instant> = None;

    {
        let mut state = shared.lock().expect("scanner state lock");
        state.source = Some(description);
        let (frame_width, frame_height) = negotiated.unwrap_or((width, height));
        state.width = frame_width;
        state.height = frame_height;
    }

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let frame = match session.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                fail(&shared, format!("capture: {error}"));
                return;
            }
        };

        let started = Instant::now();
        let outcome = match engine.scan(&frame) {
            Ok(outcome) => outcome,
            Err(error) => {
                fail(&shared, format!("scan: {error}"));
                return;
            }
        };
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let hits = outcome
            .hits
            .into_iter()
            .map(|hit| (hit.text, hit.raw))
            .collect();

        {
            let mut state = shared.lock().expect("scanner state lock");
            state.record_scan(elapsed_ms, hits);
        }

        let preview_due = last_preview.is_none_or(|last| last.elapsed() >= PREVIEW_INTERVAL);
        if preview_due {
            if let Some(jpeg) = preview_jpeg(&frame, PREVIEW_MAX_WIDTH) {
                let mut state = shared.lock().expect("scanner state lock");
                state.preview = jpeg;
                state.preview_seq += 1;
            }
            last_preview = Some(Instant::now());
        }
    }

    let mut state = shared.lock().expect("scanner state lock");
    state.running = false;
}

fn fail(shared: &Arc<Mutex<SharedState>>, message: String) {
    let mut state = shared.lock().expect("scanner state lock");
    state.error = Some(message);
    state.running = false;
}

/// Box-filtered, JPEG-encoded preview. This is the only image that crosses into
/// Dart, and it is for the eye, not for scanning.
pub fn preview_jpeg(frame: &GrayFrame, max_width: u32) -> Option<Vec<u8>> {
    // Round the scale up so the result is never wider than requested.
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

    let image = image::GrayImage::from_raw(width, height, data)?;
    let mut buffer = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 60);
    encoder
        .encode_image(&image::DynamicImage::ImageLuma8(image))
        .ok()?;
    Some(buffer)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_capture::{CaptureSource, CapturedFrame};

    fn qr_frame(text: &str, scale: u32, quiet: u32) -> GrayFrame {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("payload fits");
        let modules = code.width() as u32;
        let colors = code.to_colors();
        let side = (modules + quiet * 2) * scale;
        let mut data = vec![255u8; (side * side) as usize];
        for y in 0..modules {
            for x in 0..modules {
                if colors[(y * modules + x) as usize] == qrcode::types::Color::Dark {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = (x + quiet) * scale + dx;
                            let py = (y + quiet) * scale + dy;
                            data[(py * side + px) as usize] = 0;
                        }
                    }
                }
            }
        }
        GrayFrame::new(side, side, data).unwrap()
    }

    struct FakeSource {
        frames: Vec<CapturedFrame>,
        index: usize,
    }

    impl CaptureSource for FakeSource {
        fn next_frame(&mut self) -> Result<Option<CapturedFrame>, qr_race_capture::CaptureError> {
            if self.index < self.frames.len() {
                let frame = self.frames[self.index].clone();
                self.index += 1;
                Ok(Some(frame))
            } else {
                Ok(None)
            }
        }

        fn description(&self) -> &str {
            "fake"
        }
    }

    fn frames_for(text: &str, count: usize) -> Vec<CapturedFrame> {
        (0..count)
            .map(|index| CapturedFrame {
                gray: qr_frame(text, 3, 2),
                captured_at_ms: index as u64 * 33,
            })
            .collect()
    }

    #[test]
    fn a_session_records_hits_and_previews() {
        let shared = Arc::new(Mutex::new(SharedState::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let source = FakeSource {
            frames: frames_for("https://example.test/relay", 3),
            index: 0,
        };

        run_source(source, shared.clone(), stop, 300, 300, 30);

        let state = shared.lock().unwrap();
        assert!(!state.running);
        assert!(state.error.is_none());
        assert_eq!(state.stats.frames, 3, "three frames scanned");
        assert_eq!(state.hits.len(), 1, "the same symbol is recorded once");
        assert_eq!(
            BASE64.decode(state.hits[0].raw_b64.as_bytes()).unwrap(),
            b"https://example.test/relay"
        );
        assert!(state.preview_seq > 0);
        assert_eq!(&state.preview[0..2], &[0xFF, 0xD8], "JPEG magic");
    }

    #[test]
    fn the_same_text_is_deduplicated_even_with_different_bytes() {
        let mut state = SharedState::new();
        state.record_scan(
            1.0,
            vec![
                ("https://example.test/same".to_string(), b"first".to_vec()),
                ("https://example.test/same".to_string(), b"second".to_vec()),
            ],
        );
        assert_eq!(state.hits.len(), 1);
        assert_eq!(state.hits[0].text, "https://example.test/same");
    }

    #[test]
    fn preview_is_bounded_to_the_requested_width() {
        let frame = qr_frame("https://example.test/preview", 4, 2);
        let jpeg = preview_jpeg(&frame, 120).expect("preview encodes");
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert!(decoded.width() <= 120);
        assert!(decoded.height() > 0);
    }

    #[test]
    fn a_source_that_ends_marks_the_session_stopped() {
        let shared = Arc::new(Mutex::new(SharedState::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let source = FakeSource {
            frames: Vec::new(),
            index: 0,
        };
        run_source(source, shared.clone(), stop, 640, 480, 30);
        assert!(!shared.lock().unwrap().running);
    }
}
