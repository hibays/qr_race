//! Minimal FFI binding to libtensorflowlite_c, the LiteRT C API.
//!
//! This crate speaks tflite's own conventions: tensor layouts are NHWC and
//! shapes are whatever the model declares. Adapting that to an inference
//! abstraction is the caller's job; [`wxscan`](https://crates.io/crates/wxscan)
//! does it behind its `net::Net` trait, so nothing in the algorithm depends on
//! this crate.
//!
//! No binaries are vendored. The shared library is supplied by the caller:
//!   * point `TFLITE_LIB_DIR` at the directory to search (see build.rs), or
//!   * link it at the final link step, which is the common approach on Apple
//!     platforms
//!
//! The library name differs by distribution: builds of the C API are
//! `libtensorflowlite_c` on every platform, while Google's LiteRT AAR for
//! Android names the same API `libLiteRt`. Android links the first by default;
//! enable the `litert` feature for the second.

use std::ffi::c_void;



#[repr(C)] struct TfLiteModel { _private: [u8; 0] }
#[repr(C)] struct TfLiteInterpreterOptions { _private: [u8; 0] }
#[repr(C)] struct TfLiteInterpreter { _private: [u8; 0] }
#[repr(C)] struct TfLiteTensor { _private: [u8; 0] }
#[repr(C)] struct TfLiteDelegate { _private: [u8; 0] }

macro_rules! tflite_ffi {
    () => {
        fn TfLiteModelCreate(data: *const c_void, size: usize) -> *mut TfLiteModel;
        fn TfLiteModelDelete(model: *mut TfLiteModel);
        fn TfLiteInterpreterOptionsCreate() -> *mut TfLiteInterpreterOptions;
        fn TfLiteInterpreterOptionsSetNumThreads(o: *mut TfLiteInterpreterOptions, n: i32);
        fn TfLiteInterpreterOptionsAddDelegate(o: *mut TfLiteInterpreterOptions, d: *mut TfLiteDelegate);
        fn TfLiteInterpreterOptionsDelete(o: *mut TfLiteInterpreterOptions);
        fn TfLiteInterpreterCreate(m: *const TfLiteModel, o: *const TfLiteInterpreterOptions) -> *mut TfLiteInterpreter;
        fn TfLiteInterpreterDelete(i: *mut TfLiteInterpreter);
        fn TfLiteInterpreterAllocateTensors(i: *mut TfLiteInterpreter) -> i32;
        fn TfLiteInterpreterResizeInputTensor(i: *mut TfLiteInterpreter, idx: i32, dims: *const i32, dims_len: i32) -> i32;
        fn TfLiteInterpreterGetInputTensorCount(i: *const TfLiteInterpreter) -> i32;
        fn TfLiteInterpreterGetInputTensor(i: *const TfLiteInterpreter, idx: i32) -> *mut TfLiteTensor;
        fn TfLiteInterpreterGetOutputTensorCount(i: *const TfLiteInterpreter) -> i32;
        fn TfLiteInterpreterGetOutputTensor(i: *const TfLiteInterpreter, idx: i32) -> *mut TfLiteTensor;
        fn TfLiteInterpreterInvoke(i: *mut TfLiteInterpreter) -> i32;
        fn TfLiteTensorCopyFromBuffer(t: *mut TfLiteTensor, src: *const c_void, len: usize) -> i32;
        fn TfLiteTensorCopyToBuffer(t: *const TfLiteTensor, dst: *mut c_void, len: usize) -> i32;
        fn TfLiteTensorByteSize(t: *const TfLiteTensor) -> usize;
        fn TfLiteTensorNumDims(t: *const TfLiteTensor) -> i32;
        fn TfLiteTensorDim(t: *const TfLiteTensor, dim: i32) -> i32;

        // XNNPACK delegate. Passing NULL options uses defaults (header doc).
        fn TfLiteXNNPackDelegateCreate(options: *const c_void) -> *mut TfLiteDelegate;
        fn TfLiteXNNPackDelegateDelete(delegate: *mut TfLiteDelegate);
    };
}

// Desktop platforms link libtensorflowlite_c, the name the upstream C API
// builds use.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
#[link(name = "tensorflowlite_c")]
extern "C" { tflite_ffi!(); }

// So does Android, when the library came from a build of the C API - which is
// what a caller building TensorFlow itself ends up with, and what wxscan
// ships for every platform.
#[cfg(all(target_os = "android", not(feature = "litert")))]
#[link(name = "tensorflowlite_c")]
extern "C" { tflite_ffi!(); }

// Google's LiteRT distribution for Android names the same C API libLiteRt,
// and an AAR is where most Android callers get it. Nothing can detect which
// one is on the link line, so it is a feature rather than a guess.
#[cfg(all(target_os = "android", feature = "litert"))]
#[link(name = "LiteRt")]
extern "C" { tflite_ffi!(); }

// iOS resolves the symbols from TensorFlowLiteC.framework at the application
// link step. #[link] must not be used here: this crate is built as a static
// library and Xcode performs the final link, where the framework is already
// attached to the app target.
#[cfg(target_os = "ios")]
extern "C" { tflite_ffi!(); }

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "ios"
)))]
#[link(name = "tensorflowlite_c")]
extern "C" { tflite_ffi!(); }

// ── Safe wrapper ──

pub struct Model {
    inner: *mut TfLiteInterpreter,
    _model: *mut TfLiteModel,
    /// `TfLiteModelCreate` does NOT copy the buffer — the interpreter holds a
    /// raw pointer into it for the lifetime of the model. We must keep the
    /// bytes alive ourselves; otherwise Pad/Conv/etc. read freed memory at
    /// invoke time and either return garbage or SIGSEGV on huge bzero.
    _bytes: Vec<u8>,
    /// XNNPACK delegate must outlive the interpreter and be deleted afterwards.
    xnn_delegate: *mut TfLiteDelegate,
    input_count: i32,
    output_count: i32,
}

unsafe impl Send for Model {}
unsafe impl Sync for Model {}

impl Model {
    pub fn from_bytes(model_bytes: &[u8]) -> Result<Self, String> {
        Self::from_bytes_opts(model_bytes, true)
    }

    /// Whether the bytes are a model this build can read, and nothing further.
    pub fn parses(model_bytes: &[u8]) -> Result<(), String> {
        let model = unsafe { TfLiteModelCreate(model_bytes.as_ptr() as _, model_bytes.len()) };
        if model.is_null() {
            return Err("TfLiteModelCreate failed".into());
        }
        unsafe { TfLiteModelDelete(model) };
        Ok(())
    }

    /// `use_xnn`=false skips the XNNPACK delegate. Models whose input is
    /// resized dynamically (the PNet pyramid) must disable it: XNNPACK builds a
    /// static graph, and a resize afterwards fails with `failed to reshape
    /// runtime`.
    pub fn from_bytes_opts(model_bytes: &[u8], use_xnn: bool) -> Result<Self, String> {
        let bytes = model_bytes.to_vec();
        let model = unsafe { TfLiteModelCreate(bytes.as_ptr() as _, bytes.len()) };
        if model.is_null() { return Err("TfLiteModelCreate failed".into()); }

        let options = unsafe { TfLiteInterpreterOptionsCreate() };
        if options.is_null() { unsafe { TfLiteModelDelete(model) }; return Err("OptionsCreate failed".into()); }
        unsafe { TfLiteInterpreterOptionsSetNumThreads(options, 4); }

        let xnn_delegate = if use_xnn {
            let d = unsafe { TfLiteXNNPackDelegateCreate(std::ptr::null()) };
            if !d.is_null() { unsafe { TfLiteInterpreterOptionsAddDelegate(options, d); } }
            d
        } else {
            std::ptr::null_mut()
        };

        let inner = unsafe { TfLiteInterpreterCreate(model, options) };
        unsafe { TfLiteInterpreterOptionsDelete(options) };
        if inner.is_null() {
            if !xnn_delegate.is_null() { unsafe { TfLiteXNNPackDelegateDelete(xnn_delegate) }; }
            unsafe { TfLiteModelDelete(model) };
            return Err("InterpreterCreate failed".into());
        }
        let s = unsafe { TfLiteInterpreterAllocateTensors(inner) };
        if s != 0 {
            unsafe { TfLiteInterpreterDelete(inner) };
            if !xnn_delegate.is_null() { unsafe { TfLiteXNNPackDelegateDelete(xnn_delegate) }; }
            unsafe { TfLiteModelDelete(model) };
            return Err(format!("AllocateTensors status={s}"));
        }
        let ic = unsafe { TfLiteInterpreterGetInputTensorCount(inner) };
        let oc = unsafe { TfLiteInterpreterGetOutputTensorCount(inner) };
        Ok(Self { inner, _model: model, _bytes: bytes, xnn_delegate, input_count: ic, output_count: oc })
    }

    /// Resizes to the target shape before calling AllocateTensors, so the
    /// XNNPACK delegate binds to the final shape. This keeps dynamic input
    /// sizes while still using the delegate.
    pub fn from_bytes_resized(
        model_bytes: &[u8],
        input_index: i32,
        dims: &[i32],
        num_threads: i32,
    ) -> Result<Self, String> {
        let bytes = model_bytes.to_vec();
        let model = unsafe { TfLiteModelCreate(bytes.as_ptr() as _, bytes.len()) };
        if model.is_null() { return Err("TfLiteModelCreate failed".into()); }

        let options = unsafe { TfLiteInterpreterOptionsCreate() };
        if options.is_null() { unsafe { TfLiteModelDelete(model) }; return Err("OptionsCreate failed".into()); }
        unsafe { TfLiteInterpreterOptionsSetNumThreads(options, num_threads); }

        let xnn_delegate = unsafe { TfLiteXNNPackDelegateCreate(std::ptr::null()) };
        if !xnn_delegate.is_null() {
            unsafe { TfLiteInterpreterOptionsAddDelegate(options, xnn_delegate); }
        }

        let inner = unsafe { TfLiteInterpreterCreate(model, options) };
        unsafe { TfLiteInterpreterOptionsDelete(options) };
        if inner.is_null() {
            if !xnn_delegate.is_null() { unsafe { TfLiteXNNPackDelegateDelete(xnn_delegate) }; }
            unsafe { TfLiteModelDelete(model) };
            return Err("InterpreterCreate failed".into());
        }

        // The order matters: resize, then allocate. The delegate binds to the
        // shape at allocation time.
        let s = unsafe {
            TfLiteInterpreterResizeInputTensor(inner, input_index, dims.as_ptr(), dims.len() as i32)
        };
        if s != 0 {
            unsafe { TfLiteInterpreterDelete(inner) };
            if !xnn_delegate.is_null() { unsafe { TfLiteXNNPackDelegateDelete(xnn_delegate) }; }
            unsafe { TfLiteModelDelete(model) };
            return Err(format!("resize status={s}"));
        }
        let s = unsafe { TfLiteInterpreterAllocateTensors(inner) };
        if s != 0 {
            unsafe { TfLiteInterpreterDelete(inner) };
            if !xnn_delegate.is_null() { unsafe { TfLiteXNNPackDelegateDelete(xnn_delegate) }; }
            unsafe { TfLiteModelDelete(model) };
            return Err(format!("AllocateTensors status={s}"));
        }
        let ic = unsafe { TfLiteInterpreterGetInputTensorCount(inner) };
        let oc = unsafe { TfLiteInterpreterGetOutputTensorCount(inner) };
        Ok(Self { inner, _model: model, _bytes: bytes, xnn_delegate, input_count: ic, output_count: oc })
    }

    pub fn input_count(&self) -> i32 { self.input_count }
    pub fn output_count(&self) -> i32 { self.output_count }

    pub fn input_shape(&self, index: i32) -> Vec<i32> {
        let t = unsafe { TfLiteInterpreterGetInputTensor(self.inner, index) };
        (0..unsafe { TfLiteTensorNumDims(t) }).map(|i| unsafe { TfLiteTensorDim(t, i) }).collect()
    }
    pub fn output_shape(&self, index: i32) -> Vec<i32> {
        let t = unsafe { TfLiteInterpreterGetOutputTensor(self.inner, index) };
        (0..unsafe { TfLiteTensorNumDims(t) }).map(|i| unsafe { TfLiteTensorDim(t, i) }).collect()
    }

    /// Changes the input shape at runtime (the PNet pyramid resizes per level).
    /// Tensors must be reallocated afterwards.
    pub fn resize_input(&mut self, index: i32, dims: &[i32]) -> Result<(), String> {
        let s = unsafe {
            TfLiteInterpreterResizeInputTensor(self.inner, index, dims.as_ptr(), dims.len() as i32)
        };
        if s != 0 { return Err(format!("resize status={s}")); }
        let s = unsafe { TfLiteInterpreterAllocateTensors(self.inner) };
        if s != 0 { return Err(format!("realloc status={s}")); }
        Ok(())
    }

    pub fn set_input_f32(&mut self, index: i32, data: &[f32]) {
        let t = unsafe { TfLiteInterpreterGetInputTensor(self.inner, index) };
        unsafe { TfLiteTensorCopyFromBuffer(t, data.as_ptr() as _, data.len() * 4); }
    }

    pub fn invoke(&mut self) -> Result<(), String> {
        let s = unsafe { TfLiteInterpreterInvoke(self.inner) };
        if s != 0 { Err(format!("invoke status={s}")) } else { Ok(()) }
    }

    pub fn output_byte_size(&self, index: i32) -> usize {
        let t = unsafe { TfLiteInterpreterGetOutputTensor(self.inner, index) };
        unsafe { TfLiteTensorByteSize(t) }
    }

    pub fn get_output_f32(&self, index: i32, data: &mut [f32]) {
        let t = unsafe { TfLiteInterpreterGetOutputTensor(self.inner, index) };
        unsafe { TfLiteTensorCopyToBuffer(t, data.as_mut_ptr() as _, data.len() * 4); }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            // Interpreter must be torn down before the delegate it references.
            TfLiteInterpreterDelete(self.inner);
            if !self.xnn_delegate.is_null() {
                TfLiteXNNPackDelegateDelete(self.xnn_delegate);
            }
            TfLiteModelDelete(self._model);
        }
    }
}

/// A cached interpreter for a single-input network.
///
/// The input size of both models varies with the image, but stays constant
/// within one video stream, so the interpreter is cached by input shape and
/// rebuilt only when the shape changes.
///
/// Resizing an existing interpreter is not an option: XNNPACK builds a static
/// graph, and resizing after the delegate has taken effect (at
/// AllocateTensors) fails with `failed to reshape runtime`. Resizing before
/// allocating keeps dynamic sizes while still benefiting from XNNPACK.
pub struct TfliteNet {
    bytes: Vec<u8>,
    state: std::sync::Mutex<Option<(Vec<i32>, Model)>>,
    num_threads: i32,
}

/// One output tensor, in the layout the model produced it.
pub struct Tensor {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

impl TfliteNet {
    pub fn from_bytes(model_bytes: &[u8]) -> Result<Self, String> {
        // Only that the weights parse. This used to build a whole interpreter
        // and allocate its tensors, which is more than the check needs and
        // more than these models can answer: both take a dynamic input shape —
        // the detector scales with the image, super resolution with the crop —
        // so there is no meaningful shape to allocate for until [`forward`]
        // resizes to a real one. Some TFLite builds tolerate allocating at the
        // unset shape and some refuse it, and where they refuse it every model
        // was rejected here and the scanner fell back to having none, silently.
        Model::parses(model_bytes)?;
        Ok(Self {
            bytes: model_bytes.to_vec(),
            state: std::sync::Mutex::new(None),
            num_threads: 4,
        })
    }

    pub fn with_threads(mut self, n: i32) -> Self {
        self.num_threads = n;
        self
    }

    /// Runs the graph once.
    ///
    /// `dims` is the input shape in the model's own layout, which for these
    /// models is NHWC. The returned tensors keep the shapes the model declares;
    /// no layout conversion happens here.
    pub fn run(&self, input: &[f32], dims: &[i32]) -> Result<Vec<Tensor>, String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "tflite mutex poisoned".to_string())?;
        let need_rebuild = match guard.as_ref() {
            Some((cached_dims, _)) => cached_dims.as_slice() != dims,
            None => true,
        };
        if need_rebuild {
            let m = Model::from_bytes_resized(&self.bytes, 0, dims, self.num_threads)?;
            *guard = Some((dims.to_vec(), m));
        }
        let m = &mut guard.as_mut().unwrap().1;

        m.set_input_f32(0, input);
        m.invoke()?;

        let mut outs = Vec::new();
        for i in 0..m.output_count() {
            let shape: Vec<usize> = m.output_shape(i).iter().map(|&v| v as usize).collect();
            let len = m.output_byte_size(i) / 4;
            let mut data = vec![0f32; len];
            m.get_output_f32(i, &mut data);
            outs.push(Tensor { data, shape });
        }
        Ok(outs)
    }
}
