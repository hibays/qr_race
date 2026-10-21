use std::time::Instant;

use st_zrt::{Environment, GraphOptimizationLevel, MemoryInfo, OwnedValue, Session, SessionOptions, Tensor};

fn pct(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[((values.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let env = Environment::new().expect("ort env");
    for (name, path, side) in [
        ("detect", concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models/detect.onnx"), 384usize),
        ("sr", concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models/sr.onnx"), 224usize),
    ] {
        let bytes = std::fs::read(path).expect("read model");
        let opts = SessionOptions::new().with_opt_level(GraphOptimizationLevel::All);
        let session = Session::from_bytes(&env, &bytes, opts).expect("session");
        let memory = MemoryInfo::cpu().expect("memory");
        let input = vec![0.0f32; side * side];
        let shape = [1i64, 1, side as i64, side as i64];
        let mut warm = 0;
        while warm < 20 {
            let tensor = Tensor::from_buffer(&input, &shape, &memory).unwrap();
            let mut outputs: Vec<Option<OwnedValue>> =
                (0..session.output_count()).map(|_| None).collect();
            session.run(&[&tensor], &mut outputs).unwrap();
            warm += 1;
        }
        let mut times = Vec::new();
        for _ in 0..200 {
            let tensor = Tensor::from_buffer(&input, &shape, &memory).unwrap();
            let mut outputs: Vec<Option<OwnedValue>> =
                (0..session.output_count()).map(|_| None).collect();
            let started = Instant::now();
            session.run(&[&tensor], &mut outputs).unwrap();
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "st-zrt-0.4.1 {name} {side} p50 {:.3} ms p95 {:.3} ms",
            pct(&mut times, 0.5),
            pct(&mut times, 0.95)
        );
    }
}

