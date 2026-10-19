//! Coarse-to-fine detection and the on-demand super-resolution baseline.
//!
//! These are deliberately model-free implementations of the two stages the
//! WeChat pipeline gets from CNNs: a detector that finds symbols cheaply on a
//! downscaled frame, and a super-resolver that enlarges small crops before
//! decoding. Both sit behind the engine's traits, so a CNN backend replaces
//! them without touching the orchestration.

use crate::decoder::RxingDecoder;
use crate::engine::{Decoder, Detection, Detector, ScanEngine, SuperResolver};
use crate::error::ScanError;
use crate::frame::GrayFrame;

/// Finds symbols by decoding a downscaled frame, then reports their boxes in
/// full-frame coordinates so the engine can re-decode each crop at full detail.
pub struct PyramidDetector {
    scale: u32,
    padding: i64,
    detector_decoder: Box<dyn Decoder>,
}

impl PyramidDetector {
    pub fn new(scale: u32) -> Self {
        Self {
            scale: scale.max(2),
            padding: 4,
            detector_decoder: Box::new(RxingDecoder::new()),
        }
    }

    /// Replaces the cheap decoder (tests use a fake).
    pub fn with_decoder(mut self, decoder: Box<dyn Decoder>) -> Self {
        self.detector_decoder = decoder;
        self
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    fn box_from_points(&self, points: &[[f32; 2]], frame: &GrayFrame) -> Option<Detection> {
        if points.is_empty() {
            return None;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for [x, y] in points {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }

        let scale = self.scale as f32;
        let x = (min_x * scale) as i64 - self.padding;
        let y = (min_y * scale) as i64 - self.padding;
        let width = ((max_x - min_x) * scale) as u32 + 2 * self.padding as u32;
        let height = ((max_y - min_y) * scale) as u32 + 2 * self.padding as u32;
        if width == 0 || height == 0 {
            return None;
        }
        // Clamp the reported box to the frame; the engine clamps again on crop.
        let x = x.max(0).min(i64::from(frame.width()));
        let y = y.max(0).min(i64::from(frame.height()));
        let width = width.min(frame.width().saturating_sub(x as u32));
        let height = height.min(frame.height().saturating_sub(y as u32));
        if width == 0 || height == 0 {
            return None;
        }
        Some(Detection {
            x,
            y,
            width,
            height,
            confidence: 1.0,
        })
    }
}

impl Detector for PyramidDetector {
    fn detect(&self, frame: &GrayFrame) -> Result<Vec<Detection>, ScanError> {
        let small = frame.downscale(self.scale)?;
        let hits = self.detector_decoder.decode(&small)?;
        let mut detections = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(detection) = self.box_from_points(&hit.points, frame) {
                detections.push(detection);
            }
        }
        Ok(detections)
    }

    fn name(&self) -> &'static str {
        "pyramid"
    }
}

/// Bilinear 2x (or Nx) enlargement for small detection boxes. This is the
/// on-demand super-resolution stage's baseline: it only runs when the engine's
/// area threshold says the box is too small to trust at native resolution.
pub struct BilinearSuperResolver {
    factor: u32,
}

impl BilinearSuperResolver {
    pub fn new(factor: u32) -> Self {
        Self {
            factor: factor.max(2),
        }
    }

    pub fn factor(&self) -> u32 {
        self.factor
    }
}

impl SuperResolver for BilinearSuperResolver {
    fn resolve(&self, frame: &GrayFrame, _detection: &Detection) -> Result<GrayFrame, ScanError> {
        frame.upscale_bilinear(self.factor)
    }

    fn name(&self) -> &'static str {
        "bilinear"
    }
}

/// Convenience: the whole coarse-to-fine pipeline with one call.
pub fn pyramid_engine(scale: u32, super_res_min_area: u64) -> ScanEngine {
    ScanEngine::new(Box::new(RxingDecoder::new()))
        .with_detector(Box::new(PyramidDetector::new(scale)))
        .with_super_resolver(Box::new(BilinearSuperResolver::new(2)), super_res_min_area)
}
