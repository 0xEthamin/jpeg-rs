use crate::block::Block8x8;

const CONST_BITS: i32 = 13;
const PASS1_BITS: i32 = 2;

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

#[inline(always)]
fn descale(x: i64, n: i32) -> i32
{
    ((x + (1i64 << (n - 1))) >> n) as i32
}

pub fn fdct(block: &Block8x8) -> Block8x8
{
    let mut ws = [0i32; 64];

    // Pass 1: rows. Output scaled up by PASS1_BITS.
    for row in 0..8
    {
        let b = row * 8;
        let (d0, d1, d2, d3) = (block[b] as i64, block[b+1] as i64,
                                 block[b+2] as i64, block[b+3] as i64);
        let (d4, d5, d6, d7) = (block[b+4] as i64, block[b+5] as i64,
                                 block[b+6] as i64, block[b+7] as i64);

        let tmp0 = d0 + d7;  let tmp7 = d0 - d7;
        let tmp1 = d1 + d6;  let tmp6 = d1 - d6;
        let tmp2 = d2 + d5;  let tmp5 = d2 - d5;
        let tmp3 = d3 + d4;  let tmp4 = d3 - d4;

        // Even
        let tmp10 = tmp0 + tmp3;
        let tmp12 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp13 = tmp1 - tmp2;

        ws[b+0] = ((tmp10 + tmp11) << PASS1_BITS) as i32;
        ws[b+4] = ((tmp10 - tmp11) << PASS1_BITS) as i32;

        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        ws[b+2] = descale(z1 + tmp12 * FIX_0_765366865, CONST_BITS - PASS1_BITS);
        ws[b+6] = descale(z1 - tmp13 * FIX_1_847759065, CONST_BITS - PASS1_BITS);

        // Odd
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

        ws[b+7] = descale(tmp4 + z1 + z3, CONST_BITS - PASS1_BITS);
        ws[b+5] = descale(tmp5 + z2 + z4, CONST_BITS - PASS1_BITS);
        ws[b+3] = descale(tmp6 + z2 + z3, CONST_BITS - PASS1_BITS);
        ws[b+1] = descale(tmp7 + z1 + z4, CONST_BITS - PASS1_BITS);
    }

    // Pass 2: columns. Input scaled by PASS1_BITS; output descaled by
    // PASS1_BITS + 3 (the +3 = log2(8) is the 1/8 normalization).
    let mut result: Block8x8 = [0i16; 64];

    for col in 0..8
    {
        let d0 = ws[col + 0*8] as i64;
        let d1 = ws[col + 1*8] as i64;
        let d2 = ws[col + 2*8] as i64;
        let d3 = ws[col + 3*8] as i64;
        let d4 = ws[col + 4*8] as i64;
        let d5 = ws[col + 5*8] as i64;
        let d6 = ws[col + 6*8] as i64;
        let d7 = ws[col + 7*8] as i64;

        let tmp0 = d0 + d7;  let tmp7 = d0 - d7;
        let tmp1 = d1 + d6;  let tmp6 = d1 - d6;
        let tmp2 = d2 + d5;  let tmp5 = d2 - d5;
        let tmp3 = d3 + d4;  let tmp4 = d3 - d4;

        let tmp10 = tmp0 + tmp3;
        let tmp12 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp13 = tmp1 - tmp2;

        let pass2_descale = PASS1_BITS + 3;

        result[col + 0*8] = descale(tmp10 + tmp11, pass2_descale) as i16;
        result[col + 4*8] = descale(tmp10 - tmp11, pass2_descale) as i16;

        let col_shift = CONST_BITS + PASS1_BITS + 3;

        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        result[col + 2*8] = descale(z1 + tmp12 * FIX_0_765366865, col_shift) as i16;
        result[col + 6*8] = descale(z1 - tmp13 * FIX_1_847759065, col_shift) as i16;

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

        result[col + 7*8] = descale(tmp4 + z1 + z3, col_shift) as i16;
        result[col + 5*8] = descale(tmp5 + z2 + z4, col_shift) as i16;
        result[col + 3*8] = descale(tmp6 + z2 + z3, col_shift) as i16;
        result[col + 1*8] = descale(tmp7 + z1 + z4, col_shift) as i16;
    }

    result
}

#[cfg(test)]
pub fn fdct_reference(block: &Block8x8) -> Block8x8
{
    use std::f64::consts::PI;

    let cos = {
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
                    sum += block[y * 8 + x] as f64 * cos[x][u] * cos[y][v];
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

    #[test]
    fn fast_dct_matches_reference()
    {
        let block: Block8x8 = [
            -76, -73, -67, -62, -58, -67, -64, -55,
            -65, -69, -73, -38, -19, -43, -59, -56,
            -66, -69, -60, -15,  16, -24, -62, -55,
            -65, -70, -57,  -6,  26, -22, -58, -59,
            -61, -67, -60, -24,  -2, -40, -60, -58,
            -49, -63, -68, -58, -51, -60, -70, -53,
            -43, -57, -64, -69, -73, -67, -63, -45,
            -41, -49, -59, -60, -63, -52, -50, -34,
        ];

        let ref_result = fdct_reference(&block);
        let fast_result = fdct(&block);

        for i in 0..64
        {
            let diff = (ref_result[i] as i32 - fast_result[i] as i32).abs();
            assert!(
                diff <= 1,
                "Mismatch at [{}] (row={}, col={}): ref={}, fast={}, diff={}",
                i, i / 8, i % 8, ref_result[i], fast_result[i], diff
            );
        }
    }

    #[test]
    fn dct_of_flat_block()
    {
        let block: Block8x8 = [42; 64];
        let result = fdct(&block);
        assert!((result[0] as i32 - 336).abs() <= 1,
                "DC: expected ~336, got {}", result[0]);
        for i in 1..64
        {
            assert!(result[i].abs() <= 1,
                    "AC[{}] should be ~0, got {}", i, result[i]);
        }
    }
}