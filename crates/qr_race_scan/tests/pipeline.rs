//! Coarse-to-fine pipeline over real symbols: a downscaled detection pass, a
//! full-resolution crop per proposal, on-demand super-resolution for small
//! boxes, and parallel decoding of the crops.

use qr_race_scan::{
    BilinearSuperResolver, Detection, Detector, GrayFrame, PyramidDetector, RxingDecoder,
    ScanEngine, ScanError,
};

fn qr_frame(text: &str, scale: u32, quiet: u32) -> GrayFrame {
    let code = qrcode::QrCode::new(text.as_bytes()).expect("payload fits");
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
    GrayFrame::new(side, side, data).unwrap()
}

fn compose(width: u32, height: u32, codes: &[(&GrayFrame, u32, u32)]) -> GrayFrame {
    let mut data = vec![255u8; (width * height) as usize];
    for (code, offset_x, offset_y) in codes {
        for y in 0..code.height() {
            for x in 0..code.width() {
                let index = ((y + offset_y) * width + (x + offset_x)) as usize;
                data[index] = code.pixel(x, y).unwrap();
            }
        }
    }
    GrayFrame::new(width, height, data).unwrap()
}

struct FixedDetector {
    detections: Vec<Detection>,
}

impl Detector for FixedDetector {
    fn detect(&self, _frame: &GrayFrame) -> Result<Vec<Detection>, ScanError> {
        Ok(self.detections.clone())
    }

    fn name(&self) -> &'static str {
        "fixed"
    }
}

#[test]
fn the_pyramid_pipeline_finds_two_codes_in_a_wide_frame() {
    let left = qr_frame("https://example.test/left", 4, 2);
    let right = qr_frame("https://example.test/right", 4, 2);
    let canvas = compose(1280, 720, &[(&left, 120, 420), (&right, 760, 120)]);

    let engine = ScanEngine::new(Box::new(RxingDecoder::new()))
        .with_detector(Box::new(PyramidDetector::new(2)));

    let outcome = engine.scan(&canvas).unwrap();
    let mut texts: Vec<_> = outcome.hits.iter().map(|hit| hit.text.clone()).collect();
    texts.sort();
    assert_eq!(
        texts,
        vec!["https://example.test/left", "https://example.test/right"]
    );
    assert_eq!(
        outcome.detections, 2,
        "both proposals survived the downscale"
    );
}

#[test]
fn super_resolution_runs_on_small_boxes_before_decoding() {
    let tiny = qr_frame("tiny-payload", 2, 2);
    let canvas = compose(640, 480, &[(&tiny, 200, 160)]);
    let detection = Detection {
        x: 200,
        y: 160,
        width: tiny.width(),
        height: tiny.height(),
        confidence: 0.9,
    };

    let engine = ScanEngine::new(Box::new(RxingDecoder::new()))
        .with_detector(Box::new(FixedDetector {
            detections: vec![detection],
        }))
        .with_super_resolver(Box::new(BilinearSuperResolver::new(2)), 100_000);

    let outcome = engine.scan(&canvas).unwrap();
    assert_eq!(outcome.super_resolved, 1);
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].text, "tiny-payload");
}

#[test]
fn parallel_and_serial_decoding_agree() {
    let left = qr_frame("a", 4, 2);
    let right = qr_frame("b", 4, 2);
    let canvas = compose(800, 400, &[(&left, 60, 60), (&right, 500, 200)]);
    let detector = FixedDetector {
        detections: vec![
            Detection {
                x: 60,
                y: 60,
                width: left.width(),
                height: left.height(),
                confidence: 1.0,
            },
            Detection {
                x: 500,
                y: 200,
                width: right.width(),
                height: right.height(),
                confidence: 1.0,
            },
        ],
    };

    let parallel = ScanEngine::new(Box::new(RxingDecoder::new()))
        .with_detector(Box::new(detector))
        .scan(&canvas)
        .unwrap();
    let serial = ScanEngine::new(Box::new(RxingDecoder::new()))
        .with_detector(Box::new(FixedDetector {
            detections: vec![
                Detection {
                    x: 60,
                    y: 60,
                    width: left.width(),
                    height: left.height(),
                    confidence: 1.0,
                },
                Detection {
                    x: 500,
                    y: 200,
                    width: right.width(),
                    height: right.height(),
                    confidence: 1.0,
                },
            ],
        }))
        .without_parallel()
        .scan(&canvas)
        .unwrap();

    let mut parallel_texts: Vec<_> = parallel.hits.iter().map(|hit| hit.text.clone()).collect();
    let mut serial_texts: Vec<_> = serial.hits.iter().map(|hit| hit.text.clone()).collect();
    parallel_texts.sort();
    serial_texts.sort();
    assert_eq!(parallel_texts, vec!["a", "b"]);
    assert_eq!(parallel_texts, serial_texts);
}
