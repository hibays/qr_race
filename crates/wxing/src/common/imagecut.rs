//! Port of `zxing/common/imagecut.{hpp,cpp}`: crop a grayscale image by
//! rectangle or by a centered ratio.

use super::bytematrix::ByteMatrix;

pub struct ImageCutResult {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Port of `ImageCut::Cut(uint8_t*, ...)`. The coordinates are inclusive.
#[allow(clippy::too_many_arguments)]
pub fn cut_rect(
    src: &[u8],
    width: i32,
    height: i32,
    top_left_x: i32,
    top_left_y: i32,
    bottom_right_x: i32,
    bottom_right_y: i32,
) -> Option<ImageCutResult> {
    if top_left_x < 0 || top_left_x > bottom_right_x || bottom_right_x >= width {
        return None;
    }
    if top_left_y < 0 || top_left_y > bottom_right_y || bottom_right_y >= height {
        return None;
    }
    let new_w = (bottom_right_x - top_left_x + 1) as usize;
    let new_h = (bottom_right_y - top_left_y + 1) as usize;
    let mut data = Vec::with_capacity(new_w * new_h);
    for y in top_left_y..=bottom_right_y {
        let row = y as usize * width as usize;
        data.extend_from_slice(&src[row + top_left_x as usize..row + bottom_right_x as usize + 1]);
    }
    Some(ImageCutResult {
        data,
        width: new_w,
        height: new_h,
    })
}

/// Port of `ImageCut::Cut(Ref<ByteMatrix>, float ratio, ...)`: take the centered
/// region covering the given ratio.
pub fn cut_ratio(matrix: &ByteMatrix, ratio: f32) -> Option<ImageCutResult> {
    let w = matrix.width() as i32;
    let h = matrix.height() as i32;
    // The original truncates to int
    let min_x = (w as f32 * (1.0 - ratio) / 2.0) as i32;
    let min_y = (h as f32 * (1.0 - ratio) / 2.0) as i32;
    let max_x = (w as f32 * (1.0 + ratio) / 2.0) as i32 - 1;
    let max_y = (h as f32 * (1.0 + ratio) / 2.0) as i32 - 1;
    if min_y < 0 || min_y > max_x || max_x >= w {
        return None;
    }
    if min_x < 0 || min_x > max_x || max_x >= w {
        return None;
    }
    let new_w = (max_x - min_x + 1) as usize;
    let new_h = (max_y - min_y + 1) as usize;
    let mut data = Vec::with_capacity(new_w * new_h);
    for y in 0..new_h {
        for x in 0..new_w {
            data.push(matrix.get(x + min_x as usize, y + min_y as usize));
        }
    }
    Some(ImageCutResult {
        data,
        width: new_w,
        height: new_h,
    })
}
