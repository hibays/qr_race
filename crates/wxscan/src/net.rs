//! Abstraction layer for CNN inference.
//!
//! Upstream runs the detect and sr ONNX models through `cv::dnn::Net`. Here
//! that is a trait; the default implementation uses tflite (see the `tflite`
//! module), and a pure Rust engine such as tract only needs one more
//! implementation.

/// Output of one forward pass: data plus shape (NCHW).
pub struct NetOutput {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

impl NetOutput {
    /// Reads the value at (n, c, h, w).
    #[inline]
    pub fn at(&self, n: usize, c: usize, h: usize, w: usize) -> f32 {
        let (_, cc, hh, ww) = self.nchw();
        let _ = cc;
        self.data[((n * cc + c) * hh + h) * ww + w]
    }

    pub fn nchw(&self) -> (usize, usize, usize, usize) {
        match self.shape.len() {
            4 => (self.shape[0], self.shape[1], self.shape[2], self.shape[3]),
            3 => (1, self.shape[0], self.shape[1], self.shape[2]),
            2 => (1, 1, self.shape[0], self.shape[1]),
            _ => (1, 1, 1, self.data.len()),
        }
    }
}

/// A single-input convolutional network with a variable input size; it may
/// have several outputs.
pub trait Net {
    /// `input` is an f32 blob in NCHW layout, `shape` is (n, c, h, w).
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String>;
}

/// Placeholder implementation used when no model is loaded, corresponding to
/// the upstream fallback mode for an empty model path.
pub struct NoNet;

impl Net for NoNet {
    fn forward(&self, _input: &[f32], _shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        Err("no model loaded".to_string())
    }
}
