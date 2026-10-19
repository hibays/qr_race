//! RFC 4226 HOTP and RFC 6238 TOTP, implemented against the RFC test vectors.
//!
//! SHA-1 is not a weakness here: it is what every authenticator app speaks, and
//! the construction's security comes from the HMAC key, not the hash's collision
//! resistance. Six digits, 30-second steps, one step of clock skew.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;

/// TOTP time step in seconds (RFC 6238 default).
pub const STEP_SECONDS: u64 = 30;

/// Authenticator apps default to six digits.
pub const DEFAULT_DIGITS: u32 = 6;

pub fn step_at(now_ms: u64) -> u64 {
    now_ms / 1000 / STEP_SECONDS
}

/// HOTP value for an explicit counter (RFC 4226 §5.3).
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    let offset = (digest[19] & 0x0f) as usize;
    let binary = u32::from_be_bytes([
        digest[offset],
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]) & 0x7fff_ffff;

    binary % 10u32.pow(digits)
}

/// Verifies a code against `now_ms`, allowing `skew_steps` either side.
/// Returns the step that matched, so the caller can refuse a replay.
pub fn verify(secret: &[u8], code: &str, now_ms: u64, digits: u32, skew_steps: u32) -> Option<u64> {
    let text = code.trim();
    if text.len() != digits as usize || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let Ok(given) = text.parse::<u32>() else {
        return None;
    };

    let centered = step_at(now_ms);
    let low = centered.saturating_sub(u64::from(skew_steps));
    let high = centered.saturating_add(u64::from(skew_steps));
    for step in low..=high {
        let expected = hotp(secret, step, digits);
        if bool::from(expected.ct_eq(&given)) {
            return Some(step);
        }
    }
    None
}

/// The code an authenticator app would show right now, from a base32 secret.
/// Used by `qr_bench`-style tooling and by tests; the server never needs it.
pub fn current_code(secret_base32: &str, now_ms: u64, digits: u32) -> Option<String> {
    let normalized: String = secret_base32
        .trim()
        .trim_end_matches('=')
        .to_ascii_uppercase();
    let secret = data_encoding::BASE32_NOPAD
        .decode(normalized.as_bytes())
        .ok()?;
    Some(format!(
        "{:0width$}",
        hotp(&secret, step_at(now_ms), digits),
        width = digits as usize
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 Appendix B, SHA-1 seed "12345678901234567890", 8 digits.
    const SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn matches_the_rfc_6238_vectors() {
        let vectors = [
            (59u64, 94_287_082u32),
            (1_111_111_109, 7_081_804),
            (1_111_111_111, 14_050_471),
            (1_234_567_890, 89_005_924),
            (2_000_000_000, 69_279_037),
            (20_000_000_000, 65_353_130),
        ];
        for (unix_seconds, expected) in vectors {
            let step = unix_seconds / STEP_SECONDS;
            assert_eq!(hotp(SECRET, step, 8), expected, "t={unix_seconds}");
        }
    }

    #[test]
    fn six_digit_codes_round_trip_through_verify() {
        let now_ms = 1_700_000_000_000;
        let code = current_code("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", now_ms, 6).unwrap();
        assert_eq!(code.len(), 6);
        let secret = data_encoding::BASE32_NOPAD
            .decode(b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")
            .unwrap();
        assert_eq!(verify(&secret, &code, now_ms, 6, 1), Some(step_at(now_ms)));
    }

    #[test]
    fn skew_accepts_the_previous_step_but_not_an_old_one() {
        let now_ms = 1_700_000_000_000u64;
        let secret = b"12345678901234567890";
        let step = step_at(now_ms);
        let previous = format!("{:06}", hotp(secret, step - 1, 6));
        let old = format!("{:06}", hotp(secret, step - 5, 6));

        assert_eq!(verify(secret, &previous, now_ms, 6, 1), Some(step - 1));
        assert_eq!(verify(secret, &old, now_ms, 6, 1), None);
    }

    #[test]
    fn rejects_malformed_codes() {
        let now_ms = 1_700_000_000_000u64;
        for bad in ["", "12345", "1234567", "abcdef", "12 34 56"] {
            assert_eq!(verify(b"12345678901234567890", bad, now_ms, 6, 1), None);
        }
    }

    #[test]
    fn current_code_formats_leading_zeros() {
        // Any code is exactly `digits` wide, including one that starts with 0.
        let code = current_code("JBSWY3DPEHPK3PXP", 1_700_000_000_000, 6).unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
    }
}
