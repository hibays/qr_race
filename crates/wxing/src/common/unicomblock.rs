//! Port of `zxing/common/unicomblock.{hpp,cpp}`: 4-connected components over a
//! BitMatrix, computed lazily by flood fill.
//!
//! FinderPatternFinder uses it to check that each of the three finder patterns
//! lies in a connected component of a plausible size.

use super::bitmatrix::BitMatrix;

pub struct UnicomBlock {
    height: usize,
    width: usize,
    now_idx: u32,
    /// Component label of each pixel; 0 means it has not been computed yet.
    index: Vec<u32>,
    /// The following three are indexed by component label, not by pixel: the
    /// component size and bounding box exist once per component. Storing them
    /// per pixel would mean writing the same value back to every pixel of a
    /// component after each fill, which is 3 x W x H x 4 bytes of writes per
    /// decode_more call and wastes memory bandwidth.
    count: Vec<u32>,
    min_pnt: Vec<i32>,
    max_pnt: Vec<i32>,
    /// Seed stack for the scanline fill, packed as y<<16|x.
    stack: Vec<i32>,
}

impl UnicomBlock {
    pub fn new(max_height: usize, max_width: usize) -> Self {
        let n = max_height * max_width;
        Self {
            height: max_height,
            width: max_width,
            now_idx: 0,
            index: vec![0; n],
            // Labels start at 1, and in the worst case every pixel forms its
            // own component, so n+1 slots are reserved.
            count: vec![0; n + 1],
            min_pnt: vec![0; n + 1],
            max_pnt: vec![0; n + 1],
            stack: Vec::with_capacity(256),
        }
    }

    /// Switches to another image: clearing the per-pixel labels is enough.
    /// count/min/max are indexed by label, are read only for a valid label, and
    /// are rewritten on the next pass, so they need no clearing (as in the
    /// original).
    pub fn reset(&mut self) {
        self.index.fill(0);
        self.now_idx = 0;
    }

    pub fn get_unicom_block_index(&mut self, image: &BitMatrix, y: usize, x: usize) -> u16 {
        if y >= self.height || x >= self.width {
            return 0;
        }
        let pos = y * self.width + x;
        if self.index[pos] == 0 {
            self.bfs(image, y, x);
        }
        self.index[pos] as u16
    }

    pub fn get_unicom_block_size(&mut self, image: &BitMatrix, y: usize, x: usize) -> i32 {
        if y >= self.height || x >= self.width {
            return 0;
        }
        let pos = y * self.width + x;
        if self.index[pos] == 0 {
            self.bfs(image, y, x);
        }
        self.count[self.index[pos] as usize] as i32
    }

    /// Returns (min_y, min_x).
    pub fn get_min_point(&mut self, image: &BitMatrix, y: usize, x: usize) -> Option<(i32, i32)> {
        if y >= self.height || x >= self.width {
            return None;
        }
        let pos = y * self.width + x;
        if self.index[pos] == 0 {
            self.bfs(image, y, x);
        }
        let v = self.min_pnt[self.index[pos] as usize];
        Some((v >> 16, v & 0xFFFF))
    }

    /// Returns (max_y, max_x).
    pub fn get_max_point(&mut self, image: &BitMatrix, y: usize, x: usize) -> Option<(i32, i32)> {
        if y >= self.height || x >= self.width {
            return None;
        }
        let pos = y * self.width + x;
        if self.index[pos] == 0 {
            self.bfs(image, y, x);
        }
        let v = self.max_pnt[self.index[pos] as usize];
        Some((v >> 16, v & 0xFFFF))
    }

    /// Flood fills the 4-connected component containing `(y, x)`, recording
    /// its label for every pixel of the component along with the component size
    /// and bounding box.
    ///
    /// This uses a scanline fill rather than a per-pixel BFS: each step labels a
    /// whole same-color run along a row at once and only pushes seeds for new
    /// runs on the rows above and below. A per-pixel version does four neighbor
    /// checks plus one queue push and pop per pixel; here each pixel costs about
    /// one byte read and one label write, and the accesses are sequential.
    ///
    /// The output is identical to the per-pixel version: `index` holds the same
    /// component label, `count` is the component size plus 1, and min/max are
    /// the bounding box extremes; none of the three depend on visit order.
    fn bfs(&mut self, image: &BitMatrix, y: usize, x: usize) {
        #[cfg(feature = "profiling")]
        let _t = BfsTimer::new();
        self.now_idx += 1;
        let w = self.width;
        let h = self.height;
        let b_value = image.get(x, y);

        let (mut min_x, mut min_y) = (x as i32, y as i32);
        let (mut max_x, mut max_y) = (x as i32, y as i32);
        let mut tail = 0usize;

        self.stack.clear();
        self.stack.push(((y as i32) << 16) | x as i32);

        while let Some(node) = self.stack.pop() {
            let sy = (node >> 16) as usize;
            let sx = (node & 0xFFFF) as usize;
            let row_base = sy * w;
            if self.index[row_base + sx] != 0 || image.get(sx, sy) != b_value {
                continue; // Covered by another run after it was pushed.
            }

            // Extend left and right along this row to the ends of the same-color run.
            let mut l = sx;
            while l > 0 && self.index[row_base + l - 1] == 0 && image.get(l - 1, sy) == b_value {
                l -= 1;
            }
            let mut r = sx;
            while r + 1 < w && self.index[row_base + r + 1] == 0 && image.get(r + 1, sy) == b_value
            {
                r += 1;
            }

            self.index[row_base + l..=row_base + r].fill(self.now_idx);
            tail += r - l + 1;
            min_x = min_x.min(l as i32);
            max_x = max_x.max(r as i32);
            min_y = min_y.min(sy as i32);
            max_y = max_y.max(sy as i32);

            // Rows above and below: push one seed for each new same-color run
            // directly above or below this run.
            for ny in [sy.wrapping_sub(1), sy + 1] {
                if ny >= h {
                    continue; // When sy == 0, wrapping_sub wraps to a huge value, also caught here.
                }
                let nrow = ny * w;
                let mut xx = l;
                while xx <= r {
                    if self.index[nrow + xx] == 0 && image.get(xx, ny) == b_value {
                        self.stack.push(((ny as i32) << 16) | xx as i32);
                        while xx + 1 <= r
                            && self.index[nrow + xx + 1] == 0
                            && image.get(xx + 1, ny) == b_value
                        {
                            xx += 1;
                        }
                    }
                    xx += 1;
                }
            }
        }

        // In the original, iCount starts at 1 and is incremented for each
        // popped point, so the result is the component size plus 1; kept here.
        let mut count = tail as i32 + 1;
        if count >= (1 << 16) - 1 {
            count = 0xFFFF;
        }
        #[cfg(feature = "profiling")]
        BFS_PX.fetch_add(tail as u64, std::sync::atomic::Ordering::Relaxed);
        // Stored once per component; lookups go through the label on a pixel.
        let lab = self.now_idx as usize;
        self.count[lab] = count as u32;
        self.min_pnt[lab] = (min_y << 16) | min_x;
        self.max_pnt[lab] = (max_y << 16) | max_x;
    }
}

#[cfg(feature = "profiling")]
pub use profiling_stats::*;

/// Instrumentation for the flood fill: elapsed time, call count, and pixels
/// visited. Compiled only under the `profiling` feature, because on a noisy
/// image the fill runs 200,000 times per frame, and always-on `Instant::now()`
/// calls alone would cost over ten milliseconds and skew the measurements.
#[cfg(feature = "profiling")]
mod profiling_stats {
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static BFS_US: AtomicU64 = AtomicU64::new(0);
    pub static BFS_N: AtomicU64 = AtomicU64::new(0);
    pub static BFS_PX: AtomicU64 = AtomicU64::new(0);

    pub struct BfsTimer(crate::clock::Instant);

    impl BfsTimer {
        pub fn new() -> Self {
            BFS_N.fetch_add(1, Ordering::Relaxed);
            Self(crate::clock::Instant::now())
        }
    }

    impl Drop for BfsTimer {
        fn drop(&mut self) {
            BFS_US.fetch_add(self.0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// Takes and resets the counters: (accumulated nanoseconds, call count,
    /// pixels visited).
    pub fn take_bfs_stats() -> (u64, u64, u64) {
        (
            BFS_US.swap(0, Ordering::Relaxed),
            BFS_N.swap(0, Ordering::Relaxed),
            BFS_PX.swap(0, Ordering::Relaxed),
        )
    }
}
