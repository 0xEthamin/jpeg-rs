//! # Quantize
//!
//! DCT coefficient quantization and zig-zag reordering.
//!
//! The DCT itself is mathematically lossless (up to rounding). The actual
//! information loss in JPEG occurs here, in quantization.
//!
//! Each of the 64 DCT coefficients S is divided by a corresponding
//! *quantization step size* Q and rounded to the nearest integer
//! (T.81 §A.3.4):
//!
//! ```text
//!   Sq = round(S / Q)
//! ```
//!
//! A large step size means that many different coefficient values map to the
//! same quantized value -> more information is lost -> smaller file. A small
//! step size preserves more detail -> larger file.
//!
//! The step sizes are collected in a *quantization table*. An 8*8 matrix
//! where each entry controls how aggressively the corresponding frequency
//! is compressed. High-frequency entries (bottom-right) are typically larger
//! than low-frequency entries (top-left), because the human visual system is
//! less sensitive to fine detail.
//!
//! # Zig-zag ordering (T.81 §A.3.6, Figure A.6)
//!
//! After quantization, the 64 coefficients are reordered from the 2-D 8*8
//! matrix into a 1-D sequence that roughly goes from low frequency to high
//! frequency. This "zig-zag" traversal visits the coefficients in this
//! order:
//!
//! ```text
//!    00 01 05 06 14 15 27 28
//!    02 04 07 13 16 26 29 42
//!    03 08 12 17 25 30 41 43
//!    09 11 18 24 31 40 44 53
//!    10 19 23 32 39 45 52 54
//!    20 22 33 38 46 51 55 60
//!    21 34 37 47 50 56 59 61
//!    35 36 48 49 57 58 62 63
//! ```
//!
//! The purpose of this reordering is to place the runs of zeros (from
//! quantized-to-zero high-frequency coefficients) at the end of the
//! sequence, where the entropy coder can encode them very efficiently
//! using End-Of-Block (EOB) symbols.
//!
//! # Standard quantization tables (T.81 Annex K, Tables K.1 & K.2)
//!
//! The standard provides example quantization tables for luminance and
//! chrominance components. These were "derived empirically using luminance
//! and chrominance and 2:1 horizontal subsampling" (T.81 §K.1) and work
//! well for typical photographic images at 8-bit precision.
//!
//! **Important**: these are *examples*, not mandatory defaults. The standard
//! explicitly states "these tables are provided as examples only and are not
//! necessarily suitable for any particular application." However, virtually
//! every JPEG encoder in existence uses them as the basis for quality
//! scaling.

use crate::block::Block8x8;

/// Zig-zag scan order mapping: `ZIG_ZAG[k]` gives the natural-order index
/// (row * 8 + col) of the k-th coefficient in zig-zag sequence.
///
/// This table is the inverse of the mapping shown in T.81 Figure A.6.
///
/// Usage: to read the DCT coefficients in zig-zag order, access
/// `dct_block[ZIG_ZAG[k]]` for k = 0, 1, …, 63.
///
/// - k = 0 -> ZIG_ZAG\[0\] = 0 -> coefficient (0,0) = DC
/// - k = 1 -> ZIG_ZAG\[1\] = 1 -> coefficient (0,1)
/// - k = 2 -> ZIG_ZAG\[2\] = 8 -> coefficient (1,0)
/// - …
/// - k = 63 -> ZIG_ZAG\[63\] = 63 -> coefficient (7,7)
pub const ZIG_ZAG: [u8; 64] = 
[
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Luminance quantization table (T.81 Table K.1).
///
/// Values are in natural (row-major) order.
pub const STD_LUMINANCE_QUANT: [u16; 64] = 
[
    16, 11, 10, 16, 24, 40, 51, 61,
    12, 12, 14, 19, 26, 58, 60, 55,
    14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62,
    18, 22, 37, 56, 68, 109, 103, 77,
    24, 35, 55, 64, 81, 104, 113, 92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103, 99,
];

/// Chrominance quantization table (T.81 Table K.2).
///
/// Values are in natural (row-major) order.
pub const STD_CHROMINANCE_QUANT: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

/// A scaled quantization table ready for use during encoding.
///
/// Internally, the table stores values in *two* orderings:
///
/// - `zigzag_values`: values reordered according to [`ZIG_ZAG`], used when
///   writing the DQT marker segment to the JPEG file (T.81 §B.2.4.1
///   specifies that quantization table entries are stored in zig-zag order).
///
/// - `natural_values`: values in natural (row-major) order, used during the
///   actual quantization of DCT coefficients.
#[derive(Debug, Clone)]
pub struct QuantTable
{
    /// Quantization values in zig-zag order - written to the DQT marker.
    pub zigzag_values: [u16; 64],

    /// Quantization values in natural (row-major) order - used for
    /// dividing DCT coefficients.
    natural_values: [u16; 64],
}

/// Scale a base quantization table by the IJG quality formula.
///
/// The quality factor `q` maps to a scale factor:
///
/// ```text
///   if q < 50: scale = 5000 / q
///   if q >= 50: scale = 200 - 2q
/// ```
///
/// Each table entry is then:
///
/// ```text
///   table[i] = clamp((base[i] * scale + 50) / 100, 1, 255)
/// ```
///
/// A quality of 50 corresponds to the unscaled base table. Quality 100
/// gives all-ones (finest quantization, largest file). Quality 1 gives
/// the coarsest quantization, smallest file.
///
/// The result is clamped to [1, 255] because:
/// - A step of 0 would cause division by zero during quantization.
/// - The DQT marker with Pq = 0 (8-bit precision) stores each value as a
///   single byte, so the maximum is 255. (16-bit tables use Pq = 1, but
///   this encoder only supports 8-bit quantization tables.)
pub fn scale_quant_table(base: &[u16; 64], quality: u8) -> [u16; 64]
{
    let q = quality.max(1) as u32;
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };

    let mut table = [0u16; 64];
    for i in 0..64
    {
        let val = (base[i] as u32 * scale + 50) / 100;
        table[i] = val.clamp(1, 255) as u16;
    }
    table
}

impl QuantTable
{
    /// Create a quantization table from values in natural (row-major) order.
    ///
    /// The constructor also pre-computes the zig-zag-ordered version for
    /// writing to the JPEG bitstream.
    #[must_use]
    pub fn new(natural_order: &[u16; 64]) -> Self
    {
        let mut zigzag = [0u16; 64];
        for k in 0..64
        {
            zigzag[k] = natural_order[ZIG_ZAG[k] as usize];
        }

        Self
        {
            zigzag_values: zigzag,
            natural_values: *natural_order,
        }
    }

    /// Create a quantization table by scaling a standard base table.
    ///
    /// This is the primary constructor for production use:
    ///
    /// ```
    /// use jpeg_core::quantize::{QuantTable, STD_LUMINANCE_QUANT};
    ///
    /// let qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, 85);
    /// ```
    #[must_use]
    pub fn from_standard(base: &[u16; 64], quality: u8) -> Self
    {
        let scaled = scale_quant_table(base, quality);
        Self::new(&scaled)
    }

    /// Access the quantization values in natural (row-major) order.
    #[inline]
    #[must_use]
    pub fn natural_values(&self) -> &[u16; 64]
    {
        &self.natural_values
    }
}

/// Quantize a DCT block and reorder the coefficients into zig-zag sequence.
///
/// # Algorithm
///
/// For each zig-zag position k (0..64):
///
/// 1. Look up the natural-order index of the k-th zig-zag coefficient.
/// 2. Divide the DCT coefficient at that index by the corresponding
///    quantization step size.
/// 3. Round to the nearest integer (T.81 §A.3.4: "Rounding is to the
///    nearest integer").
///
/// # Output
///
/// A 64-element array of quantized coefficients in **zig-zag order**.
/// Element \[0\] is the quantized DC coefficient. Elements \[1..63\] are the
/// quantized AC coefficients in the order they will be entropy-coded.
///
/// This output can be passed directly to the Huffman encoder.
pub fn quantize_block(dct_block: &Block8x8, table: &QuantTable) -> [i16; 64]
{
    let mut quantized = [0i16; 64];

    for k in 0..64
    {
        let natural_idx = ZIG_ZAG[k] as usize;
        let coeff = dct_block[natural_idx] as f32;
        let q = table.natural_values[natural_idx] as f32;

        // Round to nearest integer
        quantized[k] = if coeff >= 0.0
        {
            ((coeff / q) + 0.5) as i16
        }
        else
        {
            ((coeff / q) - 0.5) as i16
        };
    }

    quantized
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn scale_quality_50_is_identity()
    {
        // Quality 50 -> scale = 200 - 100 = 100 -> table unchanged.
        let scaled = scale_quant_table(&STD_LUMINANCE_QUANT, 50);
        assert_eq!(scaled, STD_LUMINANCE_QUANT);
    }

    #[test]
    fn scale_quality_100_gives_all_ones()
    {
        // Quality 100 -> scale = 200 - 200 = 0 -> all values clamp to 1.
        let scaled = scale_quant_table(&STD_LUMINANCE_QUANT, 100);
        for &v in &scaled
        {
            assert_eq!(v, 1, "quality 100 should yield all-1 table");
        }
    }

    #[test]
    fn scale_quality_1_gives_maximum_steps()
    {
        // Quality 1 -> scale = 5000 -> very large steps, clamped to 255.
        let scaled = scale_quant_table(&STD_LUMINANCE_QUANT, 1);
        for &v in &scaled
        {
            assert!((1..=255).contains(&v));
        }
        // The smallest base entry (10) * 50 = 500 -> clamped to 255.
        assert_eq!(scaled[2], 255); // base = 10
    }

    #[test]
    fn scale_never_zero()
    {
        // Verify no division-by-zero risk: all entries ≥ 1.
        for q in 1..=100
        {
            let scaled = scale_quant_table(&STD_LUMINANCE_QUANT, q);
            for (i, &v) in scaled.iter().enumerate()
            {
                assert!(v >= 1, "quality={}, index={}, value={}", q, i, v);
            }
        }
    }

    #[test]
    fn quant_table_zigzag_reorder()
    {
        // Verify that QuantTable correctly reorders natural -> zig-zag.
        let mut natural = [0u16; 64];
        for (i, val) in natural.iter_mut().enumerate() { *val = i as u16 + 1; }

        let qt = QuantTable::new(&natural);

        // zigzag_values[k] should equal natural[ZIG_ZAG[k]]
        for k in 0..64
        {
            let expected = natural[ZIG_ZAG[k] as usize];
            assert_eq!(qt.zigzag_values[k], expected, "zig-zag position {}", k);
        }
    }

    #[test]
    fn quantize_dc_coefficient()
    {
        // Create a block with only a DC component.
        let mut dct_block: Block8x8 = [0i16; 64];
        dct_block[0] = 100; // DC in natural order

        let mut natural = [1u16; 64];
        natural[0] = 10; // Q for DC
        let qt = QuantTable::new(&natural);

        let quantized = quantize_block(&dct_block, &qt);
        // DC = round(100 / 10) = 10. DC is at zig-zag position 0.
        assert_eq!(quantized[0], 10);
    }

    #[test]
    fn quantize_rounds_correctly()
    {
        let mut dct_block: Block8x8 = [0i16; 64];
        dct_block[0] = 7; // DC

        let mut natural = [1u16; 64];
        natural[0] = 5;
        let qt = QuantTable::new(&natural);

        let quantized = quantize_block(&dct_block, &qt);
        // round(7/5) = round(1.4) = 1
        assert_eq!(quantized[0], 1);
    }

    #[test]
    fn quantize_negative_values()
    {
        let mut dct_block: Block8x8 = [0i16; 64];
        dct_block[0] = -7;

        let mut natural = [1u16; 64];
        natural[0] = 5;
        let qt = QuantTable::new(&natural);

        let quantized = quantize_block(&dct_block, &qt);
        // round(-7/5) = round(-1.4) = -1
        assert_eq!(quantized[0], -1);
    }

    #[test]
    fn quantize_zeros_out_small_ac()
    {
        // Small AC coefficient quantized to zero.
        let mut dct_block: Block8x8 = [0i16; 64];
        dct_block[1] = 3; // small AC at natural position (0,1)

        let mut natural = [1u16; 64];
        natural[1] = 10; // Q = 10 for that position
        let qt = QuantTable::new(&natural);

        let quantized = quantize_block(&dct_block, &qt);
        // round(3/10) = round(0.3) = 0
        // Find which zig-zag position corresponds to natural index 1
        let zz_pos = ZIG_ZAG.iter().position(|&z| z == 1).unwrap();
        assert_eq!(quantized[zz_pos], 0);
    }

    #[test]
    fn zigzag_table_is_bijection()
    {
        // Every natural index 0..63 appears exactly once in ZIG_ZAG.
        let mut seen = [false; 64];
        for &idx in &ZIG_ZAG
        {
            assert!(!seen[idx as usize], "duplicate index {}", idx);
            seen[idx as usize] = true;
        }
        for (i, &s) in seen.iter().enumerate()
        {
            assert!(s, "missing index {}", i);
        }
    }

    #[test]
    fn zigzag_dc_is_first()
    {
        // The DC coefficient (natural index 0) must be at zig-zag position 0.
        assert_eq!(ZIG_ZAG[0], 0);
    }

    #[test]
    fn natural_values_accessor()
    {
        let qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, 50);
        let natural = qt.natural_values();
        // Quality 50 = identity, so natural values == standard table
        assert_eq!(*natural, STD_LUMINANCE_QUANT);
    }
}
