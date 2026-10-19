//! End-to-end decode tests over generated symbols. No fixtures to rot.

use qr_race_scan::{GrayFrame, RxingDecoder, ScanEngine};

fn qr_frame(text: &str, scale: u32, quiet: u32) -> GrayFrame {
    let code = qrcode::QrCode::new(text.as_bytes()).expect("payload fits a QR symbol");
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

fn rotate_90(frame: &GrayFrame) -> GrayFrame {
    let (w, h) = (frame.width(), frame.height());
    let mut data = vec![255u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let src = frame.pixel(x, y).unwrap();
            let nx = h - 1 - y;
            let ny = x;
            data[(ny * h + nx) as usize] = src;
        }
    }
    GrayFrame::new(h, w, data).unwrap()
}

fn side_by_side(left: &GrayFrame, right: &GrayFrame, gap: u32) -> GrayFrame {
    let height = left.height().max(right.height());
    let width = left.width() + gap + right.width();
    let mut data = vec![255u8; (width * height) as usize];
    blit(&mut data, width, left, 0, 0);
    blit(&mut data, width, right, left.width() + gap, 0);
    GrayFrame::new(width, height, data).unwrap()
}

fn blit(dst: &mut [u8], dst_width: u32, src: &GrayFrame, ox: u32, oy: u32) {
    for y in 0..src.height() {
        for x in 0..src.width() {
            let index = ((y + oy) * dst_width + (x + ox)) as usize;
            dst[index] = src.pixel(x, y).unwrap();
        }
    }
}

fn engine() -> ScanEngine {
    ScanEngine::new(Box::new(RxingDecoder::new()))
}

#[test]
fn decodes_a_single_symbol() {
    let outcome = engine()
        .scan(&qr_frame("https://example.test/a", 4, 2))
        .unwrap();
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].text, "https://example.test/a");
    assert!(outcome.hits[0].format.contains("QR"));
}

#[test]
fn decodes_a_rotated_symbol() {
    let frame = rotate_90(&qr_frame("东京A区1B级103房", 4, 2));
    let outcome = engine().scan(&frame).unwrap();
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].text, "东京A区1B级103房");
}

#[test]
fn decodes_two_symbols_in_one_frame() {
    let left = qr_frame("https://example.test/first?n=1", 3, 2);
    let right = qr_frame("第二个码的内容", 3, 2);
    let frame = side_by_side(&left, &right, 24);

    let outcome = engine().scan(&frame).unwrap();
    assert_eq!(outcome.hits.len(), 2);

    // The relay carries bytes, not a charset guess: raw must be exact.
    let mut raws: Vec<Vec<u8>> = outcome.hits.iter().map(|hit| hit.raw.clone()).collect();
    raws.sort_unstable();
    let mut expected = vec![
        b"https://example.test/first?n=1".to_vec(),
        "第二个码的内容".as_bytes().to_vec(),
    ];
    expected.sort_unstable();
    assert_eq!(raws, expected, "raw bytes survive the relay");

    let texts: Vec<_> = outcome.hits.iter().map(|hit| hit.text.as_str()).collect();
    assert!(
        texts.contains(&"https://example.test/first?n=1"),
        "ASCII payload decodes verbatim: {texts:?}"
    );
}

#[test]
fn an_empty_frame_is_not_an_error() {
    let frame = GrayFrame::new(120, 120, vec![255; 120 * 120]).unwrap();
    let outcome = engine().scan(&frame).unwrap();
    assert!(outcome.hits.is_empty());
}
