//! Inference backends behind the `Net` contract.

#[cfg(feature = "lele")]
mod lele;
#[cfg(feature = "ort")]
mod ort;

#[cfg(feature = "ort")]
pub type DefaultNet = ort::OrtNet;
#[cfg(all(not(feature = "ort"), feature = "lele"))]
pub type DefaultNet = lele::LeleNet;
