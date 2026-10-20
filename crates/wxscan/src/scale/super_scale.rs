//! Port of `src/scale/super_scale.{hpp,cpp}`.
//!
//! At scale == 2 the super resolution model is preferred, falling back to
//! INTER_CUBIC when the image is too large or no model is loaded. At scale < 1
//! the image is shrunk with INTER_AREA, and at scale == 1 it is returned
//! unchanged.

use cvlite::{blob::blob_from_gray, resize, Interpolation};
use crate::net::Net;

pub struct SuperScale<N: Net> {
    net: Option<N>,
}

impl<N: Net> SuperScale<N> {
    pub fn new(net: Option<N>) -> Self {
        Self { net }
    }

    pub fn net_loaded(&self) -> bool {
        self.net.is_some()
    }

    /// Returns (data, width, height).
    pub fn process_image_scale(
        &self,
        src: &[u8],
        width: usize,
        height: usize,
        scale: f32,
        use_sr: bool,
        sr_max_size: i32,
    ) -> (Vec<u8>, usize, usize) {
        if scale == 1.0 {
            return (src.to_vec(), width, height);
        }
        if scale == 2.0 {
            // Super resolution is only worth running on small images
            if use_sr
                && (((width * height) as f64).sqrt() as i32) < sr_max_size
                && self.net.is_some()
            {
                if let Some(out) = self.super_resolution_scale(src, width, height) {
                    return out;
                }
            }
            let (w2, h2) = ((width as f32 * scale) as usize, (height as f32 * scale) as usize);
            return (resize(src, width, height, w2, h2, Interpolation::Cubic), w2, h2);
        }
        if scale < 1.0 {
            let (w2, h2) = ((width as f32 * scale) as usize, (height as f32 * scale) as usize);
            if w2 == 0 || h2 == 0 {
                return (src.to_vec(), width, height);
            }
            return (resize(src, width, height, w2, h2, Interpolation::Area), w2, h2);
        }
        (src.to_vec(), width, height)
    }

    fn super_resolution_scale(
        &self,
        src: &[u8],
        width: usize,
        height: usize,
    ) -> Option<(Vec<u8>, usize, usize)> {
        let net = self.net.as_ref()?;
        let blob = blob_from_gray(src, 1.0 / 255.0);
        let outs = net.forward(&blob, &[1, 1, height, width]).ok()?;
        let out = outs.into_iter().next()?;
        // The backend hands back NCHW; the super resolution model has one channel
        let (_, _, oh, ow) = out.nchw();
        let mut dst = vec![0u8; ow * oh];
        for i in 0..ow * oh {
            let pixel = out.data[i] * 255.0;
            dst[i] = pixel.clamp(0.0, 255.0) as u8;
        }
        Some((dst, ow, oh))
    }
}
