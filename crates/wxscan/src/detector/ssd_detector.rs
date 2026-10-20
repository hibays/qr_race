//! Port of `src/detector/ssd_detector.{hpp,cpp}`: SSD detection of QR code
//! bounding boxes.
//!
//! The only difference from upstream is how the network is run. Upstream uses
//! `cv::dnn` on the full caffe graph, including the PriorBox and
//! DetectionOutput layers. Here tflite runs only the convolutional backbone,
//! producing mbox_loc and mbox_conf_flatten, while prior box generation,
//! decoding and NMS happen in Rust in [`super::priorbox`] and
//! [`super::detection_output`]. The two paths agree numerically; see
//! `tests/ssd.rs`.

use cvlite::{blob::blob_from_gray, resize, Interpolation};
use crate::net::Net;

use super::detection_output::{forward as detection_forward, DetectionOutputParams};
use super::priorbox::generate_all_priors;

/// A detection box: four corner points, ordered top-left, top-right,
/// bottom-right, bottom-left.
pub type QuadPoints = [(f32, f32); 4];

pub struct SSDDetector<N: Net> {
    net: N,
    params: DetectionOutputParams,
}

impl<N: Net> SSDDetector<N> {
    pub fn new(net: N) -> Self {
        Self { net, params: DetectionOutputParams::default() }
    }

    /// The decoding and NMS parameters in use.
    pub fn params(&self) -> &DetectionOutputParams {
        &self.params
    }

    /// The same, to change. Loosening `confidence_threshold` recalls more weak
    /// symbols at the cost of false positives; tightening does the reverse.
    pub fn params_mut(&mut self) -> &mut DetectionOutputParams {
        &mut self.params
    }

    pub fn forward(
        &self,
        img: &[u8],
        img_w: usize,
        img_h: usize,
        target_width: usize,
        target_height: usize,
    ) -> Result<Vec<QuadPoints>, String> {
        let t0 = crate::clock::Instant::now();
        let input = resize(img, img_w, img_h, target_width, target_height, Interpolation::Cubic);
        let blob = blob_from_gray(&input, 1.0 / 255.0);
        let t1 = crate::clock::Instant::now();
        let outs = self.net.forward(&blob, &[1, 1, target_height, target_width])?;
        let t2 = crate::clock::Instant::now();
        if outs.len() < 2 {
            return Err(format!("detect model should have 2 outputs, got {}", outs.len()));
        }

        // The two outputs are mbox_loc (4 per box) and mbox_conf_flatten (2 per
        // box); they are told apart by element count rather than by the tensor
        // order inside the tflite model
        let (loc, conf) = if outs[0].data.len() > outs[1].data.len() {
            (&outs[0].data, &outs[1].data)
        } else {
            (&outs[1].data, &outs[0].data)
        };

        let t3 = crate::clock::Instant::now();
        let priors = generate_all_priors(target_width, target_height);
        if priors.len() * 4 != loc.len() {
            return Err(format!(
                "prior/loc mismatch: {} priors vs loc len {}",
                priors.len(),
                loc.len()
            ));
        }

        let t4 = crate::clock::Instant::now();
        let dets = detection_forward(loc, conf, &priors, &self.params);
        let t5 = crate::clock::Instant::now();
        crate::detector::ssd_detector::record_stage_us(
            (t1 - t0).as_micros() as u64,
            (t2 - t1).as_micros() as u64,
            (t4 - t3).as_micros() as u64,
            (t5 - t4).as_micros() as u64,
            target_width,
            target_height,
        );

        let clip = |x: f32, lo: f32, hi: f32| x.max(lo).min(hi);
        let mut point_list = Vec::new();
        for d in dets {
            // As upstream: keep only boxes with label == 1 and score above 1e-5
            if d.label != 1.0 || d.score <= 1e-5 {
                continue;
            }
            let x0 = clip(d.xmin * img_w as f32, 0.0, img_w as f32 - 1.0);
            let y0 = clip(d.ymin * img_h as f32, 0.0, img_h as f32 - 1.0);
            let x1 = clip(d.xmax * img_w as f32, 0.0, img_w as f32 - 1.0);
            let y1 = clip(d.ymax * img_h as f32, 0.0, img_h as f32 - 1.0);
            point_list.push([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        }
        Ok(point_list)
    }
}


/// Accumulated time in microseconds for the stages inside detection, used only
/// for locating bottlenecks.
pub static STAGE_PRE_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static STAGE_NET_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static STAGE_PRIOR_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static STAGE_POST_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static STAGE_INPUT_W: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static STAGE_INPUT_H: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn record_stage_us(pre: u64, net: u64, prior: u64, post: u64, w: usize, h: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    STAGE_PRE_US.fetch_add(pre, Relaxed);
    STAGE_NET_US.fetch_add(net, Relaxed);
    STAGE_PRIOR_US.fetch_add(prior, Relaxed);
    STAGE_POST_US.fetch_add(post, Relaxed);
    STAGE_INPUT_W.store(w as u64, Relaxed);
    STAGE_INPUT_H.store(h as u64, Relaxed);
}

/// Takes the accumulated values and resets them: (pre, net, prior, post, w, h)
pub fn take_stage_us() -> (u64, u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        STAGE_PRE_US.swap(0, Relaxed),
        STAGE_NET_US.swap(0, Relaxed),
        STAGE_PRIOR_US.swap(0, Relaxed),
        STAGE_POST_US.swap(0, Relaxed),
        STAGE_INPUT_W.load(Relaxed),
        STAGE_INPUT_H.load(Relaxed),
    )
}
