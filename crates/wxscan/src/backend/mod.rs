//! Adapters that connect an inference library to [`crate::net::Net`].
//!
//! Each one lives behind the feature that pulls in its library, so a build
//! selects only what it uses. An out-of-tree backend does the same thing from
//! its own crate: implement `Net` for its type, since the trait lives here.
//!
//! [`tract`] is the exception that also defines the type it runs, there being
//! no separate binding crate to adapt.

#[cfg(feature = "tflite")]
mod tflite;

#[cfg(feature = "tract")]
pub mod tract;
