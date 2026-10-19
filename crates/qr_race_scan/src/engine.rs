//! The scan engine: one frame in, hits and per-stage timings out.
//!
//! With no detector the engine decodes the whole frame (M0). With a detector it
//! crops each proposal, optionally super-resolves crops whose detection box is
//! below the area threshold, and decodes each crop. Super-resolution is never
//! unconditional: the threshold is the whole point of the stage.

use std::collections::HashSet;
use std::time::Duration;

use crate::decoder::ScanHit;
use crate::error::ScanError;
use crate::frame::GrayFrame;
use crate::timing::Instant;

/// Something that finds candidate symbols in a frame (the CNN detector later).
pub trait Detector: Send + Sync {
    fn detect(&self, frame: &GrayFrame) -> Result<Vec<Detection>, ScanError>;

    fn name(&self) -> &'static str;
}

/// Something that enlarges a crop (the WeChat super-resolution stage later).
pub trait SuperResolver: Send + Sync {
    fn resolve(&self, frame: &GrayFrame, detection: &Detection) -> Result<GrayFrame, ScanError>;

    fn name(&self) -> &'static str;
}

/// Something that turns a frame or crop into hits (rxing today).
pub trait Decoder: Send + Sync {
    fn decode(&self, frame: &GrayFrame) -> Result<Vec<ScanHit>, ScanError>;

    fn name(&self) -> &'static str;
}

/// One detector proposal, in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
}

impl Detection {
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// Where a scan spent its time. `total` includes everything, not just the sum.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanTimings {
    pub detect: Duration,
    pub super_res: Duration,
    pub decode: Duration,
    pub total: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    pub hits: Vec<ScanHit>,
    pub timings: ScanTimings,
    pub detections: usize,
    pub super_resolved: usize,
}

/// Orchestrates detect → optional super-resolution → decode.
pub struct ScanEngine {
    decoder: Box<dyn Decoder>,
    detector: Option<Box<dyn Detector>>,
    super_resolver: Option<Box<dyn SuperResolver>>,
    /// A detection box smaller than this many pixels triggers super-resolution.
    super_res_min_area: u64,
    /// White padding added around every crop so the symbol keeps its quiet zone.
    padding: u32,
    dedupe: bool,
    parallel: bool,
}

impl ScanEngine {
    pub fn new(decoder: Box<dyn Decoder>) -> Self {
        Self {
            decoder,
            detector: None,
            super_resolver: None,
            super_res_min_area: 0,
            padding: 8,
            dedupe: true,
            parallel: true,
        }
    }

    pub fn with_detector(mut self, detector: Box<dyn Detector>) -> Self {
        self.detector = Some(detector);
        self
    }

    pub fn with_super_resolver(mut self, resolver: Box<dyn SuperResolver>, min_area: u64) -> Self {
        self.super_resolver = Some(resolver);
        self.super_res_min_area = min_area;
        self
    }

    pub fn with_padding(mut self, padding: u32) -> Self {
        self.padding = padding;
        self
    }

    pub fn without_dedupe(mut self) -> Self {
        self.dedupe = false;
        self
    }

    /// Decodes multiple crops one after another instead of on threads.
    pub fn without_parallel(mut self) -> Self {
        self.parallel = false;
        self
    }

    pub fn decoder_name(&self) -> &'static str {
        self.decoder.name()
    }

    pub fn detector_name(&self) -> Option<&'static str> {
        self.detector.as_ref().map(|detector| detector.name())
    }

    pub fn scan(&self, frame: &GrayFrame) -> Result<ScanOutcome, ScanError> {
        let started = Instant::now();
        let mut timings = ScanTimings::default();
        let mut outcome = ScanOutcome::default();

        let Some(detector) = &self.detector else {
            self.decode_whole(frame, &mut outcome, &mut timings)?;
            timings.total = started.elapsed();
            outcome.timings = timings;
            return Ok(outcome);
        };

        let detect_started = Instant::now();
        let detections = detector.detect(frame)?;
        timings.detect = detect_started.elapsed();
        outcome.detections = detections.len();

        if detections.is_empty() {
            self.decode_whole(frame, &mut outcome, &mut timings)?;
        } else {
            let mut crops = Vec::with_capacity(detections.len());
            for detection in &detections {
                let mut crop = match frame.crop_padded(
                    detection.x,
                    detection.y,
                    detection.width,
                    detection.height,
                    self.padding,
                ) {
                    Ok(crop) => crop,
                    // A proposal that lands entirely outside the frame is a
                    // detector bug, not a reason to lose the other codes.
                    Err(ScanError::BadCrop) => continue,
                    Err(err) => return Err(err),
                };

                if self.should_super_resolve(detection) {
                    let Some(resolver) = &self.super_resolver else {
                        return Err(ScanError::SuperRes("no resolver configured".into()));
                    };
                    let resolve_started = Instant::now();
                    crop = resolver.resolve(&crop, detection)?;
                    timings.super_res += resolve_started.elapsed();
                    outcome.super_resolved += 1;
                }

                crops.push(crop);
            }
            let hits = self.decode_crops(&crops, &mut timings)?;
            outcome.hits = self.maybe_dedupe(hits);
        }

        timings.total = started.elapsed();
        outcome.timings = timings;
        Ok(outcome)
    }

    /// Decodes every crop, in parallel when there is more than one. Multiple
    /// codes in one frame are the case the 30fps budget is hardest on, so they
    /// should not share a single decoder serially.
    fn decode_crops(
        &self,
        crops: &[GrayFrame],
        timings: &mut ScanTimings,
    ) -> Result<Vec<ScanHit>, ScanError> {
        let started = Instant::now();
        let mut hits = Vec::new();

        if self.parallel && crops.len() > 1 && !cfg!(target_arch = "wasm32") {
            let results: Vec<Result<Vec<ScanHit>, ScanError>> = std::thread::scope(|scope| {
                let handles: Vec<_> = crops
                    .iter()
                    .map(|crop| scope.spawn(move || self.decoder.decode(crop)))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| {
                        handle.join().unwrap_or_else(|_| {
                            Err(ScanError::Decode("decode thread panicked".into()))
                        })
                    })
                    .collect()
            });
            for result in results {
                hits.extend(result?);
            }
        } else {
            for crop in crops {
                hits.extend(self.decoder.decode(crop)?);
            }
        }

        timings.decode = started.elapsed();
        Ok(hits)
    }

    fn decode_whole(
        &self,
        frame: &GrayFrame,
        outcome: &mut ScanOutcome,
        timings: &mut ScanTimings,
    ) -> Result<(), ScanError> {
        let started = Instant::now();
        let hits = self.decoder.decode(frame)?;
        timings.decode = started.elapsed();
        outcome.hits = self.maybe_dedupe(hits);
        Ok(())
    }

    fn should_super_resolve(&self, detection: &Detection) -> bool {
        self.super_resolver.is_some()
            && self.super_res_min_area > 0
            && detection.area() < self.super_res_min_area
    }

    fn maybe_dedupe(&self, hits: Vec<ScanHit>) -> Vec<ScanHit> {
        if !self.dedupe {
            return hits;
        }
        let mut seen = HashSet::with_capacity(hits.len());
        hits.into_iter()
            .filter(|hit| seen.insert(hit.text.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn hit(text: &str) -> ScanHit {
        ScanHit {
            text: text.to_string(),
            format: "QR_CODE".into(),
            raw: Vec::new(),
            points: Vec::new(),
        }
    }

    struct CountingDecoder {
        calls: Arc<AtomicUsize>,
        hits: Vec<ScanHit>,
    }

    impl Decoder for CountingDecoder {
        fn decode(&self, _frame: &GrayFrame) -> Result<Vec<ScanHit>, ScanError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.hits.clone())
        }

        fn name(&self) -> &'static str {
            "counting"
        }
    }

    struct FixedDetector {
        detections: Vec<Detection>,
    }

    impl Detector for FixedDetector {
        fn detect(&self, _frame: &GrayFrame) -> Result<Vec<Detection>, ScanError> {
            Ok(self.detections.clone())
        }

        fn name(&self) -> &'static str {
            "fixed"
        }
    }

    struct CountingResolver {
        calls: Arc<AtomicUsize>,
    }

    impl SuperResolver for CountingResolver {
        fn resolve(
            &self,
            frame: &GrayFrame,
            _detection: &Detection,
        ) -> Result<GrayFrame, ScanError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(frame.clone())
        }

        fn name(&self) -> &'static str {
            "counting"
        }
    }

    fn frame() -> GrayFrame {
        GrayFrame::new(8, 8, vec![255; 64]).unwrap()
    }

    fn detection(x: i64, y: i64, width: u32, height: u32) -> Detection {
        Detection {
            x,
            y,
            width,
            height,
            confidence: 0.9,
        }
    }

    #[test]
    fn decodes_the_whole_frame_without_a_detector() {
        let calls = Arc::new(AtomicUsize::new(0));
        let decoder = CountingDecoder {
            calls: calls.clone(),
            hits: vec![hit("a")],
        };
        let engine = ScanEngine::new(Box::new(decoder));
        let outcome = engine.scan(&frame()).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.detections, 0);
        assert_eq!(outcome.super_resolved, 0);
    }

    #[test]
    fn decodes_each_detection_as_its_own_crop() {
        let calls = Arc::new(AtomicUsize::new(0));
        let decoder = CountingDecoder {
            calls: calls.clone(),
            hits: vec![hit("a")],
        };
        let detector = FixedDetector {
            detections: vec![detection(0, 0, 4, 4), detection(4, 4, 4, 4)],
        };
        let engine = ScanEngine::new(Box::new(decoder)).with_detector(Box::new(detector));
        let outcome = engine.scan(&frame()).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(outcome.detections, 2);
        assert_eq!(outcome.hits.len(), 1, "identical text is deduped");
    }

    #[test]
    fn super_resolves_only_small_detections() {
        let calls = Arc::new(AtomicUsize::new(0));
        let sr_calls = Arc::new(AtomicUsize::new(0));
        let decoder = CountingDecoder {
            calls: calls.clone(),
            hits: vec![hit("a")],
        };
        let detector = FixedDetector {
            detections: vec![detection(0, 0, 2, 2), detection(4, 4, 4, 4)],
        };
        let engine = ScanEngine::new(Box::new(decoder))
            .with_detector(Box::new(detector))
            .with_super_resolver(
                Box::new(CountingResolver {
                    calls: sr_calls.clone(),
                }),
                10,
            );
        let outcome = engine.scan(&frame()).unwrap();
        assert_eq!(sr_calls.load(Ordering::SeqCst), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(outcome.super_resolved, 1);
    }

    #[test]
    fn keeps_distinct_hits_in_order() {
        let calls = Arc::new(AtomicUsize::new(0));
        let decoder = CountingDecoder {
            calls: calls.clone(),
            hits: vec![hit("a"), hit("b"), hit("a")],
        };
        let engine = ScanEngine::new(Box::new(decoder));
        let outcome = engine.scan(&frame()).unwrap();
        let texts: Vec<_> = outcome.hits.iter().map(|hit| hit.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b"]);
    }
}
