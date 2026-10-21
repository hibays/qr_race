//! Unified QR scan orchestration.
//!
//! The two engines are deliberately asymmetric:
//!
//! * zxing-cpp owns *decoding* (whole frame and candidate crops);
//! * the WeChat CNN owns *detection and super resolution only*.
//!
//! `ScanPipeline` is the only place that composes them. A backend can be
//! replaced through the `QrDecoder` / `QrDetector` traits without changing the
//! camera, relay, or UI layers.

use qr_race_scan::{Decoder, GrayFrame, ScanHit, ZxingDecoder};
use qr_race_wechat::{CnnEngine, CnnQuad};
/// How often the CNN detector runs. `1` means every frame; raise it to trade
/// detection latency for CPU. This is intentionally a code-level constant.
pub const CNN_FRAME_INTERVAL: u64 = 1;

/// Every Nth CNN pass uses the 768 detector for far/small symbols.
pub const CNN_LARGE_INPUT_INTERVAL: u64 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanMode {
    /// zxing full-frame decode first; CNN only when it misses.
    Hybrid,
    /// zxing only.
    Zxing,
    /// CNN detect/SR only, then zxing decodes the candidate crops.
    Cnn,
}

#[derive(Clone, Debug)]
pub struct PipelineOutcome {
    pub hits: Vec<ScanHit>,
    /// Every CNN candidate, including ones that did not decode.
    pub detections: Vec<CnnQuad>,
    /// `fast`, `cnn`, or `cnn+sr`.
    pub tier: &'static str,
    pub detect_ms: f64,
    pub decode_ms: f64,
}

/// The decoding half. zxing is the current implementation; keeping this trait
/// means a future decoder does not need to know about the CNN.
pub trait QrDecoder: Send + Sync {
    fn decode_full(&self, frame: &GrayFrame, color: Option<&[u8]>) -> Vec<ScanHit>;
    fn decode_crop(&self, data: &[u8], width: usize, height: usize) -> Vec<ScanHit>;
}

/// The candidate half. It intentionally has no decode method.
pub trait QrDetector: Send + Sync {
    fn detect(&self, gray: &[u8], width: usize, height: usize, target: usize) -> Vec<CnnQuad>;
    fn super_resolve(
        &self,
        crop: &[u8],
        width: usize,
        height: usize,
    ) -> Option<(Vec<u8>, usize, usize)>;
}

pub struct ZxingBackend {
    fast: ZxingDecoder,
    hard: ZxingDecoder,
}

impl ZxingBackend {
    pub fn new() -> Self {
        Self {
            fast: ZxingDecoder::new(),
            hard: ZxingDecoder::new().try_harder(true),
        }
    }
}

impl Default for ZxingBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl QrDecoder for ZxingBackend {
    fn decode_full(&self, frame: &GrayFrame, color: Option<&[u8]>) -> Vec<ScanHit> {
        if let Some(rgb) = color {
            let factor = if frame.width() >= 1600 { 3 } else { 1 };
            let hits = self
                .fast
                .decode_rgb_scaled(rgb, frame.width(), frame.height(), factor)
                .unwrap_or_default();
            if !hits.is_empty() {
                return hits;
            }
            let hard = self
                .hard
                .decode_rgb_scaled(rgb, frame.width(), frame.height(), factor)
                .unwrap_or_default();
            if !hard.is_empty() {
                return hard;
            }
        }
        self.fast.decode(frame).unwrap_or_default()
    }

    fn decode_crop(&self, data: &[u8], width: usize, height: usize) -> Vec<ScanHit> {
        let Ok(frame) = GrayFrame::new(width as u32, height as u32, data.to_vec()) else {
            return Vec::new();
        };
        self.hard.decode(&frame).unwrap_or_default()
    }
}

pub struct CnnBackend {
    inner: CnnEngine,
}

impl CnnBackend {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            inner: CnnEngine::new()?,
        })
    }
}

impl QrDetector for CnnBackend {
    fn detect(&self, gray: &[u8], width: usize, height: usize, target: usize) -> Vec<CnnQuad> {
        self.inner.detect(gray, width, height, target)
    }

    fn super_resolve(
        &self,
        crop: &[u8],
        width: usize,
        height: usize,
    ) -> Option<(Vec<u8>, usize, usize)> {
        self.inner.super_resolve(crop, width, height)
    }
}

pub struct ScanPipeline {
    decoder: Box<dyn QrDecoder>,
    detector: Option<Box<dyn QrDetector>>,
}

impl ScanPipeline {
    pub fn new() -> Result<Self, String> {
        let detector = CnnBackend::new()
            .ok()
            .map(|cnn| Box::new(cnn) as Box<dyn QrDetector>);
        Ok(Self {
            decoder: Box::new(ZxingBackend::new()),
            detector,
        })
    }

    pub fn with_backends(
        decoder: Box<dyn QrDecoder>,
        detector: Option<Box<dyn QrDetector>>,
    ) -> Self {
        Self { decoder, detector }
    }

    pub fn has_cnn(&self) -> bool {
        self.detector.is_some()
    }

    pub fn scan(
        &self,
        frame: &GrayFrame,
        color: Option<&[u8]>,
        mode: ScanMode,
        frame_seq: u64,
    ) -> PipelineOutcome {
        let started = std::time::Instant::now();
        let mut hits = Vec::new();
        let mut tier = "fast";

        if mode != ScanMode::Cnn {
            hits = self.decoder.decode_full(frame, color);
        }
        let fast_hit = !hits.is_empty();

        let mut detections = Vec::new();
        let mut detect_ms = 0.0;
        let mut decode_ms = started.elapsed().as_secs_f64() * 1000.0;
        if mode != ScanMode::Zxing && frame_seq.is_multiple_of(CNN_FRAME_INTERVAL) {
            let Some(detector) = self.detector.as_ref() else {
                return PipelineOutcome {
                    hits,
                    detections,
                    tier,
                    detect_ms,
                    decode_ms,
                };
            };
            let detect_started = std::time::Instant::now();
            let target = if frame_seq.is_multiple_of(128) {
                768
            } else {
                384
            };
            detections = detector.detect(
                frame.data(),
                frame.width() as usize,
                frame.height() as usize,
                target,
            );
            detect_ms = detect_started.elapsed().as_secs_f64() * 1000.0;
            let decode_started = std::time::Instant::now();
            let mut sr_used = false;
            for quad in &detections {
                let Some((crop, cw, ch, ox, oy)) = crop_quad(frame, quad) else {
                    continue;
                };
                let (decoded, sw, sh) = if (cw * ch) < 160 * 160 {
                    match detector.super_resolve(&crop, cw, ch) {
                        Some((up, uw, uh)) => {
                            sr_used = true;
                            (self.decoder.decode_crop(&up, uw, uh), uw, uh)
                        }
                        None => (self.decoder.decode_crop(&crop, cw, ch), cw, ch),
                    }
                } else {
                    (self.decoder.decode_crop(&crop, cw, ch), cw, ch)
                };
                let sx = sw as f32 / cw as f32;
                let sy = sh as f32 / ch as f32;
                for mut hit in decoded {
                    for point in &mut hit.points {
                        point[0] = point[0] / sx + ox as f32;
                        point[1] = point[1] / sy + oy as f32;
                    }
                    if !hits
                        .iter()
                        .any(|existing| existing.raw == hit.raw || existing.text == hit.text)
                    {
                        hits.push(hit);
                    }
                }
            }
            decode_ms = decode_started.elapsed().as_secs_f64() * 1000.0;
            if !fast_hit {
                tier = if sr_used { "cnn+sr" } else { "cnn" };
            }
        }

        PipelineOutcome {
            hits,
            detections,
            tier,
            detect_ms,
            decode_ms,
        }
    }
}

fn crop_quad(frame: &GrayFrame, quad: &CnnQuad) -> Option<(Vec<u8>, usize, usize, u32, u32)> {
    let x0 = quad.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let y0 = quad.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let x1 = quad.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let y1 = quad.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    let pad_x = ((x1 - x0) * 0.1).max(15.0);
    let pad_y = ((y1 - y0) * 0.1).max(15.0);
    let left = (x0 - pad_x).max(0.0) as u32;
    let top = (y0 - pad_y).max(0.0) as u32;
    let right = ((x1 + pad_x).min(frame.width() as f32 - 1.0)) as u32;
    let bottom = ((y1 + pad_y).min(frame.height() as f32 - 1.0)) as u32;
    if right <= left || bottom <= top {
        return None;
    }
    let w = right - left + 1;
    let h = bottom - top + 1;
    let mut data = Vec::with_capacity(w as usize * h as usize);
    for y in top..=bottom {
        for x in left..=right {
            data.push(frame.pixel(x, y).unwrap_or(255));
        }
    }
    Some((data, w as usize, h as usize, left, top))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_scan::GrayFrame;

    fn qr_frame(text: &str) -> GrayFrame {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("payload fits");
        let modules = code.width() as u32;
        let scale = 8u32;
        let quiet = 4u32;
        let side = (modules + quiet * 2) * scale;
        let colors = code.to_colors();
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
        GrayFrame::new(side, side, data).expect("valid frame")
    }

    #[test]
    fn zxing_mode_decodes_a_clear_frame() {
        let pipeline = ScanPipeline::with_backends(Box::new(ZxingBackend::new()), None);
        let frame = qr_frame("https://example.test/relay");
        let outcome = pipeline.scan(&frame, None, ScanMode::Zxing, 1);
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.tier, "fast");
    }

    #[test]
    fn cnn_mode_never_falls_back_to_full_frame_decode() {
        let pipeline = ScanPipeline::with_backends(Box::new(ZxingBackend::new()), None);
        let frame = qr_frame("https://example.test/relay");
        let outcome = pipeline.scan(&frame, None, ScanMode::Cnn, 1);
        assert!(outcome.hits.is_empty());
        assert!(outcome.detections.is_empty());
    }
}
