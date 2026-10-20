//! `Instant`, except on wasm, where the host lends its clock.
//!
//! The stage timings throughout the scanner feed the profiling recorders, which
//! are compiled in unconditionally. `std::time::Instant::now()` panics on
//! `wasm32-unknown-unknown` — the platform has no clock the standard library
//! can reach — so there the time comes from the host through
//! `wxscan_host_now_us`, alongside the two inference imports in
//! [`wxscan_ffi::host_net`]. A browser answers it with `performance.now()`.
//!
//! Every host that instantiates the module must provide the import, the same
//! way it must provide `malloc`'s memory: a wasm module cannot read a clock on
//! its own. A host with nothing to offer can return a constant, and every stage
//! then reports zero microseconds, which is what this file did before.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wxscan")]
extern "C" {
    /// Microseconds from any fixed origin the host likes; only differences are
    /// ever read.
    fn wxscan_host_now_us() -> f64;
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub(crate) struct Instant(f64);

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub(crate) fn now() -> Self {
        // SAFETY: the import takes nothing and returns a float; the host cannot
        // make it misbehave beyond returning a number this file does not trust.
        Self(unsafe { wxscan_host_now_us() })
    }
}

#[cfg(target_arch = "wasm32")]
impl core::ops::Sub for Instant {
    type Output = Elapsed;

    fn sub(self, earlier: Instant) -> Elapsed {
        Elapsed(self.0 - earlier.0)
    }
}

/// What subtracting two [`Instant`]s gives on wasm.
#[cfg(target_arch = "wasm32")]
pub(crate) struct Elapsed(f64);

#[cfg(target_arch = "wasm32")]
impl Elapsed {
    /// A host clock that runs backwards, or does not run, reports nothing
    /// rather than an enormous number.
    pub(crate) fn as_micros(&self) -> u128 {
        if self.0 > 0.0 { self.0 as u128 } else { 0 }
    }
}
