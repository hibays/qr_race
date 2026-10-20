//! Port of `zxing/common/mathutils.hpp`.

#[inline]
pub fn distance(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
pub fn distance_4_int(ax: i32, ay: i32, bx: i32, by: i32) -> f32 {
    (((ax - bx) * (ax - bx) + (ay - by) * (ay - by)) as f32).sqrt()
}

/// Port of `getRangeValues`: normalize [minValue, maxValue] into an ordered
/// interval and clamp it to [min, max].
#[inline]
pub fn get_range_values(min_value: &mut i32, max_value: &mut i32, min: i32, max: i32) {
    let (mut lo, mut hi) = if *min_value < *max_value {
        (*min_value, *max_value)
    } else {
        (*max_value, *min_value)
    };
    lo = lo.max(min);
    hi = hi.min(max);
    *min_value = lo;
    *max_value = hi;
}

#[inline]
pub fn is_in_range(x: f32, y: f32, width: f32, height: f32) -> bool {
    x >= 0.0 && x <= width - 1.0 && y >= 0.0 && y <= height - 1.0
}

#[inline]
pub fn vec_cross(v1: [f32; 2], v2: [f32; 2]) -> f32 {
    v1[0] * v2[1] - v1[1] * v2[0]
}

/// Port of `Stddev`. Note that the original divides by N, not N-1.
pub fn stddev(result_set: &[f32]) -> (f32, f32) {
    let n = result_set.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let sum: f64 = result_set.iter().map(|&v| v as f64).sum();
    let avg = (sum / n as f64) as f32;
    let accum: f64 = result_set
        .iter()
        .map(|&v| ((v - avg) as f64) * ((v - avg) as f64))
        .sum();
    let sd = (accum / n as f64).sqrt() as f32;
    (avg, sd)
}
