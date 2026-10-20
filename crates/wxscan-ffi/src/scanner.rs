//! The scanner handle: construction, destruction, and its inference backend.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use wxscan::net::{Net, NetOutput};
use wxscan::detector::ssd_detector::QuadPoints;
use wxscan::{QRCodeResult, WeChatQRCode};

/// Inference backend. Empty when no backend feature is on, in which case a
/// scanner can only be created without models.
///
/// With both `tflite` and `tract` on, the weights decide: the model buffer is
/// tried as TFLite first and then as ONNX, so one binary takes either format.
pub(crate) enum Backend {
    #[cfg(feature = "tflite")]
    Tflite(wxscan::tflite::TfliteNet),
    #[cfg(feature = "tract")]
    Tract(wxscan::backend::tract::TractNet),
    /// Inference happens outside this library; see [`crate::host_net`].
    #[cfg(all(feature = "host-net", target_arch = "wasm32"))]
    Host(crate::host_net::HostNet),
}

impl Net for Backend {
    #[allow(unused_variables)]
    fn forward(&self, input: &[f32], shape: &[usize]) -> Result<Vec<NetOutput>, String> {
        match self {
            #[cfg(feature = "tflite")]
            Backend::Tflite(n) => n.forward(input, shape),
            #[cfg(feature = "tract")]
            Backend::Tract(n) => n.forward(input, shape),
            #[cfg(all(feature = "host-net", target_arch = "wasm32"))]
            Backend::Host(n) => n.forward(input, shape),
            #[cfg(not(any(feature = "tflite", feature = "tract", all(feature = "host-net", target_arch = "wasm32"))))]
            _ => Err("wxscan: built without an inference backend".to_string()),
        }
    }
}

/// A scanner handle.
///
/// **Not a pointer.** It is a number this library hands out and looks up in a
/// table of its own, and that is the whole point: a handle that has been
/// released, or was never valid, or was invented by a caller, resolves to
/// nothing and comes back as an ordinary failure. Were it an address, each of
/// those would instead be a read of freed or arbitrary memory, crashing
/// somewhere with no trace of where the mistake was made.
///
/// This matters because a scanner is routinely held by two sides at once that
/// cannot see each other's lifetimes: a managed application holding one for
/// still pictures while a camera binding, in another language, decodes frames
/// with the same handle. Reference counting alone ([`wxscan_scanner_retain`])
/// settles who frees it, but only a handle that is not an address makes a
/// stale one safe to present — and after a hot restart of the managed side,
/// stale handles are exactly what turns up.
///
/// Zero is never a scanner. It means "none", and is what a failed
/// [`wxscan_scanner_new`] returns.
///
/// Handles are never reused. A released number stays dead for the life of the
/// process, so a stale one can never come to name a different scanner — which
/// would put every one of the above problems back, silently.
pub type WxScanScannerId = usize;

/// A scanner instance.
///
/// Scanning takes `&self`, but the underlying algorithm keeps mutable state
/// (decoder rotation, connected-component caches), so one instance scans one
/// frame at a time. Create several instances to scan in parallel.
pub struct WxScanScanner {
    pub(crate) inner: Mutex<WeChatQRCode<Backend>>,
}

/// A scanner and the holders that are keeping it registered.
struct Registered {
    scanner: Arc<WxScanScanner>,
    /// One per holder: [`wxscan_scanner_new`] leaves one, each
    /// [`wxscan_scanner_retain`] adds one, each [`wxscan_scanner_release`]
    /// takes one away. At zero the entry goes.
    ///
    /// The `Arc` cannot stand in for this. It counts the entry plus whatever
    /// scans are running right now, which is a different question from how
    /// many holders exist.
    holders: usize,
}

/// Every live scanner, by handle.
///
/// A lock around the table only, never around a scan: a lookup clones the `Arc`
/// out and lets the guard go, so one scanner decoding a frame for a second or
/// two blocks nothing else here.
fn registry() -> &'static RwLock<HashMap<WxScanScannerId, Registered>> {
    static SCANNERS: OnceLock<RwLock<HashMap<WxScanScannerId, Registered>>> = OnceLock::new();
    SCANNERS.get_or_init(Default::default)
}

/// Puts a scanner in the table with one holder and returns its handle.
fn register(scanner: WxScanScanner) -> WxScanScannerId {
    // Starts at 1 so that zero is always "none", and only ever counts up.
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    write_registry().insert(
        id,
        Registered {
            scanner: Arc::new(scanner),
            holders: 1,
        },
    );
    id
}

/// The table, for writing.
///
/// A panic while the table is held would poison the lock and take every
/// scanner in the process down with it, which is a steep price for a map
/// operation that cannot fail. The poison is stepped over: nothing here leaves
/// the table half-written, so there is no broken state to protect.
fn write_registry() -> std::sync::RwLockWriteGuard<'static, HashMap<WxScanScannerId, Registered>> {
    registry().write().unwrap_or_else(|e| e.into_inner())
}

/// Looks a handle up, or returns nothing if it names no live scanner.
///
/// Public because a binding that links this crate as a Rust library — the wasm
/// module does — needs the results in a form the C ABI cannot carry, and so
/// resolves the handle itself and calls [`WxScanScanner::scan_upright`].
///
/// A debug build says so on stderr. In release it is silent: a handle arriving
/// after its scanner is gone is a real bug, but it is the caller's bug, and a
/// library that writes to a shipped application's log every frame is worse
/// than one that returns an empty result.
pub fn lookup_scanner(id: WxScanScannerId) -> Option<Arc<WxScanScanner>> {
    let found = registry()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .map(|e| Arc::clone(&e.scanner));

    #[cfg(debug_assertions)]
    if found.is_none() {
        eprintln!(
            "wxscan: handle {id} names no scanner. It was released, or it never \
             existed. Nothing was scanned."
        );
    }
    found
}

/// Runs something with the scanner a handle names, if it names one.
pub(crate) fn with_scanner<R>(
    id: WxScanScannerId,
    f: impl FnOnce(&WxScanScanner) -> R,
) -> Option<R> {
    // The Arc is cloned out and the table unlocked before `f` runs: `f` is
    // usually a scan, which takes milliseconds and the scanner's own lock.
    let scanner = lookup_scanner(id)?;
    Some(f(&scanner))
}

impl WxScanScanner {
    /// Scan an upright, tightly packed grayscale image, returning the decoded
    /// results together with the detector candidates.
    ///
    /// This is for Rust callers that link the crate as a library, such as a
    /// platform binding that needs the results in a form the C ABI does not
    /// carry. C callers go through [`crate::wxscan_scan_gray`] instead.
    ///
    /// Concurrent calls serialise on the scanner's own lock rather than
    /// failing: a second thread waits for the first.
    ///
    /// Returns empty vectors only if that lock is poisoned, which means some
    /// thread panicked while holding it. Every C entry point aborts on a panic
    /// before it could poison anything, so this is reachable only from a Rust
    /// caller that catches unwinds — and from then on that scanner is inert.
    pub fn scan_upright(
        &self,
        gray: &[u8],
        width: usize,
        height: usize,
    ) -> (Vec<QRCodeResult>, Vec<QuadPoints>) {
        match self.inner.lock() {
            Ok(inner) => inner.detect_and_decode_gray_with_candidates(gray, width, height),
            Err(_) => (Vec::new(), Vec::new()),
        }
    }
}

/// One weight buffer to a backend, or None when nothing in this build takes it.
///
/// Shared by the two constructors so that the buffer and the file forms cannot
/// drift apart on which formats they accept.
fn backend_from_bytes(bytes: &[u8]) -> Option<Backend> {
    // Each enabled backend gets a turn, so a build with both takes either
    // weight format and the caller does not have to say which it has.
    #[cfg(feature = "tflite")]
    match wxscan::tflite::TfliteNet::from_bytes(bytes) {
        Ok(n) => return Some(Backend::Tflite(n)),
        Err(e) => eprintln!("wxscan: tflite refused the weights: {e}"),
    }
    #[cfg(feature = "tract")]
    if let Ok(n) = wxscan::backend::tract::TractNet::from_bytes(bytes) {
        return Some(Backend::Tract(n));
    }
    let _ = bytes;
    None
}

/// Create a scanner from in-memory model buffers.
///
/// Passing NULL for both models selects the image-processing-only mode. It
/// still decodes, but the detection rate for small or distant symbols is
/// considerably lower, since that is what the CNN stage contributes.
///
/// Returns zero if a model fails to load. Release with
/// [`wxscan_scanner_release`].
///
/// # Safety
/// `detect` and `sr`, when not NULL, must point to at least the corresponding
/// number of readable bytes.
#[no_mangle]
pub unsafe extern "C" fn wxscan_scanner_new(
    detect: *const u8,
    detect_len: usize,
    sr: *const u8,
    sr_len: usize,
) -> WxScanScannerId {
    let load = |p: *const u8, n: usize| -> Result<Option<Backend>, ()> {
        if p.is_null() || n == 0 {
            return Ok(None);
        }
        backend_from_bytes(std::slice::from_raw_parts(p, n))
            .map(Some)
            .ok_or(())
    };

    let (detect, sr) = match (load(detect, detect_len), load(sr, sr_len)) {
        (Ok(d), Ok(s)) => (d, s),
        _ => return 0,
    };

    register(WxScanScanner {
        inner: Mutex::new(WeChatQRCode::new(detect, sr)),
    })
}

/// Create a scanner from model files on disk.
///
/// The same scanner [`wxscan_scanner_new`] builds, for a caller that has paths
/// rather than bytes — weights downloaded to a cache directory, say. The files
/// are read here, so a megabyte of weights never crosses the caller's language
/// boundary, and a binding does not need a file API of its own to offer this.
///
/// Either path may be NULL, meaning that network is simply absent, exactly as
/// a NULL buffer is to [`wxscan_scanner_new`]. Both NULL is the mode without
/// models.
///
/// Returns zero on any failure, and sets `status`, when not NULL, to say
/// which: a path that is not UTF-8 is [`WxScanStatus::BadArgument`], a file
/// that will not open is [`WxScanStatus::Unreadable`], and one that reads but
/// is not weights this build can load is [`WxScanStatus::WeightsRefused`].
/// Those are three different mistakes — a typo, a download that has not
/// happened, a file that is not a model — and only the caller can tell which
/// it made.
///
/// Release with [`wxscan_scanner_release`].
///
/// # Safety
/// Each path, when not NULL, must be a NUL terminated string, and `status`,
/// when not NULL, must point to a writable [`WxScanStatus`].
#[cfg(feature = "model-fs")]
#[no_mangle]
pub unsafe extern "C" fn wxscan_scanner_new_path(
    detect_path: *const std::ffi::c_char,
    sr_path: *const std::ffi::c_char,
    status: *mut crate::WxScanStatus,
) -> WxScanScannerId {
    use crate::WxScanStatus;

    let set = |s: WxScanStatus| {
        if !status.is_null() {
            *status = s;
        }
    };

    let load = |p: *const std::ffi::c_char| -> Result<Option<Backend>, WxScanStatus> {
        if p.is_null() {
            return Ok(None);
        }
        let path = std::ffi::CStr::from_ptr(p)
            .to_str()
            .map_err(|_| WxScanStatus::BadArgument)?;
        // Absent and unloadable are kept apart all the way out to the caller:
        // a path that is not there is a mistake in the calling code, while a
        // file that is there and will not load is a mistake in what was
        // downloaded, and the two are fixed in different places.
        let bytes = std::fs::read(path).map_err(|_| WxScanStatus::Unreadable)?;
        backend_from_bytes(&bytes)
            .map(Some)
            .ok_or(WxScanStatus::WeightsRefused)
    };

    let (detect, sr) = match (load(detect_path), load(sr_path)) {
        (Ok(d), Ok(s)) => (d, s),
        (Err(e), _) | (_, Err(e)) => {
            set(e);
            return 0;
        }
    };

    set(WxScanStatus::Ok);
    register(WxScanScanner {
        inner: Mutex::new(WeChatQRCode::new(detect, sr)),
    })
}

/// Create a scanner whose inference runs in the host, for the wasm build.
///
/// The weights are never passed in: the host holds them, and each flag says
/// only whether that network is available. Passing zero for both is the mode
/// without models, exactly as for [`wxscan_scanner_new`].
///
/// Release with [`wxscan_scanner_release`].
#[cfg(all(feature = "host-net", target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn wxscan_scanner_new_host(has_detector: i32, has_sr: i32) -> WxScanScannerId {
    let detect = (has_detector != 0).then(crate::host_net::HostNet::detector);
    let sr = (has_sr != 0).then(crate::host_net::HostNet::super_resolution);
    register(WxScanScanner {
        inner: Mutex::new(WeChatQRCode::new(
            detect.map(Backend::Host),
            sr.map(Backend::Host),
        )),
    })
}

/// Set the downscale factor applied before detection.
///
/// Values outside `(0, 1]` restore the default, which targets a 400x400 area.
#[no_mangle]
pub extern "C" fn wxscan_scanner_set_scale_factor(s: WxScanScannerId, v: f32) {
    with_scanner(s, |s| {
        if let Ok(mut inner) = s.inner.lock() {
            inner.set_scale_factor(v);
        }
    });
}

/// How many scanners are alive in this process.
///
/// For finding a holder that never gave its handle back. A scanner that is
/// leaked rather than released costs whatever its weights cost for the life of
/// the process, and without this there is no way to see that from outside —
/// which is the usual reason such a leak survives for months.
///
/// A test can assert this is back where it started; a debug build of an
/// application can watch it across a screen that opens and closes.
#[no_mangle]
pub extern "C" fn wxscan_scanner_count() -> usize {
    registry().read().unwrap_or_else(|e| e.into_inner()).len()
}

/// Take a reference to a scanner, returning the same handle for convenience.
///
/// For a second holder — typically a camera binding handed a scanner the
/// application already owns. It keeps the scanner alive whichever side lets go
/// first. Returns zero, and takes nothing, if the handle names no scanner.
///
/// Every retain must be matched by a [`wxscan_scanner_release`].
#[no_mangle]
pub extern "C" fn wxscan_scanner_retain(s: WxScanScannerId) -> WxScanScannerId {
    match write_registry().get_mut(&s) {
        Some(entry) => {
            entry.holders += 1;
            s
        }
        None => {
            #[cfg(debug_assertions)]
            eprintln!("wxscan: cannot retain handle {s}: it names no scanner");
            0
        }
    }
}

/// Give up a reference. The scanner is freed when the last holder goes.
///
/// Releasing a handle that names no scanner — one already released, or never
/// valid — does nothing. It is a bug on the caller's side, and a debug build
/// says so, but it is not one this library can do anything about at that point
/// and it is certainly not a reason to corrupt anything.
#[no_mangle]
pub extern "C" fn wxscan_scanner_release(s: WxScanScannerId) {
    // Taken out of the table under the lock, and dropped after it: the last
    // release runs the scanner's destructor, which tears down an inference
    // interpreter, and nothing else should wait on the table for that.
    let removed = {
        let mut table = write_registry();
        match table.get_mut(&s) {
            Some(entry) if entry.holders > 1 => {
                entry.holders -= 1;
                None
            }
            Some(_) => table.remove(&s),
            None => {
                #[cfg(debug_assertions)]
                eprintln!("wxscan: cannot release handle {s}: it names no scanner");
                None
            }
        }
    };
    drop(removed);
}

/// The downscale factor applied before detection, as set by
/// [`wxscan_scanner_set_scale_factor`]. A negative value means the default,
/// which targets a 400x400 area — or that `s` names no scanner.
#[no_mangle]
pub extern "C" fn wxscan_scanner_scale_factor(s: WxScanScannerId) -> f32 {
    match with_scanner(s, |s| s.inner.lock().ok().map(|i| i.scale_factor())).flatten() {
        Some(v) => v,
        None => -1.0,
    }
}

/// How confident the detector must be to report a candidate, 0.2 by default.
///
/// Lower recalls more weak symbols along with more false positives; higher does
/// the reverse. Values outside `(0, 1)` are ignored. Without models there is no
/// detector and this does nothing.
#[no_mangle]
pub extern "C" fn wxscan_scanner_set_confidence_threshold(s: WxScanScannerId, v: f32) {
    if !(v > 0.0 && v < 1.0) {
        return;
    }
    with_scanner(s, |s| {
        if let Some(params) = s.inner.lock().ok().as_mut().and_then(|i| i.detection_params_mut()) {
            params.confidence_threshold = v;
        }
    });
}

/// The confidence threshold in use, or a negative value when no detector is
/// loaded — or when `s` names no scanner, which is not distinguished here.
#[no_mangle]
pub extern "C" fn wxscan_scanner_confidence_threshold(s: WxScanScannerId) -> f32 {
    with_scanner(s, |s| match s.inner.lock() {
        Ok(inner) => inner.detection_params().map_or(-1.0, |p| p.confidence_threshold),
        Err(_) => -1.0,
    })
    .unwrap_or(-1.0)
}

/// The IoU above which two overlapping candidates are treated as one symbol,
/// 0.45 by default. Values outside `(0, 1)` are ignored.
#[no_mangle]
pub extern "C" fn wxscan_scanner_set_nms_threshold(s: WxScanScannerId, v: f32) {
    if !(v > 0.0 && v < 1.0) {
        return;
    }
    with_scanner(s, |s| {
        if let Some(params) = s.inner.lock().ok().as_mut().and_then(|i| i.detection_params_mut()) {
            params.nms_threshold = v;
        }
    });
}

/// The NMS threshold in use, or a negative value when no detector is loaded —
/// or when `s` names no scanner, which is not distinguished here.
#[no_mangle]
pub extern "C" fn wxscan_scanner_nms_threshold(s: WxScanScannerId) -> f32 {
    with_scanner(s, |s| match s.inner.lock() {
        Ok(inner) => inner.detection_params().map_or(-1.0, |p| p.nms_threshold),
        Err(_) => -1.0,
    })
    .unwrap_or(-1.0)
}

/// Whether the detector network is loaded. False also for a handle that names
/// no scanner: ask right after taking a reference, when the two cannot be
/// confused.
#[no_mangle]
pub extern "C" fn wxscan_scanner_has_detector(s: WxScanScannerId) -> i32 {
    with_scanner(s, |s| match s.inner.lock() {
        Ok(inner) => i32::from(inner.has_detector()),
        Err(_) => 0,
    })
    .unwrap_or(0)
}

/// Whether the super resolution network is loaded. False also for a handle
/// that names no scanner, as for the detector.
#[no_mangle]
pub extern "C" fn wxscan_scanner_has_super_resolution(s: WxScanScannerId) -> i32 {
    with_scanner(s, |s| match s.inner.lock() {
        Ok(inner) => i32::from(inner.has_super_resolution()),
        Err(_) => 0,
    })
    .unwrap_or(0)
}
