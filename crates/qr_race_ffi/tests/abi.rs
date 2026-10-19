//! The C ABI without a camera: shape and error contract only.

use qr_race_ffi::{qr_qr_webp, qr_race_ffi_version, qr_scanner_preview, qr_scanner_state};

#[test]
fn state_without_a_session_is_valid_json() {
    let mut buffer = vec![0u8; 4096];
    let length = unsafe { qr_scanner_state(buffer.as_mut_ptr(), buffer.len()) };
    assert!(length > 0, "expected JSON, got {length}");

    let json: serde_json::Value =
        serde_json::from_slice(&buffer[..length as usize]).expect("valid JSON");
    assert_eq!(json["running"], false);
    assert_eq!(json["preview_seq"], 0);
    assert_eq!(json["hits"], serde_json::json!([]));
}

#[test]
fn a_small_buffer_reports_the_needed_size() {
    let mut buffer = [0u8; 4];
    let length = unsafe { qr_scanner_state(buffer.as_mut_ptr(), buffer.len()) };
    assert!(length < 0, "expected -(needed), got {length}");
    assert!(-length > 4);
}

#[test]
fn null_and_empty_buffers_are_refused() {
    assert_eq!(unsafe { qr_scanner_state(std::ptr::null_mut(), 0) }, -1);
    assert_eq!(unsafe { qr_scanner_preview(std::ptr::null_mut(), 0) }, -1);
}

#[test]
fn preview_is_absent_without_a_session() {
    let mut buffer = vec![0u8; 16];
    assert_eq!(
        unsafe { qr_scanner_preview(buffer.as_mut_ptr(), buffer.len()) },
        -1
    );
}

#[test]
fn qr_webp_bytes_decode_back() {
    let payload = "https://example.test/relay";
    let mut buffer = vec![0u8; 1024 * 1024];
    let written = unsafe {
        qr_qr_webp(
            payload.as_ptr(),
            payload.len(),
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    assert!(written > 0, "expected WebP bytes, got {written}");
    buffer.truncate(written as usize);
    assert_eq!(&buffer[0..4], b"RIFF");

    let (luma, width, height) = qr_race_qr::webp_luma(&buffer).expect("WebP decodes");
    let frame = qr_race_scan::GrayFrame::new(width, height, luma).expect("frame builds");
    let engine = qr_race_scan::ScanEngine::new(Box::new(qr_race_scan::RxingDecoder::new()));
    let outcome = engine.scan(&frame).expect("scan runs");
    assert_eq!(outcome.hits[0].text, payload);
}

#[test]
fn qr_webp_rejects_a_null_payload() {
    assert_eq!(
        unsafe { qr_qr_webp(std::ptr::null(), 0, std::ptr::null_mut(), 0) },
        -1
    );
}

#[test]
fn the_abi_is_versioned() {
    assert_eq!(qr_race_ffi_version(), 1);
}
