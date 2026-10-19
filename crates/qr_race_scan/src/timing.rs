//! A stopwatch that can be switched off.
//!
//! `std::time::Instant::now()` panics on `wasm32-unknown-unknown` (no clock in
//! the bare target). The wasm build disables the `timing` feature, keeps the
//! same code shape, and reports zero durations; the browser measures wall time
//! itself.

#[cfg(feature = "timing")]
pub use std::time::Instant;

#[cfg(not(feature = "timing"))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Instant;

#[cfg(not(feature = "timing"))]
impl Instant {
    pub fn now() -> Self {
        Self
    }

    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}
