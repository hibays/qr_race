//! Scan errors. A frame with no code is not an error; malformed input is.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScanError {
    #[error("frame dimensions must be non-zero")]
    EmptyFrame,

    #[error("frame data is {actual} bytes, expected {expected}")]
    FrameSize { expected: usize, actual: usize },

    #[error("crop is outside the frame or empty")]
    BadCrop,

    #[error("could not read image: {0}")]
    Image(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("detector failed: {0}")]
    Detector(String),

    #[error("super-resolution failed: {0}")]
    SuperRes(String),
}
