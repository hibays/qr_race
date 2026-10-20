//! Frame preparation: rotate a padded Y plane upright before scanning.
//!
//! Camera frames arrive in sensor orientation with a row stride that may exceed
//! the visible width. Every binding needs the same rotation and compaction
//! step, so it lives here rather than in one of them.

/// Tile side length for the blocked transpose. A (64+1) x 64 working set stays
/// in L1; a naive per-pixel transpose walks one side with a full-row stride and
/// misses the cache on almost every access.
const TILE: usize = 64;

/// Rotate a padded Y plane upright and return a tightly packed grayscale image.
///
/// `rotation` is the clockwise angle in degrees needed to bring the frame
/// upright. Decoding is rotation invariant, but rotating first makes the
/// returned coordinates line up with the displayed image.
pub fn upright_gray(
    src: &[u8],
    width: usize,
    height: usize,
    row_stride: usize,
    rotation: i32,
) -> (Vec<u8>, usize, usize) {
    match ((rotation % 360) + 360) % 360 {
        // 90 clockwise: out (x, y) <- src (y, height-1-x)
        90 => {
            let (ow, oh) = (height, width);
            let mut out = vec![0u8; ow * oh];
            rotate90_tiled(src, width, height, row_stride, &mut out, false);
            (out, ow, oh)
        }
        180 => {
            let mut out = vec![0u8; width * height];
            for y in 0..height {
                let srow = &src[y * row_stride..y * row_stride + width];
                let drow = &mut out[(height - 1 - y) * width..(height - 1 - y) * width + width];
                for (d, s) in drow.iter_mut().zip(srow.iter().rev()) {
                    *d = *s;
                }
            }
            (out, width, height)
        }
        // 90 counter-clockwise: out (x, y) <- src (width-1-y, x)
        270 => {
            let (ow, oh) = (height, width);
            let mut out = vec![0u8; ow * oh];
            rotate90_tiled(src, width, height, row_stride, &mut out, true);
            (out, ow, oh)
        }
        _ => {
            let mut out = Vec::with_capacity(width * height);
            for y in 0..height {
                let off = y * row_stride;
                out.extend_from_slice(&src[off..off + width]);
            }
            (out, width, height)
        }
    }
}

/// Blocked 90 degree rotation. `ccw = false` rotates clockwise.
fn rotate90_tiled(
    src: &[u8],
    width: usize,
    height: usize,
    row_stride: usize,
    out: &mut [u8],
    ccw: bool,
) {
    let ow = height; // output width equals input height
    let mut y0 = 0usize;
    while y0 < height {
        let y1 = (y0 + TILE).min(height);
        let mut x0 = 0usize;
        while x0 < width {
            let x1 = (x0 + TILE).min(width);
            for sy in y0..y1 {
                let srow = &src[sy * row_stride..sy * row_stride + width];
                for sx in x0..x1 {
                    let (dx, dy) = if ccw {
                        (sy, width - 1 - sx)
                    } else {
                        (height - 1 - sy, sx)
                    };
                    out[dy * ow + dx] = srow[sx];
                }
            }
            x0 = x1;
        }
        y0 = y1;
    }
}
