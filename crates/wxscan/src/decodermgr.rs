//! Port of `src/decodermgr.{hpp,cpp}` and `src/binarizermgr.{hpp,cpp}`: rotates
//! through four binarizers on the same image, attempting one decode with each.

use std::sync::Arc;

use wxing::binarizer::{Binarizer, BinarizerKind};
use wxing::common::unicomblock::UnicomBlock;
use wxing::luminance_source::ImgSource;
use wxing::qrcode::qrcode_reader::QRCodeReader;
use wxing::result::{DecodeResult, ResultPoint};

pub struct DecoderMgr {
    now_rotate_index: usize,
    next_once_binarizer: Option<BinarizerKind>,
}

impl Default for DecoderMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoderMgr {
    pub fn new() -> Self {
        Self { now_rotate_index: 0, next_once_binarizer: None }
    }

    pub fn cur_binarizer(&self) -> BinarizerKind {
        self.next_once_binarizer
            .unwrap_or(BinarizerKind::ROTATE_ORDER[self.now_rotate_index])
    }

    pub fn switch_binarizer(&mut self) {
        self.now_rotate_index = (self.now_rotate_index + 1) % BinarizerKind::ROTATE_ORDER.len();
    }

    pub fn set_next_once_binarizer(&mut self, kind: Option<BinarizerKind>) {
        self.next_once_binarizer = kind;
    }

    /// Port of `DecoderMgr::decodeImage`.
    ///
    /// Returns (text, four corner points). The corner order matches the
    /// original: zxing yields [bottom-left, top-left, top-right, bottom-right],
    /// which is reordered as (1, 2, 3, 0) into [top-left, top-right,
    /// bottom-right, bottom-left].
    ///
    /// Running the four binarizers in parallel was measured, since they are
    /// independent: on a Snapdragon 865 wall clock rose from 358ms to 400ms.
    /// Nearly all the time is spent in FastWindow (the other three together
    /// take less than half of it), and parallelism only moved that one onto a
    /// little core, where it alone went from 176ms to 398ms. The loop therefore
    /// stays sequential, trying the binarizers in rotation order.
    pub fn decode_image(
        &mut self,
        src: &[u8],
        width: usize,
        height: usize,
        use_nn_detector: bool,
    ) -> Option<Vec<(DecodeResult, Vec<ResultPoint>)>> {
        if width <= 20 || height <= 20 {
            return None; // too little data for a reliable result
        }
        let pixels = Arc::new(src.to_vec());
        let mut block = UnicomBlock::new(height, width);
        let mut reader = QRCodeReader::new();

        // Try each of the four binarizers in turn
        for _ in 0..4 {
            let source = ImgSource::new(Arc::clone(&pixels), width, height);
            let mut binarizer = Binarizer::new(source, self.cur_binarizer());
            let results = reader.decode(&mut binarizer, &mut block, use_nn_detector);
            if !results.is_empty() {
                let binary_method = self.cur_binarizer() as i32;
                let out = results
                    .into_iter()
                    .map(|mut r| {
                        r.binary_method = binary_method;
                        let pts = reorder_points(&r.result_points);
                        (r, pts)
                    })
                    .collect();
                return Some(out);
            }
            self.switch_binarizer();
        }
        None
    }
}

/// Converts the zxing 4-point order to the order exposed by the fork: for each
/// group of four points, take indices 1, 2, 3, 0.
fn reorder_points(points: &[ResultPoint]) -> Vec<ResultPoint> {
    let mut out = Vec::with_capacity(points.len());
    for chunk in points.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        out.push(chunk[1]);
        out.push(chunk[2]);
        out.push(chunk[3]);
        out.push(chunk[0]);
    }
    out
}


