//! # DCT (Discrete Cosine Transform)
//!
//! Forward Discrete Cosine Transform (FDCT).
//!
//! # What the DCT does and why JPEG uses it
//!
//! The DCT converts an 8*8 block of spatial-domain samples (pixel brightness
//! values) into an 8*8 block of frequency-domain *coefficients*. Each
//! coefficient represents the amplitude of a specific 2-D cosine basis
//! function. Intuitively, "how much of this particular spatial frequency
//! pattern is present in this block".
//!
//! The top-left coefficient (index [0,0]) is called the **DC coefficient**.
//! It represents the average value of all 64 samples in the block. The
//! zero-frequency component. The remaining 63 coefficients are called
//! **AC coefficients** and represent progressively higher spatial
//! frequencies.
//!
//! The key property that makes the DCT useful for compression: for typical
//! photographic images, most of the signal energy is concentrated in the
//! low-frequency coefficients (the top-left corner of the 8*8 matrix). The
//! high-frequency coefficients (bottom-right) are often close to zero.
//! After quantization (see [`crate::quantize`]), many of these small AC
//! coefficients become zero, which the entropy coder can represent
//! very compactly.
//!
//! # Mathematical definition (T.81 §A.3.3)
//!
//! The ideal 2-D FDCT of an 8x8 block s_xy is:
//!
//! ```text
//!                          7   7 
//!   S_vu =  1/4 * C_u C_v sum sum s_xy * cos((2*x+1)*u*pi/16) * cos((2*y+1)*v*pi/16)
//!                         x=0 y=0
//! ```
//!
//! where C_k = 1/sqrt(2) for k = 0, and C_k = 1 otherwise.
//!
//! # Implementation: AAN (Arai, Agui, Nakajima) fast integer DCT
//!
//! Computing the naive 2-D DCT requires 64 * 64 = 4096 multiplications.
//! The AAN algorithm reduces this dramatically by:
//!
//! 1. **Separability** : the 2-D DCT is computed as two passes of 1-D DCTs:
//!    first on each row, then on each column. This reduces complexity from
//!    O(N⁴) to O(N³) = 512 multiplications for N = 8.
//!
//! 2. **Butterfly decomposition** : each 1-D 8-point DCT is decomposed into
//!    even/odd stages (similar to the FFT), using only 5 multiplications
//!    and 29 additions per 1-D transform.
//!
//! 3. **Fixed-point arithmetic** : all cosine constants are pre-scaled by
//!    2^13 (`CONST_BITS`) to avoid floating-point operations entirely.
//!    The intermediate row results are further scaled by 2^2 (`PASS1_BITS`)
//!    to preserve precision between the two passes.

use crate::block::Block8x8;

/// Number of fractional bits in the cosine constants.
const CONST_BITS: i32 = 13;

/// Extra precision bits carried between the row pass and the column pass.
/// The row pass output is scaled up by 2^PASS1_BITS. The column pass
/// descales by PASS1_BITS + 3 (the +3 accounts for the 1/8 normalisation
/// factor of the DCT, since 1/8 = 2^(-3)).
const PASS1_BITS: i32 = 2;

// Cosine constants scaled by 2^13.
// Each name `FIX_x_xxxxxxxxx` encodes the decimal value of the constant.
const FIX_0_298631336: i64 = 2446;
const FIX_0_390180644: i64 = 3196;
const FIX_0_541196100: i64 = 4433;
const FIX_0_765366865: i64 = 6270;
const FIX_0_899976223: i64 = 7373;
const FIX_1_175875602: i64 = 9633;
const FIX_1_501321110: i64 = 12299;
const FIX_1_847759065: i64 = 15137;
const FIX_1_961570560: i64 = 16069;
const FIX_2_053119869: i64 = 16819;
const FIX_2_562915447: i64 = 20995;
const FIX_3_072711026: i64 = 25172;

/// Descale a fixed-point value by `n` bits with rounding.
///
/// Equivalent to `(x + 2^(n-1)) >> n`, which rounds to nearest integer.
#[inline(always)]
fn descale(x: i64, n: i32) -> i32
{
    ((x + (1i64 << (n - 1))) >> n) as i32
}

/// Compute the Forward DCT of an 8*8 block.
///
/// # Input
///
/// A [`Block8x8`] of level-shifted samples (i.e. pixel values with 128
/// subtracted, as produced by [`crate::block::extract_blocks`]).
///
/// # Output
///
/// A [`Block8x8`] of DCT coefficients in natural (row-major) order.
/// The DC coefficient is at index 0. These coefficients will later be
/// reordered into zig-zag sequence during quantization (see
/// [`crate::quantize`]).
///
/// # Precision
///
/// The fast integer algorithm introduces rounding errors of at most +-1
/// per coefficient compared to the ideal floating-point DCT. This is
/// well within the accuracy requirements of T.81 Part 2.
pub fn fdct(block: &Block8x8) -> Block8x8
{
    let mut workspace = [0i32; 64];

    
    // Pass 1: transform each row.
    //
    // Input: 8 spatial-domain samples (i16, range [-128, 127]).
    // Output: 8 frequency-domain coefficients, scaled up by 2^PASS1_BITS.
    //
    // The even/odd decomposition splits the 8 inputs into:
    //   tmp0..tmp3 = s[0]+s[7], s[1]+s[6], s[2]+s[5], s[3]+s[4] (even)
    //   tmp4..tmp7 = s[3]-s[4], s[2]-s[5], s[1]-s[6], s[0]-s[7] (odd)
    //
    // The even part produces coefficients 0, 2, 4, 6.
    // The odd part produces coefficients 1, 3, 5, 7.
    for row in 0..8
    {
        let b = row * 8;
        let (d0, d1, d2, d3) = 
        (
            block[b] as i64, block[b + 1] as i64,
            block[b + 2] as i64, block[b + 3] as i64,
        );
        let (d4, d5, d6, d7) = 
        (
            block[b + 4] as i64, block[b + 5] as i64,
            block[b + 6] as i64, block[b + 7] as i64,
        );

        let tmp0 = d0 + d7;
        let tmp7 = d0 - d7;
        let tmp1 = d1 + d6;
        let tmp6 = d1 - d6;
        let tmp2 = d2 + d5;
        let tmp5 = d2 - d5;
        let tmp3 = d3 + d4;
        let tmp4 = d3 - d4;

        // Even part
        let tmp10 = tmp0 + tmp3;
        let tmp12 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp13 = tmp1 - tmp2;

        workspace[b]     = ((tmp10 + tmp11) << PASS1_BITS) as i32;
        workspace[b + 4] = ((tmp10 - tmp11) << PASS1_BITS) as i32;

        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        workspace[b + 2] = descale(z1 + tmp12 * FIX_0_765366865, CONST_BITS - PASS1_BITS);
        workspace[b + 6] = descale(z1 - tmp13 * FIX_1_847759065, CONST_BITS - PASS1_BITS);

        // Odd part
        let z1 = tmp4 + tmp7;
        let z2 = tmp5 + tmp6;
        let z3 = tmp4 + tmp6;
        let z4 = tmp5 + tmp7;
        let z5 = (z3 + z4) * FIX_1_175875602;

        let tmp4 = tmp4 * FIX_0_298631336;
        let tmp5 = tmp5 * FIX_2_053119869;
        let tmp6 = tmp6 * FIX_3_072711026;
        let tmp7 = tmp7 * FIX_1_501321110;
        let z1 = z1 * -FIX_0_899976223;
        let z2 = z2 * -FIX_2_562915447;
        let z3 = z3 * -FIX_1_961570560 + z5;
        let z4 = z4 * -FIX_0_390180644 + z5;

        workspace[b + 7] = descale(tmp4 + z1 + z3, CONST_BITS - PASS1_BITS);
        workspace[b + 5] = descale(tmp5 + z2 + z4, CONST_BITS - PASS1_BITS);
        workspace[b + 3] = descale(tmp6 + z2 + z3, CONST_BITS - PASS1_BITS);
        workspace[b + 1] = descale(tmp7 + z1 + z4, CONST_BITS - PASS1_BITS);
    }

    // Pass 2: transform each column.
    //
    // Input: workspace values scaled by 2^PASS1_BITS from pass 1.
    // Output: final DCT coefficients, descaled by 2^(PASS1_BITS + 3).
    //
    // The +3 is log2(8) and accounts for the 1/N normalisation factor of
    // the DCT (N = 8 for the 1-D case. The 2-D factor is 1/64 = 1/8 * 1/8,
    // applied once in each pass as 1/8 = 2^(-3)).
    let mut result: Block8x8 = [0i16; 64];

    for col in 0..8
    {
        let d0 = workspace[col] as i64;
        let d1 = workspace[col + 8] as i64;
        let d2 = workspace[col + 16] as i64;
        let d3 = workspace[col + 24] as i64;
        let d4 = workspace[col + 32] as i64;
        let d5 = workspace[col + 40] as i64;
        let d6 = workspace[col + 48] as i64;
        let d7 = workspace[col + 56] as i64;

        let tmp0 = d0 + d7;
        let tmp7 = d0 - d7;
        let tmp1 = d1 + d6;
        let tmp6 = d1 - d6;
        let tmp2 = d2 + d5;
        let tmp5 = d2 - d5;
        let tmp3 = d3 + d4;
        let tmp4 = d3 - d4;

        let tmp10 = tmp0 + tmp3;
        let tmp12 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp13 = tmp1 - tmp2;

        let pass2_descale = PASS1_BITS + 3;

        result[col]      = descale(tmp10 + tmp11, pass2_descale) as i16;
        result[col + 32] = descale(tmp10 - tmp11, pass2_descale) as i16;

        let col_shift = CONST_BITS + PASS1_BITS + 3;

        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        result[col + 16] = descale(z1 + tmp12 * FIX_0_765366865, col_shift) as i16;
        result[col + 48] = descale(z1 - tmp13 * FIX_1_847759065, col_shift) as i16;

        let z1 = tmp4 + tmp7;
        let z2 = tmp5 + tmp6;
        let z3 = tmp4 + tmp6;
        let z4 = tmp5 + tmp7;
        let z5 = (z3 + z4) * FIX_1_175875602;

        let tmp4 = tmp4 * FIX_0_298631336;
        let tmp5 = tmp5 * FIX_2_053119869;
        let tmp6 = tmp6 * FIX_3_072711026;
        let tmp7 = tmp7 * FIX_1_501321110;
        let z1 = z1 * -FIX_0_899976223;
        let z2 = z2 * -FIX_2_562915447;
        let z3 = z3 * -FIX_1_961570560 + z5;
        let z4 = z4 * -FIX_0_390180644 + z5;

        result[col + 56] = descale(tmp4 + z1 + z3, col_shift) as i16;
        result[col + 40] = descale(tmp5 + z2 + z4, col_shift) as i16;
        result[col + 24] = descale(tmp6 + z2 + z3, col_shift) as i16;
        result[col +  8] = descale(tmp7 + z1 + z4, col_shift) as i16;
    }

    result
}


/// Naive floating-point FDCT, used only in tests to verify the fast integer
/// implementation.
///
/// This directly implements the mathematical definition from T.81 §A.3.3.
/// It is accurate but far too slow for production use (~4096
/// multiplications per block vs. ~80 for the AAN algorithm).
#[cfg(test)]
pub fn fdct_reference(block: &Block8x8) -> Block8x8
{
    use std::f64::consts::PI;

    // Pre-compute the cosine lookup table:
    // cos_table[k][n] = cos((2k + 1) * n * π / 16)
    let cos_table = 
    {
        let mut table = [[0.0f64; 8]; 8];
        for k in 0..8
        {
            for n in 0..8
            {
                table[k][n] = ((2 * k + 1) as f64 * n as f64 * PI / 16.0).cos();
            }
        }
        table
    };

    // C(n) = 1/sqrt(2) for n = 0, 1.0 otherwise.
    #[inline]
    fn c(n: usize) -> f64
    {
        if n == 0 { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 }
    }

    let mut result: Block8x8 = [0i16; 64];
    for v in 0..8
    {
        for u in 0..8
        {
            let mut sum = 0.0f64;
            for y in 0..8
            {
                for x in 0..8
                {
                    sum += block[y * 8 + x] as f64
                        * cos_table[x][u]
                        * cos_table[y][v];
                }
            }
            result[v * 8 + u] = (0.25 * c(u) * c(v) * sum).round() as i16;
        }
    }
    result
}

#[cfg(test)]
mod tests
{
    use super::*;

    /// Verify that the fast integer DCT matches the reference floating-point
    /// DCT within +-1 per coefficient.
    #[test]
    fn fast_dct_matches_reference()
    {
        // Test vector: a block with varied values that exercises all
        // frequency components.
        let block: Block8x8 = 
        [
            -76, -73, -67, -62, -58, -67, -64, -55,
            -65, -69, -73, -38, -19, -43, -59, -56,
            -66, -69, -60, -15, 16, -24, -62, -55,
            -65, -70, -57, -6, 26, -22, -58, -59,
            -61, -67, -60, -24, -2, -40, -60, -58,
            -49, -63, -68, -58, -51, -60, -70, -53,
            -43, -57, -64, -69, -73, -67, -63, -45,
            -41, -49, -59, -60, -63, -52, -50, -34,
        ];

        let ref_result = fdct_reference(&block);
        let fast_result = fdct(&block);

        for i in 0..64
        {
            let diff = (ref_result[i] as i32 - fast_result[i] as i32).abs();
            assert!
            (
                diff <= 1,
                "Mismatch at [{}] (row={}, col={}): ref={}, fast={}, diff={}",
                i, i / 8, i % 8, ref_result[i], fast_result[i], diff,
            );
        }
    }

    /// A flat block (all samples identical) should have a large DC coefficient
    /// and all AC coefficients close to zero.
    #[test]
    fn dct_of_flat_block()
    {
        let block: Block8x8 = [42; 64];
        let result = fdct(&block);

        assert!
        (
            (result[0] as i32 - 336).abs() <= 1,
            "DC: expected ~336, got {}",
            result[0],
        );

        for i in 1..64
        {
            assert!
            (
                result[i].abs() <= 1,
                "AC[{}] should be ~0, got {}",
                i, result[i],
            );
        }
    }
}
