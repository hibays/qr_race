//! Far/small-code rate sweep for the tiered pipeline.
//!
//! Generates a 1280x720 textured frame with one QR at each requested pixel
//! size, optionally blurred, then reports which tier decoded it. This is a
//! synthetic proxy for distance; it does not replace a multi-device field run.

use qr_race_scan::GrayFrame;
use qr_race_wechat::TieredScanner;

fn qr_symbol(text: &str, target: u32) -> GrayFrame {
    let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
    let modules = code.width() as u32;
    let quiet = 4;
    let total = modules + quiet * 2;
    let module = (target / total).max(1);
    let side = total * module;
    let mut luma = vec![255u8; (side * side) as usize];
    let colors = code.to_colors();
    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..module {
                for dx in 0..module {
                    let px = (quiet + x) * module + dx;
                    let py = (quiet + y) * module + dy;
                    luma[(py * side + px) as usize] = 0;
                }
            }
        }
    }
    GrayFrame::new(side, side, luma).expect("symbol")
}

fn canvas(symbol: &GrayFrame) -> GrayFrame {
    let (w, h) = (1280u32, 720u32);
    let mut data = vec![236u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let texture = (x as i32 / 17 + y as i32 / 13) % 9 - 4;
            data[(y * w + x) as usize] = (236 + texture).clamp(0, 255) as u8;
        }
    }
    let ox = (w - symbol.width()) / 2;
    let oy = (h - symbol.height()) / 2;
    for y in 0..symbol.height() {
        for x in 0..symbol.width() {
            data[((oy + y) * w + ox + x) as usize] = symbol.pixel(x, y).unwrap_or(255);
        }
    }
    GrayFrame::new(w, h, data).expect("canvas")
}

fn box_blur(frame: &GrayFrame, radius: u32) -> GrayFrame {
    if radius == 0 {
        return frame.clone();
    }
    let (w, h) = (frame.width(), frame.height());
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            let mut count = 0u32;
            for dy in -(radius as i32)..=radius as i32 {
                for dx in -(radius as i32)..=radius as i32 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                        sum += u32::from(frame.pixel(nx as u32, ny as u32).unwrap_or(255));
                        count += 1;
                    }
                }
            }
            out[(y * w + x) as usize] = (sum / count) as u8;
        }
    }
    GrayFrame::new(w, h, out).expect("blurred")
}

fn main() {
    let scanner = TieredScanner::new().expect("models load");
    println!("size\tblur\tfast\ttiled\ttier\ttotal_ms");
    for size in [50u32, 60, 80, 100, 120, 160, 220, 300] {
        let symbol = qr_symbol("https://example.test/far-sweep", size);
        for blur in [0u32, 1, 2] {
            let frame = box_blur(&canvas(&symbol), blur);
            let started = std::time::Instant::now();
            let first = scanner.scan(&frame);
            let mut tier = first.tier;
            let mut hit = !first.hits.is_empty();
            if !hit {
                let tiled = scanner.scan_cnn_at(&frame, 768);
                hit = !tiled.hits.is_empty();
                tier = if hit { tiled.tier } else { "miss" };
            }
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{size}\t{blur}\t{}\t{}\t{tier}\t{ms:.1}",
                if first.tier != "miss" && !first.hits.is_empty() {
                    "yes"
                } else {
                    "no"
                },
                if hit { "yes" } else { "no" }
            );
        }
    }
}

