//! Port of `src/zxing/qrcode/decoder/**`.

pub mod bitmatrixparser;
pub mod datablock;
pub mod datamask;
pub mod decoded_bit_stream_parser;
#[allow(clippy::module_inception)]
pub mod decoder;
pub mod mode;
