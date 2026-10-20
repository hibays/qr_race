//! Adapts [`wxscan_tflite::TfliteNet`] to [`crate::net::Net`].
//!
//! The binding crate speaks tflite's conventions, where tensors are NHWC. The
//! `Net` contract is NCHW. Converting between them is this file's whole job,
//! which is what keeps the layout assumption out of the algorithm and lets a
//! backend with different conventions be dropped in the same way.

use wxscan_tflite::{Tensor, TfliteNet};

use crate::net::{Net, NetOutput};

impl Net for TfliteNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        let dims = nchw_to_nhwc_dims(shape);
        let outs = self.run(input, &dims)?;
        Ok(outs.into_iter().map(to_nchw).collect())
    }
}

/// Reorders an NCHW input shape into the NHWC one tflite expects. Shapes that
/// are not four-dimensional pass through, since there is no channel axis to
/// move.
fn nchw_to_nhwc_dims(shape: &[usize]) -> Vec<i32> {
    match shape.len() {
        4 => vec![
            shape[0] as i32,
            shape[2] as i32,
            shape[3] as i32,
            shape[1] as i32,
        ],
        _ => shape.iter().map(|&v| v as i32).collect(),
    }
}

/// Converts one output tensor from NHWC to NCHW.
///
/// With a single channel the elements are already in the right order, so only
/// the shape changes; that is the case for both models used here. The general
/// case is handled anyway, so the adapter stays correct for other models.
fn to_nchw(t: Tensor) -> NetOutput {
    let Tensor { data, shape } = t;
    if shape.len() != 4 {
        return NetOutput { data, shape };
    }
    let (n, h, w, c) = (shape[0], shape[1], shape[2], shape[3]);
    let nchw_shape = vec![n, c, h, w];
    if c <= 1 {
        return NetOutput {
            data,
            shape: nchw_shape,
        };
    }

    let mut out = vec![0f32; data.len()];
    let plane = h * w;
    for ni in 0..n {
        for ci in 0..c {
            for i in 0..plane {
                out[(ni * c + ci) * plane + i] = data[(ni * plane + i) * c + ci];
            }
        }
    }
    NetOutput {
        data: out,
        shape: nchw_shape,
    }
}
