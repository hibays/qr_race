//! Port of `zxing/errorhandler.hpp`.
//!
//! The original passes an assignable `ErrorHandler` out-parameter to carry the
//! error code out of a function. This port uses `Result` instead, keeping the
//! error categories in one-to-one correspondence with the C++ sources.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrKind {
    NotFound,
    CheckSum,
    Reader,
    IllegalArgument,
    ReedSolomon,
    Format,
    Detector,
    IllegalState,
}

#[derive(Debug, Clone)]
pub struct ZXError {
    pub kind: ErrKind,
    pub msg: String,
}

impl ZXError {
    pub fn new(kind: ErrKind, msg: impl Into<String>) -> Self {
        Self {
            kind,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for ZXError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.msg)
    }
}

impl std::error::Error for ZXError {}

pub type ZXResult<T> = Result<T, ZXError>;

macro_rules! err_ctor {
    ($name:ident, $kind:ident) => {
        #[inline]
        pub fn $name(msg: impl Into<String>) -> ZXError {
            ZXError::new(ErrKind::$kind, msg)
        }
    };
}

err_ctor!(not_found, NotFound);
err_ctor!(check_sum, CheckSum);
err_ctor!(reader, Reader);
err_ctor!(illegal_argument, IllegalArgument);
err_ctor!(reed_solomon, ReedSolomon);
err_ctor!(format, Format);
err_ctor!(detector, Detector);
err_ctor!(illegal_state, IllegalState);
