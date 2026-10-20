fn main() {
    // zxing-cpp's upstream CMake build uses the debug CRT for a debug Rust
    // profile while the Rust objects use the release CRT. Linking msvcrtd
    // supplies the debug CRT symbols (__imp__CrtDbgReport) for local tests.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("PROFILE").as_deref() == Ok("debug")
    {
        println!("cargo:rustc-link-lib=msvcrtd");
    }
}
