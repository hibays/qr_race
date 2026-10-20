//! Port of `zxing/common/grid_sampler.{hpp,cpp}`: resample an image through a
//! perspective transform into a dimension x dimension module grid.

use super::bitmatrix::BitMatrix;
use super::perspective_transform::PerspectiveTransform;
use crate::error::{reader, ZXResult};

pub fn sample_grid(
    image: &BitMatrix,
    dimension: usize,
    transform: &PerspectiveTransform,
) -> ZXResult<BitMatrix> {
    let mut bits = BitMatrix::square(dimension)?;
    let mut points = vec![0.0f32; dimension << 1];
    let mut outlier: i32 = 0;
    let max_outlier = (dimension * dimension * 3 / 10) as i32 - 1;

    for y in 0..dimension {
        let y_value = y as f32 + 0.5;
        let max = points.len();
        let mut x = 0;
        while x < max {
            points[x] = (x >> 1) as f32 + 0.5;
            points[x + 1] = y_value;
            x += 2;
        }
        transform.transform_points(&mut points);
        // Coarse check only: points outside the image are clamped to the border,
        // and too large an excursion fails
        outlier += check_and_nudge_points(image.width(), image.height(), &mut points)?;
        if outlier >= max_outlier {
            return Err(reader("Over 30% points out of bounds."));
        }
        let mut x = 0;
        while x < max {
            let px = points[x] as i32;
            let py = points[x + 1] as i32;
            if px >= 0
                && py >= 0
                && (px as usize) < image.width()
                && (py as usize) < image.height()
                && image.get(px as usize, py as usize) != 0
            {
                bits.set(x >> 1, y);
            }
            x += 2;
        }
    }
    Ok(bits)
}

/// Returns the number of out-of-bounds points, or an error if any is too far out.
pub fn check_and_nudge_points(width: usize, height: usize, points: &mut [f32]) -> ZXResult<i32> {
    if points.is_empty() {
        return Err(reader("checkAndNudgePoints:: no points!"));
    }
    let size = (points.len() / 2) as i32;
    let width_i = width as i32;
    let height_i = height as i32;
    // The original divides as integers and then multiplies by 3
    let maxborder = (width_i / size * 3) as f32;

    let mut out_count = 0;
    let mut offset = 0;
    while offset + 1 < points.len() {
        let x = points[offset] as i32;
        let y = points[offset + 1] as i32;
        if x < -1 || x > width_i || y < -1 || y > height_i {
            out_count += 1;
            if x as f32 > width_i as f32 + maxborder
                || y as f32 > height_i as f32 + maxborder
                || (x as f32) < -maxborder
                || (y as f32) < -maxborder
            {
                return Err(reader("checkAndNudgePoints::Out of bounds!"));
            }
        }
        if x <= -1 {
            points[offset] = 0.0;
        } else if x >= width_i {
            points[offset] = (width_i - 1) as f32;
        }
        if y <= -1 {
            points[offset + 1] = 0.0;
        } else if y >= height_i {
            points[offset + 1] = (height_i - 1) as f32;
        }
        offset += 2;
    }
    Ok(out_count)
}
