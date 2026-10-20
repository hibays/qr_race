//! qr_bench: the measurement half of the "30fps or it did not happen" rule.
//!
//! Decodes one still image repeatedly and reports the wall-clock distribution,
//! plus the stage split the engine recorded. Without `--detector` the engine
//! decodes the whole frame; `--detector pyramid` runs the coarse detection pass
//! and decodes crops instead.
//!
//! It can also generate its own fixtures: a clean symbol, or a full camera-sized
//! scene with one symbol drawn at a chosen pixel size and angle. That is how the
//! 33ms budget gets tested against a realistic frame instead of a postage stamp.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use qr_race_fixtures::scenes;
use qr_race_scan::{
    BilinearSuperResolver, PyramidDetector, ScanEngine, ZxingDecoder, load_gray_file,
};

struct Args {
    input: Option<PathBuf>,
    generate: Option<PathBuf>,
    text: String,
    iterations: u32,
    warmup: u32,
    json: bool,
    try_harder: bool,
    detector: String,
    pyramid_scale: u32,
    super_res: String,
    super_res_min_area: u64,
    dump_gray: Option<PathBuf>,
    suite: bool,
    scene: bool,
    width: u32,
    height: u32,
    qr_size: u32,
    rotate: f32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            input: None,
            generate: None,
            text: "https://example.test/relay".to_string(),
            iterations: 100,
            warmup: 5,
            json: false,
            try_harder: false,
            detector: "none".to_string(),
            pyramid_scale: 2,
            super_res: "none".to_string(),
            super_res_min_area: 0,
            dump_gray: None,
            suite: false,
            scene: false,
            width: 1280,
            height: 720,
            qr_size: 160,
            rotate: 0.0,
        }
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("qr_bench: {err}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if let Some(path) = &args.generate {
        let result = if args.scene {
            generate_scene(
                path,
                &args.text,
                args.width,
                args.height,
                args.qr_size,
                args.rotate,
            )
        } else {
            generate_qr(path, &args.text)
        };
        return match result {
            Ok(()) => {
                println!("wrote {} ({} bytes)", path.display(), byte_len(path));
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("qr_bench: {err}");
                ExitCode::FAILURE
            }
        };
    }

    if args.suite {
        return run_suite(&args);
    }

    run_bench(&args)
}

const USAGE: &str = "\
usage:
  qr_bench --input <image> [--iterations N] [--warmup N] [--json] [--try-harder]
           [--detector none|pyramid [--pyramid-scale 2]]
           [--sr none|bilinear [--sr-min-area N]] [--dump-gray <frame.gray>]
  qr_bench --suite [--iterations N] [--warmup N] [--json]
           [--detector none|pyramid] [--sr none|bilinear [--sr-min-area N]]
  qr_bench --generate <image.png> [--text <payload>]
  qr_bench --generate <scene.png> --scene [--width 1280] [--height 720]
           [--qr-size 160] [--rotate 0] [--text <payload>]";

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--input" => {
                args.input = Some(PathBuf::from(iter.next().ok_or("--input needs a path")?));
            }
            "--generate" => {
                args.generate = Some(PathBuf::from(iter.next().ok_or("--generate needs a path")?));
            }
            "--text" => args.text = iter.next().ok_or("--text needs a value")?,
            "--iterations" => {
                args.iterations = iter
                    .next()
                    .ok_or("--iterations needs a value")?
                    .parse()
                    .map_err(|_| "--iterations must be a positive integer")?;
                if args.iterations == 0 {
                    return Err("--iterations must be greater than zero".into());
                }
            }
            "--warmup" => {
                args.warmup = iter
                    .next()
                    .ok_or("--warmup needs a value")?
                    .parse()
                    .map_err(|_| "--warmup must be an integer")?;
            }
            "--width" => args.width = parse_u32(&mut iter, "--width")?,
            "--height" => args.height = parse_u32(&mut iter, "--height")?,
            "--qr-size" => args.qr_size = parse_u32(&mut iter, "--qr-size")?,
            "--detector" => args.detector = iter.next().ok_or("--detector needs a value")?,
            "--pyramid-scale" => args.pyramid_scale = parse_u32(&mut iter, "--pyramid-scale")?,
            "--sr" => args.super_res = iter.next().ok_or("--sr needs a value")?,
            "--sr-min-area" => {
                args.super_res_min_area = iter
                    .next()
                    .ok_or("--sr-min-area needs a value")?
                    .parse()
                    .map_err(|_| "--sr-min-area must be an integer")?;
            }
            "--rotate" => {
                args.rotate = iter
                    .next()
                    .ok_or("--rotate needs a value")?
                    .parse()
                    .map_err(|_| "--rotate must be a number of degrees")?;
            }
            "--dump-gray" => {
                args.dump_gray = Some(PathBuf::from(
                    iter.next().ok_or("--dump-gray needs a path")?,
                ));
            }
            "--suite" => args.suite = true,
            "--scene" => args.scene = true,
            "--json" => args.json = true,
            "--try-harder" => args.try_harder = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(None);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    if !args.suite && args.input.is_none() && args.generate.is_none() {
        return Err("one of --input or --generate is required".into());
    }
    if args.width == 0 || args.height == 0 || args.qr_size == 0 {
        return Err("dimensions must be greater than zero".into());
    }
    match args.detector.as_str() {
        "none" | "pyramid" => {}
        other => return Err(format!("unknown detector {other:?}")),
    }
    match args.super_res.as_str() {
        "none" | "bilinear" => {}
        other => return Err(format!("unknown super-resolution {other:?}")),
    }
    Ok(Some(args))
}

fn parse_u32(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, String> {
    iter.next()
        .ok_or(format!("{flag} needs a value"))?
        .parse()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn generate_qr(path: &Path, text: &str) -> Result<(), String> {
    let (side, data) = render_qr(text, 6, 4)?;
    let image =
        image::GrayImage::from_raw(side, side, data).ok_or("could not build the demo QR image")?;
    image.save(path).map_err(|err| err.to_string())
}

/// Renders `text` as a QR symbol at an integer module scale, with a quiet zone.
fn render_qr(text: &str, scale: u32, quiet: u32) -> Result<(u32, Vec<u8>), String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|err| err.to_string())?;
    let modules = code.width() as u32;
    let colors = code.to_colors();
    let side = (modules + quiet * 2) * scale;
    let mut data = vec![255u8; (side * side) as usize];

    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] == qrcode::types::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + quiet) * scale + dx;
                        let py = (y + quiet) * scale + dy;
                        data[(py * side + px) as usize] = 0;
                    }
                }
            }
        }
    }
    Ok((side, data))
}

fn generate_scene(
    path: &Path,
    text: &str,
    width: u32,
    height: u32,
    qr_size: u32,
    rotate_deg: f32,
) -> Result<(), String> {
    // Pick the integer module size that lands closest to the requested symbol
    // size; a fractional module would not survive nearest-neighbour rotation.
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|err| err.to_string())?;
    let modules = code.width() as u32;
    let quiet = 4u32;
    let total_modules = modules + quiet * 2;
    let scale = (qr_size / total_modules).max(1);
    let (side, qr) = render_qr(text, scale, quiet)?;

    let angle = rotate_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let half = side as f32 / 2.0;
    let mut canvas = vec![255u8; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            // Inverse-rotate the canvas pixel into the symbol's own frame.
            let sx = dx * cos + dy * sin + half;
            let sy = -dx * sin + dy * cos + half;
            if sx >= 0.0 && sy >= 0.0 && sx < side as f32 && sy < side as f32 {
                let px = sx as u32;
                let py = sy as u32;
                canvas[(y * width + x) as usize] = qr[(py * side + px) as usize];
            }
        }
    }

    let image = image::GrayImage::from_raw(width, height, canvas)
        .ok_or("could not build the scene image")?;
    image.save(path).map_err(|err| err.to_string())
}

fn byte_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn run_bench(args: &Args) -> ExitCode {
    let Some(input) = &args.input else {
        eprintln!("qr_bench: --input is required to measure");
        return ExitCode::from(2);
    };

    let frame = match load_gray_file(input) {
        Ok(frame) => frame,
        Err(err) => {
            eprintln!("qr_bench: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Raw frame export for the wasm probe: width/height little-endian, then the
    // grayscale bytes.
    if let Some(path) = &args.dump_gray {
        let mut out = Vec::with_capacity(8 + frame.data().len());
        out.extend_from_slice(&frame.width().to_le_bytes());
        out.extend_from_slice(&frame.height().to_le_bytes());
        out.extend_from_slice(frame.data());
        return match std::fs::write(path, &out) {
            Ok(()) => {
                println!(
                    "wrote {} ({}x{}, {} bytes)",
                    path.display(),
                    frame.width(),
                    frame.height(),
                    out.len()
                );
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("qr_bench: could not write {}: {err}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let engine = build_engine(args);

    for _ in 0..args.warmup {
        if let Err(err) = engine.scan(&frame) {
            eprintln!("qr_bench: warmup scan failed: {err}");
            return ExitCode::FAILURE;
        }
    }

    let mut total_us = Vec::with_capacity(args.iterations as usize);
    let mut decode_us = Vec::with_capacity(args.iterations as usize);
    let mut detect_us = Vec::with_capacity(args.iterations as usize);
    let mut sr_us = Vec::with_capacity(args.iterations as usize);
    let mut hits = 0usize;

    for _ in 0..args.iterations {
        match engine.scan(&frame) {
            Ok(outcome) => {
                hits = outcome.hits.len();
                total_us.push(outcome.timings.total.as_micros() as u64);
                decode_us.push(outcome.timings.decode.as_micros() as u64);
                detect_us.push(outcome.timings.detect.as_micros() as u64);
                sr_us.push(outcome.timings.super_res.as_micros() as u64);
            }
            Err(err) => {
                eprintln!("qr_bench: scan failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    let total = Stats::from(total_us);
    let decode = Stats::from(decode_us);
    let detect = Stats::from(detect_us);
    let super_res = Stats::from(sr_us);
    let fps_from_p50 = if total.p50 > 0.0 {
        1_000_000.0 / total.p50
    } else {
        0.0
    };

    if args.json {
        let report = serde_json::json!({
            "input": input.display().to_string(),
            "frame": { "width": frame.width(), "height": frame.height() },
            "iterations": args.iterations,
            "warmup": args.warmup,
            "hits": hits,
            "try_harder": args.try_harder,
            "detector": &args.detector,
            "super_res": &args.super_res,
            "wall_us": {
                "min": total.min,
                "p50": total.p50,
                "p95": total.p95,
                "max": total.max,
                "mean": total.mean,
            },
            "fps_from_p50": fps_from_p50,
            "stages_us": {
                "detect_p50": detect.p50,
                "super_res_p50": super_res.p50,
                "decode_p50": decode.p50,
            },
        });
        println!("{report}");
    } else {
        println!(
            "input  {} ({}x{}, {hits} hits)",
            input.display(),
            frame.width(),
            frame.height()
        );
        println!(
            "runs   {} iterations + {} warmup (try_harder: {}, detector: {}, sr: {})",
            args.iterations, args.warmup, args.try_harder, args.detector, args.super_res
        );
        println!(
            "total  p50 {:.3} ms   p95 {:.3} ms   max {:.3} ms   mean {:.3} ms",
            total.p50 / 1000.0,
            total.p95 / 1000.0,
            total.max / 1000.0,
            total.mean / 1000.0
        );
        println!(
            "stages p50: detect {:.3} ms   super_res {:.3} ms   decode {:.3} ms",
            detect.p50 / 1000.0,
            super_res.p50 / 1000.0,
            decode.p50 / 1000.0
        );
        println!("budget p50 implies {fps_from_p50:.1} fps (33 ms = 30 fps)");
    }

    ExitCode::SUCCESS
}

fn build_engine(args: &Args) -> ScanEngine {
    let mut engine = ScanEngine::new(Box::new(ZxingDecoder::new().try_harder(args.try_harder)));
    if args.detector == "pyramid" {
        engine = engine.with_detector(Box::new(PyramidDetector::new(args.pyramid_scale)));
    }
    if args.super_res == "bilinear" {
        engine = engine.with_super_resolver(
            Box::new(BilinearSuperResolver::new(2)),
            args.super_res_min_area,
        );
    }
    engine
}

/// Runs the deterministic perturbed-texture suite: texture, noise, blur, low
/// contrast, gradients, rotation, perspective, a small code and multiple codes.
fn run_suite(args: &Args) -> ExitCode {
    let engine = build_engine(args);
    let mut reports = Vec::new();
    let mut complete = true;

    for scene in scenes() {
        for _ in 0..args.warmup {
            if let Err(error) = engine.scan(&scene.frame) {
                eprintln!("qr_bench: {} warmup failed: {error}", scene.name);
                return ExitCode::FAILURE;
            }
        }

        let mut total_us = Vec::with_capacity(args.iterations as usize);
        let mut hits = 0usize;
        for _ in 0..args.iterations {
            match engine.scan(&scene.frame) {
                Ok(outcome) => {
                    hits = outcome.hits.len();
                    total_us.push(outcome.timings.total.as_micros() as u64);
                }
                Err(error) => {
                    eprintln!("qr_bench: {} scan failed: {error}", scene.name);
                    return ExitCode::FAILURE;
                }
            }
        }

        let stats = Stats::from(total_us);
        let fps = if stats.p50 > 0.0 {
            1_000_000.0 / stats.p50
        } else {
            0.0
        };
        let expected = scene.expected.len();
        if hits < expected {
            complete = false;
        }

        if args.json {
            reports.push(serde_json::json!({
                "scene": scene.name,
                "hits": hits,
                "expected": expected,
                "p50_ms": stats.p50 / 1000.0,
                "p95_ms": stats.p95 / 1000.0,
                "fps_from_p50": fps,
            }));
        } else {
            println!(
                "{:<14} hits {}/{}  p50 {:>7.3} ms  p95 {:>7.3} ms  {:>7.1} fps",
                scene.name,
                hits,
                expected,
                stats.p50 / 1000.0,
                stats.p95 / 1000.0,
                fps
            );
        }
    }

    if args.json {
        println!("{}", serde_json::json!({ "suite": reports }));
    }
    if complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Stats {
    min: f64,
    p50: f64,
    p95: f64,
    max: f64,
    mean: f64,
}

impl Stats {
    fn from(mut samples: Vec<u64>) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_unstable();
        let len = samples.len();
        let sum: u64 = samples.iter().sum();
        let p50_index = (len - 1) * 50 / 100;
        let p95_index = (len - 1) * 95 / 100;
        Self {
            min: samples[0] as f64,
            p50: samples[p50_index] as f64,
            p95: samples[p95_index] as f64,
            max: samples[len - 1] as f64,
            mean: sum as f64 / len as f64,
        }
    }
}
