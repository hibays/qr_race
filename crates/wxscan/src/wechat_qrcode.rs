//! Port of `src/wechat_qrcode.cpp`: the public detection and decoding entry
//! point.
//!
//! The flow matches upstream:
//!   1. If a CNN detector is present, run it to obtain candidate boxes;
//!      otherwise the whole image is the only candidate.
//!   2. Crop each candidate with padding.
//!   3. Pick a set of scales based on the image size, including super
//!      resolution, and try decoding at each until one succeeds.
//!   4. Divide the point coordinates by the scale and map them back to the
//!      source image.

use cvlite::color;
use crate::decodermgr::DecoderMgr;
use crate::detector::align::Align;
use crate::detector::detection_output::DetectionOutputParams;
use crate::detector::ssd_detector::{QuadPoints, SSDDetector};
use crate::net::Net;
use crate::scale::super_scale::SuperScale;

/// The result of one recognition.
pub struct QRCodeResult {
    /// Raw bytes; the encoding is given by `charset`, and the fork performs no
    /// conversion
    pub bytes: Vec<u8>,
    pub charset: String,
    /// Four corner points in source image coordinates: top-left, top-right,
    /// bottom-right, bottom-left
    pub points: [(f32, f32); 4],
    pub qrcode_version: i32,
    pub ec_level: String,
    pub charset_mode: String,
    pub binary_method: i32,
}

impl QRCodeResult {
    /// Lossy UTF-8 interpretation. GB2312 content should be handled by the
    /// caller with a matching decoder.
    pub fn text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

pub struct WeChatQRCode<N: Net> {
    detector: Option<SSDDetector<N>>,
    super_resolution: SuperScale<N>,
    use_nn_detector: bool,
    use_nn_sr: bool,
    /// Port of `setScaleFactor`: -1 means the factor is derived automatically
    /// from a 400x400 target area
    scale_factor: f32,
}

impl<N: Net> WeChatQRCode<N> {
    /// A scanner with no CNN stages: plain decoding, which still reads ordinary
    /// symbols and loses the detection rate on small or distant ones.
    ///
    /// Equivalent to `new(None, None)`, and clearer at a call site than two
    /// `None`s of an inferred type.
    pub fn without_models() -> Self {
        Self::new(None, None)
    }

    /// Passing None for `detector_net` or `sr_net` selects the upstream
    /// fallback mode for an empty model path.
    ///
    /// The two arguments have the same type, so passing them the wrong way
    /// round compiles; [`builder`](Self::builder) names them instead.
    pub fn new(detector_net: Option<N>, sr_net: Option<N>) -> Self {
        let use_nn_detector = detector_net.is_some();
        let use_nn_sr = sr_net.is_some();
        Self {
            detector: detector_net.map(SSDDetector::new),
            super_resolution: SuperScale::new(sr_net),
            use_nn_detector,
            use_nn_sr,
            scale_factor: -1.0,
        }
    }

    pub fn set_scale_factor(&mut self, v: f32) {
        self.scale_factor = if v > 0.0 && v <= 1.0 { v } else { -1.0 };
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Whether the SSD detector network is loaded.
    pub fn has_detector(&self) -> bool {
        self.use_nn_detector
    }

    /// Whether the super resolution network is loaded.
    pub fn has_super_resolution(&self) -> bool {
        self.use_nn_sr
    }

    /// Starts a [`Builder`], which names the two networks rather than relying
    /// on argument order.
    pub fn builder() -> Builder<N> {
        Builder { detector_net: None, sr_net: None, scale_factor: -1.0, params: None }
    }

    /// The detector's decoding and NMS parameters, if a detector is loaded.
    ///
    /// Without models there is no detector and this is None.
    pub fn detection_params(&self) -> Option<&DetectionOutputParams> {
        self.detector.as_ref().map(|d| d.params())
    }

    /// The same, to change. Loosening `confidence_threshold` recalls more weak
    /// symbols at the cost of false positives; tightening does the reverse.
    pub fn detection_params_mut(&mut self) -> Option<&mut DetectionOutputParams> {
        self.detector.as_mut().map(|d| d.params_mut())
    }

    /// Takes a grayscale image.
    pub fn detect_and_decode_gray(
        &self,
        img: &[u8],
        width: usize,
        height: usize,
    ) -> Vec<QRCodeResult> {
        self.detect_and_decode_gray_timed(img, width, height).0
    }

    /// Same as [`Self::detect_and_decode_gray`], and additionally returns the
    /// candidate boxes from the detection stage.
    ///
    /// Non-empty candidates with no results means a code was seen but could not
    /// be decoded, usually because it is too small or too blurry. A caller can
    /// use that to zoom in automatically; see the auto-zoom strategy on the
    /// Dart side.
    pub fn detect_and_decode_gray_with_candidates(
        &self,
        img: &[u8],
        width: usize,
        height: usize,
    ) -> (Vec<QRCodeResult>, Vec<QuadPoints>) {
        if width <= 20 || height <= 20 {
            return (Vec::new(), Vec::new());
        }
        let candidates = self.detect(img, width, height);
        let results = self.decode(img, width, height, &candidates);
        (results, candidates)
    }

    /// Same as [`Self::detect_and_decode_gray`], and additionally returns the
    /// microseconds spent in detection and in decoding, for locating
    /// bottlenecks.
    pub fn detect_and_decode_gray_timed(
        &self,
        img: &[u8],
        width: usize,
        height: usize,
    ) -> (Vec<QRCodeResult>, u64, u64) {
        if width <= 20 || height <= 20 {
            return (Vec::new(), 0, 0);
        }
        let t0 = crate::clock::Instant::now();
        let candidates = self.detect(img, width, height);
        let t1 = crate::clock::Instant::now();
        let results = self.decode(img, width, height, &candidates);
        let t2 = crate::clock::Instant::now();
        (
            results,
            (t1 - t0).as_micros() as u64,
            (t2 - t1).as_micros() as u64,
        )
    }

    /// Takes an interleaved BGR image, equivalent to the upstream
    /// `cvtColor(BGR2GRAY)` followed by the grayscale path.
    pub fn detect_and_decode_bgr(
        &self,
        img: &[u8],
        width: usize,
        height: usize,
    ) -> Vec<QRCodeResult> {
        let gray = color::bgr_to_gray(img, width, height);
        self.detect_and_decode_gray(&gray, width, height)
    }

    fn detect(&self, img: &[u8], width: usize, height: usize) -> Vec<QuadPoints> {
        if let Some(detector) = self.detector.as_ref() {
            let target_area = 400.0f32 * 400.0;
            let tmp_scale = if self.scale_factor == -1.0 {
                1.0f32.min((target_area / (width * height) as f32).sqrt())
            } else {
                self.scale_factor
            };
            let detect_width = (width as f32 * tmp_scale) as usize;
            let detect_height = (height as f32 * tmp_scale) as usize;
            if detect_width > 0 && detect_height > 0 {
                match detector.forward(img, width, height, detect_width, detect_height) {
                    Ok(points) => return points,
                    // Not fatal, but not nothing either: a detector that errors
                    // reports no candidates, which is indistinguishable from a
                    // frame with no code in it. Saying so is the difference
                    // between a bug that is found and one that is not.
                    Err(e) => eprintln!("wxscan: the detector failed on a {detect_width}x{detect_height} input: {e}"),
                }
            }
            return Vec::new();
        }
        // No CNN detector: the whole image is the only candidate
        let w = width as f32 - 1.0;
        let h = height as f32 - 1.0;
        vec![[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]]
    }

    fn decode(
        &self,
        img: &[u8],
        width: usize,
        height: usize,
        candidates: &[QuadPoints],
    ) -> Vec<QRCodeResult> {
        // The structure mirrors upstream `WeChatQRCode::Impl::decode`,
        // including two of its properties:
        //   * `check_points` is scoped to a single decodeImage call, so
        //     duplicates across candidate boxes are not removed; when SSD
        //     returns two boxes for the same code, upstream does emit two
        //     identical results;
        //   * the duplicate test compares only against the last entry in
        //     check_points, because the inner loop of the original overwrites
        //     isDuplicate.
        // Both are reproduced here, leaving deduplication to the caller.
        let mut decode_results: Vec<QRCodeResult> = Vec::new();

        for point in candidates.iter() {
            let mut aligner = Align::new();
            let (cropped, cw, ch) = if self.use_nn_detector {
                let c = aligner.crop(img, width, height, point, 0.1, 0.1, 15);
                (c.data, c.width, c.height)
            } else {
                (img.to_vec(), width, height)
            };

            for cur_scale in get_scale_list(cw, ch) {
                let ts0 = crate::clock::Instant::now();
                let (scaled, sw, sh) = self.super_resolution.process_image_scale(
                    &cropped,
                    cw,
                    ch,
                    cur_scale,
                    self.use_nn_sr,
                    160,
                );
                let ts1 = crate::clock::Instant::now();
                let mut mgr = DecoderMgr::new();
                let decoded_opt = mgr.decode_image(&scaled, sw, sh, self.use_nn_detector);
                let ts2 = crate::clock::Instant::now();
                record_decode_stage_us(
                    (ts1 - ts0).as_micros() as u64,
                    (ts2 - ts1).as_micros() as u64,
                    sw,
                    sh,
                );
                let decoded = match decoded_opt {
                    Some(d) => d,
                    None => continue,
                };

                // The original pushes the text into decode_results inside
                // decodeImage
                let base = decode_results.len();
                let mut quads: Vec<[(f32, f32); 4]> = Vec::new();
                for (result, pts) in decoded {
                    if pts.len() < 4 {
                        continue;
                    }
                    let scaled_back: Vec<(f32, f32)> =
                        pts[..4].iter().map(|p| (p.x / cur_scale, p.y / cur_scale)).collect();
                    let mapped = if self.use_nn_detector {
                        aligner.warp_back(&scaled_back)
                    } else {
                        scaled_back
                    };
                    let quad: [(f32, f32); 4] = [mapped[0], mapped[1], mapped[2], mapped[3]];
                    quads.push(quad);
                    decode_results.push(QRCodeResult {
                        bytes: result.text,
                        charset: result.charset,
                        points: quad,
                        qrcode_version: result.qrcode_version,
                        ec_level: result.ec_level,
                        charset_mode: result.charset_mode,
                        binary_method: result.binary_method,
                    });
                }

                let mut check_points: Vec<[(f32, f32); 4]> = Vec::new();
                let mut removed = 0usize;
                for (i, quad) in quads.iter().enumerate() {
                    if is_duplicate(&check_points, quad) {
                        // The original calls erase(decode_results.begin()+i),
                        // indexing into the whole accumulated list
                        let idx = i.saturating_sub(removed);
                        if idx < decode_results.len() {
                            decode_results.remove(idx);
                            removed += 1;
                        }
                    } else {
                        check_points.push(*quad);
                    }
                }
                let _ = base;
                break; // this candidate decoded; skip the remaining scales
            }
        }
        decode_results
    }
}

/// Builds a [`WeChatQRCode`] with the networks named rather than positional.
///
/// ```no_run
/// # use wxscan::{WeChatQRCode, net::NoNet};
/// let scanner = WeChatQRCode::<NoNet>::builder()
///     .confidence_threshold(0.15)
///     .build();
/// ```
pub struct Builder<N: Net> {
    detector_net: Option<N>,
    sr_net: Option<N>,
    scale_factor: f32,
    params: Option<DetectionOutputParams>,
}

impl<N: Net> Builder<N> {
    /// The SSD detector network. Without it the pipeline decodes without
    /// locating symbols first.
    pub fn detector(mut self, net: N) -> Self {
        self.detector_net = Some(net);
        self
    }

    /// The super resolution network, used to upscale small crops before
    /// decoding.
    pub fn super_resolution(mut self, net: N) -> Self {
        self.sr_net = Some(net);
        self
    }

    /// Scales the image down before detection. Values outside `(0, 1]` restore
    /// the default, which targets an area of 400x400.
    pub fn scale_factor(mut self, v: f32) -> Self {
        self.scale_factor = v;
        self
    }

    /// The detector's decoding and NMS parameters, replaced wholesale.
    pub fn detection_params(mut self, params: DetectionOutputParams) -> Self {
        self.params = Some(params);
        self
    }

    /// How confident the detector must be to report a candidate, 0.2 by
    /// default. Lower recalls more weak symbols and more false positives.
    pub fn confidence_threshold(mut self, v: f32) -> Self {
        self.params.get_or_insert_with(Default::default).confidence_threshold = v;
        self
    }

    /// The IoU above which two overlapping candidates are treated as one, 0.45
    /// by default.
    pub fn nms_threshold(mut self, v: f32) -> Self {
        self.params.get_or_insert_with(Default::default).nms_threshold = v;
        self
    }

    pub fn build(self) -> WeChatQRCode<N> {
        let mut scanner = WeChatQRCode::new(self.detector_net, self.sr_net);
        scanner.set_scale_factor(self.scale_factor);
        if let (Some(params), Some(target)) = (self.params, scanner.detection_params_mut()) {
            *target = params;
        }
        scanner
    }
}

/// Port of `Impl::getScaleList`.
pub fn get_scale_list(width: usize, height: usize) -> Vec<f32> {
    if width < 320 || height < 320 {
        return vec![1.0, 2.0, 0.5];
    }
    if width < 640 && height < 640 {
        return vec![1.0, 0.5];
    }
    vec![0.5, 1.0]
}

/// Port of the deduplication in the original: two results are duplicates when
/// all four corner points are within 10px of each other.
///
/// The inner loop of the original repeatedly overwrites `isDuplicate`, so only
/// the last entry in check_points decides the outcome. That behavior is
/// reproduced here.
fn is_duplicate(check_points: &[[(f32, f32); 4]], quad: &[(f32, f32); 4]) -> bool {
    const EPS: f32 = 10.0;
    match check_points.last() {
        None => false,
        Some(tmp) => tmp
            .iter()
            .zip(quad.iter())
            .all(|(a, b)| (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() < EPS),
    }
}


/// Accumulated time in microseconds for the stages inside decoding, used only
/// for locating bottlenecks.
pub static DEC_SR_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static DEC_ZXING_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static DEC_TRIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static DEC_LAST_W: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static DEC_LAST_H: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn record_decode_stage_us(sr: u64, zxing: u64, w: usize, h: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    DEC_SR_US.fetch_add(sr, Relaxed);
    DEC_ZXING_US.fetch_add(zxing, Relaxed);
    DEC_TRIES.fetch_add(1, Relaxed);
    DEC_LAST_W.store(w as u64, Relaxed);
    DEC_LAST_H.store(h as u64, Relaxed);
}

/// Takes the values and resets them: (super resolution us, zxing us, attempts,
/// last width, last height)
pub fn take_decode_stage_us() -> (u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        DEC_SR_US.swap(0, Relaxed),
        DEC_ZXING_US.swap(0, Relaxed),
        DEC_TRIES.swap(0, Relaxed),
        DEC_LAST_W.load(Relaxed),
        DEC_LAST_H.load(Relaxed),
    )
}
