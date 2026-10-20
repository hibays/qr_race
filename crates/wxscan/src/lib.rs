#![allow(clippy::all)]
//! Rust port of the wechat_qrcode algorithm from OpenCV contrib: CNN detector,
//! super resolution upscaling and decoding.
//!
//! The module layout follows upstream `opencv_contrib/modules/wechat_qrcode/src`:
//!   - [`detector`] ← `src/detector/**` (SSD detection and affine alignment)
//!   - [`scale`]    ← `src/scale/**` (super resolution upscaling)
//!   - [`wechat_qrcode`] ← `src/wechat_qrcode.cpp` + `decodermgr` + `binarizermgr`
//!
//! Two further parts live in separate crates, since neither is specific to this
//! algorithm:
//!   - [`cvlite`] — the OpenCV functions used here
//!   - [`wxing`]  — the ZXing fork used by WeChat
//!
//! CNN inference sits behind the [`net::Net`] trait, and nothing in the
//! algorithm knows which library runs it. Two backends ship, both in
//! [`backend`]:
//!   - `tflite`, on by default, pulling in [`wxscan_tflite`]
//!   - `tract`, pure Rust, running the ONNX weights with no C dependency at all
//!
//! Without models the pipeline degrades to a plain decoder, since the CNN
//! stages are what this algorithm adds, which is why a backend is on by
//! default.
//!
//! For CoreML, NNAPI or anything else, set `default-features = false` and
//! implement [`net::Net`] for your own type; the trait is the whole contract.
//!
//! The weights themselves are in no crate at all. Pass your own buffers to the
//! constructor, or take the prebuilt ones from
//! <https://github.com/wilinz/wxscan-weights>, which also holds the scripts
//! that convert them from the upstream Caffe models.

mod clock;

pub mod backend;
pub mod decodermgr;
pub mod frame;
pub mod detector;
pub mod net;
pub mod scale;
pub mod wechat_qrcode;

/// The tflite inference backend, available with the default `tflite` feature.
///
/// This is a re-export of [`wxscan_tflite`]; the implementation of [`net::Net`]
/// for its type lives in [`backend`].
#[cfg(feature = "tflite")]
pub use wxscan_tflite as tflite;

pub use wxing::error::{ZXError, ZXResult};
pub use wechat_qrcode::{QRCodeResult, WeChatQRCode};

