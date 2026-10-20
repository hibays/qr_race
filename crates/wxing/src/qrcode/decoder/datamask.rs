//! Port of `zxing/qrcode/decoder/datamask.{hpp,cpp}`: the 8 data masks.

use crate::common::bitmatrix::BitMatrix;
use crate::error::{illegal_argument, ZXResult};

#[inline]
fn is_masked(reference: u8, x: usize, y: usize) -> bool {
    match reference {
        0 => (x + y) % 2 == 0,
        1 => x % 2 == 0,
        2 => y % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => ((x >> 1) + (y / 3)) % 2 == 0,
        5 => {
            let t = x * y;
            (t % 2) + (t % 3) == 0
        }
        6 => {
            let t = x * y;
            ((t % 2) + (t % 3)) % 2 == 0
        }
        7 => (((x + y) % 2) + ((x * y) % 3)) % 2 == 0,
        _ => false,
    }
}

/// Port of `DataMask::unmaskBitMatrix`. The mask is self-inverse, so the same
/// call both removes it and applies it again.
///
/// Note that the original passes (y, x) into `isMasked(x, y)`, so the x in the
/// mask formula is really the row index. Symmetric masks are unaffected, but
/// asymmetric ones (such as 001 and 010) must keep this argument order to
/// decode correctly.
pub fn unmask_bit_matrix(reference: u8, bits: &mut BitMatrix, dimension: usize) -> ZXResult<()> {
    if reference > 7 {
        return Err(illegal_argument("reference must be between 0 and 7"));
    }
    for y in 0..dimension {
        for x in 0..dimension {
            if is_masked(reference, y, x) {
                bits.flip(x, y);
            }
        }
    }
    Ok(())
}
