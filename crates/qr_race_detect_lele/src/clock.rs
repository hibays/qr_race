//! Clock shim: detector timings are irrelevant in WASM; native keeps std time.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub(crate) struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub(crate) fn now() -> Self {
        Self
    }
}

#[cfg(target_arch = "wasm32")]
impl core::ops::Sub for Instant {
    type Output = Elapsed;
    fn sub(self, _earlier: Instant) -> Elapsed {
        Elapsed
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct Elapsed;

#[cfg(target_arch = "wasm32")]
impl Elapsed {
    pub(crate) fn as_micros(&self) -> u128 {
        0
    }
}
