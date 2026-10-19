//! The scan pipeline: detect → on-demand super-resolution → decode.
//!
//! Decoding is rxing. The detector and super-resolver are traits because the
//! WeChat-style CNN stages are per-platform backends; the engine must not change
//! shape when they slot in. Frames arrive as grayscale and never leave native.

mod decoder;
mod detector;
mod engine;
mod error;
mod frame;
#[cfg(feature = "image-io")]
mod image_io;
mod timing;

pub use decoder::{RxingDecoder, ScanHit};
pub use detector::{BilinearSuperResolver, PyramidDetector, pyramid_engine};
pub use engine::{
    Decoder, Detection, Detector, ScanEngine, ScanOutcome, ScanTimings, SuperResolver,
};
pub use error::ScanError;
pub use frame::GrayFrame;
#[cfg(feature = "image-io")]
pub use image_io::{load_gray_bytes, load_gray_file};

/// The three stages every measured scan reports.
pub const PIPELINE_STAGES: [&str; 3] = ["detect", "super_res", "decode"];
