//! Capture failures. A device that ends is not an error; a device that lies
//! about its buffer size is.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaptureError {
    #[error("frame dimensions must be non-zero")]
    EmptyDimensions,

    #[error("frame buffer is {actual} bytes, expected at least {expected}")]
    BufferTooSmall { expected: usize, actual: usize },

    #[error("row stride {stride} is smaller than the {width}-pixel row needs")]
    StrideTooSmall { stride: usize, width: usize },

    #[error("target frame rate must be greater than zero")]
    ZeroFrameRate,

    #[error("capture source failed: {0}")]
    Source(String),

    #[error(transparent)]
    Frame(#[from] qr_race_scan::ScanError),
}
