//! Port of `zxing/qrcode/decoder/datablock.{hpp,cpp}`: splits the interleaved
//! codeword stream back into individual error correction blocks.

use crate::error::{illegal_argument, ZXResult};
use crate::qrcode::error_correction_level::ErrorCorrectionLevel;
use crate::qrcode::version::Version;

pub struct DataBlock {
    pub num_data_codewords: i32,
    pub codewords: Vec<u8>,
}

pub fn get_data_blocks(
    raw_codewords: &[u8],
    version: &Version,
    ec_level: ErrorCorrectionLevel,
) -> ZXResult<Vec<DataBlock>> {
    let ec_blocks = version.ec_blocks_for_level(ec_level);

    // First reserve the capacity of each block according to the block table
    let mut result: Vec<DataBlock> = Vec::new();
    for ecb in ec_blocks.ec_blocks.iter() {
        for _ in 0..ecb.count {
            let num_block_codewords = ec_blocks.ec_codewords + ecb.data_codewords;
            result.push(DataBlock {
                num_data_codewords: ecb.data_codewords,
                codewords: vec![0u8; num_block_codewords as usize],
            });
        }
    }
    if result.is_empty() {
        return Err(illegal_argument("no data blocks"));
    }

    // A long block can hold at most one codeword more than a short block
    let shorter_blocks_total_codewords = result[0].codewords.len();
    let mut longer_blocks_start_at = result.len() as i32 - 1;
    while longer_blocks_start_at >= 0 {
        let num_codewords = result[longer_blocks_start_at as usize].codewords.len();
        if num_codewords == shorter_blocks_total_codewords {
            break;
        }
        if num_codewords != shorter_blocks_total_codewords + 1 {
            return Err(illegal_argument("Data block sizes differ by more than 1"));
        }
        longer_blocks_start_at -= 1;
    }
    longer_blocks_start_at += 1;
    let longer_blocks_start_at = longer_blocks_start_at as usize;

    let shorter_blocks_num_data_codewords =
        shorter_blocks_total_codewords - ec_blocks.ec_codewords as usize;
    let num_result_blocks = result.len();
    let mut offset = 0usize;

    // Data codewords: interleaved by column
    for i in 0..shorter_blocks_num_data_codewords {
        for block in result.iter_mut() {
            block.codewords[i] = raw_codewords[offset];
            offset += 1;
        }
    }
    // The one extra data codeword held by the long blocks
    for block in result.iter_mut().skip(longer_blocks_start_at) {
        block.codewords[shorter_blocks_num_data_codewords] = raw_codewords[offset];
        offset += 1;
    }
    // Error correction codewords
    let max = shorter_blocks_total_codewords;
    for i in shorter_blocks_num_data_codewords..max {
        for j in 0..num_result_blocks {
            let i_offset = if j < longer_blocks_start_at { i } else { i + 1 };
            result[j].codewords[i_offset] = raw_codewords[offset];
            offset += 1;
        }
    }

    if offset != raw_codewords.len() {
        return Err(illegal_argument(
            "rawCodewordsOffset != rawCodewords.length",
        ));
    }
    Ok(result)
}
