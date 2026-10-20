//! Inference backends behind the `Net` contract.

#[cfg(feature = "lele")]
mod lele;
#[cfg(feature = "st-zrt")]
mod st_zrt;

#[cfg(feature = "lele")]
pub use lele::LeleNet;
#[cfg(feature = "st-zrt")]
pub use st_zrt::StZrtNet;

#[cfg(feature = "st-zrt")]
pub type DefaultNet = StZrtNet;
#[cfg(all(not(feature = "st-zrt"), feature = "lele"))]
pub type DefaultNet = LeleNet;
