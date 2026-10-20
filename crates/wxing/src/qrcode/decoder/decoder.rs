//! Port of `zxing/qrcode/decoder/decoder.{hpp,cpp}`: the full module matrix to
//! text decoding chain, including the fallback that reads once normally and,
//! on failure, reads again mirrored.

use crate::common::bitmatrix::BitMatrix;
use crate::common::reedsolomon::{GenericGF, ReedSolomonDecoder};
use crate::error::{reader, ZXResult};
use crate::qrcode::version::Version;
use crate::result::{DecoderResult, QRCodeDecoderMetaData};

use super::bitmatrixparser::BitMatrixParser;
use super::datablock::get_data_blocks;
use super::decoded_bit_stream_parser;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoderState {
    NotStart,
    Start,
    ReadVersion,
    ReadErrorCorrectionLevel,
    ReadCodewords,
    Finish,
}

pub struct Decoder {
    rs_decoder: ReedSolomonDecoder,
    pub state: DecoderState,
    /// Fixed pattern matching score; the fork uses it as the confidence that
    /// this region really is a QR code
    pub possible_fix: f32,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            rs_decoder: ReedSolomonDecoder::new(GenericGF::qr_code_field_256()),
            state: DecoderState::NotStart,
            possible_fix: 0.0,
        }
    }

    /// Decodes in the normal orientation first, and on failure mirrors the
    /// matrix and decodes again (matching the original's two-stage `decode`).
    pub fn decode(&mut self, bits: &mut BitMatrix) -> ZXResult<DecoderResult> {
        let mut bits2 = BitMatrix::from_bits(bits.width(), bits.height(), bits.as_slice())?;
        if let Ok(rst) = self.decode_once(bits, false) {
            return Ok(rst);
        }
        let mut result = self.decode_once(&mut bits2, true)?;
        result.other = Some(QRCodeDecoderMetaData { mirrored: true });
        Ok(result)
    }

    fn decode_once(&mut self, bits: &mut BitMatrix, is_mirror: bool) -> ZXResult<DecoderResult> {
        let mut parser = BitMatrixParser::new(bits)?;
        if is_mirror {
            parser.remask();
            parser.set_mirror(true);
            parser
                .read_version()
                .map_err(|_| reader("Decoder::decode mirror & no mirror"))?;
            parser.read_format_information()?;
            // Version and format information read in the mirrored orientation
            // means the code really is mirrored, so transpose the matrix and
            // start over
            parser.mirror_matrix();
        }

        self.state = DecoderState::Start;
        self.possible_fix = 0.0;

        let version: &Version = parser
            .read_version()
            .map_err(|_| reader("Decoder::decode mirror & no mirror"))?;
        self.state = DecoderState::ReadVersion;

        let fixed_pattern_score = estimate_fixed_pattern(parser.bits_ref(), version)?;

        let format_info = parser.read_format_information()?;
        let ec_level = format_info.error_correction_level();
        self.state = DecoderState::ReadErrorCorrectionLevel;

        let codewords = parser
            .read_codewords()
            .map_err(|_| reader("Decoder::decode mirror & no mirror"))?;
        self.state = DecoderState::ReadCodewords;
        self.possible_fix = fixed_pattern_score;

        let mut data_blocks = get_data_blocks(&codewords, version, ec_level)?;
        let total_bytes: i32 = data_blocks.iter().map(|b| b.num_data_codewords).sum();

        let mut result_bytes = Vec::with_capacity(total_bytes as usize);
        for block in data_blocks.iter_mut() {
            let num_data = block.num_data_codewords as usize;
            self.correct_errors(&mut block.codewords, num_data)?;
            result_bytes.extend_from_slice(&block.codewords[..num_data]);
        }
        self.state = DecoderState::Finish;

        decoded_bit_stream_parser::decode(
            &result_bytes,
            version,
            ec_level,
            version.version_number(),
        )
    }

    /// Port of `correctErrors`: runs Reed-Solomon correction over one block and
    /// writes back only the data portion.
    fn correct_errors(&self, codeword_bytes: &mut [u8], num_data_codewords: usize) -> ZXResult<()> {
        let num_codewords = codeword_bytes.len();
        let mut codeword_ints: Vec<i32> = codeword_bytes.iter().map(|&b| b as i32).collect();
        let num_ec_codewords = (num_codewords - num_data_codewords) as i32;
        self.rs_decoder
            .decode(&mut codeword_ints, num_ec_codewords)?;
        for i in 0..num_data_codewords {
            codeword_bytes[i] = codeword_ints[i] as u8;
        }
        Ok(())
    }
}

/// Port of `Decoder::estimateFixedPattern`: how well the actual values at the
/// fixed pattern positions agree with the expected ones, normalized to [0,1].
/// Note that the original samples with x and y swapped (`get(i, j)`, where i
/// is the row); that is reproduced here.
pub fn estimate_fixed_pattern(bits: &BitMatrix, version: &Version) -> ZXResult<f32> {
    let fixed_pattern_value = version.build_fixed_pattern_value()?;
    let fixed_pattern_template = version.build_fixed_pattern_template()?;

    let mut sum = 0i32;
    let mut count = 0i32;
    for i in 0..bits.height() {
        for j in 0..bits.width() {
            if fixed_pattern_template.get(i, j) != 0 {
                sum += 1;
                if bits.get(i, j) == fixed_pattern_value.get(i, j) {
                    count += 1;
                }
            }
        }
    }
    if sum == 0 {
        return Ok(0.0);
    }
    let score = 2.0 * count as f32 / sum as f32 - 1.0;
    Ok(if score > 0.0 { score } else { 0.0 })
}
