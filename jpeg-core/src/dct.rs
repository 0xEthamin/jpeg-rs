use crate::block::Block8x8;
use std::f64::consts::PI;

fn cos_table() -> [[f64; 8]; 8]
{
    let mut table = [[0.0f64; 8]; 8];
    let mut k = 0;
    while k < 8
    {
        let mut n = 0;
        while n < 8
        {
            table[k][n] = ((2 * k + 1) as f64 * n as f64 * PI / 16.0).cos();
            n += 1;
        }
        k += 1;
    }
    table
}

#[inline]
fn c(n: usize) -> f64
{
    if n == 0
    {
        std::f64::consts::FRAC_1_SQRT_2
    }
    else
    {
        1.0
    }
}

pub fn fdct(block: &Block8x8) -> Block8x8
{
    let cos = cos_table();
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
                        * cos[x][u]
                        * cos[y][v];
                }
            }

            let normalized = 0.25 * c(u) * c(v) * sum;
            result[v * 8 + u] = normalized.round() as i16;
        }
    }
    
    result
}
