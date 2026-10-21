//! ONNX Runtime through the `ort` crate for all native targets.
//!
//! The Android APK packages ONNX Runtime 1.29 and `ort` loads it dynamically;
//! desktop packages the same 1.29 DLL. Keeping one runtime avoids Cargo's
//! `links = "onnxruntime"` conflict between `ort-sys` and `st-zrt-sys`.

use std::sync::{Mutex, OnceLock};

use ort::session::Session;
use ort::value::Tensor;

use crate::net::{Net, NetOutput};

pub struct OrtNet {
    session: Mutex<Session>,
}

fn environment() -> &'static ort::environment::Environment {
    static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();
    ENV.get_or_init(|| {
        ort::init()
            .build()
            .expect("ONNX Runtime environment initializes")
    })
}

impl OrtNet {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let session = Session::builder(environment())
            .and_then(|mut builder| builder.commit_from_memory(bytes))
            .map_err(|error| format!("ort: cannot load model: {error}"))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl Net for OrtNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        let tensor = Tensor::from_array((shape.to_vec(), input.to_vec()))
            .map_err(|error| format!("ort: input: {error}"))?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "ort: session mutex poisoned".to_string())?;
        let outputs = session
            .run(ort::inputs!["data" => tensor])
            .map_err(|error| format!("ort: run: {error}"))?;
        let mut result = Vec::with_capacity(outputs.len());
        for index in 0..outputs.len() {
            let (dims, data) = outputs[index]
                .try_extract_tensor::<f32>()
                .map_err(|error| format!("ort: output {index}: {error}"))?;
            result.push(NetOutput {
                shape: dims.iter().map(|dim| *dim as usize).collect(),
                data: data.to_vec(),
            });
        }
        Ok(result)
    }
}
