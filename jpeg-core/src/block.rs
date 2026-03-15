pub type Block8x8 = [i16; 64];

pub fn extract_blocks
(
    plane: &[u8],
    width: u32,
    height: u32,
) -> Vec<Block8x8>
{
    let blocks_h = ((width + 7) / 8) as usize;
    let blocks_v = ((height + 7) / 8) as usize;

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
                    let sy = (by as u32 * 8 + row).min(height - 1);
                    let sx = (bx as u32 * 8 + col).min(width - 1);

                    let sample = plane[(sy * width + sx) as usize];

                    block[(row * 8 + col) as usize] = sample as i16 - 128;
                }
            }

            blocks.push(block);
        }
    }

    blocks
}

#[inline]
pub const fn blocks_wide(width: u32) -> u32
{
    (width + 7) / 8
}

#[inline]
pub const fn blocks_high(height: u32) -> u32
{
    (height + 7) / 8
}
