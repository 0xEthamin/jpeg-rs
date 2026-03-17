use crate::bitstream::BitWriter;
use crate::entropy::huffman_table::{category, HuffmanTable};

pub fn encode_blocks
(
    blocks: &[[i16; 64]],
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    prev_dc: &mut i16,
    writer: &mut BitWriter,
)
{
    for block in blocks
    {
        encode_block(block, dc_table, ac_table, prev_dc, writer);
    }
}

pub fn encode_block
(
    zz: &[i16; 64],
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    prev_dc: &mut i16,
    writer: &mut BitWriter,
)
{
    let dc = zz[0];
    let diff = dc - *prev_dc;
    *prev_dc = dc;

    encode_dc_coefficient(diff, dc_table, writer);

    encode_ac_coefficients(&zz[1..], ac_table, writer);
}

fn encode_dc_coefficient
(
    diff: i16,
    table: &HuffmanTable,
    writer: &mut BitWriter,
)
{
    let ssss = category(diff);

    let code = table.ehufco[ssss as usize];
    let size = table.ehufsi[ssss as usize];

    debug_assert!(
        size > 0,
        "DC category {} (diff={}) has no Huffman code (ehufsi=0). \
         This indicates a bug in Huffman table construction.",
        ssss, diff
    );

    if size > 0
    {
        writer.write_bits(code, size);
    }

    if ssss > 0
    {
        let additional = encode_additional_bits(diff, ssss);
        writer.write_bits(additional as u32, ssss);
    }
}

fn encode_ac_coefficients
(
    ac: &[i16],   // 63 coefficients (zz[1..64])
    table: &HuffmanTable,
    writer: &mut BitWriter,
)
{
    let mut zero_run: u8 = 0;

    for k in 0..63
    {
        let coeff = ac[k];

        if coeff == 0
        {
            zero_run += 1;
            continue;
        }

        while zero_run > 15
        {
            let code = table.ehufco[0xF0];
            let size = table.ehufsi[0xF0];
            debug_assert!(
                size > 0,
                "ZRL symbol (0xF0) has no Huffman code"
            );
            writer.write_bits(code, size);
            zero_run -= 16;
        }

        let ssss = category(coeff);
        let rs = ((zero_run as u16) << 4) | (ssss as u16);

        let code = table.ehufco[rs as usize];
        let size = table.ehufsi[rs as usize];
        debug_assert!(
            size > 0,
            "AC symbol 0x{:02X} (run={}, cat={}, coeff={}) has no Huffman code",
            rs, zero_run, ssss, coeff
        );
        writer.write_bits(code, size);

        let additional = encode_additional_bits(coeff, ssss);
        writer.write_bits(additional as u32, ssss);

        zero_run = 0;
    }

    if zero_run > 0
    {
        let code = table.ehufco[0x00]; // EOB
        let size = table.ehufsi[0x00];
        debug_assert!(
            size > 0,
            "EOB symbol (0x00) has no Huffman code"
        );
        writer.write_bits(code, size);
    }
}

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