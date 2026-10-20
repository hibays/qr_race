//! Port of `src/detector/align.{hpp,cpp}`.
//!
//! The pipeline only needs two operations: cropping around a detection box with
//! padding, and mapping coordinates back to the source image. The
//! `calcWarpMatrix` and `warpPerspective` branches of the original have no
//! caller in wechat_qrcode.cpp and are not ported.

/// A cropped image, together with its offset within the source image.
pub struct Cropped {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

pub struct Align {
    crop_x: i32,
    crop_y: i32,
    rotate90: bool,
}

impl Default for Align {
    fn default() -> Self {
        Self::new()
    }
}

impl Align {
    pub fn new() -> Self {
        Self { crop_x: 0, crop_y: 0, rotate90: false }
    }

    pub fn set_rotate90(&mut self, v: bool) {
        self.rotate90 = v;
    }

    /// Maps coordinates in the cropped image back to source image coordinates.
    pub fn warp_back(&self, dst_pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
        dst_pts
            .iter()
            .map(|&(x, y)| {
                let src_x = (if self.rotate90 { y } else { x }) + self.crop_x as f32;
                let src_y = (if self.rotate90 { x } else { y }) + self.crop_y as f32;
                (src_x, src_y)
            })
            .collect()
    }

    /// `src_pts` holds the four detection box points, in the order produced by
    /// SSDDetector. Padding is the larger of a proportional amount and a
    /// minimum, and the box is cropped with that padding applied.
    pub fn crop(
        &mut self,
        input: &[u8],
        img_width: usize,
        img_height: usize,
        src_pts: &[(f32, f32); 4],
        padding_w: f32,
        padding_h: f32,
        min_padding: i32,
    ) -> Cropped {
        let x0 = src_pts[0].0 as i32;
        let y0 = src_pts[0].1 as i32;
        let x2 = src_pts[2].0 as i32;
        let y2 = src_pts[2].1 as i32;
        let width = x2 - x0 + 1;
        let height = y2 - y0 + 1;
        let padx = (padding_w * width as f32).max(min_padding as f32) as i32;
        let pady = (padding_h * height as f32).max(min_padding as f32) as i32;

        self.crop_x = (x0 - padx).max(0);
        self.crop_y = (y0 - pady).max(0);
        let end_x = (x2 + padx).min(img_width as i32 - 1);
        let end_y = (y2 + pady).min(img_height as i32 - 1);

        let w = (end_x - self.crop_x + 1).max(1) as usize;
        let h = (end_y - self.crop_y + 1).max(1) as usize;
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = (self.crop_y as usize + y) * img_width + self.crop_x as usize;
            data.extend_from_slice(&input[row..row + w]);
        }

        if self.rotate90 {
            // Transpose
            let mut t = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    t[x * h + y] = data[y * w + x];
                }
            }
            return Cropped { data: t, width: h, height: w };
        }
        Cropped { data, width: w, height: h }
    }
}
