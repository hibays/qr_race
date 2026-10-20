//! Generate `include/wxscan.h` from the exported C ABI.
//!
//! The header is written into the source tree and committed, so that consumers
//! of the prebuilt library do not need cbindgen or a Rust toolchain.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = crate_dir.join("include").join("wxscan.h");
    if std::fs::create_dir_all(out.parent().unwrap()).is_err() {
        return;
    }
    match cbindgen::generate(&crate_dir) {
        Ok(bindings) => {
            bindings.write_to_file(&out);
        }
        // Header generation is a convenience; never fail the build over it.
        Err(e) => println!("cargo:warning=cbindgen: {e}"),
    }
}
