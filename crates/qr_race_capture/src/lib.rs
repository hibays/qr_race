//! Capture core: the one place frames become grayscale and the one place the
//! frame rate is policed.
//!
//! Platform backends (capture-android, capture-apple, capture-desktop,
//! capture-wasm) deliver raw buffers through [`CaptureSource`]; everything above
//! this crate only ever sees [`GrayFrame`], which is also what never crosses
//! into Dart.

mod error;
mod pacer;
mod session;
mod source;

pub use error::CaptureError;
pub use pacer::{Pacer, PacerStats};
pub use session::{CaptureConfig, CaptureSession, CaptureStats};
pub use source::{CaptureSource, CapturedFrame, bgra_to_gray, rgba_to_gray, rgba_to_gray_strided};

pub use qr_race_scan::GrayFrame;
