//! Port of `zxing/result.{hpp,cpp}`, `common/decoder_result.hpp`,
//! `common/detector_result.hpp` and `resultpoint.{hpp,cpp}`.
//!
//! The original splits these across several reference-counted classes; this
//! port merges them into one file with value semantics.

use super::common::bitmatrix::BitMatrix;
use super::qrcode::error_correction_level::ErrorCorrectionLevel;

/// Port of `ResultPoint`: a point in image coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResultPoint {
    pub x: f32,
    pub y: f32,
}

impl ResultPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Port of `ResultPoint::distance`.
    pub fn distance(a: &ResultPoint, b: &ResultPoint) -> f32 {
        super::common::mathutils::distance(a.x, a.y, b.x, b.y)
    }

    /// Port of `ResultPoint::orderBestPatterns`: order three finder patterns as
    /// (bottom-left, top-left, top-right).
    pub fn order_best_patterns(patterns: &mut [ResultPoint; 3]) {
        // The point opposite the longest edge is the top-left corner
        let zero_one = Self::distance(&patterns[0], &patterns[1]);
        let one_two = Self::distance(&patterns[1], &patterns[2]);
        let zero_two = Self::distance(&patterns[0], &patterns[2]);

        let (mut point_a, point_b, mut point_c);
        if one_two >= zero_one && one_two >= zero_two {
            point_b = patterns[0];
            point_a = patterns[1];
            point_c = patterns[2];
        } else if zero_two >= one_two && zero_two >= zero_one {
            point_b = patterns[1];
            point_a = patterns[0];
            point_c = patterns[2];
        } else {
            point_b = patterns[2];
            point_a = patterns[0];
            point_c = patterns[1];
        }

        // The cross product fixes the handedness
        if cross_product_z(&point_a, &point_b, &point_c) < 0.0 {
            std::mem::swap(&mut point_a, &mut point_c);
        }
        patterns[0] = point_a;
        patterns[1] = point_b;
        patterns[2] = point_c;
    }
}

/// Port of `ResultPoint::crossProductZ`.
fn cross_product_z(a: &ResultPoint, b: &ResultPoint, c: &ResultPoint) -> f32 {
    let bx = b.x;
    let by = b.y;
    ((c.x - bx) * (a.y - by)) - ((c.y - by) * (a.x - bx))
}

/// Port of `DetectorResult`: the output of the detection stage.
pub struct DetectorResult {
    pub bits: BitMatrix,
    pub points: Vec<ResultPoint>,
    pub dimension: i32,
    pub modulesize: f32,
}

/// Port of `QRCodeDecoderMetaData`.
#[derive(Clone, Copy, Debug, Default)]
pub struct QRCodeDecoderMetaData {
    pub mirrored: bool,
}

/// Port of `DecoderResult`.
///
/// `text` holds raw bytes: the fork compiles with `NO_ICONV_INSIDE` always
/// true, so `DecodedBitStreamParser::append` performs no charset conversion and
/// only appends bytes. The encoding is reported through the `charset` field
/// ("GB2312" for HANZI segments, "UTF-8" otherwise).
pub struct DecoderResult {
    pub raw_bytes: Vec<u8>,
    pub text: Vec<u8>,
    pub byte_segments: Vec<Vec<u8>>,
    pub ec_level: String,
    pub charset: String,
    pub qrcode_version: i32,
    pub charset_mode: String,
    pub other: Option<QRCodeDecoderMetaData>,
}

/// Port of `Result`: the complete output of one successful decode.
pub struct DecodeResult {
    pub text: Vec<u8>,
    pub raw_bytes: Vec<u8>,
    pub result_points: Vec<ResultPoint>,
    pub charset: String,
    pub qrcode_version: i32,
    pub pyramid_lv: i32,
    pub binary_method: i32,
    pub ec_level: String,
    pub charset_mode: String,
    pub scale_list: String,
    pub decode_scale: f32,
}

impl DecodeResult {
    pub fn new(
        text: Vec<u8>,
        raw_bytes: Vec<u8>,
        result_points: Vec<ResultPoint>,
        charset: String,
        qrcode_version: i32,
        ec_level: String,
        charset_mode: String,
    ) -> Self {
        Self {
            text,
            raw_bytes,
            result_points,
            charset,
            qrcode_version,
            pyramid_lv: 0,
            binary_method: 0,
            ec_level,
            charset_mode,
            scale_list: String::new(),
            decode_scale: 1.0,
        }
    }

    /// Port of `Result::enlargeResultPoints`: scale coordinates back to the
    /// original image size by pyramid level.
    pub fn enlarge_result_points(&mut self, scale: i32) {
        for p in self.result_points.iter_mut() {
            p.x *= scale as f32;
            p.y *= scale as f32;
        }
    }
}

/// Equivalent of the original `(string)ecLevel` for `ErrorCorrectionLevel`.
pub fn ec_level_name(level: ErrorCorrectionLevel) -> String {
    level.name().to_string()
}
