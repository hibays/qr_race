//! Port of `zxing/luminance_source.{hpp,cpp}` and `imgsource.{hpp,cpp}`.
//!
//! The pipeline input is always single-channel grayscale, so only `ImgSource`
//! is kept; the GreyscaleLuminanceSource and rotated variants of upstream ZXing
//! are unused here and are not ported.

use std::sync::Arc;

use super::common::bytematrix::ByteMatrix;
use crate::error::{illegal_argument, ZXResult};

/// A grayscale image with a crop window. The pixels are shared through `Rc`, so
/// crop and rotate only change the window and copy no data.
#[derive(Clone)]
pub struct ImgSource {
    data: Arc<Vec<u8>>,
    data_width: usize,
    data_height: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl ImgSource {
    pub fn new(pixels: Arc<Vec<u8>>, width: usize, height: usize) -> Self {
        Self {
            data: pixels,
            data_width: width,
            data_height: height,
            left: 0,
            top: 0,
            width,
            height,
        }
    }

    pub fn cropped(
        pixels: Arc<Vec<u8>>,
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        crop_width: usize,
        crop_height: usize,
    ) -> ZXResult<Self> {
        if left + crop_width > width || top + crop_height > height {
            return Err(illegal_argument(
                "Crop rectangle does not fit within image data.",
            ));
        }
        Ok(Self {
            data: pixels,
            data_width: width,
            data_height: height,
            left,
            top,
            width: crop_width,
            height: crop_height,
        })
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn max_size(&self) -> usize {
        self.data_width * self.data_height
    }

    pub fn row(&self, y: usize) -> ZXResult<&[u8]> {
        if y >= self.height {
            return Err(illegal_argument("Requested row is outside the image"));
        }
        let offset = (y + self.top) * self.data_width + self.left;
        Ok(&self.data[offset..offset + self.width])
    }

    /// Port of `getMatrix`: zero-copy for the full image, row-wise copy when cropped.
    pub fn matrix(&self) -> Vec<u8> {
        if self.width == self.data_width && self.height == self.data_height {
            return self.data.as_ref().clone();
        }
        let mut out = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            let off = (self.top + y) * self.data_width + self.left;
            out.extend_from_slice(&self.data[off..off + self.width]);
        }
        out
    }

    pub fn byte_matrix(&self) -> ByteMatrix {
        ByteMatrix::from_vec(self.width, self.height, self.matrix())
    }

    pub fn crop(
        &self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> ZXResult<ImgSource> {
        ImgSource::cropped(
            Arc::clone(&self.data),
            self.data_width,
            self.data_height,
            self.left + left,
            self.top + top,
            width,
            height,
        )
    }

    /// `isRotateSupported()` returns false for ImgSource in the original, so this
    /// is kept for completeness but unused by the pipeline.
    pub fn rotate_counter_clockwise(&self) -> ZXResult<ImgSource> {
        ImgSource::cropped(
            Arc::clone(&self.data),
            self.data_width,
            self.data_height,
            self.top,
            self.left,
            self.height,
            self.width,
        )
    }
}
