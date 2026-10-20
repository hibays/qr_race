//! Simple benchmark: uses camera-sized frames and measures the per-frame
//! cost with and without the CNN models.
//!
//! Usage: cargo run --release --features tflite --example bench --
//! <detect.tflite> <sr.tflite> <image>
//!
//! Built with `--features tflite,tract` it also runs the same
//! frames through tract on the same weights in ONNX form, which is the head to head
//! between the two backends.

use std::time::Instant;

use wxscan::net::{Net, NetOutput, NoNet};
use wxscan::tflite::TfliteNet;
use wxscan::WeChatQRCode;

enum Backend {
    Tflite(TfliteNet),
    #[cfg(feature = "tract")]
    Tract(wxscan::backend::tract::TractNet),
    // Constructed only in the no-model configuration below.
    #[allow(dead_code)]
    None(NoNet),
}

impl Net for Backend {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        match self {
            Backend::Tflite(n) => n.forward(input, shape),
            #[cfg(feature = "tract")]
            Backend::Tract(n) => n.forward(input, shape),
            Backend::None(n) => n.forward(input, shape),
        }
    }
}

/// The same measurements through tract, for comparison. Needs the ONNX weights,
/// which live in the wxscan-weights repository rather than in a crate; point
/// `WXSCAN_WEIGHTS_DIR` at them or check that repository out beside this one.
#[cfg(feature = "tract")]
fn bench_tract(gray: &[u8], w: usize, h: usize) {
    use wxscan::backend::tract::TractNet;

    let dir = std::env::var_os("WXSCAN_WEIGHTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../wxscan-weights/models")
        });
    let read = |name: &str| std::fs::read(dir.join(name));
    let (Ok(detect_bytes), Ok(sr_bytes)) = (read("detect.onnx"), read("sr.onnx")) else {
        println!("tract: skipped, no ONNX weights in {}", dir.display());
        return;
    };

    let load = |bytes: &[u8]| Backend::Tract(TractNet::from_bytes(bytes).expect("load onnx"));
    let nn_only = WeChatQRCode::new(Some(load(&detect_bytes)), None);
    bench("tract detect only", &nn_only, gray, w, h);
    let nn = WeChatQRCode::new(Some(load(&detect_bytes)), Some(load(&sr_bytes)));
    bench("tract detect + sr", &nn, gray, w, h);

    // The forward pass on its own, at the size the detector uses, which is the
    // only part the backend actually decides. Everything else is shared.
    use cvlite::{blob::blob_from_gray, resize, Interpolation};
    let detect = TractNet::from_bytes(&detect_bytes).unwrap();
    let (tw, th) = ((w as f32 * 0.2777) as usize, (h as f32 * 0.2777) as usize);
    let small = resize(gray, w, h, tw, th, Interpolation::Cubic);
    let blob = blob_from_gray(&small, 1.0 / 255.0);
    let _ = detect.forward(&blob, &[1, 1, th, tw]);
    let start = Instant::now();
    for _ in 0..10 {
        let _ = detect.forward(&blob, &[1, 1, th, tw]).unwrap();
    }
    println!("{:22} {:7.1} ms/call   ({tw}x{th})", "tract SSD forward", start.elapsed().as_secs_f64() * 100.0);
}

#[cfg(not(feature = "tract"))]
fn bench_tract(_gray: &[u8], _w: usize, _h: usize) {}

fn bench(name: &str, scanner: &WeChatQRCode<Backend>, gray: &[u8], w: usize, h: usize) {
    // Warm up; the first frame has to build the interpreter.
    let _ = scanner.detect_and_decode_gray(gray, w, h);

    const N: usize = 10;
    let start = Instant::now();
    let mut hits = 0;
    for _ in 0..N {
        hits += scanner.detect_and_decode_gray(gray, w, h).len();
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / N as f64;
    println!("{name:22} {ms:7.1} ms/frame   (hits {hits}/{N})");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let detect_path = args.next().expect("detect.tflite");
    let sr_path = args.next().expect("sr.tflite");
    let img_path = args.next().expect("image");

    let img = image::open(&img_path).expect("open").to_luma8();
    let (w, h) = img.dimensions();
    let gray = img.into_raw();
    println!("image {w}x{h}");

    let no_nn: WeChatQRCode<Backend> = WeChatQRCode::new(None, None);
    bench("no model", &no_nn, &gray, w as usize, h as usize);

    let detect = TfliteNet::from_bytes(&std::fs::read(&detect_path).unwrap()).unwrap();
    let nn_only: WeChatQRCode<Backend> =
        WeChatQRCode::new(Some(Backend::Tflite(detect)), None);
    bench("CNN detect only", &nn_only, &gray, w as usize, h as usize);

    let detect = TfliteNet::from_bytes(&std::fs::read(&detect_path).unwrap()).unwrap();
    let sr = TfliteNet::from_bytes(&std::fs::read(&sr_path).unwrap()).unwrap();
    let nn = WeChatQRCode::new(Some(Backend::Tflite(detect)), Some(Backend::Tflite(sr)));
    bench("CNN detect + super res", &nn, &gray, w as usize, h as usize);

    bench_tract(&gray, w as usize, h as usize);

    // Time the SSD forward pass on its own, to see how much of the cost is
    // detection itself.
    {
        use cvlite::{blob::blob_from_gray, resize, Interpolation};
        let detect = TfliteNet::from_bytes(&std::fs::read(&detect_path).unwrap()).unwrap();
        let tw = (w as f32 * 0.2777) as usize;
        let th = (h as f32 * 0.2777) as usize;
        let small = resize(&gray, w as usize, h as usize, tw, th, Interpolation::Cubic);
        let blob = blob_from_gray(&small, 1.0 / 255.0);
        let _ = detect.forward(&blob, &[1, 1, th, tw]);
        let start = Instant::now();
        for _ in 0..10 {
            let _ = detect.forward(&blob, &[1, 1, th, tw]).unwrap();
        }
        println!("{:22} {:7.1} ms/call   ({tw}x{th})", "SSD forward", start.elapsed().as_secs_f64() * 100.0);

        let start = Instant::now();
        for _ in 0..10 {
            let _ = resize(&gray, w as usize, h as usize, tw, th, Interpolation::Cubic);
        }
        println!("{:22} {:7.1} ms/call", "cv::resize(cubic)", start.elapsed().as_secs_f64() * 100.0);

        // Count the candidate boxes; decode time is roughly proportional to
        // the number of boxes.
        use wxscan::detector::detection_output::{forward as det_fwd, DetectionOutputParams};
        use wxscan::detector::priorbox::generate_all_priors;
        let outs = detect.forward(&blob, &[1, 1, th, tw]).unwrap();
        let (loc, conf) = if outs[0].data.len() > outs[1].data.len() {
            (&outs[0].data, &outs[1].data)
        } else {
            (&outs[1].data, &outs[0].data)
        };
        let priors = generate_all_priors(tw, th);
        let dets = det_fwd(loc, conf, &priors, &DetectionOutputParams::default());
        let kept: Vec<_> = dets.iter().filter(|d| d.label == 1.0 && d.score > 1e-5).collect();
        println!("{:22} {} (label=1: {})", "SSD candidates", dets.len(), kept.len());
        for d in kept.iter().take(8) {
            println!("      score={:.3} box=({:.3},{:.3})-({:.3},{:.3})", d.score, d.xmin, d.ymin, d.xmax, d.ymax);
        }
    }
}
