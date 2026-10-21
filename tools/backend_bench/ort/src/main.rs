use std::time::Instant;

fn pct(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[((values.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let env = ort::init().build().expect("ort env");
    for (name, path, side) in [
        ("detect", concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models/detect.onnx"), 384usize),
        ("sr", concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models/sr.onnx"), 224usize),
    ] {
        let bytes = std::fs::read(path).expect("read model");
        let mut session = ort::session::Session::builder(&env)
            .unwrap()
            .commit_from_memory(&bytes)
            .expect("session");
        let input = vec![0.0f32; side * side];
        let mut warm = 0;
        while warm < 20 {
            let tensor =
                ort::value::Tensor::from_array(([1usize, 1, side, side], input.clone())).unwrap();
            let _ = session.run(ort::inputs!["data" => tensor]).unwrap();
            warm += 1;
        }
        let mut times = Vec::new();
        for _ in 0..200 {
            let tensor =
                ort::value::Tensor::from_array(([1usize, 1, side, side], input.clone())).unwrap();
            let started = Instant::now();
            let _ = session.run(ort::inputs!["data" => tensor]).unwrap();
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "ort-rc15 {name} {side} p50 {:.3} ms p95 {:.3} ms",
            pct(&mut times, 0.5),
            pct(&mut times, 0.95)
        );
    }
}


