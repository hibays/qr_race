//! Per-stage profiling: runs one image N times and prints the accumulated
//! per-stage counters averaged over the frames.
//!
//! Usage: cargo run --release --features tflite --example profile --
//! <detect.tflite> <sr.tflite> <image>

use std::time::Instant;

use wxscan::net::{Net, NetOutput, NoNet};
use wxscan::tflite::TfliteNet;
use wxscan::WeChatQRCode;

enum Backend {
    Tflite(TfliteNet),
    #[allow(dead_code)]
    None(NoNet),
}

impl Net for Backend {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        match self {
            Backend::Tflite(n) => n.forward(input, shape),
            Backend::None(n) => n.forward(input, shape),
        }
    }
}

const N: usize = 10;

fn main() {
    let mut args = std::env::args().skip(1);
    let detect_path = args.next().expect("detect.tflite");
    let sr_path = args.next().expect("sr.tflite");
    let img_path = args.next().expect("image");

    let img = image::open(&img_path).expect("open").to_luma8();
    let (w, h) = img.dimensions();
    let (w, h) = (w as usize, h as usize);
    let gray = img.into_raw();

    let detect = TfliteNet::from_bytes(&std::fs::read(&detect_path).unwrap()).unwrap();
    let sr = TfliteNet::from_bytes(&std::fs::read(&sr_path).unwrap()).unwrap();
    let scanner: WeChatQRCode<Backend> =
        WeChatQRCode::new(Some(Backend::Tflite(detect)), Some(Backend::Tflite(sr)));

    // Warm up and clear the counters.
    scanner.detect_and_decode_gray(&gray, w, h);
    let _ = wxscan::detector::ssd_detector::take_stage_us();
    let _ = wxscan::wechat_qrcode::take_decode_stage_us();
    let _ = wxing::qrcode::qrcode_reader::take_reader_us();
    let _ = wxing::qrcode::detector::finder_pattern_finder::take_finder_us();
    let _ = wxing::qrcode::detector::finder_pattern_finder::take_hpc_us();
    let _ = wxing::common::unicomblock::take_bfs_stats();

    let start = Instant::now();
    let mut det_us = 0u64;
    let mut dec_us = 0u64;
    for _ in 0..N {
        let (_, d, e) = scanner.detect_and_decode_gray_timed(&gray, w, h);
        det_us += d;
        dec_us += e;
    }
    let total = start.elapsed().as_secs_f64() * 1000.0 / N as f64;

    let (pre, net, prior, post, iw, ih) = wxscan::detector::ssd_detector::take_stage_us();
    let (sr_us, zx_us, tries, dw, dh) = wxscan::wechat_qrcode::take_decode_stage_us();
    let (rb, rd, rl, rp, rc) = wxing::qrcode::qrcode_reader::take_reader_us();
    let (f1, f2, c1, c2, fnn) =
        wxing::qrcode::detector::finder_pattern_finder::take_finder_us();
    let (hpc_us, hpc_n) =
        wxing::qrcode::detector::finder_pattern_finder::take_hpc_us();

    let ms = |us: u64| us as f64 / N as f64 / 1000.0;
    println!("image {w}x{h}    total {total:.1} ms/frame");
    println!("├─ detect            {:7.1} ms   input {iw}x{ih}", ms(det_us));
    println!("│   ├─ resize+blob   {:7.1}", ms(pre));
    println!("│   ├─ tflite        {:7.1}", ms(net));
    println!("│   └─ priors+post   {:7.1}", ms(prior + post));
    println!("└─ decode            {:7.1} ms   last {dw}x{dh}  {} tries/frame", ms(dec_us), tries / N as u64);
    println!("    ├─ super res     {:7.1}", ms(sr_us));
    println!("    └─ zxing         {:7.1}   {} reader calls/frame", ms(zx_us), rc / N as u64);
    println!("        ├─ block     {:7.1}", ms(rb));
    println!("        ├─ find      {:7.1}", ms(rd));
    println!("        │   ├─ pass1 {:7.1}", ms(f1));
    println!("        │   ├─ pass2 {:7.1}", ms(f2));
    println!("        │   └─ candidates {}→{} ({} finder calls/frame)", c1 / fnn.max(1), c2 / fnn.max(1), fnn / N as u64);
    println!("        └─ cand loop {:7.1}", ms(rl));
    println!("    handle_possible_center  {:7.1} ms/frame   {} calls/frame  {:.2}us each",
        ms(hpc_us), hpc_n / N as u64, hpc_us as f64 / hpc_n.max(1) as f64);
    let (bfs_ns, bfs_n, bfs_px) = wxing::common::unicomblock::take_bfs_stats();
    println!(
        "    UnicomBlock BFS         {:7.1} ms/frame   {} calls/frame  {} pixels visited/frame",
        bfs_ns as f64 / N as f64 / 1_000_000.0,
        bfs_n / N as u64,
        bfs_px / N as u64
    );
    let _ = rp;
}
