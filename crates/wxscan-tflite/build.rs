//! Tells the linker where to find libtensorflowlite_c.
//!
//! This crate vendors no binaries; the library is supplied by the caller and
//! its directory is passed through `TFLITE_LIB_DIR`:
//!
//! ```sh
//! TFLITE_LIB_DIR=/path/to/libs cargo build
//! ```
//!
//! On Apple platforms the variable is commonly left unset: the crate is built
//! as a static library, and the symbols are resolved when Xcode links the app,
//! against the framework listed in the podspec's vendored_frameworks (see the
//! cfg branches around `#[link]` in lib.rs).
fn main() {
    println!("cargo:rerun-if-env-changed=TFLITE_LIB_DIR");
    if let Ok(dir) = std::env::var("TFLITE_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
}
