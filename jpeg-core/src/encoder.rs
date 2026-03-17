use crate::bitstream::BitWriter;
use crate::block::extract_blocks;
use crate::color::{grayscale_to_y_plane, rgb_to_ycbcr_planar};
use crate::dct::fdct;
use crate::entropy::huffman_encoder;
use crate::entropy::huffman_table::
{
    build_table, collect_frequencies, HuffmanTable, MAX_DC_CATEGORIES,
};
use crate::error::Result;
use crate::marker::
{
    self, FrameComponent, ScanComponent,
};
use crate::quantize::
{
    quantize_block, QuantTable,
    STD_CHROMINANCE_QUANT, STD_LUMINANCE_QUANT,
};
use crate::sampling::subsample_chroma;
use crate::types::{ColorSpace, EncoderConfig, RawImage};

pub fn encode(image: &RawImage, config: &EncoderConfig) -> Result<Vec<u8>>
{
    image.validate()?;
    config.validate()?;

    match image.color_space
    {
        ColorSpace::Rgb => encode_color(image, config),
        ColorSpace::Grayscale => encode_grayscale(image, config),
    }
}

struct ComponentData
{
    quantized: Vec<[i16; 64]>,
    width: u32,
    height: u32,
}

fn encode_color(image: &RawImage, config: &EncoderConfig) -> Result<Vec<u8>>
{
    let w = image.width;
    let h = image.height;

    let (y_plane, cb_plane, cr_plane) = rgb_to_ycbcr_planar(image.data, w, h);

    let (cb_sub, cb_w, cb_h, cr_sub, cr_w, cr_h) =
        subsample_chroma(&cb_plane, &cr_plane, w, h, config.subsampling);

    let lum_qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, config.quality);
    let chr_qt = QuantTable::from_standard(&STD_CHROMINANCE_QUANT, config.quality);

    let y_data = process_component(&y_plane, w, h, &lum_qt);
    let cb_data = process_component(&cb_sub, cb_w, cb_h, &chr_qt);
    let cr_data = process_component(&cr_sub, cr_w, cr_h, &chr_qt);

    let (y_dc_freq, y_ac_freq) = collect_frequencies(&y_data.quantized);
    let (cb_dc_freq, cb_ac_freq) = collect_frequencies(&cb_data.quantized);
    let (cr_dc_freq, cr_ac_freq) = collect_frequencies(&cr_data.quantized);

    let lum_dc_table = build_table(&y_dc_freq.counts, MAX_DC_CATEGORIES - 1);
    let lum_ac_table = build_table(&y_ac_freq.counts, 255);

    let mut chr_dc_counts = [0u32; MAX_DC_CATEGORIES];
    let mut chr_ac_counts = [0u32; 256];
    for i in 0..MAX_DC_CATEGORIES
    {
        chr_dc_counts[i] = cb_dc_freq.counts[i] + cr_dc_freq.counts[i];
    }
    for i in 0..256
    {
        chr_ac_counts[i] = cb_ac_freq.counts[i] + cr_ac_freq.counts[i];
    }
    let chr_dc_table = build_table(&chr_dc_counts, MAX_DC_CATEGORIES - 1);
    let chr_ac_table = build_table(&chr_ac_counts, 255);

    let est_size = (w as usize * h as usize) / 2;
    let mut writer = BitWriter::with_capacity(est_size);

    // --- File structure per JFIF 1.02 + T.81 Annex B ---
    marker::write_soi(&mut writer);
    marker::write_app0_jfif(&mut writer, config);

    marker::write_dqt(&mut writer, 0, &lum_qt);
    marker::write_dqt(&mut writer, 1, &chr_qt);

    let (hy, vy, _, _, _, _) = config.subsampling.factors();
    let frame_components =
    [
        FrameComponent { id: 1, h_sampling: hy, v_sampling: vy, quant_table_id: 0 },
        FrameComponent { id: 2, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
        FrameComponent { id: 3, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
    ];
    marker::write_sof0(&mut writer, w as u16, h as u16, &frame_components);

    marker::write_dht(&mut writer, 0, 0, &lum_dc_table);
    marker::write_dht(&mut writer, 1, 0, &lum_ac_table);
    marker::write_dht(&mut writer, 0, 1, &chr_dc_table);
    marker::write_dht(&mut writer, 1, 1, &chr_ac_table);

    marker::write_dri(&mut writer, config.restart_interval);

    let scan_components =
    [
        ScanComponent { selector: 1, dc_table_id: 0, ac_table_id: 0 },
        ScanComponent { selector: 2, dc_table_id: 1, ac_table_id: 1 },
        ScanComponent { selector: 3, dc_table_id: 1, ac_table_id: 1 },
    ];
    marker::write_sos(&mut writer, &scan_components);

    encode_interleaved_scan
    (
        &y_data, &cb_data, &cr_data,
        config,
        &lum_dc_table, &lum_ac_table,
        &chr_dc_table, &chr_ac_table,
        &mut writer,
    );

    writer.flush_with_ones();
    marker::write_eoi(&mut writer);

    Ok(writer.into_bytes())
}

fn encode_grayscale(image: &RawImage, config: &EncoderConfig) -> Result<Vec<u8>>
{
    let w = image.width;
    let h = image.height;

    let y_plane = grayscale_to_y_plane(image.data);
    let lum_qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, config.quality);

    let y_data = process_component(&y_plane, w, h, &lum_qt);
    let (dc_freq, ac_freq) = collect_frequencies(&y_data.quantized);
    let dc_table = build_table(&dc_freq.counts, MAX_DC_CATEGORIES - 1);
    let ac_table = build_table(&ac_freq.counts, 255);

    let est_size = (w as usize * h as usize) / 2;
    let mut writer = BitWriter::with_capacity(est_size);

    marker::write_soi(&mut writer);
    marker::write_app0_jfif(&mut writer, config);
    marker::write_dqt(&mut writer, 0, &lum_qt);

    let frame_components =
    [
        FrameComponent { id: 1, h_sampling: 1, v_sampling: 1, quant_table_id: 0 },
    ];
    marker::write_sof0(&mut writer, w as u16, h as u16, &frame_components);

    marker::write_dht(&mut writer, 0, 0, &dc_table);
    marker::write_dht(&mut writer, 1, 0, &ac_table);
    marker::write_dri(&mut writer, config.restart_interval);

    let scan_components =
    [
        ScanComponent { selector: 1, dc_table_id: 0, ac_table_id: 0 },
    ];
    marker::write_sos(&mut writer, &scan_components);

    encode_grayscale_scan
    (
        &y_data,
        config,
        &dc_table,
        &ac_table,
        &mut writer,
    );

    writer.flush_with_ones();
    marker::write_eoi(&mut writer);

    Ok(writer.into_bytes())
}

fn process_component
(
    plane: &[u8],
    width: u32,
    height: u32,
    quant_table: &QuantTable,
) -> ComponentData
{
    let blocks = extract_blocks(plane, width, height);

    let quantized: Vec<[i16; 64]> = blocks
        .iter()
        .map(|block|
        {
            let dct_block = fdct(block);
            quantize_block(&dct_block, quant_table)
        })
        .collect();

    ComponentData
    {
        quantized,
        width,
        height,
    }
}

fn encode_grayscale_scan
(
    y_data: &ComponentData,
    config: &EncoderConfig,
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    writer: &mut BitWriter,
)
{
    let total_mcus = y_data.quantized.len();
    let ri = config.restart_interval as usize;
    let mut prev_dc: i16 = 0;
    let mut rst_counter: u16 = 0;

    for mcu_idx in 0..total_mcus
    {
        if ri > 0 && mcu_idx > 0 && mcu_idx % ri == 0
        {
            writer.flush_with_ones();
            marker::write_rst(writer, rst_counter);
            rst_counter += 1;
            prev_dc = 0;
        }

        huffman_encoder::encode_block
        (
            &y_data.quantized[mcu_idx],
            dc_table,
            ac_table,
            &mut prev_dc,
            writer,
        );
    }
}

fn encode_interleaved_scan
(
    y_data: &ComponentData,
    cb_data: &ComponentData,
    cr_data: &ComponentData,
    config: &EncoderConfig,
    lum_dc_table: &HuffmanTable,
    lum_ac_table: &HuffmanTable,
    chr_dc_table: &HuffmanTable,
    chr_ac_table: &HuffmanTable,
    writer: &mut BitWriter,
)
{
    let (hy, vy, _, _, _, _) = config.subsampling.factors();
    let h_max = config.subsampling.h_max() as u32;
    let v_max = config.subsampling.v_max() as u32;

    let mcu_width = h_max;
    let mcu_height = v_max;

    let y_blocks_h = (y_data.width + 7) / 8;
    let y_blocks_v = (y_data.height + 7) / 8;
    let mcus_h = (y_blocks_h + mcu_width - 1) / mcu_width;
    let mcus_v = (y_blocks_v + mcu_height - 1) / mcu_height;

    let cb_blocks_h = (cb_data.width + 7) / 8;
    let cr_blocks_h = (cr_data.width + 7) / 8;

    let mut y_prev_dc: i16 = 0;
    let mut cb_prev_dc: i16 = 0;
    let mut cr_prev_dc: i16 = 0;

    let ri = config.restart_interval as usize;
    let mut mcu_count: usize = 0;
    let mut rst_counter: u16 = 0;

    for mcu_row in 0..mcus_v
    {
        for mcu_col in 0..mcus_h
        {
            if ri > 0 && mcu_count > 0 && mcu_count % ri == 0
            {
                writer.flush_with_ones();
                marker::write_rst(writer, rst_counter);
                rst_counter += 1;
                y_prev_dc = 0;
                cb_prev_dc = 0;
                cr_prev_dc = 0;
            }

            for v in 0..vy as u32
            {
                for h in 0..hy as u32
                {
                    let block_row = mcu_row * v_max + v;
                    let block_col = mcu_col * h_max + h;

                    let idx = if block_row < y_blocks_v && block_col < y_blocks_h
                    {
                        (block_row * y_blocks_h + block_col) as usize
                    }
                    else
                    {
                        let br = block_row.min(y_blocks_v - 1);
                        let bc = block_col.min(y_blocks_h - 1);
                        (br * y_blocks_h + bc) as usize
                    };

                    if idx < y_data.quantized.len()
                    {
                        huffman_encoder::encode_block
                        (
                            &y_data.quantized[idx],
                            lum_dc_table,
                            lum_ac_table,
                            &mut y_prev_dc,
                            writer,
                        );
                    }
                }
            }

            // Encode Cb block
            let cb_idx = (mcu_row * cb_blocks_h + mcu_col) as usize;
            if cb_idx < cb_data.quantized.len()
            {
                huffman_encoder::encode_block
                (
                    &cb_data.quantized[cb_idx],
                    chr_dc_table,
                    chr_ac_table,
                    &mut cb_prev_dc,
                    writer,
                );
            }

            // Encode Cr block
            let cr_idx = (mcu_row * cr_blocks_h + mcu_col) as usize;
            if cr_idx < cr_data.quantized.len()
            {
                huffman_encoder::encode_block
                (
                    &cr_data.quantized[cr_idx],
                    chr_dc_table,
                    chr_ac_table,
                    &mut cr_prev_dc,
                    writer,
                );
            }

            mcu_count += 1;
        }
    }
}