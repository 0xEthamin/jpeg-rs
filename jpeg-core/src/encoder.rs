//! # Encoder
//!
//! Top-level JPEG encoding pipeline.
//!
//! This module orchestrates the complete encoding process, from raw pixel
//! data to a valid JPEG bitstream. The pipeline is:
//!
//! ```text
//! Raw pixels (RGB or Grayscale)
//!        |
//!        |
//! ------------------------------
//! | Color conversion           | RGB -> YCbCr (see [crate::color])
//! ------------------------------
//!              |
//!              v   
//!       ----------------
//!       | Y    Cb   Cr | Three separate component planes
//!       ----------------
//!           |    |
//!           |    v
//!           |  -------------
//!           |  | Subsample |
//!           |  | Reduce Cb/Cr resolution (see [crate::sampling])
//!           |  -------------
//!           |        |
//!           v        v
//! ------------------------------
//! | For each component:        |
//! | 1. Extract 8x8             | Partition into blocks (see [crate::block])
//! | 2. FDCT                    | Forward DCT (see [crate::dct])
//! | 3. Quantize                | Divide by step sizes (see [crate::quantize])
//! ------------------------------
//!              |
//!              v
//! ------------------------------
//! | Huffman table              |
//! | construction               | Count symbol frequencies, build optimal
//! |                            | Huffman codes (see [crate::entropy::huffman_table])
//! ------------------------------
//!              |
//!              v
//! ------------------------------
//! | Bitstream assembly         | Write markers + entropy-coded data
//! ------------------------------ (see [crate::marker], [crate::entropy::huffman_encoder])
//!              |
//!              v
//!       JPEG file bytes
//! ```
//! 
//! # Interleaved scan order (T.81 §A.2.3)
//!
//! For color images, all three components (Y, Cb, Cr) are encoded in a
//! single *interleaved* scan. Within each Minimum Coded Unit (MCU), the
//! data units appear in the order defined by the sampling factors:
//!
//!   - First: all Y blocks in the MCU (Hy * Vy blocks, raster order).
//!   - Then: one Cb block.
//!   - Then: one Cr block.
//!
//! For example, with 4:2:0 subsampling (Hy=2, Vy=2), each MCU contains:
//!
//! ```text
//!   MCU = [Y00, Y01, Y10, Y11, Cb, Cr]
//!          ------------------- --  --
//!           4 luma blocks       1   1 chroma blocks
//! ```
//!
//! This interleaving order is mandated by T.81 §A.2.3 and Figure A.3.

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
use crate::marker::{self, FrameComponent, ScanComponent};
use crate::quantize::
{
    quantize_block, QuantTable,
    STD_CHROMINANCE_QUANT, STD_LUMINANCE_QUANT,
};
use crate::sampling::subsample_chroma;
use crate::types::{ColorSpace, EncoderConfig, RawImage};

/// Encode a raw image to JPEG format.
///
/// This is the main entry point for the encoder. It validates the input,
/// selects the appropriate encoding path (color or grayscale), and returns
/// the complete JPEG file as a byte vector.
///
/// # Arguments
///
/// * `image` - the raw image to encode.
/// * `config` - encoding parameters (quality, subsampling, etc.).
///
/// # Errors
///
/// * [`crate::error::Error::InvalidDimensions`] - width/height out of range.
/// * [`crate::error::Error::BufferSizeMismatch`] - pixel buffer wrong size.
/// * [`crate::error::Error::InvalidQuality`] - quality not in 1..=100.
/// * [`crate::error::Error::MissingHuffmanCode`] - internal bug in table
///   construction (should never occur with valid input).
///
/// # Example
///
/// ```no_run
/// use jpeg_core::{encode, ColorSpace, EncoderConfig, RawImage};
///
/// let pixels: Vec<u8> = vec![128; 64 * 64 * 3]; // 64*64 gray RGB
/// let image = RawImage 
/// {
///     width: 64,
///     height: 64,
///     color_space: ColorSpace::Rgb,
///     data: &pixels,
/// };
/// let jpeg_bytes = encode(&image, &EncoderConfig::default()).unwrap();
/// ```
pub fn encode(image: &RawImage, config: &EncoderConfig) -> Result<Vec<u8>>
{
    image.validate()?;
    config.validate()?;

    match image.color_space
    {
        ColorSpace::Rgb       => encode_color(image, config),
        ColorSpace::Grayscale => encode_grayscale(image, config),
    }
}

/// Quantized blocks for a single image component, ready for entropy coding.
struct ComponentData
{
    /// Quantized DCT coefficients in zig-zag order, one array per 8*8 block.
    quantized: Vec<[i16; 64]>,

    /// Width of this component's plane in samples (after subsampling).
    width: u32,

    /// Height of this component's plane in samples (after subsampling).
    height: u32,
}

/// Encode a 3-component (YCbCr) image.
fn encode_color(image: &RawImage, config: &EncoderConfig) -> Result<Vec<u8>>
{
    let w = image.width;
    let h = image.height;

    // Color conversion
    let (y_plane, cb_plane, cr_plane) = rgb_to_ycbcr_planar(image.data, w, h);

    // Chroma subsampling
    let (cb_sub, cb_w, cb_h, cr_sub, cr_w, cr_h) =
        subsample_chroma(&cb_plane, &cr_plane, w, h, config.subsampling);

    // Build quantization tables
    let lum_qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, config.quality);
    let chr_qt = QuantTable::from_standard(&STD_CHROMINANCE_QUANT, config.quality);

    // Block extraction + DCT + quantization
    let y_data = process_component(&y_plane, w, h, &lum_qt);
    let cb_data = process_component(&cb_sub, cb_w, cb_h, &chr_qt);
    let cr_data = process_component(&cr_sub, cr_w, cr_h, &chr_qt);

    // Build optimised Huffman tables
    let (y_dc_freq, y_ac_freq)   = collect_frequencies(&y_data.quantized);
    let (cb_dc_freq, cb_ac_freq) = collect_frequencies(&cb_data.quantized);
    let (cr_dc_freq, cr_ac_freq) = collect_frequencies(&cr_data.quantized);

    // Luminance uses its own tables.
    let lum_dc_table = build_table(&y_dc_freq.counts, MAX_DC_CATEGORIES - 1);
    let lum_ac_table = build_table(&y_ac_freq.counts, 255);

    // Chrominance Cb and Cr share a single pair of tables.
    let mut chr_dc_counts = [0u32; MAX_DC_CATEGORIES];
    let mut chr_ac_counts = [0u32; 256];
    for (i, count) in chr_dc_counts.iter_mut().enumerate()
    {
        *count = cb_dc_freq.counts[i] + cr_dc_freq.counts[i];
    }
    for (i, count) in chr_ac_counts.iter_mut().enumerate()
    {
        *count = cb_ac_freq.counts[i] + cr_ac_freq.counts[i];
    }
    let chr_dc_table = build_table(&chr_dc_counts, MAX_DC_CATEGORIES - 1);
    let chr_ac_table = build_table(&chr_ac_counts, 255);

    // Assemble the JPEG bitstream
    let est_size = (w as usize * h as usize) / 2;
    let mut writer = BitWriter::with_capacity(est_size);

    // File headers
    marker::write_soi(&mut writer)?;
    marker::write_app0_jfif(&mut writer, config)?;
    marker::write_dqt(&mut writer, 0, &lum_qt)?;
    marker::write_dqt(&mut writer, 1, &chr_qt)?;

    let (hy, vy, _, _, _, _) = config.subsampling.factors();
    let frame_components = 
    [
        FrameComponent { id: 1, h_sampling: hy, v_sampling: vy, quant_table_id: 0 },
        FrameComponent { id: 2, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
        FrameComponent { id: 3, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
    ];
    marker::write_sof0(&mut writer, w as u16, h as u16, &frame_components)?;

    // Huffman tables
    marker::write_dht(&mut writer, 0, 0, &lum_dc_table)?; // DC class=0, id=0
    marker::write_dht(&mut writer, 1, 0, &lum_ac_table)?; // AC class=1, id=0
    marker::write_dht(&mut writer, 0, 1, &chr_dc_table)?; // DC class=0, id=1
    marker::write_dht(&mut writer, 1, 1, &chr_ac_table)?; // AC class=1, id=1

    // Restart interval (optional)
    marker::write_dri(&mut writer, config.restart_interval)?;

    // Scan header
    let scan_components = 
    [
        ScanComponent { selector: 1, dc_table_id: 0, ac_table_id: 0 },
        ScanComponent { selector: 2, dc_table_id: 1, ac_table_id: 1 },
        ScanComponent { selector: 3, dc_table_id: 1, ac_table_id: 1 },
    ];
    marker::write_sos(&mut writer, &scan_components)?;

    // Entropy-coded data
    encode_interleaved_scan
    (
        &y_data, &cb_data, &cr_data,
        config,
        &lum_dc_table, &lum_ac_table,
        &chr_dc_table, &chr_ac_table,
        &mut writer,
    )?;

    writer.flush_with_ones()?;
    marker::write_eoi(&mut writer)?;

    Ok(writer.into_bytes())
}

/// Encode a single-component (grayscale) image.
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

    marker::write_soi(&mut writer)?;
    marker::write_app0_jfif(&mut writer, config)?;
    marker::write_dqt(&mut writer, 0, &lum_qt)?;

    let frame_components = 
    [
        FrameComponent { id: 1, h_sampling: 1, v_sampling: 1, quant_table_id: 0 },
    ];
    marker::write_sof0(&mut writer, w as u16, h as u16, &frame_components)?;

    marker::write_dht(&mut writer, 0, 0, &dc_table)?;
    marker::write_dht(&mut writer, 1, 0, &ac_table)?;
    marker::write_dri(&mut writer, config.restart_interval)?;

    let scan_components = 
    [
        ScanComponent { selector: 1, dc_table_id: 0, ac_table_id: 0 },
    ];
    marker::write_sos(&mut writer, &scan_components)?;

    encode_grayscale_scan
    (
        &y_data,
        config,
        &dc_table,
        &ac_table,
        &mut writer,
    )?;

    writer.flush_with_ones()?;
    marker::write_eoi(&mut writer)?;

    Ok(writer.into_bytes())
}

/// Process a single component plane: extract blocks -> FDCT -> quantize.
///
/// This is the "inner loop" of the encoder, applied independently to each
/// component (Y, Cb, Cr or just Y for grayscale).
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

    ComponentData { quantized, width, height }
}

/// Encode a grayscale (non-interleaved) scan.
///
/// For a single-component image, each MCU contains exactly one 8*8 block
/// (T.81 §A.2.1: "For non-interleaved data the MCU is one data unit").
fn encode_grayscale_scan
(
    y_data: &ComponentData,
    config: &EncoderConfig,
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    writer: &mut BitWriter,
) -> Result<()>
{
    let total_mcus = y_data.quantized.len();
    let ri = config.restart_interval as usize;
    let mut prev_dc: i16 = 0;
    let mut rst_counter: u16 = 0;

    for mcu_idx in 0..total_mcus
    {
        // Insert restart marker if we've reached the restart boundary.
        if ri > 0 && mcu_idx > 0 && mcu_idx % ri == 0
        {
            writer.flush_with_ones()?;
            marker::write_rst(writer, rst_counter)?;
            rst_counter += 1;
            // DC prediction resets to 0 at each restart (T.81 §E.1.4,
            // §F.1.1.5.1).
            prev_dc = 0;
        }

        huffman_encoder::encode_block
        (
            &y_data.quantized[mcu_idx],
            dc_table,
            ac_table,
            &mut prev_dc,
            writer,
        )?;
    }

    Ok(())
}

/// Encode a color interleaved scan (Y + Cb + Cr).
///
/// The scan traverses the image in MCU order (left-to-right, top-to-bottom).
/// Within each MCU, blocks are emitted in the order specified by T.81
/// §A.2.3:
///
///   1. All Y blocks in the MCU (Hy * Vy blocks, raster order within
///      the MCU).
///   2. One Cb block.
///   3. One Cr block.
///
/// The number of MCU columns and rows is determined by the luminance
/// component's block grid, divided by the maximum sampling factors:
///
///   mcus_h = (Y_blocks_wide) / Hmax
///   mcus_v = (Y_blocks_high) / Vmax
#[allow(clippy::too_many_arguments)]
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
) -> Result<()>
{
    let (hy, vy, _, _, _, _) = config.subsampling.factors();
    let h_max = config.subsampling.h_max() as u32;
    let v_max = config.subsampling.v_max() as u32;

    // MCU dimensions in blocks.
    let mcu_width = h_max;
    let mcu_height = v_max;

    // Number of blocks in the Y component grid.
    let y_blocks_h = y_data.width.div_ceil(8);
    let y_blocks_v = y_data.height.div_ceil(8);

    // Number of MCUs that tile the image.
    let mcus_h = y_blocks_h.div_ceil(mcu_width);
    let mcus_v = y_blocks_v.div_ceil(mcu_height);

    // Block grid dimensions for chroma components.
    let cb_blocks_h = cb_data.width.div_ceil(8);
    let cr_blocks_h = cr_data.width.div_ceil(8);

    // Running DC predictions (reset at each restart boundary).
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
            // Restart boundary
            if ri > 0 && mcu_count > 0 && mcu_count.is_multiple_of(ri)
            {
                writer.flush_with_ones()?;
                marker::write_rst(writer, rst_counter)?;
                rst_counter += 1;
                y_prev_dc = 0;
                cb_prev_dc = 0;
                cr_prev_dc = 0;
            }

            // Y blocks (Hy * Vy per MCU)
            for v in 0..vy as u32
            {
                for h in 0..hy as u32
                {
                    let block_row = mcu_row * v_max + v;
                    let block_col = mcu_col * h_max + h;

                    // Clamp to valid block indices (handles partial MCUs
                    // at image boundaries, per T.81 §A.2.4).
                    let idx = 
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
                        )?;
                    }
                }
            }

            // Cb block (one per MCU)
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
                )?;
            }

            // Cr block (one per MCU)
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
                )?;
            }

            mcu_count += 1;
        }
    }

    Ok(())
}
