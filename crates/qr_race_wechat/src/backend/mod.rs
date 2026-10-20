//! Inference backends behind the `Net` contract.

#[cfg(feature = "ort")]
mod ort;
#[cfg(feature = "tract")]
mod tract;

#[cfg(feature = "ort")]
pub use ort::OrtNet;
#[cfg(feature = "tract")]
pub use tract::TractNet;

#[cfg(feature = "ort")]
pub type DefaultNet = OrtNet;
#[cfg(all(not(feature = "ort"), feature = "tract"))]
pub type DefaultNet = TractNet;
