//! A single grayscale frame, the only image type the pipeline moves.

use crate::error::ScanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayFrame {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl GrayFrame {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ScanError> {
        if width == 0 || height == 0 {
            return Err(ScanError::EmptyFrame);
        }
        let expected = width as usize * height as usize;
        if data.len() != expected {
            return Err(ScanError::FrameSize {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        self.data.get(index).copied()
    }

    /// Crops to the clamped intersection of the requested box and the frame.
    /// An empty intersection is an error, not a silently empty frame.
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Self, ScanError> {
        let left = x.min(self.width);
        let top = y.min(self.height);
        let right = x.saturating_add(width).min(self.width);
        let bottom = y.saturating_add(height).min(self.height);
        if right <= left || bottom <= top {
            return Err(ScanError::BadCrop);
        }
        let out_width = right - left;
        let out_height = bottom - top;
        let mut data = Vec::with_capacity(out_width as usize * out_height as usize);
        for row in top..bottom {
            let start = (row as usize) * (self.width as usize) + (left as usize);
            let end = start + out_width as usize;
            data.extend_from_slice(&self.data[start..end]);
        }
        Self::new(out_width, out_height, data)
    }

    /// Pads a crop with white so a symbol keeps the quiet zone its decoder needs.
    pub fn crop_padded(
        &self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        padding: u32,
    ) -> Result<Self, ScanError> {
        let pad = i64::from(padding);
        let left = (x - pad).max(0);
        let top = (y - pad).max(0);
        let right = (x + i64::from(width) + pad).min(i64::from(self.width));
        let bottom = (y + i64::from(height) + pad).min(i64::from(self.height));
        if right <= left || bottom <= top {
            return Err(ScanError::BadCrop);
        }
        self.crop(
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    }

    /// Box-filtered downscale. Used by the coarse detection pass: the decoder
    /// looks for symbols on a smaller frame, then the engine re-decodes crops.
    pub fn downscale(&self, factor: u32) -> Result<Self, ScanError> {
        if factor <= 1 {
            return Ok(self.clone());
        }
        let width = (self.width / factor).max(1);
        let height = (self.height / factor).max(1);
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0u32;
                let mut count = 0u32;
                for dy in 0..factor {
                    for dx in 0..factor {
                        if let Some(pixel) = self.pixel(x * factor + dx, y * factor + dy) {
                            sum += u32::from(pixel);
                            count += 1;
                        }
                    }
                }
                data.push(sum.checked_div(count).unwrap_or(0) as u8);
            }
        }
        Self::new(width, height, data)
    }

    /// Bilinear upscale for the on-demand super-resolution stage. A CNN
    /// super-resolver replaces this behind the same trait; this one is the
    /// measurable baseline and the fallback when no model is shipped.
    pub fn upscale_bilinear(&self, factor: u32) -> Result<Self, ScanError> {
        if factor <= 1 {
            return Ok(self.clone());
        }
        let width = self.width * factor;
        let height = self.height * factor;
        let mut data = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            let source_y = ((y as f32 + 0.5) / factor as f32 - 0.5)
                .clamp(0.0, self.height.saturating_sub(1) as f32);
            let y0 = source_y.floor() as u32;
            let y1 = (y0 + 1).min(self.height - 1);
            let wy = source_y - y0 as f32;

            for x in 0..width {
                let source_x = ((x as f32 + 0.5) / factor as f32 - 0.5)
                    .clamp(0.0, self.width.saturating_sub(1) as f32);
                let x0 = source_x.floor() as u32;
                let x1 = (x0 + 1).min(self.width - 1);
                let wx = source_x - x0 as f32;

                let top = lerp(
                    f32::from(self.pixel(x0, y0).unwrap_or(0)),
                    f32::from(self.pixel(x1, y0).unwrap_or(0)),
                    wx,
                );
                let bottom = lerp(
                    f32::from(self.pixel(x0, y1).unwrap_or(0)),
                    f32::from(self.pixel(x1, y1).unwrap_or(0)),
                    wx,
                );
                data.push(lerp(top, bottom, wy).round().clamp(0.0, 255.0) as u8);
            }
        }
        Self::new(width, height, data)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_dimensions_and_lengths() {
        assert_eq!(
            GrayFrame::new(0, 10, Vec::new()),
            Err(ScanError::EmptyFrame)
        );
        assert_eq!(
            GrayFrame::new(2, 2, vec![0; 3]),
            Err(ScanError::FrameSize {
                expected: 4,
                actual: 3
            })
        );
    }

    #[test]
    fn crops_and_clamps() {
        let frame = GrayFrame::new(4, 4, (0..16).collect()).unwrap();
        let crop = frame.crop(1, 1, 2, 2).unwrap();
        assert_eq!(crop.data(), &[5u8, 6, 9, 10]);
        assert_eq!(crop.area(), 4);

        let clamped = frame.crop(3, 3, 10, 10).unwrap();
        assert_eq!(clamped.data(), &[15u8]);
        assert_eq!(frame.crop(4, 0, 1, 1), Err(ScanError::BadCrop));
        assert_eq!(frame.pixel(3, 3), Some(15));
        assert_eq!(frame.pixel(4, 3), None);
    }

    #[test]
    fn padded_crop_keeps_the_quiet_zone() {
        let frame = GrayFrame::new(8, 8, vec![255; 64]).unwrap();
        let crop = frame.crop_padded(2, 2, 2, 2, 1).unwrap();
        assert_eq!((crop.width(), crop.height()), (4, 4));
        let crop = frame.crop_padded(-1, -1, 2, 2, 1).unwrap();
        assert_eq!((crop.width(), crop.height()), (2, 2));
        assert_eq!(
            frame.crop_padded(-3, -3, 2, 2, 1),
            Err(ScanError::BadCrop),
            "a box entirely outside the frame has nothing to decode"
        );
    }

    #[test]
    fn downscale_averages_blocks() {
        let frame = GrayFrame::new(4, 2, vec![0, 10, 20, 30, 40, 50, 60, 70]).unwrap();
        let small = frame.downscale(2).unwrap();
        assert_eq!((small.width(), small.height()), (2, 1));
        assert_eq!(small.data(), &[25u8, 45], "each output is a 2x2 box mean");
    }

    #[test]
    fn upscale_doubles_the_grid_and_keeps_the_edges() {
        let frame = GrayFrame::new(2, 1, vec![0, 100]).unwrap();
        let big = frame.upscale_bilinear(2).unwrap();
        assert_eq!((big.width(), big.height()), (4, 2));
        assert_eq!(big.pixel(0, 0), Some(0));
        assert_eq!(big.pixel(3, 0), Some(100));
        assert!(big.pixel(1, 0).unwrap() <= big.pixel(2, 0).unwrap());
    }

    #[test]
    fn scale_one_is_the_identity() {
        let frame = GrayFrame::new(2, 2, vec![1, 2, 3, 4]).unwrap();
        assert_eq!(frame.downscale(1).unwrap(), frame);
        assert_eq!(frame.upscale_bilinear(1).unwrap(), frame);
    }
}
