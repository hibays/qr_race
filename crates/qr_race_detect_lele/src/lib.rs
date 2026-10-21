#![allow(dead_code)]

//! lele AOT WeChat detector, split out so it can be compiled for WASM without
//! zxing-cpp. Native decode/capture stay in `qr_race_scan`/`qr_race_ffi`.

#[path = "../../qr_race_wechat/src/net.rs"]
mod net;

#[allow(warnings)]
#[path = "../../qr_race_wechat/src/backend/lele.rs"]
mod lele;

#[allow(warnings)]
#[path = "../../qr_race_wechat/src/detector/mod.rs"]
mod detector;

mod clock;

use detector::ssd_detector::SSDDetector;
use lele::LeleNet;

const DETECT_ONNX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../models/detect.onnx"
));

pub struct LeleDetector {
    inner: SSDDetector<LeleNet>,
}

impl LeleDetector {
    pub fn new() -> Result<Self, String> {
        let net = LeleNet::from_bytes(DETECT_ONNX)?;
        Ok(Self {
            inner: SSDDetector::new(net),
        })
    }

    /// Returns one `[x0, y0, x1, y1, x2, y2, x3, y3]` array per detection.
    pub fn detect(
        &self,
        gray: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<[f32; 8]>, String> {
        let quads = self.inner.forward(gray, width, height, 384, 384)?;
        Ok(quads
            .into_iter()
            .map(|quad| {
                let mut out = [0.0; 8];
                for (i, point) in quad.iter().enumerate() {
                    out[i * 2] = point.0;
                    out[i * 2 + 1] = point.1;
                }
                out
            })
            .collect())
    }
}

#[cfg(feature = "wasm")]
thread_local! {
    static DETECTOR: std::cell::RefCell<Option<LeleDetector>> = const { std::cell::RefCell::new(None) };
}

#[cfg(feature = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn detect(gray: &[u8], width: usize, height: usize) -> Result<Vec<f32>, String> {
    DETECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(LeleDetector::new()?);
        }
        let detector = slot.as_ref().expect("detector initialized");
        let boxes = detector.detect(gray, width, height)?;
        Ok(boxes.into_iter().flatten().collect())
    })
}
