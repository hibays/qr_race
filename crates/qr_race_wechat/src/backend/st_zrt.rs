//! ONNX Runtime backend through `st-zrt` 0.4.
//!
//! Android packages ONNX Runtime 1.29 (`libonnxruntime.so`); desktop uses tract.
//! `st-zrt` sessions are `&self`-reentrant, so the pipeline no longer needs a
//! mutex around `run`.

use std::sync::OnceLock;

use st_zrt::{
    Environment, GraphOptimizationLevel, MemoryInfo, OwnedValue, Session, SessionOptions, Tensor,
};

use crate::net::{Net, NetOutput};

pub struct StZrtNet {
    session: Session,
}

fn environment() -> &'static Environment {
    static ENV: OnceLock<Environment> = OnceLock::new();
    ENV.get_or_init(|| Environment::new().expect("st-zrt: ONNX Runtime environment initializes"))
}

impl StZrtNet {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let options = SessionOptions::new().with_opt_level(GraphOptimizationLevel::All);
        let session = Session::from_bytes(environment(), bytes, options)
            .map_err(|error| format!("st-zrt: cannot load model: {error}"))?;
        Ok(Self { session })
    }
}

impl Net for StZrtNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        let shape: Vec<i64> = shape.iter().map(|dim| *dim as i64).collect();
        let memory = MemoryInfo::cpu().map_err(|error| format!("st-zrt: memory: {error}"))?;
        let tensor = Tensor::from_buffer(input, &shape, &memory)
            .map_err(|error| format!("st-zrt: input: {error}"))?;
        let mut outputs: Vec<Option<OwnedValue>> =
            (0..self.session.output_count()).map(|_| None).collect();
        self.session
            .run(&[&tensor], &mut outputs)
            .map_err(|error| format!("st-zrt: run: {error}"))?;
        let mut result = Vec::with_capacity(outputs.len());
        for (index, output) in outputs.iter().enumerate() {
            let output = output
                .as_ref()
                .ok_or_else(|| format!("st-zrt: output {index} missing"))?;
            let (_, dims) = output
                .element_type_and_shape()
                .map_err(|error| format!("st-zrt: output {index} shape: {error}"))?;
            let data = output
                .as_slice::<f32>()
                .map_err(|error| format!("st-zrt: output {index}: {error}"))?;
            result.push(NetOutput {
                shape: dims.iter().map(|dim| *dim as usize).collect(),
                data: data.to_vec(),
            });
        }
        Ok(result)
    }
}
