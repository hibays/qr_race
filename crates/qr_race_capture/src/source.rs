//! Raw device buffers become grayscale here, and only here.

use qr_race_scan::GrayFrame;

use crate::error::CaptureError;

/// One frame plus the moment the device produced it. The capture core only ever
/// compares timestamps, so epoch or monotonic milliseconds both work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub gray: GrayFrame,
    pub captured_at_ms: u64,
}

/// A platform camera backend.
///
/// Deliberately not `Send`: several native camera APIs (Media Foundation among
/// them) are thread-affine, so the thread that opens a source keeps it.
pub trait CaptureSource {
    /// `Ok(None)` means the stream ended (device unplugged, window closed).
    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError>;

    fn description(&self) -> &str;

    /// The negotiated frame size, when the backend can know it before the first
    /// frame. Used for diagnostics; `None` is a honest answer for test sources.
    fn resolution(&self) -> Option<(u32, u32)> {
        None
    }
}

/// Converts packed RGBA8 to the grayscale frame the scanner wants (BT.601).
pub fn rgba_to_gray(width: u32, height: u32, rgba: &[u8]) -> Result<GrayFrame, CaptureError> {
    rgba_to_gray_strided(width, height, rgba, width as usize * 4)
}

/// Same as [`rgba_to_gray`], for buffers whose rows are padded.
pub fn rgba_to_gray_strided(
    width: u32,
    height: u32,
    rgba: &[u8],
    stride: usize,
) -> Result<GrayFrame, CaptureError> {
    convert(width, height, rgba, stride, ChannelOrder::Rgba)
}

/// Converts packed BGRA8 (what several desktop and Apple stacks hand over).
pub fn bgra_to_gray(width: u32, height: u32, bgra: &[u8]) -> Result<GrayFrame, CaptureError> {
    convert(width, height, bgra, width as usize * 4, ChannelOrder::Bgra)
}

#[derive(Debug, Clone, Copy)]
enum ChannelOrder {
    Rgba,
    Bgra,
}

fn convert(
    width: u32,
    height: u32,
    bytes: &[u8],
    stride: usize,
    order: ChannelOrder,
) -> Result<GrayFrame, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::EmptyDimensions);
    }
    let row_bytes = width as usize * 4;
    if stride < row_bytes {
        return Err(CaptureError::StrideTooSmall {
            stride,
            width: width as usize,
        });
    }
    // The last row may omit the stride padding some drivers never allocate.
    let required = stride * (height as usize - 1) + row_bytes;
    if bytes.len() < required {
        return Err(CaptureError::BufferTooSmall {
            expected: required,
            actual: bytes.len(),
        });
    }

    let mut data = Vec::with_capacity(width as usize * height as usize);
    for row in 0..height as usize {
        let start = row * stride;
        let pixels = &bytes[start..start + row_bytes];
        for pixel in pixels.as_chunks::<4>().0 {
            let (r, g, b) = match order {
                ChannelOrder::Rgba => (pixel[0], pixel[1], pixel[2]),
                ChannelOrder::Bgra => (pixel[2], pixel[1], pixel[0]),
            };
            data.push(luma(r, g, b));
        }
    }
    Ok(GrayFrame::new(width, height, data)?)
}

/// BT.601 luma, integer weights that sum to 256.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((77 * u32::from(r) + 150 * u32::from(g) + 29 * u32::from(b)) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_primary_colors() {
        let rgba = [
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 255, 255,
        ];
        let frame = rgba_to_gray(2, 2, &rgba).unwrap();
        assert_eq!(frame.pixel(0, 0), Some(76));
        assert_eq!(frame.pixel(1, 0), Some(149));
        assert_eq!(frame.pixel(0, 1), Some(28));
        assert_eq!(frame.pixel(1, 1), Some(255));
    }

    #[test]
    fn bgra_swaps_red_and_blue() {
        let bgra = [0, 0, 255, 255];
        let frame = bgra_to_gray(1, 1, &bgra).unwrap();
        assert_eq!(frame.pixel(0, 0), Some(76), "this pixel is red in BGRA");
    }

    #[test]
    fn honours_row_stride() {
        // One pixel wide, two rows, with four bytes of padding per row.
        let stride = 8;
        let mut bytes = vec![0u8; stride * 2];
        bytes[0..4].copy_from_slice(&[255, 0, 0, 255]);
        bytes[stride..stride + 4].copy_from_slice(&[0, 255, 0, 255]);
        let frame = rgba_to_gray_strided(1, 2, &bytes, stride).unwrap();
        assert_eq!(frame.pixel(0, 0), Some(76));
        assert_eq!(frame.pixel(0, 1), Some(149));
    }

    #[test]
    fn rejects_short_buffers_and_small_strides() {
        assert_eq!(
            rgba_to_gray(2, 2, &[0; 15]),
            Err(CaptureError::BufferTooSmall {
                expected: 16,
                actual: 15
            })
        );
        assert_eq!(
            rgba_to_gray_strided(2, 1, &[0; 16], 4),
            Err(CaptureError::StrideTooSmall {
                stride: 4,
                width: 2
            })
        );
        assert_eq!(rgba_to_gray(0, 2, &[]), Err(CaptureError::EmptyDimensions));
    }

    #[test]
    fn the_last_row_may_omit_stride_padding() {
        // Row 0 has 4 bytes of pixel + 4 bytes of padding; row 1 is exact.
        let stride = 8;
        let mut bytes = vec![0u8; stride + 4];
        bytes[0..4].copy_from_slice(&[255, 255, 255, 255]);
        bytes[stride..stride + 4].copy_from_slice(&[0, 0, 0, 255]);
        let frame = rgba_to_gray_strided(1, 2, &bytes, stride).unwrap();
        assert_eq!(frame.pixel(0, 0), Some(255));
        assert_eq!(frame.pixel(0, 1), Some(0));
    }
}
