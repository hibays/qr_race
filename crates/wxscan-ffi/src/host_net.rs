//! Inference performed by the host, for the browser.
//!
//! The wasm module that runs in a browser is 242 KB with no backend compiled
//! in and 12 MB with tract, because an ONNX runtime is most of an ONNX
//! runtime. The browser already has one — LiteRT.js runs the same `.tflite`
//! weights as the native build, on the GPU — so this backend hands the tensor
//! out through two wasm imports and takes the answer back.
//!
//! # The protocol
//!
//! `wxscan_host_forward` receives one NCHW input and returns the size in bytes
//! of the result it prepared, or zero if inference failed. The module then
//! allocates that much and calls `wxscan_host_fetch` to have it written in.
//! Two calls rather than one so that every allocation belongs to the module:
//! the host never has to free anything.
//!
//! The block is little-endian 32-bit words:
//!
//! ```text
//! word 0                     number of outputs
//! then, per output           rank, then `rank` dimensions
//! then                       the outputs' f32 data, one after another
//! ```
//!
//! Each output's element count is the product of its dimensions, so the reader
//! knows where one ends and the next begins.

use wxscan::net::NetOutput;
#[cfg(target_arch = "wasm32")]
use wxscan::net::Net;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wxscan")]
extern "C" {
    /// Run network `net` — 0 the detector, 1 super resolution — over an NCHW
    /// input. Returns the byte length of the prepared result, or 0 on failure.
    fn wxscan_host_forward(net: u32, input: *const f32, len: u32, shape: *const u32, rank: u32)
        -> u32;

    /// Write the result prepared by the last [`wxscan_host_forward`] into `dst`.
    /// Returns non-zero on success.
    fn wxscan_host_fetch(dst: *mut u8, len: u32) -> u32;
}

/// Which of the two networks a scanner slot stands for.
#[cfg(target_arch = "wasm32")]
pub struct HostNet {
    which: u32,
}

#[cfg(target_arch = "wasm32")]
impl HostNet {
    /// The detector.
    pub fn detector() -> Self {
        Self { which: 0 }
    }

    /// Super resolution.
    pub fn super_resolution() -> Self {
        Self { which: 1 }
    }
}

#[cfg(target_arch = "wasm32")]
impl Net for HostNet {
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        let dims: Vec<u32> = shape.iter().map(|d| *d as u32).collect();
        // SAFETY: both slices outlive the call, and their lengths are passed
        // alongside so the host reads nothing beyond them.
        let bytes = unsafe {
            wxscan_host_forward(
                self.which,
                input.as_ptr(),
                input.len() as u32,
                dims.as_ptr(),
                dims.len() as u32,
            )
        };
        if bytes == 0 {
            return Err("wxscan: the host reported no inference result".to_string());
        }
        if bytes as usize % 4 != 0 {
            return Err(format!("wxscan: host result of {bytes} bytes is not a whole number of words"));
        }

        let mut buffer = vec![0u32; bytes as usize / 4];
        // SAFETY: the buffer holds exactly the number of bytes just reported.
        let ok = unsafe { wxscan_host_fetch(buffer.as_mut_ptr() as *mut u8, bytes) };
        if ok == 0 {
            return Err("wxscan: the host failed to hand back its result".to_string());
        }
        parse(&buffer)
    }
}

/// Read the descriptor block documented above.
///
/// Split out from the import so that it can be tested on any target; off wasm
/// the tests are its only caller.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse(words: &[u32]) -> Result<Vec<NetOutput>, String> {
    let malformed = || "wxscan: the host result is truncated".to_string();

    let count = *words.first().ok_or_else(malformed)? as usize;
    let mut shapes = Vec::with_capacity(count);
    let mut cursor = 1;
    for _ in 0..count {
        let rank = *words.get(cursor).ok_or_else(malformed)? as usize;
        cursor += 1;
        let dims: Vec<usize> = words
            .get(cursor..cursor + rank)
            .ok_or_else(malformed)?
            .iter()
            .map(|d| *d as usize)
            .collect();
        cursor += rank;
        shapes.push(dims);
    }

    let mut outputs = Vec::with_capacity(count);
    for shape in shapes {
        let len: usize = shape.iter().product();
        let raw = words.get(cursor..cursor + len).ok_or_else(malformed)?;
        cursor += len;
        outputs.push(NetOutput {
            data: raw.iter().map(|w| f32::from_bits(*w)).collect(),
            shape,
        });
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_two_outputs() {
        let words = vec![
            2, // two outputs
            2, 1, 3, // first is 1x3
            1, 2, // second is 2
            1.0f32.to_bits(), 2.0f32.to_bits(), 3.0f32.to_bits(),
            4.0f32.to_bits(), 5.0f32.to_bits(),
        ];
        let out = parse(&words).expect("parse");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].shape, vec![1, 3]);
        assert_eq!(out[0].data, vec![1.0, 2.0, 3.0]);
        assert_eq!(out[1].shape, vec![2]);
        assert_eq!(out[1].data, vec![4.0, 5.0]);
    }

    #[test]
    fn rejects_a_truncated_block() {
        assert!(parse(&[2, 1, 4]).is_err());
    }
}
