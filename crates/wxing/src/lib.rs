#![allow(clippy::all)]
//! Rust port of the ZXing fork used by WeChat's QR code implementation.
//!
//! The name follows the precedent of [`rxing`](https://crates.io/crates/rxing),
//! the Rust port of ZXing: this is a port of the **W**eChat fork. That fork
//! differs from upstream ZXing in several places, among them the N:1:3:1:1
//! finder pattern ratio, a rotation over four binarizers, and finder pattern
//! validation by connected-component area. The port follows the C++ sources
//! closely.
//!
//! The only dependency is [`cvlite`], for adaptive Gaussian thresholding and
//! rounding. Nothing here involves the CNN stage, so this crate can be used on
//! its own to decode QR codes. The CNN detector and super resolution stages
//! live in [`wxscan`](https://crates.io/crates/wxscan).

mod clock;

pub mod binarizer;
pub mod common;
pub mod error;
pub mod luminance_source;
pub mod qrcode;
pub mod result;

/// A cancellation flag that is never set, for call sites that cannot be cancelled.
pub fn never_cancel() -> &'static std::sync::atomic::AtomicBool {
    static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &NEVER
}
