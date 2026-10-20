//! Port of `cv::dnn::blobFromImage`, limited to the call shape used here.
//!
//! The original call is
//! `blobFromImage(src, blob, 1.0/255, Size(w,h), {0}, false, false)`: no mean
//! subtraction, no R/B swap, and the output size equals the input size. It
//! therefore reduces to scaling by 1/255 and laying the data out as NCHW.

/// Single-channel grayscale image to an NCHW float blob of shape (1,1,H,W).
pub fn blob_from_gray(src: &[u8], scale: f32) -> Vec<f32> {
    src.iter().map(|&v| v as f32 * scale).collect()
}

/// Three-channel interleaved image to an NCHW float blob of shape (1,3,H,W).
pub fn blob_from_image(src: &[u8], width: usize, height: usize, scale: f32) -> Vec<f32> {
    let plane = width * height;
    let mut out = vec![0f32; plane * 3];
    for c in 0..3 {
        for i in 0..plane {
            out[c * plane + i] = src[i * 3 + c] as f32 * scale;
        }
    }
    out
}
