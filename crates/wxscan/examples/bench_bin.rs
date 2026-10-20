//! Timing comparison of the binarizers, for finding hot spots.
//!
//! Usage: cargo run --release --example bench_bin -- <image>

use std::sync::Arc;
use std::time::Instant;

use wxing::binarizer::{
    binarize_adaptive_threshold_mean, binarize_fast_window, binarize_global_histogram,
    binarize_hybrid, binarize_simple_adaptive,
};
use wxing::luminance_source::ImgSource;

fn main() {
    let path = std::env::args().nth(1).expect("image");
    let img = image::open(&path).expect("open").to_luma8();
    let (w, h) = img.dimensions();
    let src = ImgSource::new(Arc::new(img.into_raw()), w as usize, h as usize);
    println!("image {w}x{h}");

    let bench = |name: &str, f: &dyn Fn() -> bool| {
        f();
        const N: usize = 5;
        let t = Instant::now();
        for _ in 0..N {
            f();
        }
        println!("{name:24} {:7.2} ms", t.elapsed().as_secs_f64() * 1000.0 / N as f64);
    };

    bench("Hybrid", &|| binarize_hybrid(&src).is_ok());
    bench("FastWindow", &|| binarize_fast_window(&src).is_ok());
    bench("SimpleAdaptive", &|| binarize_simple_adaptive(&src).is_ok());
    bench("AdaptiveThresholdMean", &|| binarize_adaptive_threshold_mean(&src).is_ok());
    bench("GlobalHistogram", &|| binarize_global_histogram(&src).is_ok());
}
