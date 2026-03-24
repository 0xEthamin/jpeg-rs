//! # block.rs
//!
//! 8*8 block extraction from component planes.
//!
//! JPEG does not compress an image as a whole. Instead, each component plane
//! (Y, Cb, or Cr) is partitioned into non-overlapping 8*8 blocks of samples.
//! Each block is then independently transformed, quantized, and entropy-coded.
//!
//! # Handling non-multiple-of-8 dimensions
//!
//! If the image width or height is not a multiple of 8, the standard requires
//! that the encoding process extend the image to complete the rightmost /
//! bottommost blocks (T.81 §A.2.4):
//!
//! The recommended strategy (ibid.) is to replicate the rightmost column and
//! the bottommost row. This is what `extract_blocks` does: when a sample
//! coordinate falls outside the image, it is clamped to the nearest valid
//! coordinate.

/// An 8*8 block of signed 16-bit coefficients.
///
/// The 64 elements are stored in *natural* (row-major) order:
///
/// ```text
///   index = row * 8 + column
///
///     col 0 col 1 … col 7
///   ┌──────┬──────┬───┬──────┐
///   | [0]  | [1]  | … | [7]  | row 0
///   | [8]  | [9]  | … | [15] | row 1
///   | …    | …    | … | …    |
///   | [56] | [57] | … | [63] | row 7
///   └──────┴──────┴───┴──────┘
/// ```
pub type Block8x8 = [i16; 64];

/// Extract all 8*8 blocks from a single-component plane.
///
/// # Arguments
///
/// * `plane` - sample data in row-major order, one byte per sample.
/// * `width`, `height` - the dimensions of the plane in samples.
///
/// # Returns
///
/// A `Vec` of [`Block8x8`] values in raster order (left-to-right, then
/// top-to-bottom), with the level shift already applied (each sample
/// has 128 subtracted, per T.81 §A.3.1).
pub fn extract_blocks
(
    plane: &[u8],
    width: u32,
    height: u32,
) -> Vec<Block8x8>
{
    let blocks_h = width.div_ceil(8) as usize;
    let blocks_v = height.div_ceil(8) as usize;

    let mut blocks = Vec::with_capacity(blocks_h * blocks_v);

    for by in 0..blocks_v
    {
        for bx in 0..blocks_h
        {
            let mut block: Block8x8 = [0i16; 64];

            for row in 0..8u32
            {
                for col in 0..8u32
                {
                    // Clamp to image bounds
                    let sy = (by as u32 * 8 + row).min(height - 1);
                    let sx = (bx as u32 * 8 + col).min(width - 1);

                    let sample = plane[(sy * width + sx) as usize];

                    // Level shift: unsigned [0, 255] -> signed [ -128, 127].
                    block[(row * 8 + col) as usize] = sample as i16 - 128;
                }
            }

            blocks.push(block);
        }
    }

    blocks
}

/// Number of 8*8 blocks that span `width` samples horizontally.
#[inline]
#[must_use]
pub const fn blocks_wide(width: u32) -> u32
{
    width.div_ceil(8)
}

/// Number of 8*8 blocks that span `height` samples vertically.
#[inline]
#[must_use]
pub const fn blocks_high(height: u32) -> u32
{
    height.div_ceil(8)
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn blocks_wide_exact_multiple()
    {
        assert_eq!(blocks_wide(16), 2);
        assert_eq!(blocks_wide(8), 1);
        assert_eq!(blocks_wide(64), 8);
    }

    #[test]
    fn blocks_wide_non_multiple()
    {
        assert_eq!(blocks_wide(1), 1);
        assert_eq!(blocks_wide(9), 2);
        assert_eq!(blocks_wide(15), 2);
        assert_eq!(blocks_wide(17), 3);
    }

    #[test]
    fn blocks_high_values()
    {
        assert_eq!(blocks_high(8), 1);
        assert_eq!(blocks_high(7), 1);
        assert_eq!(blocks_high(9), 2);
    }

    #[test]
    fn extract_single_block_level_shift()
    {
        let plane = vec![200u8; 64];
        let blocks = extract_blocks(&plane, 8, 8);
        assert_eq!(blocks.len(), 1);
        for &val in &blocks[0]
        {
            assert_eq!(val, 72);
        }
    }

    #[test]
    fn extract_level_shift_zero()
    {
        let plane = vec![128u8; 64];
        let blocks = extract_blocks(&plane, 8, 8);
        assert_eq!(blocks.len(), 1);
        for &val in &blocks[0]
        {
            assert_eq!(val, 0);
        }
    }

    #[test]
    fn extract_level_shift_range()
    {
        let plane = vec![0u8; 64];
        let blocks = extract_blocks(&plane, 8, 8);
        assert_eq!(blocks[0][0], -128);

        let plane = vec![255u8; 64];
        let blocks = extract_blocks(&plane, 8, 8);
        assert_eq!(blocks[0][0], 127);
    }

    #[test]
    fn extract_multiple_blocks()
    {
        let plane = vec![100u8; 16 * 8];
        let blocks = extract_blocks(&plane, 16, 8);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn extract_non_multiple_of_8_pads_by_replication()
    {
        let mut plane = vec![0u8; 25];
        plane[4 * 5 + 4] = 200;

        let blocks = extract_blocks(&plane, 5, 5);
        assert_eq!(blocks.len(), 1);

        let shifted_200 = 200i16 - 128;
        let shifted_0   = 0i16 - 128;

        // (row=4, col=4) = the original corner
        assert_eq!(blocks[0][4 * 8 + 4], shifted_200);
        // (row=4, col=5) = replicated from col 4
        assert_eq!(blocks[0][4 * 8 + 5], shifted_200);
        // (row=5, col=4) = replicated from row 4
        assert_eq!(blocks[0][5 * 8 + 4], shifted_200);
        // (row=7, col=7) = replicated corner
        assert_eq!(blocks[0][7 * 8 + 7], shifted_200);
        // (row=0, col=0) = original top-left = 0
        assert_eq!(blocks[0][0], shifted_0);
    }

    #[test]
    fn extract_block_count_non_multiple()
    {
        let plane = vec![128u8; 13 * 11];
        let blocks = extract_blocks(&plane, 13, 11);
        assert_eq!(blocks.len(), 4);
    }
}
