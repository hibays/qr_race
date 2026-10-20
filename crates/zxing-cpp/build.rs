use std::env;

fn main() -> miette::Result<()> {
	if cfg!(feature = "bundled") {
		let target = env::var("TARGET").unwrap_or_default();
		// Android: the crate's cmake invocation passes an extra generic
		// `--target`, which breaks the NDK toolchain sysroot. The build script
		// for qr_race_ffi builds libZXing.so with the NDK CMake toolchain and
		// points this variable at it instead.
		if target.contains("android") {
			if let Ok(lib_dir) = env::var("ZXING_CPP_LIB_DIR") {
				println!("cargo:rustc-link-search=native={lib_dir}");
				println!("cargo:rustc-link-lib=dylib=ZXing");
				println!("cargo:rustc-link-lib=dylib=c++_shared");
				return Ok(());
			}
		}
		// Builds the project in the directory located in `core`, installing it into $OUT_DIR
		let mut config = cmake::Config::new("core");
		config
			.define("BUILD_SHARED_LIBS", "OFF")
			.define("ZXING_READERS", "ON")
			.define("ZXING_WRITERS", "ON")
			.define("ZXING_EXPERIMENTAL_API", "OFF")
			.define("ZXING_C_API", "ON")
			.define("ZXING_USE_BUNDLED_ZINT", "ON")
			.define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL")
			.define("CMAKE_INSTALL_LIBDIR", "lib");
		if target.contains("android") {
			let abi = if target.starts_with("aarch64") {
				"arm64-v8a"
			} else if target.starts_with("x86_64") {
				"x86_64"
			} else {
				"armeabi-v7a"
			};
			config.define("ANDROID_ABI", abi).define("ANDROID_PLATFORM", "android-24");
		}
		let mut dst = config.build();
		dst.push("lib");
		println!("cargo:rustc-link-search=native={}", dst.display());
		println!("cargo:rustc-link-lib=static=ZXing");

		if target.contains("android") {
			println!("cargo:rustc-link-lib=dylib=c++_shared");
		} else if target.contains("apple") {
			println!("cargo:rustc-link-lib=dylib=c++");
		} else if target.contains("linux") {
			println!("cargo:rustc-link-lib=dylib=stdc++");
		}
	} else if let Ok(lib_dir) = env::var("ZXING_CPP_LIB_DIR") {
		println!("cargo:rustc-link-search=native={lib_dir}");
		println!("cargo:rustc-link-lib=dylib=ZXing");
	} else {
		// panic!("ZXing library not found. Use feature 'bundled' or set environment variabale ZXING_CPP_LIB_DIR.")
	}

	// manual bindings.rs generation:
	// bindgen core/src/ZXingC.h -o src/bindings.rs --no-prepend-enum-name --merge-extern-blocks --use-core --no-doc-comments --no-layout-tests --with-derive-partialeq --allowlist-item "ZXing.*" -- -Itarget/release/build/zxing-cpp-*/out/include

	// replace enum decimal constants with hexadecimal ones for better readability
	// perl -0777 -i -pe 's/(pub const (?:ZXing_(?:Barcode|Image)Format_\w+: ZXing_(?:Barcode|Image)Format) = )(\d+);/$1 . sprintf("0x%04X;", $2)/ge' src/bindings.rs
	Ok(())
}
