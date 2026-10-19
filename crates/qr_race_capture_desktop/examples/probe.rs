//! Probe the desktop camera: list devices, open one (optionally at a requested
//! format), pull frames, report capture stats.
//!
//! `cargo run -p qr_race_capture_desktop --example probe`
//! `cargo run -p qr_race_capture_desktop --example probe 1280 720 30`

use qr_race_capture::{CaptureConfig, CaptureSession, CaptureSource};
use qr_race_capture_desktop::{CameraConfig, CameraSource, list_cameras};

fn main() {
    match list_cameras() {
        Ok(cameras) if cameras.is_empty() => println!("no cameras reported"),
        Ok(cameras) => {
            println!("cameras:");
            for camera in cameras {
                println!("  {camera}");
            }
        }
        Err(error) => println!("camera enumeration failed: {error}"),
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let source = if args.len() >= 3 {
        let config = CameraConfig {
            width: args[0].parse().expect("width"),
            height: args[1].parse().expect("height"),
            fps: args[2].parse().expect("fps"),
        };
        println!(
            "requesting {}x{}@{}",
            config.width, config.height, config.fps
        );
        CameraSource::open_with(0, config)
    } else {
        CameraSource::open(0)
    };
    let source = match source {
        Ok(source) => source,
        Err(error) => {
            eprintln!("could not open camera 0: {error}");
            std::process::exit(1);
        }
    };

    let description = source.description().to_string();
    let (width, height) = source.resolution();
    let config = CaptureConfig::new(width, height, 30).expect("valid config");
    let mut session = CaptureSession::new(source, config).expect("session");

    println!("{description}");
    let started = std::time::Instant::now();
    let mut shown = 0;
    for _ in 0..60 {
        match session.next_frame() {
            Ok(Some(frame)) => {
                if shown < 3 {
                    println!(
                        "frame {}x{} at t+{}ms",
                        frame.width(),
                        frame.height(),
                        started.elapsed().as_millis()
                    );
                    shown += 1;
                }
            }
            Ok(None) => {
                println!("stream ended");
                break;
            }
            Err(error) => {
                eprintln!("capture failed: {error}");
                std::process::exit(1);
            }
        }
    }
    println!(
        "stats: {:?} in {}ms",
        session.stats(),
        started.elapsed().as_millis()
    );
}
