//! Loads a still image into a gray frame. Live capture feeds frames directly.

use std::path::Path;

use crate::error::ScanError;
use crate::frame::GrayFrame;

pub fn load_gray_file(path: impl AsRef<Path>) -> Result<GrayFrame, ScanError> {
    let path = path.as_ref();
    let image =
        image::open(path).map_err(|err| ScanError::Image(format!("{}: {err}", path.display())))?;
    gray_from_image(image)
}

pub fn load_gray_bytes(bytes: &[u8]) -> Result<GrayFrame, ScanError> {
    let image = image::load_from_memory(bytes).map_err(|err| ScanError::Image(err.to_string()))?;
    gray_from_image(image)
}

fn gray_from_image(image: image::DynamicImage) -> Result<GrayFrame, ScanError> {
    let luma = image.to_luma8();
    let (width, height) = luma.dimensions();
    GrayFrame::new(width, height, luma.into_raw())
}
