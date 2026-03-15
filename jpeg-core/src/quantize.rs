use crate::block::Block8x8;

pub const ZIG_ZAG: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

pub const STD_LUMINANCE_QUANT: [u16; 64] = [
    16,  11,  10,  16,  24,  40,  51,  61,
    12,  12,  14,  19,  26,  58,  60,  55,
    14,  13,  16,  24,  40,  57,  69,  56,
    14,  17,  22,  29,  51,  87,  80,  62,
    18,  22,  37,  56,  68, 109, 103,  77,
    24,  35,  55,  64,  81, 104, 113,  92,
    49,  64,  78,  87, 103, 121, 120, 101,
    72,  92,  95,  98, 112, 100, 103,  99,
];

pub const STD_CHROMINANCE_QUANT: [u16; 64] = [
    17,  18,  24,  47,  99,  99,  99,  99,
    18,  21,  26,  66,  99,  99,  99,  99,
    24,  26,  56,  99,  99,  99,  99,  99,
    47,  66,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
];

#[derive(Debug, Clone)]
pub struct QuantTable
{
    pub values: [u16; 64],

    natural: [u16; 64],
}

pub fn scale_quant_table(base: &[u16; 64], quality: u8) -> [u16; 64]
{
    let q = quality.max(1) as u32;
    let scale = if q < 50
    {
        5000 / q
    }
    else
    {
        200 - 2 * q
    };

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
    pub fn new(natural_order: &[u16; 64]) -> Self
    {
        let mut zz_values = [0u16; 64];
        for k in 0..64
        {
            zz_values[k] = natural_order[ZIG_ZAG[k] as usize];
        }

        Self
        {
            values: zz_values,
            natural: *natural_order,
        }
    }

    pub fn from_standard(base: &[u16; 64], quality: u8) -> Self
    {
        let scaled = scale_quant_table(base, quality);
        Self::new(&scaled)
    }
}

pub fn quantize_block(dct_block: &Block8x8, table: &QuantTable) -> [i16; 64]
{
    let mut quantized = [0i16; 64];

    for k in 0..64
    {
        let natural_idx = ZIG_ZAG[k] as usize;
        let coeff = dct_block[natural_idx] as f32;
        let q = table.natural[natural_idx] as f32;

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
