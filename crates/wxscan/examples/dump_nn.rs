//! Dumps JSON like `dump`, but loads the tflite models, matching the
//! upstream "with models" mode.
//!
//! Usage: cargo run --features tflite --example dump_nn -- <detect.tflite>
//! <sr.tflite> <image...>

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

fn main() {
    let mut args = std::env::args().skip(1);
    let detect_path = args.next().expect("detect.tflite");
    let sr_path = args.next().expect("sr.tflite");

    let detect = TfliteNet::from_bytes(&std::fs::read(&detect_path).unwrap()).unwrap();
    let sr = TfliteNet::from_bytes(&std::fs::read(&sr_path).unwrap()).unwrap();
    let scanner = WeChatQRCode::new(Some(Backend::Tflite(detect)), Some(Backend::Tflite(sr)));

    let mut items: Vec<String> = Vec::new();
    for path in args {
        let img = match image::open(&path) {
            Ok(i) => i.to_luma8(),
            Err(e) => {
                eprintln!("skip {path}: {e}");
                continue;
            }
        };
        let (w, h) = img.dimensions();
        let results = scanner.detect_and_decode_gray(&img.into_raw(), w as usize, h as usize);
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().to_string();
        let texts: Vec<String> = results.iter().map(|r| format!("{:?}", r.text_lossy())).collect();
        let points: Vec<String> = results
            .iter()
            .map(|r| {
                let p = r.points;
                format!(
                    "[[{:.4},{:.4}],[{:.4},{:.4}],[{:.4},{:.4}],[{:.4},{:.4}]]",
                    p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1, p[3].0, p[3].1
                )
            })
            .collect();
        items.push(format!(
            "\"{}\": {{\"texts\": [{}], \"points\": [{}]}}",
            name,
            texts.join(","),
            points.join(",")
        ));
    }
    println!("{{{}}}", items.join(",\n"));
}
