use std::time::Instant;

use qr_race_scan::GrayFrame;
use qr_race_wechat::TieredScanner;

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

fn report(label: &str, mut times: Vec<f64>, hits: usize) {
    println!(
        "{label}: p50 {:.1} ms, p95 {:.1} ms, hits {hits}",
        pct(&mut times, 0.5),
        pct(&mut times, 0.95)
    );
}

fn main() {
    let load = Instant::now();
    let scanner = TieredScanner::new().expect("model load");
    println!(
        "model load: {:.1} ms",
        load.elapsed().as_secs_f64() * 1000.0
    );

    // Clear symbol: the fast path should handle it and never need CNN.
    let frame = qr_frame("https://example.test/wechat-bench", 1280, 720, 5);
    let _ = scanner.scan(&frame);
    let mut fast = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let outcome = scanner.scan(&frame);
        fast.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(outcome.tier, "fast");
    }
    report("fast 1280x720", fast, 1);

    // Force the CNN stage on the same frame; this measures detector + crop +
    // decode without relying on the fast path.
    let _ = scanner.scan_cnn_at(&frame, 384);
    let mut cnn = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        let outcome = scanner.scan_cnn_at(&frame, 384);
        cnn.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(!outcome.hits.is_empty());
    }
    report("cnn 384 1280x720", cnn, 1);

    // Far-code schedule: four overlapping tiles at the fixed 384 input.
    let far = qr_frame("https://example.test/far-bench", 1280, 720, 2);
    let _ = scanner.scan_cnn_at(&far, 768);
    let mut tiled = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let outcome = scanner.scan_cnn_at(&far, 768);
        tiled.push(started.elapsed().as_secs_f64() * 1000.0);
        println!(
            "  tiled detail: {:.1} ms, tier {}, hits {}",
            tiled.last().copied().unwrap_or(0.0),
            outcome.tier,
            outcome.hits.len()
        );
    }
    report("tiled far 1280x720", tiled, 1);
}
