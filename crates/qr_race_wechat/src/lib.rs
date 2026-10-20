#![allow(dead_code)] // ported reference functions kept for parity/benchmarks
#![allow(clippy::all)] // ported algorithm modules are kept close to upstream

//! WeChat `wechat_qrcode` detector + super-resolution, implemented in this
//! crate with a tract ONNX backend over our converted weights.
//!
//! The pipeline modules (`detector`, `scale`, `decodermgr`, `wechat_qrcode`)
//! are a studied port of OpenCV contrib's `wechat_qrcode` and the Apache-2.0
//! WeChat QR pipeline, derived from the wxscan Rust port. Only cvlite remains
//! vendored; the wxscan/wxing sources were deleted. Keeping the pipeline here
//! lets us swap the inference backend without depending on a registry crate.
//!
//! The rest of the project uses zxing-cpp as the fallback decoder. The FFI worker
//! can run either or both.

mod backend;
mod clock;
mod decodermgr;
mod detector;
mod frame;
mod net;
mod scale;
mod wechat_qrcode;

use std::time::Instant;

use backend::DefaultNet;
use detector::ssd_detector::{QuadPoints, SSDDetector};
use qr_race_scan::{Decoder, GrayFrame, ScanHit, ZxingDecoder};
use scale::super_scale::SuperScale;
use wechat_qrcode::WeChatQRCode;

const DETECT_ONNX: &[u8] = include_bytes!("../../../models/detect.onnx");
const SR_ONNX: &[u8] = include_bytes!("../../../models/sr.onnx");

/// One decoded WeChat result.
#[derive(Debug, Clone)]
pub struct WeChatHit {
    pub text: String,
    pub raw: Vec<u8>,
    pub points: [(f32, f32); 4],
}

/// What one frame cost and produced.
#[derive(Debug, Clone, Default)]
pub struct WeChatOutcome {
    pub hits: Vec<WeChatHit>,
    pub detect_decode_ms: f64,
}

/// A loaded WeChat CNN scanner.
pub struct WeChatScanner {
    inner: WeChatQRCode<DefaultNet>,
}

impl WeChatScanner {
    pub fn new() -> Result<Self, String> {
        let detector = DefaultNet::from_bytes(DETECT_ONNX)?;
        let sr = DefaultNet::from_bytes(SR_ONNX)?;
        Ok(Self {
            inner: WeChatQRCode::new(Some(detector), Some(sr)),
        })
    }

    pub fn has_models(&self) -> bool {
        self.inner.has_detector() && self.inner.has_super_resolution()
    }

    pub fn scan(&self, frame: &GrayFrame) -> WeChatOutcome {
        let started = Instant::now();
        let results = self.inner.detect_and_decode_gray(
            frame.data(),
            frame.width() as usize,
            frame.height() as usize,
        );
        let hits = results
            .into_iter()
            .map(|result| WeChatHit {
                text: result.text_lossy(),
                raw: result.bytes,
                points: result.points,
            })
            .collect();
        WeChatOutcome {
            hits,
            detect_decode_ms: started.elapsed().as_secs_f64() * 1000.0,
        }
    }
}

/// One frame result from the tiered pipeline.
#[derive(Debug, Clone)]
pub struct TieredOutcome {
    pub hits: Vec<ScanHit>,
    /// `fast`, `cnn`, or `cnn+sr`.
    pub tier: &'static str,
    pub detect_ms: f64,
    pub decode_ms: f64,
}

/// Tiered WeChat pipeline:
///
/// 1. zxing-cpp on the full frame (fast path, most frames with a clear code);
/// 2. only if that misses, the WeChat CNN detector on a 384x384 view;
/// 3. crop each candidate at **full resolution** and decode with zxing-cpp;
/// 4. for small crops, run the super-resolution network before decoding.
///
/// This keeps the 30 fps path at zxing-cpp speed while spending CNN work only on
/// frames where the fast decoder failed — exactly the far/small-code case.
pub struct TieredScanner {
    detector: SSDDetector<DefaultNet>,
    sr: SuperScale<DefaultNet>,
    decoder: ZxingDecoder,
}

impl TieredScanner {
    pub fn new() -> Result<Self, String> {
        let detector = DefaultNet::from_bytes(DETECT_ONNX)?;
        let sr = DefaultNet::from_bytes(SR_ONNX)?;
        Ok(Self {
            detector: SSDDetector::new(detector),
            sr: SuperScale::new(Some(sr)),
            decoder: ZxingDecoder::new(),
        })
    }

    pub fn scan(&self, frame: &GrayFrame) -> TieredOutcome {
        let started = Instant::now();
        let fast = self.decoder.decode(frame).unwrap_or_default();
        if !fast.is_empty() {
            return TieredOutcome {
                hits: dedup(fast),
                tier: "fast",
                detect_ms: 0.0,
                decode_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
        }
        self.scan_cnn(frame)
    }

    /// Runs the CNN detector at the default 384x384 resolution.
    pub fn scan_cnn(&self, frame: &GrayFrame) -> TieredOutcome {
        self.scan_cnn_at(frame, 384)
    }

    /// Runs the detector at an explicit resolution. 768 preserves twice the
    /// module resolution for far/small symbols, at roughly four times the cost;
    /// callers should schedule it rather than use it every frame.
    pub fn scan_cnn_at(&self, frame: &GrayFrame, target: usize) -> TieredOutcome {
        let detect_started = Instant::now();
        let boxes = self.detect_boxes(frame, target);
        let detect_ms = detect_started.elapsed().as_secs_f64() * 1000.0;
        let decode_started = Instant::now();
        let mut hits = Vec::new();
        let mut used_sr = false;
        for quad in boxes {
            let (crop, width, height) = crop_quad(frame, &quad, 0.18);
            if width < 8 || height < 8 {
                continue;
            }
            // Try the full-resolution crop first. SR is only the fallback for
            // a crop that is genuinely too small; running it unconditionally
            // made the tiled path roughly three times slower.
            let mut direct = self.decode_crop(&crop, width, height);
            if !direct.is_empty() {
                // zxing points are crop-local; shift them into frame
                // coordinates so the UI can put the hit bubble on the QR.
                let x0 = quad.iter().map(|p| p.0).fold(f32::MAX, f32::min).max(0.0) as f32;
                let y0 = quad.iter().map(|p| p.1).fold(f32::MAX, f32::min).max(0.0) as f32;
                for hit in &mut direct {
                    for point in &mut hit.points {
                        point[0] += x0;
                        point[1] += y0;
                    }
                }
                hits.extend(direct);
                continue;
            }
            let area = (width * height) as f64;
            if area < 120.0 * 120.0 {
                if let Some(sr_hits) = self.decode_with_sr(&crop, width, height) {
                    used_sr = true;
                    hits.extend(sr_hits);
                }
            }
        }
        TieredOutcome {
            hits: dedup(hits),
            tier: if used_sr { "cnn+sr" } else { "cnn" },
            detect_ms,
            decode_ms: decode_started.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Detect at 384, or split the frame into four overlapping tiles when a
    /// larger target is requested. Tiling preserves small modules while keeping
    /// the model's fixed 384×384 input.
    fn detect_boxes(&self, frame: &GrayFrame, target: usize) -> Vec<QuadPoints> {
        if target <= 384 {
            return self
                .detector
                .forward(
                    frame.data(),
                    frame.width() as usize,
                    frame.height() as usize,
                    384,
                    384,
                )
                .unwrap_or_default();
        }
        let (w, h) = (frame.width(), frame.height());
        let tile_w = (w * 3 / 5).max(1);
        let tile_h = (h * 3 / 5).max(1);
        let xs = [0, w.saturating_sub(tile_w)];
        let ys = [0, h.saturating_sub(tile_h)];
        let mut boxes = Vec::new();
        for y0 in ys {
            for x0 in xs {
                let (crop, cw, ch) = crop_rect(frame, x0, y0, tile_w, tile_h);
                let mut found = self
                    .detector
                    .forward(&crop, cw, ch, 384, 384)
                    .unwrap_or_default();
                for quad in &mut found {
                    for point in quad.iter_mut() {
                        point.0 += x0 as f32;
                        point.1 += y0 as f32;
                    }
                }
                boxes.extend(found);
            }
        }
        boxes
    }

    fn decode_crop(&self, data: &[u8], width: usize, height: usize) -> Vec<ScanHit> {
        let Ok(frame) = GrayFrame::new(width as u32, height as u32, data.to_vec()) else {
            return Vec::new();
        };
        self.decoder.decode(&frame).unwrap_or_default()
    }

    fn decode_with_sr(&self, data: &[u8], width: usize, height: usize) -> Option<Vec<ScanHit>> {
        let (square, side) = square_pad(data, width, height);
        // The SR graph is fixed 224×224. Resize the padded crop instead of
        // feeding an arbitrary square (tract tolerated it, ORT does not).
        let fixed = cvlite::resize(&square, side, side, 224, 224, cvlite::Interpolation::Cubic);
        let (up, up_w, up_h) = self.sr.super_resolution_scale(&fixed, 224, 224)?;
        Some(self.decode_crop(&up, up_w, up_h))
    }
}

fn crop_rect(frame: &GrayFrame, x0: u32, y0: u32, w: u32, h: u32) -> (Vec<u8>, usize, usize) {
    let x1 = (x0 + w).min(frame.width());
    let y1 = (y0 + h).min(frame.height());
    let cw = x1.saturating_sub(x0) as usize;
    let ch = y1.saturating_sub(y0) as usize;
    let mut data = Vec::with_capacity(cw * ch);
    for y in y0..y1 {
        for x in x0..x1 {
            data.push(frame.pixel(x, y).unwrap_or(255));
        }
    }
    (data, cw, ch)
}

fn crop_quad(frame: &GrayFrame, quad: &QuadPoints, pad: f32) -> (Vec<u8>, usize, usize) {
    let xs = quad.iter().map(|p| p.0);
    let ys = quad.iter().map(|p| p.1);
    let (min_x, max_x) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let (min_y, max_y) = ys.fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let w = (max_x - min_x).max(1.0);
    let h = (max_y - min_y).max(1.0);
    let pad_x = w * pad;
    let pad_y = h * pad;
    let x0 = (min_x - pad_x).floor().max(0.0) as u32;
    let y0 = (min_y - pad_y).floor().max(0.0) as u32;
    let x1 = ((max_x + pad_x).ceil() as u32).min(frame.width());
    let y1 = ((max_y + pad_y).ceil() as u32).min(frame.height());
    let cw = x1.saturating_sub(x0) as usize;
    let ch = y1.saturating_sub(y0) as usize;
    let mut data = Vec::with_capacity(cw * ch);
    for y in y0..y1 {
        for x in x0..x1 {
            data.push(frame.pixel(x, y).unwrap_or(255));
        }
    }
    (data, cw, ch)
}

fn square_pad(data: &[u8], width: usize, height: usize) -> (Vec<u8>, usize) {
    let side = width.max(height);
    let mut out = vec![255u8; side * side];
    let ox = (side - width) / 2;
    let oy = (side - height) / 2;
    for y in 0..height {
        let dst = (y + oy) * side + ox;
        out[dst..dst + width].copy_from_slice(&data[y * width..(y + 1) * width]);
    }
    (out, side)
}

fn dedup(hits: Vec<ScanHit>) -> Vec<ScanHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(hit.text.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qr_frame(text: &str, size: u32) -> GrayFrame {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
        let modules = code.width() as u32;
        let scale = (size / (modules + 8)).max(1);
        let quiet = 4;
        let drawn = (modules + quiet * 2) * scale;
        let offset = (size - drawn) / 2;
        let mut luma = vec![255u8; (size * size) as usize];
        let colors = code.to_colors();
        for y in 0..modules {
            for x in 0..modules {
                if colors[(y * modules + x) as usize] == qrcode::Color::Dark {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = offset + (quiet + x) * scale + dx;
                            let py = offset + (quiet + y) * scale + dy;
                            luma[(py * size + px) as usize] = 0;
                        }
                    }
                }
            }
        }
        GrayFrame::new(size, size, luma).expect("frame")
    }

    #[test]
    fn loads_both_models_and_decodes_a_symbol() {
        let scanner = WeChatScanner::new().expect("pipeline loads");
        assert!(scanner.has_models());
        let frame = qr_frame("https://example.test/wechat-cnn", 720);
        let outcome = scanner.scan(&frame);
        assert!(
            outcome
                .hits
                .iter()
                .any(|hit| hit.text == "https://example.test/wechat-cnn"),
            "hits: {:?} in {:.1} ms",
            outcome.hits,
            outcome.detect_decode_ms
        );
    }

    fn qr_frame_modules(text: &str, width: u32, height: u32, module: u32) -> GrayFrame {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
        let modules = code.width() as u32;
        let quiet = 4;
        let drawn = (modules + quiet * 2) * module;
        let ox = (width - drawn) / 2;
        let oy = (height - drawn) / 2;
        let mut luma = vec![255u8; (width * height) as usize];
        let colors = code.to_colors();
        for y in 0..modules {
            for x in 0..modules {
                if colors[(y * modules + x) as usize] == qrcode::Color::Dark {
                    for dy in 0..module {
                        for dx in 0..module {
                            let px = ox + (quiet + x) * module + dx;
                            let py = oy + (quiet + y) * module + dy;
                            luma[(py * width + px) as usize] = 0;
                        }
                    }
                }
            }
        }
        GrayFrame::new(width, height, luma).expect("frame")
    }

    #[test]
    fn cnn_path_reads_a_small_distant_code() {
        let scanner = TieredScanner::new().expect("tiered loads");
        let frame = qr_frame_modules("https://example.test/far-small", 1280, 720, 2);
        let outcome = scanner.scan_cnn(&frame);
        assert!(
            outcome
                .hits
                .iter()
                .any(|hit| hit.text == "https://example.test/far-small"),
            "tier={} detect={:.1}ms decode={:.1}ms hits={:?}",
            outcome.tier,
            outcome.detect_ms,
            outcome.decode_ms,
            outcome.hits
        );
    }

    #[test]
    fn repeated_frames_keep_working() {
        let scanner = WeChatScanner::new().expect("pipeline loads");
        let frame = qr_frame("https://example.test/repeat", 720);
        for _ in 0..2 {
            let outcome = scanner.scan(&frame);
            assert_eq!(outcome.hits.len(), 1);
        }
    }

    #[test]
    fn tiered_pipeline_uses_the_fast_path_for_a_clear_code() {
        let scanner = TieredScanner::new().expect("tiered loads");
        let frame = qr_frame("https://example.test/tiered-fast", 720);
        let outcome = scanner.scan(&frame);
        assert_eq!(outcome.tier, "fast");
        assert!(
            outcome
                .hits
                .iter()
                .any(|hit| hit.text == "https://example.test/tiered-fast")
        );
    }

    #[test]
    fn perturbed_suite_reports_the_detection_rate() {
        let scanner = TieredScanner::new().expect("tiered loads");
        let mut scenes = 0;
        let mut fully_matched = 0;
        for scene in qr_race_fixtures::scenes() {
            scenes += 1;
            let outcome = scanner.scan(&scene.frame);
            let texts: Vec<&str> = outcome.hits.iter().map(|hit| hit.text.as_str()).collect();
            let matched = scene
                .expected
                .iter()
                .filter(|expected| texts.contains(&expected.as_str()))
                .count();
            let complete = matched == scene.expected.len();
            fully_matched += usize::from(complete);
            println!(
                "suite {}: tier={} hits={}/{} detect={:.1}ms decode={:.1}ms",
                scene.name,
                outcome.tier,
                matched,
                scene.expected.len(),
                outcome.detect_ms,
                outcome.decode_ms
            );
        }
        // Diagnostic floor, not the final acceptance rate.
        assert!(
            fully_matched * 2 >= scenes,
            "only {fully_matched}/{scenes} scenes fully decoded"
        );
    }
}
