//! # Huffman_encoder
//!
//! This module implements the encoding procedures defined in T.81 §F.1.2:
//!
//!   - §F.1.2.1: Huffman encoding of DC coefficients.
//!   - §F.1.2.2: Huffman encoding of AC coefficients (Figure F.2 and F.3).
//!
//! # How a single 8*8 block is encoded
//!
//! Each quantized block consists of 64 coefficients in zig-zag order:
//!
//!   - `zz[0]`     = DC coefficient (the average brightness of the block).
//!   - `zz[1..63]` = AC coefficients (spatial frequency detail).
//!
//! ## DC encoding
//!
//! The DC coefficient is encoded *differentially*: instead of encoding the
//! absolute value, we encode the difference from the previous block's DC
//! value (T.81 §A.3.5, §F.1.1.5.1):
//!
//!   DIFF = DC_current - DC_previous
//!
//! DIFF is encoded in two parts:
//!
//! 1. **Category (SSSS)** - the number of bits needed to represent |DIFF|.
//!    This is Huffman-coded using the DC Huffman table.
//!
//! 2. **Additional bits** - the SSSS least-significant bits of DIFF (or
//!    of DIFF-1 if DIFF is negative). These are appended raw, without
//!    Huffman coding.
//!
//! ## AC encoding (T.81 §F.1.2.2, Figures F.2 and F.3)
//!
//! AC coefficients are encoded using a run-length scheme:
//!
//! - Zeros are counted as a *run*.
//! - When a non-zero coefficient is found, the encoder emits:
//!   1. A Huffman code for the composite value RS = (run << 4) | category.
//!   2. Additional bits for the exact coefficient value.
//!
//! Special symbols:
//! - **EOB (0x00)**: signals that all remaining coefficients are zero.
//! - **ZRL (0xF0)**: signals a run of exactly 16 zeros (when the run
//!   exceeds 15).

use crate::bitstream::BitWriter;
use crate::entropy::huffman_table::{category, HuffmanTable};
use crate::error::{Error, Result};

/// Encode a sequence of quantized blocks into the bitstream.
///
/// This is a convenience wrapper that encodes multiple blocks while
/// maintaining the running DC prediction across blocks.
///
/// # Errors
///
/// Returns [`Error::MissingHuffmanCode`] if a required Huffman code is
/// not present in the table. This indicates a bug in table construction.
pub fn encode_blocks
(
    blocks: &[[i16; 64]],
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    prev_dc: &mut i16,
    writer: &mut BitWriter,
) -> Result<()>
{
    for block in blocks
    {
        encode_block(block, dc_table, ac_table, prev_dc, writer)?;
    }
    Ok(())
}

/// Encode a single quantized 8*8 block.
///
/// # Arguments
///
/// * `zz` - 64 quantized coefficients in zig-zag order.
/// * `dc_table` / `ac_table` - Huffman tables for DC and AC symbols.
/// * `prev_dc` - mutable reference to the DC prediction.
/// * `writer` - the bitstream to write to.
///
/// # Errors
///
/// Returns [`Error::MissingHuffmanCode`] if any required symbol has no
/// code assigned.
pub fn encode_block
(
    zz: &[i16; 64],
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    prev_dc: &mut i16,
    writer: &mut BitWriter,
) -> Result<()>
{
    let dc = zz[0];
    let diff = dc - *prev_dc;
    *prev_dc = dc;

    encode_dc_coefficient(diff, dc_table, writer)?;
    encode_ac_coefficients(&zz[1..], ac_table, writer)?;

    Ok(())
}

/// Encode a DC difference value (T.81 §F.1.2.1).
///
/// The DC difference DIFF is encoded as:
///   1. Huffman code for category SSSS (looked up in `table`).
///   2. SSSS additional bits specifying the exact value.
fn encode_dc_coefficient
(
    diff: i16,
    table: &HuffmanTable,
    writer: &mut BitWriter,
) -> Result<()>
{
    let ssss = category(diff);

    let code = table.ehufco[ssss as usize];
    let size = table.ehufsi[ssss as usize];

    if size == 0
    {
        return Err(Error::MissingHuffmanCode {
            symbol: ssss as u16,
            context: format!("DC category {} (diff={})", ssss, diff),
        });
    }

    writer.write_bits(code, size)?;

    // Append additional bits for the exact value of DIFF.
    // For SSSS = 0 (DIFF = 0), no additional bits are needed.
    if ssss > 0
    {
        let additional = encode_additional_bits(diff, ssss);
        writer.write_bits(additional as u32, ssss)?;
    }

    Ok(())
}

/// Encode the 63 AC coefficients of a block (T.81 §F.1.2.2, Figure F.2).
///
/// Walks through the coefficients in zig-zag order, counting zero-runs
/// and encoding non-zero coefficients as (run, category, additional bits)
/// triples.
fn encode_ac_coefficients
(
    ac: &[i16], // 63 coefficients: zz[1..64]
    table: &HuffmanTable,
    writer: &mut BitWriter,
) -> Result<()>
{
    let mut zero_run: u8 = 0;

    for &coeff in ac.iter().take(63)
    {
        if coeff == 0
        {
            zero_run += 1;
            continue;
        }

        // Emit ZRL (0xF0) for each complete run of 16 zeros.
        while zero_run > 15
        {
            let code = table.ehufco[0xF0];
            let size = table.ehufsi[0xF0];
            if size == 0
            {
                return Err
                (
                    Error::MissingHuffmanCode 
                    {
                        symbol: 0xF0,
                        context: "AC ZRL symbol (run of 16 zeros)".to_string(),
                    }
                );
            }
            writer.write_bits(code, size)?;
            zero_run -= 16;
        }

        // Encode the run/category composite RS = (RRRR << 4) | SSSS.
        let ssss = category(coeff);
        let rs = ((zero_run as u16) << 4) | (ssss as u16);

        let code = table.ehufco[rs as usize];
        let size = table.ehufsi[rs as usize];
        if size == 0
        {
            return Err
            (
                Error::MissingHuffmanCode 
                {
                    symbol: rs,
                    context: format!
                    (
                        "AC symbol 0x{:02X} (run={}, category={}, coeff={})",
                        rs, zero_run, ssss, coeff,
                    ),
                }
            );
        }
        writer.write_bits(code, size)?;

        let additional = encode_additional_bits(coeff, ssss);
        writer.write_bits(additional as u32, ssss)?;

        zero_run = 0;
    }

    // If the block ends with trailing zeros, emit EOB (0x00).
    if zero_run > 0
    {
        let code = table.ehufco[0x00];
        let size = table.ehufsi[0x00];
        if size == 0
        {
            return Err
            (
                Error::MissingHuffmanCode 
                {
                    symbol: 0x00,
                    context: "AC EOB symbol (end of block)".to_string(),
                }
            );
        }
        writer.write_bits(code, size)?;
    }

    Ok(())
}

/// Compute the additional bits for a coefficient value.
///
/// T.81 §F.1.2.1.1 and §F.1.2.2.1:
///
/// - If the value is positive: the additional bits are the SSSS
///   least-significant bits of the value itself.
/// - If the value is negative: the additional bits are the SSSS
///   least-significant bits of (value - 1).
///
/// This encoding maps each pair (positive, negative) in a category to
/// distinct bit patterns. For example, in category 2 (range -3..-2,
/// 2..3):
///
/// | Value | Additional bits |
/// |-------|-----------------|
/// |  -3   | 00              |
/// |  -2   | 01              |
/// |  +2   | 10              |
/// |  +3   | 11              |
#[inline]
fn encode_additional_bits(value: i16, ssss: u8) -> u16
{
    if value >= 0
    {
        value as u16
    }
    else
    {
        let mask = (1u16 << ssss) - 1;
        (value as u16).wrapping_sub(1) & mask
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use crate::entropy::huffman_table::{build_table, collect_frequencies, MAX_DC_CATEGORIES};

    /// Helper: build DC and AC tables from a set of blocks.
    fn tables_for(blocks: &[[i16; 64]]) -> (HuffmanTable, HuffmanTable)
    {
        let (dc_freq, ac_freq) = collect_frequencies(blocks);
        let dc_table = build_table(&dc_freq.counts, MAX_DC_CATEGORIES - 1);
        let ac_table = build_table(&ac_freq.counts, 255);
        (dc_table, ac_table)
    }

    #[test]
    fn encode_single_zero_block()
    {
        let block = [0i16; 64];
        let (dc, ac) = tables_for(&[block]);
        let mut writer = BitWriter::with_capacity(64);
        let mut prev_dc = 0i16;
        encode_block(&block, &dc, &ac, &mut prev_dc, &mut writer).unwrap();
        writer.flush_with_ones().unwrap();
        let data = writer.into_bytes();
        assert!(!data.is_empty());
        assert_eq!(prev_dc, 0);
    }

    #[test]
    fn encode_dc_prediction_updates()
    {
        let mut b1 = [0i16; 64]; b1[0] = 10;
        let mut b2 = [0i16; 64]; b2[0] = 20;
        let blocks = [b1, b2];
        let (dc, ac) = tables_for(&blocks);

        let mut writer = BitWriter::with_capacity(128);
        let mut prev_dc = 0i16;

        encode_block(&blocks[0], &dc, &ac, &mut prev_dc, &mut writer).unwrap();
        assert_eq!(prev_dc, 10);

        encode_block(&blocks[1], &dc, &ac, &mut prev_dc, &mut writer).unwrap();
        assert_eq!(prev_dc, 20);
    }

    #[test]
    fn encode_blocks_batch()
    {
        let mut blocks = Vec::new();
        for i in 0..5
        {
            let mut b = [0i16; 64];
            b[0] = i * 10;
            b[1] = 5;
            blocks.push(b);
        }
        let (dc, ac) = tables_for(&blocks);

        let mut writer = BitWriter::with_capacity(256);
        let mut prev_dc = 0i16;
        encode_blocks(&blocks, &dc, &ac, &mut prev_dc, &mut writer).unwrap();
        assert_eq!(prev_dc, 40); // Last block's DC = 40
    }

    #[test]
    fn encode_block_with_large_ac_run()
    {
        // 20 zeros then a non-zero coefficient -> ZRL + run/size.
        let mut block = [0i16; 64];
        block[0] = 0;   // DC
        block[21] = 10; // AC at position 21: run of 20 zeros
        let (dc, ac) = tables_for(&[block]);

        let mut writer = BitWriter::with_capacity(128);
        let mut prev_dc = 0i16;
        encode_block(&block, &dc, &ac, &mut prev_dc, &mut writer).unwrap();
    }

    #[test]
    fn encode_block_with_all_nonzero_ac()
    {
        // Block with no trailing zeros -> no EOB needed.
        let mut block = [0i16; 64];
        block[0] = 50;
        for coeff in block.iter_mut().skip(1) { *coeff = 1; }
        let (dc, ac) = tables_for(&[block]);

        let mut writer = BitWriter::with_capacity(256);
        let mut prev_dc = 0i16;
        encode_block(&block, &dc, &ac, &mut prev_dc, &mut writer).unwrap();
    }

    #[test]
    fn encode_negative_dc_diff()
    {
        let mut b1 = [0i16; 64]; b1[0] = 50;
        let mut b2 = [0i16; 64]; b2[0] = 10; // diff = -40
        let blocks = [b1, b2];
        let (dc, ac) = tables_for(&blocks);

        let mut writer = BitWriter::with_capacity(128);
        let mut prev_dc = 0i16;
        encode_blocks(&blocks, &dc, &ac, &mut prev_dc, &mut writer).unwrap();
    }

    #[test]
    fn encode_additional_bits_positive()
    {
        // For value=5, ssss=3: additional bits = 5 = 0b101.
        assert_eq!(encode_additional_bits(5, 3), 5);
    }

    #[test]
    fn encode_additional_bits_negative()
    {
        // For value=-5, ssss=3: additional bits = (-5 - 1) & 0b111 = -6 & 7 = 2.
        // This maps to the lower half of category 3.
        assert_eq!(encode_additional_bits(-5, 3), 2);
    }

    #[test]
    fn encode_additional_bits_minus_one()
    {
        // For value=-1, ssss=1: (-1 - 1) & 1 = -2 & 1 = 0.
        assert_eq!(encode_additional_bits(-1, 1), 0);
    }

    #[test]
    fn encode_additional_bits_plus_one()
    {
        assert_eq!(encode_additional_bits(1, 1), 1);
    }

    #[test]
    fn encode_dc_missing_code_returns_error()
    {
        // Table vide : aucun code assigné
        let empty_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],  // Tout à 0 = pas de code
        };
        let block = [0i16; 64]; // DC diff = 0, category 0
        let mut writer = BitWriter::with_capacity(64);
        let mut prev_dc = 1i16; // Force diff != 0 -> category 1 -> missing

        let result = encode_block(
            &block, &empty_table, &empty_table, &mut prev_dc, &mut writer,
        );
        assert!(result.is_err());
    }

    #[test]
    fn encode_ac_eob_missing_code_returns_error()
    {
        // Table avec code pour DC category 0 mais pas pour AC EOB (0x00)
        let mut dc_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],
        };
        dc_table.ehufsi[0] = 1; // Category 0 has a code
        dc_table.ehufco[0] = 0;

        let ac_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256], // No AC codes at all
        };

        let block = [0i16; 64]; // DC=0 (diff=0, cat 0), all AC=0 -> needs EOB
        let mut writer = BitWriter::with_capacity(64);
        let mut prev_dc = 0i16;

        let result = encode_block(
            &block, &dc_table, &ac_table, &mut prev_dc, &mut writer,
        );
        assert!(result.is_err());
    }

    #[test]
    fn encode_ac_rs_missing_code_returns_error()
    {
        let mut dc_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],
        };
        dc_table.ehufsi[0] = 1;
        dc_table.ehufco[0] = 0;

        let mut ac_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],
        };
        // Give EOB a code but NOT the RS=0x01 (run=0, cat=1)
        ac_table.ehufsi[0x00] = 1;
        ac_table.ehufco[0x00] = 0;

        let mut block = [0i16; 64];
        block[1] = 1; // AC coeff: run=0, value=1, cat=1 -> RS=0x01 -> missing

        let mut writer = BitWriter::with_capacity(64);
        let mut prev_dc = 0i16;

        let result = encode_block(
            &block, &dc_table, &ac_table, &mut prev_dc, &mut writer,
        );
        assert!(result.is_err());
    }

    #[test]
    fn encode_ac_zrl_missing_code_returns_error()
    {
        let mut dc_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],
        };
        dc_table.ehufsi[0] = 1;
        dc_table.ehufco[0] = 0;

        let ac_table = HuffmanTable
        {
            bits: [0u8; 16],
            values: vec![],
            ehufco: [0u32; 256],
            ehufsi: [0u8; 256],
        };
        // No ZRL code (0xF0)

        // 17 zeros then a non-zero -> needs ZRL
        let mut block = [0i16; 64];
        block[17] = 5;

        let mut writer = BitWriter::with_capacity(64);
        let mut prev_dc = 0i16;

        let result = encode_block(
            &block, &dc_table, &ac_table, &mut prev_dc, &mut writer,
        );
        assert!(result.is_err());
    }
}
