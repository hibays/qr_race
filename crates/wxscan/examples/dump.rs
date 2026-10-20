//! Dumps the recognition results for a set of images as JSON, for a
//! point-by-point comparison against the upstream C++ implementation.
//!
//! Usage: cargo run --example dump -- <image...>

use wxscan::net::NoNet;
use wxscan::WeChatQRCode;

fn main() {
    let scanner: WeChatQRCode<NoNet> = WeChatQRCode::new(None, None);
    let mut items: Vec<String> = Vec::new();

    for path in std::env::args().skip(1) {
        let img = match image::open(&path) {
            Ok(i) => i.to_luma8(),
            Err(e) => {
                eprintln!("skip {path}: {e}");
                continue;
            }
        };
        let (w, h) = img.dimensions();
        let results = scanner.detect_and_decode_gray(&img.into_raw(), w as usize, h as usize);

        let name = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let texts: Vec<String> = results
            .iter()
            .map(|r| format!("{:?}", r.text_lossy()))
            .collect();
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
