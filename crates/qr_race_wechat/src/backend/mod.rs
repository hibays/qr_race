//! Inference backends behind the `Net` contract.

#[cfg(feature = "st-zrt")]
mod st_zrt;
#[cfg(feature = "tract")]
mod tract;

#[cfg(feature = "st-zrt")]
pub use st_zrt::StZrtNet;
#[cfg(feature = "tract")]
pub use tract::TractNet;

#[cfg(feature = "st-zrt")]
pub type DefaultNet = StZrtNet;
#[cfg(all(not(feature = "st-zrt"), feature = "tract"))]
pub type DefaultNet = TractNet;
