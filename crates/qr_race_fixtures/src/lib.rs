//! Deterministic, programmatically perturbed scan fixtures.
//!
//! Real camera input has texture, noise, blur, lighting gradients and geometry.
//! These scenes are generated from rxing QR codes (the same encoder the client
//! uses) so performance and decode tests do not depend on a hand-made PNG or a
//! physical camera. Everything is seeded, so a failure is reproducible.

use qr_race_qr::{qr_webp, webp_luma};
use qr_race_scan::GrayFrame;

/// A generated frame plus the payloads it must yield.
pub struct Scene {
    pub name: &'static str,
    pub frame: GrayFrame,
    pub expected: Vec<String>,
}

/// All scenarios, from trivial to camera-like.
pub fn scenes() -> Vec<Scene> {
    vec![
        clean(),
        noise(),
        blur(),
        low_contrast(),
        gradient(),
        rotated(),
        perspective(),
        small(),
        multi(),
        camera_like(),
    ]
}

fn payload(name: &str) -> String {
    format!("https://example.test/{name}")
}

/// A QR bitmap at roughly `target` pixels, decoded from our own WebP encoder.
fn code(payload: &str, target: u32) -> GrayFrame {
    let webp = qr_webp(payload).expect("payload fits a QR code");
    let (luma, width, height) = webp_luma(&webp).expect("our WebP decodes");
    let full = GrayFrame::new(width, height, luma).expect("valid frame");
    let factor = (width / target.max(1)).max(1);
    full.downscale(factor).expect("downscale works")
}

fn textured_canvas(width: u32, height: u32, seed: u64) -> GrayFrame {
    let mut rng = Rng(seed);
    let mut data = vec![0u8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let wave = 26.0 * (x as f32 / 41.0).sin() + 22.0 * (y as f32 / 29.0).cos();
            let grain = rng.range(-18, 18);
            data[(y * width + x) as usize] = (118.0 + wave + grain as f32).clamp(0.0, 255.0) as u8;
        }
    }
    GrayFrame::new(width, height, data).expect("valid canvas")
}

fn plain_canvas(width: u32, height: u32) -> GrayFrame {
    GrayFrame::new(width, height, vec![255; (width * height) as usize]).expect("valid canvas")
}

fn blit(canvas: &mut GrayFrame, code: &GrayFrame, x: u32, y: u32) {
    let mut data = canvas.data().to_vec();
    for cy in 0..code.height() {
        for cx in 0..code.width() {
            let dx = x + cx;
            let dy = y + cy;
            if dx >= canvas.width() || dy >= canvas.height() {
                continue;
            }
            data[(dy * canvas.width() + dx) as usize] = code.pixel(cx, cy).unwrap_or(255);
        }
    }
    *canvas = GrayFrame::new(canvas.width(), canvas.height(), data).expect("frame rebuild");
}

fn add_noise(frame: &GrayFrame, seed: u64, amount: i32) -> GrayFrame {
    let mut rng = Rng(seed);
    let data = frame
        .data()
        .iter()
        .map(|pixel| (i32::from(*pixel) + rng.range(-amount, amount)).clamp(0, 255) as u8)
        .collect();
    GrayFrame::new(frame.width(), frame.height(), data).expect("frame rebuild")
}

fn box_blur(frame: &GrayFrame) -> GrayFrame {
    let (width, height) = (frame.width(), frame.height());
    let mut data = vec![0u8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0u32;
            let mut count = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    sum += u32::from(frame.pixel(nx as u32, ny as u32).unwrap_or(255));
                    count += 1;
                }
            }
            data[(y * width + x) as usize] = (sum / count) as u8;
        }
    }
    GrayFrame::new(width, height, data).expect("frame rebuild")
}

fn apply_low_contrast(frame: &GrayFrame) -> GrayFrame {
    let data = frame
        .data()
        .iter()
        .map(|pixel| (128 + ((i32::from(*pixel) - 128) * 40) / 100).clamp(0, 255) as u8)
        .collect();
    GrayFrame::new(frame.width(), frame.height(), data).expect("frame rebuild")
}

fn add_gradient(frame: &GrayFrame) -> GrayFrame {
    let width = frame.width();
    let data = frame
        .data()
        .iter()
        .enumerate()
        .map(|(index, pixel)| {
            let x = (index as u32) % width;
            let shift = (x as i32 * 80 / width as i32) - 40;
            (i32::from(*pixel) + shift).clamp(0, 255) as u8
        })
        .collect();
    GrayFrame::new(frame.width(), frame.height(), data).expect("frame rebuild")
}

fn rotate(code: &GrayFrame, degrees: f32) -> GrayFrame {
    let side = ((code.width() as f32) * 1.5).ceil() as u32;
    let center = side as f32 / 2.0;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let mut data = vec![255u8; (side * side) as usize];
    for y in 0..side {
        for x in 0..side {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let sx = cos * dx + sin * dy + code.width() as f32 / 2.0;
            let sy = -sin * dx + cos * dy + code.height() as f32 / 2.0;
            if let Some(value) = sample(code, sx, sy) {
                data[(y * side + x) as usize] = if value < 128 { 0 } else { 255 };
            }
        }
    }
    GrayFrame::new(side, side, data).expect("valid rotated frame")
}

fn warp_perspective(code: &GrayFrame, inset: f32) -> GrayFrame {
    let width = code.width();
    let height = code.height();
    let w = width as f32;
    let h = height as f32;
    let quad = [
        (w * inset, h * 0.03),
        (w * (1.0 - 0.02), h * inset),
        (w * (1.0 - inset), h * (1.0 - 0.03)),
        (w * 0.02, h * (1.0 - inset)),
    ];
    let unit = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let inverse = homography(quad, unit);

    let mut data = vec![255u8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let (u, v) = apply(inverse, x as f32, y as f32);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let sx = u * (width - 1) as f32;
            let sy = v * (height - 1) as f32;
            if let Some(value) = sample(code, sx, sy) {
                data[(y * width + x) as usize] = if value < 128 { 0 } else { 255 };
            }
        }
    }
    GrayFrame::new(width, height, data).expect("valid warped frame")
}

fn sample(frame: &GrayFrame, x: f32, y: f32) -> Option<u8> {
    if x < 0.0 || y < 0.0 || x > (frame.width() - 1) as f32 || y > (frame.height() - 1) as f32 {
        return None;
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(frame.width() - 1);
    let y1 = (y0 + 1).min(frame.height() - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = f32::from(frame.pixel(x0, y0)?);
    let p10 = f32::from(frame.pixel(x1, y0)?);
    let p01 = f32::from(frame.pixel(x0, y1)?);
    let p11 = f32::from(frame.pixel(x1, y1)?);
    let top = p00 + (p10 - p00) * fx;
    let bottom = p01 + (p11 - p01) * fx;
    Some((top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8)
}

fn apply(matrix: [f32; 9], x: f32, y: f32) -> (f32, f32) {
    let denominator = matrix[6] * x + matrix[7] * y + matrix[8];
    (
        (matrix[0] * x + matrix[1] * y + matrix[2]) / denominator,
        (matrix[3] * x + matrix[4] * y + matrix[5]) / denominator,
    )
}

/// Solves the homography from four `from` points to four `to` points.
fn homography(from: [(f32, f32); 4], to: [(f32, f32); 4]) -> [f32; 9] {
    let mut matrix = [[0.0f32; 9]; 8];
    for index in 0..4 {
        let (x, y) = from[index];
        let (u, v) = to[index];
        matrix[index * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        matrix[index * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    for column in 0..8 {
        let mut pivot = column;
        for row in column + 1..8 {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        for value in matrix[column].iter_mut().skip(column) {
            *value /= divisor;
        }
        for row in 0..8 {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            let pivot = matrix[column];
            for (index, value) in matrix[row].iter_mut().enumerate().skip(column) {
                *value -= factor * pivot[index];
            }
        }
    }
    [
        matrix[0][8],
        matrix[1][8],
        matrix[2][8],
        matrix[3][8],
        matrix[4][8],
        matrix[5][8],
        matrix[6][8],
        matrix[7][8],
        1.0,
    ]
}

/// Small deterministic PRNG (xorshift64*), so fixtures need no dependency.
struct Rng(u64);

impl Rng {
    fn range(&mut self, low: i32, high: i32) -> i32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let span = (high - low + 1).max(1) as u64;
        low + ((self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 32) % span) as i32
    }
}

fn scene_code(name: &'static str, size: u32, x: u32, y: u32) -> (Scene, String) {
    let payload = payload(name);
    let mut canvas = plain_canvas(1280, 720);
    let image = code(&payload, size);
    blit(&mut canvas, &image, x, y);
    (
        Scene {
            name,
            frame: canvas,
            expected: vec![payload.clone()],
        },
        payload,
    )
}

fn clean() -> Scene {
    scene_code("clean", 300, 460, 200).0
}

fn noise() -> Scene {
    let mut scene = scene_code("noise", 300, 460, 200).0;
    scene.frame = add_noise(&scene.frame, 11, 22);
    scene
}

fn blur() -> Scene {
    let mut scene = scene_code("blur", 300, 460, 200).0;
    scene.frame = box_blur(&scene.frame);
    scene
}

fn low_contrast() -> Scene {
    let mut scene = scene_code("low_contrast", 300, 460, 200).0;
    scene.frame = apply_low_contrast(&scene.frame);
    scene
}

fn gradient() -> Scene {
    let mut scene = scene_code("gradient", 300, 460, 200).0;
    scene.frame = add_gradient(&scene.frame);
    scene
}

fn rotated() -> Scene {
    let payload = payload("rotated");
    let image = rotate(&code(&payload, 320), 12.0);
    let mut canvas = textured_canvas(1280, 720, 7);
    blit(&mut canvas, &image, 400, 160);
    Scene {
        name: "rotated",
        frame: canvas,
        expected: vec![payload],
    }
}

fn perspective() -> Scene {
    let payload = payload("perspective");
    let image = warp_perspective(&code(&payload, 320), 0.12);
    let mut canvas = plain_canvas(1280, 720);
    blit(&mut canvas, &image, 420, 180);
    Scene {
        name: "perspective",
        frame: canvas,
        expected: vec![payload],
    }
}

fn small() -> Scene {
    scene_code("small", 128, 560, 280).0
}

fn multi() -> Scene {
    let left = payload("multi-left");
    let right = payload("multi-right");
    let mut canvas = textured_canvas(1280, 720, 21);
    blit(&mut canvas, &code(&left, 220), 120, 380);
    blit(&mut canvas, &code(&right, 240), 820, 120);
    Scene {
        name: "multi",
        frame: canvas,
        expected: vec![left, right],
    }
}

fn camera_like() -> Scene {
    let payload = payload("camera");
    let image = rotate(&code(&payload, 300), 8.0);
    let mut canvas = textured_canvas(1280, 720, 42);
    blit(&mut canvas, &image, 440, 180);
    let frame = box_blur(&add_gradient(&add_noise(&canvas, 42, 16)));
    Scene {
        name: "camera_like",
        frame,
        expected: vec![payload],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_scan::{RxingDecoder, ScanEngine};

    #[test]
    fn every_scene_decodes_its_expected_payloads() {
        let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
        for scene in scenes() {
            let outcome = engine.scan(&scene.frame).expect("scan runs");
            let mut actual: Vec<String> = outcome.hits.into_iter().map(|hit| hit.text).collect();
            let mut expected = scene.expected.clone();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "scene {}", scene.name);
        }
    }

    #[test]
    fn perturbed_scenes_are_not_blank() {
        for scene in scenes() {
            let min = *scene.frame.data().iter().min().unwrap();
            let max = *scene.frame.data().iter().max().unwrap();
            assert!(max - min > 40, "scene {} has contrast", scene.name);
            assert!(
                min < 110,
                "scene {} contains darkish modules (min {min})",
                scene.name
            );
        }
    }
}
