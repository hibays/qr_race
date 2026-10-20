//! Port of `zxing/qrcode/qrcode_reader.{hpp,cpp}`: chains detection ->
//! sampling -> decoding together, and on failure retries in turn with the
//! inverted image, several alignment pattern candidates, and several
//! dimension guesses.
//!
//! The original's `smooth()` / `initIntegral()` and the inter-frame
//! statistics (`decodeID` and so on) have no callers in this module and
//! were not ported.

use crate::binarizer::Binarizer;
use crate::common::bitmatrix::BitMatrix;
use crate::common::unicomblock::UnicomBlock;
use crate::result::{DecodeResult, ResultPoint};

use super::decoder::decoder::Decoder;
use super::detector::detector::Detector;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReaderState {
    Start = 0,
    DetectStart = 1,
    DetectFindFinderPattern = 2,
    DetectFindAlignPattern = 3,
    DecodeStart = 4,
    DecodeReadVersion = 5,
    DecodeReadErrorCorrectionLevel = 6,
    DecodeReadCodewords = 7,
    DecodeFinish = 8,
    DetectFailed = 9,
}

#[derive(Default)]
pub struct PossibleQRCodeInfo {
    pub possible_fix: f32,
    pub pattern_possible_fix: f32,
    pub possible_module_size: f32,
    pub qrcode_border: Vec<ResultPoint>,
}

pub struct QRCodeReader {
    decoder: Decoder,
    pub reader_state: ReaderState,
    pub detected_dimension: i32,
    pub possible_module_size: f32,
    pub possible_ap_count: u32,
    pub possible_qrcode_info: PossibleQRCodeInfo,
    /// Reused working matrix. A single decode pass runs over more than ten
    /// bitmaps (4 binarizations x normal/inverted); allocating a new one for
    /// each would repeatedly allocate and clear the bitmap itself plus three
    /// W x H row run-length buffers.
    scratch: BitMatrix,
}

impl Default for QRCodeReader {
    fn default() -> Self {
        Self::new()
    }
}

impl QRCodeReader {
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            reader_state: ReaderState::Start,
            detected_dimension: -1,
            possible_module_size: 0.0,
            possible_ap_count: 0,
            possible_qrcode_info: PossibleQRCodeInfo::default(),
            scratch: BitMatrix::default(),
        }
    }

    /// Decodes with the normal binary image first, then retries with the
    /// inverted image.
    pub fn decode(
        &mut self,
        binarizer: &mut Binarizer,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
    ) -> Vec<DecodeResult> {
        self.decode_cancellable(binarizer, block, use_nn_detector, crate::never_cancel())
    }

    /// Same as [`Self::decode`], but cancellable; see
    /// `FinderPatternFinder::find_cancellable`.
    pub fn decode_cancellable(
        &mut self,
        binarizer: &mut Binarizer,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Vec<DecodeResult> {
        {
            let matrix = match binarizer.black_matrix() {
                Ok(m) => m,
                Err(_) => return Vec::new(),
            };
            let mut image = std::mem::take(&mut self.scratch);
            if image
                .reuse_from(matrix.width(), matrix.height(), matrix.as_slice())
                .is_err()
            {
                self.scratch = image;
                return Vec::new();
            }
            let rst = self.decode_more(&mut image, block, use_nn_detector, cancel);
            self.scratch = image;
            if !rst.is_empty() {
                return rst;
            }
        }
        let inverted = match binarizer.inverted_matrix() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut image = std::mem::take(&mut self.scratch);
        if image
            .reuse_from(inverted.width(), inverted.height(), inverted.as_slice())
            .is_err()
        {
            self.scratch = image;
            return Vec::new();
        }
        let out = self.decode_more(&mut image, block, use_nn_detector, cancel);
        self.scratch = image;
        out
    }

    fn decode_more(
        &mut self,
        image: &mut BitMatrix,
        block: &mut UnicomBlock,
        use_nn_detector: bool,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Vec<DecodeResult> {
        let mut result_list: Vec<DecodeResult> = Vec::new();
        let tr0 = crate::clock::Instant::now();
        block.reset();
        let tr1 = crate::clock::Instant::now();

        let mut detector = Detector::new();
        let det_res = detector.detect_cancellable(image, block, use_nn_detector, cancel);
        let tr2 = crate::clock::Instant::now();
        crate::qrcode::qrcode_reader::record_reader_us(
            (tr1 - tr0).as_micros() as u64,
            (tr2 - tr1).as_micros() as u64,
            detector.possible_pattern_count() as u64,
        );
        if det_res.is_err() {
            self.set_reader_state_from_detector(detector.state);
            return result_list;
        }
        self.set_reader_state_from_detector(detector.state);

        let possible_pattern_count = detector.possible_pattern_count();
        if possible_pattern_count == 0 {
            return result_list;
        }

        let tr3 = crate::clock::Instant::now();
        for i in 0..possible_pattern_count {
            let pattern_info = detector.possible_pattern_results[i].finder_pattern_info;
            self.possible_qrcode_info.pattern_possible_fix = pattern_info.possible_fix;
            // Apart from the first group, skip candidates whose shape is too
            // far from a right triangle
            if pattern_info.angle_possible_fix < 0.6 && i > 0 {
                continue;
            }

            let possible_alignment_count = detector.possible_alignment_count(image, i);
            if possible_alignment_count < 0 {
                continue;
            }
            let possible_alignment_count = possible_alignment_count as usize;
            self.detected_dimension = detector.possible_pattern_results[i].possible_dimension;
            self.possible_module_size = detector.possible_pattern_results[i].possible_module_size;
            self.set_possible_ap_count_by_version(
                detector.possible_pattern_results[i].possible_version,
            );

            let mut pattern_found = false;
            let mut need_try_various_dimensions = vec![false; possible_alignment_count];

            for j in 0..possible_alignment_count {
                if pattern_found {
                    break;
                }
                match self.try_decode_at(&detector, image, i, j, self.detected_dimension) {
                    Some(result) => {
                        result_list.push(result);
                        pattern_found = true;
                        if use_nn_detector {
                            return result_list;
                        }
                    }
                    None => {
                        // The finder patterns match well but the decode score
                        // is very low, so the dimension estimate may be
                        // wrong; retry later with other dimensions
                        if pattern_info.possible_fix > 0.9 && self.decoder.possible_fix < 0.1 {
                            need_try_various_dimensions[j] = true;
                        }
                    }
                }
            }

            for j in 0..possible_alignment_count {
                if pattern_found {
                    break;
                }
                if !need_try_various_dimensions[j] {
                    continue;
                }
                let possible_dimensions = get_possible_dimensions(self.detected_dimension);
                for &dimension in possible_dimensions.iter().skip(1) {
                    if let Some(result) = self.try_decode_at(&detector, image, i, j, dimension) {
                        self.detected_dimension = dimension;
                        result_list.push(result);
                        pattern_found = true;
                        if use_nn_detector {
                            return result_list;
                        }
                    }
                }
            }
        }
        record_loop_us(tr3.elapsed().as_micros() as u64);
        result_list
    }

    fn try_decode_at(
        &mut self,
        detector: &Detector,
        image: &BitMatrix,
        i: usize,
        j: usize,
        dimension: i32,
    ) -> Option<DecodeResult> {
        let detector_result = detector
            .get_result_via_alignment(image, i, j, dimension)
            .ok()?;
        let mut points = detector_result.points.clone();
        let mut bits = detector_result.bits;
        let decoder_result = match self.decoder.decode(&mut bits) {
            Ok(r) => r,
            Err(_) => {
                self.set_decoder_fix(self.decoder.possible_fix, &points);
                self.set_reader_state_from_decoder();
                return None;
            }
        };
        // For a mirrored code, swap the bottom-left and top-right corners back
        if let Some(meta) = decoder_result.other {
            if meta.mirrored && points.len() >= 3 {
                points.swap(0, 2);
            }
        }
        self.set_decoder_fix(self.decoder.possible_fix, &points);
        self.set_reader_state_from_decoder();

        let result = DecodeResult::new(
            decoder_result.text,
            decoder_result.raw_bytes,
            points.clone(),
            decoder_result.charset,
            decoder_result.qrcode_version,
            decoder_result.ec_level,
            decoder_result.charset_mode,
        );
        self.set_succ_fix(&points);
        Some(result)
    }

    fn set_decoder_fix(&mut self, possible_fix: f32, border: &[ResultPoint]) {
        if self.possible_qrcode_info.possible_fix < possible_fix {
            self.possible_qrcode_info.possible_fix = possible_fix;
            self.possible_qrcode_info.possible_module_size = self.possible_module_size;
            self.possible_qrcode_info.qrcode_border = border.iter().take(4).cloned().collect();
        }
    }

    fn set_succ_fix(&mut self, border: &[ResultPoint]) {
        self.possible_qrcode_info.possible_module_size = self.possible_module_size;
        self.possible_qrcode_info.qrcode_border = border.iter().take(4).cloned().collect();
    }

    fn set_possible_ap_count_by_version(&mut self, version: u32) {
        self.possible_ap_count = match version {
            0..=1 => 0,
            2..=6 => 1,
            7..=13 => 2,
            14..=20 => 3,
            21..=27 => 4,
            28..=34 => 5,
            _ => 6,
        };
    }

    fn set_reader_state_from_detector(&mut self, state: super::detector::detector::DetectorState) {
        use super::detector::detector::DetectorState as D;
        self.reader_state = match state {
            D::Start => ReaderState::DetectStart,
            D::FindFinderPattern => ReaderState::DetectFindFinderPattern,
            D::FindAlignPattern => ReaderState::DetectFindAlignPattern,
        };
    }

    fn set_reader_state_from_decoder(&mut self) {
        use super::decoder::decoder::DecoderState as S;
        let target = match self.decoder.state {
            S::NotStart => {
                self.reader_state = ReaderState::DetectFailed;
                return;
            }
            S::Start => ReaderState::DecodeStart,
            S::ReadVersion => ReaderState::DecodeReadVersion,
            S::ReadErrorCorrectionLevel => ReaderState::DecodeReadErrorCorrectionLevel,
            S::ReadCodewords => ReaderState::DecodeReadCodewords,
            S::Finish => ReaderState::DecodeFinish,
        };
        if self.reader_state < target {
            self.reader_state = target;
        }
    }

    pub fn possible_fix(&self) -> f32 {
        self.possible_qrcode_info.possible_fix
    }
}

/// Port of `getPossibleDimentions`: the fallbacks (+-4, +-8) used when the
/// dimension estimate is inaccurate.
pub fn get_possible_dimensions(detect_dimension: i32) -> Vec<i32> {
    let mut out = Vec::new();
    if detect_dimension < 0 {
        return out;
    }
    out.push(detect_dimension);
    if (73..=169).contains(&detect_dimension) {
        out.push(detect_dimension + 4);
        out.push(detect_dimension - 4);
        out.push(detect_dimension - 8);
        out.push(detect_dimension + 8);
    } else if (45..=69).contains(&detect_dimension) {
        out.push(detect_dimension + 4);
        out.push(detect_dimension - 4);
    }
    if detect_dimension == 19 {
        out.push(21);
    }
    out
}

/// Cumulative time in microseconds of each detection/decoding stage; used only
/// for locating bottlenecks.
pub static RD_BLOCK_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RD_DETECT_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RD_LOOP_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RD_PATTERNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RD_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn record_reader_us(block_us: u64, detect_us: u64, patterns: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    RD_BLOCK_US.fetch_add(block_us, Relaxed);
    RD_DETECT_US.fetch_add(detect_us, Relaxed);
    RD_PATTERNS.fetch_add(patterns, Relaxed);
    RD_CALLS.fetch_add(1, Relaxed);
}

pub(crate) fn record_loop_us(us: u64) {
    crate::qrcode::qrcode_reader::RD_LOOP_US.fetch_add(us, std::sync::atomic::Ordering::Relaxed);
}

/// Takes and resets: (block.reset us, detect us, candidate loop us, candidate
/// group count, call count)
pub fn take_reader_us() -> (u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        RD_BLOCK_US.swap(0, Relaxed),
        RD_DETECT_US.swap(0, Relaxed),
        RD_LOOP_US.swap(0, Relaxed),
        RD_PATTERNS.swap(0, Relaxed),
        RD_CALLS.swap(0, Relaxed),
    )
}
