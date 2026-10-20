//! A pure Rust backend: [`tract`](https://crates.io/crates/tract-onnx) running
//! the ONNX weights, behind the `tract` feature.
//!
//! Unlike the tflite backend there is no library to adapt, so the type that
//! implements [`crate::net::Net`] is defined here. tract is NCHW, like the
//! trait contract and like the Caffe models the weights come from, so nothing
//! has to be reordered.
//!
//! The models keep height and width symbolic, because super resolution runs on
//! crops of whatever size the pipeline hands it, while tract wants concrete
//! shapes before it can optimize. So a plan is built per input size and kept:
//! the detector uses a handful of sizes over a session, and super resolution a
//! few dozen, so the cache settles quickly and the cost is paid once per size.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tract_onnx::prelude::*;

use crate::net::{Net, NetOutput};

type Plan = TypedSimplePlan<TypedModel>;

/// An ONNX model run by tract.
pub struct TractNet {
    model: InferenceModel,
    plans: Mutex<HashMap<(usize, usize), Arc<Plan>>>,
}

impl TractNet {
    /// Loads a model from ONNX bytes, such as `wxscan::models::onnx::DETECT`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(bytes))
            .map_err(|e| format!("tract: cannot read model: {e}"))?;
        Ok(Self {
            model,
            plans: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the plan for one input size, building and caching it on first
    /// use.
    fn plan(&self, h: usize, w: usize) -> Result<Arc<Plan>, String> {
        let mut plans = self.plans.lock().map_err(|_| "tract: plan cache poisoned")?;
        if let Some(p) = plans.get(&(h, w)) {
            return Ok(p.clone());
        }
        let plan = self
            .model
            .clone()
            .with_input_fact(0, f32::fact([1, 1, h, w]).into())
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|e| format!("tract: cannot plan {w}x{h}: {e}"))?;
        let plan = Arc::new(plan);
        plans.insert((h, w), plan.clone());
        Ok(plan)
    }
}

impl Net for TractNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        if shape.len() != 4 || shape[0] != 1 || shape[1] != 1 {
            return Err(format!("tract: expected a 1x1xHxW input, got {shape:?}"));
        }
        let (h, w) = (shape[2], shape[3]);
        let plan = self.plan(h, w)?;
        let tensor = Tensor::from_shape(shape, input)
            .map_err(|e| format!("tract: cannot build input tensor: {e}"))?;
        let outs = plan
            .run(tvec!(tensor.into()))
            .map_err(|e| format!("tract: forward failed: {e}"))?;
        outs.into_iter().map(to_net_output).collect()
    }
}

fn to_net_output(t: TValue) -> Result<NetOutput, String> {
    let shape = t.shape().to_vec();
    let data = t
        .as_slice::<f32>()
        .map_err(|e| format!("tract: output is not f32: {e}"))?
        .to_vec();
    Ok(NetOutput { data, shape })
}
