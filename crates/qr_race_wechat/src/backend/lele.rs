//! AOT-compiled desktop backend: lele turns the two ONNX graphs into Rust
//! source + weight blobs at build time, so no ONNX parser or C++ runtime is
//! shipped on desktop.

use std::sync::Mutex;

use lele::tensor::TensorView;

use crate::net::{Net, NetOutput};

#[allow(warnings)]
mod detect {
    include!(concat!(env!("OUT_DIR"), "/detect_model.rs"));
}
#[allow(warnings)]
mod sr {
    include!(concat!(env!("OUT_DIR"), "/sr_model.rs"));
}

const DETECT_MODEL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../models/detect.onnx"
));
const SR_MODEL: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/sr.onnx"));
const DETECT_WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/detect_weights.bin"));
const SR_WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sr_weights.bin"));

enum Inner {
    Detect {
        model: detect::Detect<'static>,
        workspace: Mutex<detect::DetectWorkspace>,
    },
    Sr {
        model: sr::Sr<'static>,
        workspace: Mutex<sr::SrWorkspace>,
    },
}

pub struct LeleNet {
    inner: Inner,
}

impl LeleNet {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes == DETECT_MODEL {
            return Ok(Self {
                inner: Inner::Detect {
                    model: detect::Detect::new(DETECT_WEIGHTS),
                    workspace: Mutex::new(detect::DetectWorkspace::new()),
                },
            });
        }
        if bytes == SR_MODEL {
            return Ok(Self {
                inner: Inner::Sr {
                    model: sr::Sr::new(SR_WEIGHTS),
                    workspace: Mutex::new(sr::SrWorkspace::new()),
                },
            });
        }
        Err("lele: model bytes do not match the AOT-generated detect/sr graphs".to_string())
    }
}

fn output(value: TensorView<'_, f32>) -> NetOutput {
    NetOutput {
        data: value.data.to_vec(),
        shape: value.shape.to_vec(),
    }
}

impl Net for LeleNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        let value = TensorView::from_slice(input, shape.to_vec());
        match &self.inner {
            Inner::Detect { model, workspace } => {
                let mut workspace = workspace
                    .lock()
                    .map_err(|_| "lele: detect workspace mutex poisoned".to_string())?;
                let (loc, conf) = model.forward_with_workspace(&mut workspace, value);
                // The ONNX graph applies Reshape([1, N, 2]) -> Softmax(axis=2)
                // -> Flatten. lele 0.1.x cannot lower that dynamic reshape, so
                // emit the pre-reshape classes and softmax each adjacent pair.
                let mut conf = output(conf);
                for pair in conf.data.chunks_exact_mut(2) {
                    let max = pair[0].max(pair[1]);
                    let a = (pair[0] - max).exp();
                    let b = (pair[1] - max).exp();
                    let sum = a + b;
                    pair[0] = a / sum;
                    pair[1] = b / sum;
                }
                Ok(vec![output(loc), conf])
            }
            Inner::Sr { model, workspace } => {
                let mut workspace = workspace
                    .lock()
                    .map_err(|_| "lele: sr workspace mutex poisoned".to_string())?;
                let value = model.forward_with_workspace(&mut workspace, value);
                Ok(vec![output(value)])
            }
        }
    }
}
