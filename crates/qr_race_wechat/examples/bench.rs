use std::time::Instant;

use qr_race_scan::{GrayFrame, RxingDecoder, ScanEngine};
use qr_race_wechat::WeChatScanner;

fn qr_frame(text: &str, width: u32, height: u32, module: u32) -> GrayFrame {
    let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
    let modules = code.width() as u32;
    let quiet = 4;
    let drawn = (modules + quiet * 2) * module;
    let offset_x = (width - drawn) / 2;
    let offset_y = (height - drawn) / 2;
    let mut luma = vec![255u8; (width * height) as usize];
    let colors = code.to_colors();
    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] == qrcode::Color::Dark {
                for dy in 0..module {
                    for dx in 0..module {
                        let px = offset_x + (quiet + x) * module + dx;
                        let py = offset_y + (quiet + y) * module + dy;
                        luma[(py * width + px) as usize] = 0;
                    }
                }
            }
        }
    }
    GrayFrame::new(width, height, luma).expect("frame")
}

fn pct(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let index = ((values.len() - 1) as f64 * p).round() as usize;
    values[index]
}

fn main() {
    let iterations = 10;

    let load = Instant::now();
    let scanner = WeChatScanner::new().expect("tract loads models");
    println!(
        "wechat model load: {:.1} ms (detector+sr: {})",
        load.elapsed().as_secs_f64() * 1000.0,
        scanner.has_models()
    );

    for (width, height, module) in [(640u32, 480u32, 3u32), (960, 720, 4), (1280, 720, 5)] {
        let frame = qr_frame("https://example.test/wechat-bench", width, height, module);
        let _ = scanner.scan(&frame);
        let mut wechat = Vec::new();
        for _ in 0..iterations {
            let started = Instant::now();
            let outcome = scanner.scan(&frame);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(
                !outcome.hits.is_empty(),
                "wechat missed at {width}x{height}: {outcome:?}"
            );
            wechat.push(ms);
        }

        let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
        let _ = engine.scan(&frame);
        let mut rxing = Vec::new();
        for _ in 0..iterations {
            let started = Instant::now();
            let outcome = engine.scan(&frame).expect("rxing scan");
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(!outcome.hits.is_empty(), "rxing missed at {width}x{height}");
            rxing.push(ms);
        }

        println!(
            "{width}x{height} wechat: p50 {:.1} ms, p95 {:.1} ms | rxing: p50 {:.1} ms, p95 {:.1} ms",
            pct(&mut wechat, 0.5),
            pct(&mut wechat, 0.95),
            pct(&mut rxing, 0.5),
            pct(&mut rxing, 0.95)
        );
    }
}
