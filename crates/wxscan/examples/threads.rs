//! How much the detector and super resolution gain from threads, measured by
//! running each net at a fixed shape with 1, 2 and 4 threads.
//!
//! Usage: cargo run --features tflite --release --example threads --
//! <detect.tflite> <sr.tflite>

use wxscan::net::Net;
use wxscan::tflite::TfliteNet;

fn time(net: &TfliteNet, input: &[f32], shape: &[usize], runs: usize) -> f64 {
    let _ = net.forward(input, shape);
    let started = std::time::Instant::now();
    for _ in 0..runs {
        net.forward(input, shape).unwrap();
    }
    started.elapsed().as_secs_f64() * 1000.0 / runs as f64
}

fn main() {
    let mut args = std::env::args().skip(1);
    let detect = std::fs::read(args.next().unwrap()).unwrap();
    let sr = std::fs::read(args.next().unwrap()).unwrap();

    // The shapes each net actually sees: the detector at its fixed input, and
    // super resolution over a crop of the size that triggers it.
    let cases: [(&str, &[u8], [usize; 4]); 2] = [
        ("detect", &detect, [1, 1, 384, 384]),
        ("sr", &sr, [1, 1, 80, 80]),
    ];
    for (name, bytes, shape) in cases {
        let len: usize = shape.iter().product();
        let input = vec![0.5f32; len];
        print!("{name} {}x{}:", shape[2], shape[3]);
        let mut base = 0.0;
        for threads in [1, 2, 4, 8] {
            let net = TfliteNet::from_bytes(bytes).unwrap().with_threads(threads);
            let ms = time(&net, &input, &shape, 30);
            if threads == 1 {
                base = ms;
            }
            print!("  {threads}t {ms:.1}ms ({:.2}x)", base / ms);
        }
        println!();
    }
}
