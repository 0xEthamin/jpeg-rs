use crate::bitstream::BitWriter;
use crate::entropy::huffman_table::HuffmanTable;
use crate::quantize::QuantTable;
use crate::types::EncoderConfig;

pub const MARKER_SOI: u16 = 0xFFD8;
pub const MARKER_EOI: u16 = 0xFFD9;
pub const MARKER_APP0: u16 = 0xFFE0;
pub const MARKER_DQT: u16 = 0xFFDB;
pub const MARKER_DHT: u16 = 0xFFC4;
pub const MARKER_SOF0: u16 = 0xFFC0; // Baseline DCT
pub const MARKER_SOS: u16 = 0xFFDA;
pub const MARKER_DRI: u16 = 0xFFDD;
pub const MARKER_RST0: u16 = 0xFFD0;

pub fn write_soi(w: &mut BitWriter)
{
    w.write_u16_be(MARKER_SOI);
}

pub fn write_eoi(w: &mut BitWriter)
{
    w.write_u16_be(MARKER_EOI);
}


pub fn write_app0_jfif(w: &mut BitWriter, config: &EncoderConfig)
{
    w.write_u16_be(MARKER_APP0);
    w.write_u16_be(16); // Length

    // Identifier: "JFIF\0"
    w.write_raw_bytes(&[0x4A, 0x46, 0x49, 0x46, 0x00]);

    // Version 1.02
    w.write_raw_byte(1);  // Major
    w.write_raw_byte(2);  // Minor

    // Density
    w.write_raw_byte(config.density_units);
    w.write_u16_be(config.x_density);
    w.write_u16_be(config.y_density);

    // No thumbnail
    w.write_raw_byte(0);
    w.write_raw_byte(0);
}

pub fn write_dqt(w: &mut BitWriter, table_id: u8, table: &QuantTable)
{
    w.write_u16_be(MARKER_DQT);
    w.write_u16_be(67); // 2 + 1 + 64 = 67
    w.write_raw_byte(table_id & 0x0F); // Pq=0 (8-bit) | Tq
    for &qk in &table.values
    {
        w.write_raw_byte(qk as u8);
    }
}

pub fn write_dht(
    w: &mut BitWriter,
    class: u8,
    table_id: u8,
    table: &HuffmanTable,
)
{
    w.write_u16_be(MARKER_DHT);

    let mt: u16 = table.bits.iter().map(|&b| b as u16).sum();
    let length = 2 + 1 + 16 + mt;
    w.write_u16_be(length);

    w.write_raw_byte((class << 4) | (table_id & 0x0F));

    for &count in &table.bits
    {
        w.write_raw_byte(count);
    }

    for &val in &table.values
    {
        w.write_raw_byte(val);
    }
}

pub struct FrameComponent
{
    pub id: u8,
    pub h_sampling: u8,
    pub v_sampling: u8,
    pub quant_table_id: u8,
}

pub fn write_sof0(
    w: &mut BitWriter,
    width: u16,
    height: u16,
    components: &[FrameComponent],
)
{
    w.write_u16_be(MARKER_SOF0);

    let nf = components.len() as u16;
    w.write_u16_be(8 + 3 * nf); // Lf

    w.write_raw_byte(8); // Sample precision P = 8 bits
    w.write_u16_be(height); // Y (number of lines)
    w.write_u16_be(width);  // X (samples per line)
    w.write_raw_byte(nf as u8);

    for comp in components
    {
        w.write_raw_byte(comp.id);
        w.write_raw_byte((comp.h_sampling << 4) | comp.v_sampling);
        w.write_raw_byte(comp.quant_table_id);
    }
}

pub struct ScanComponent
{
    pub selector: u8,
    pub dc_table_id: u8,
    pub ac_table_id: u8,
}

pub fn write_sos(
    w: &mut BitWriter,
    components: &[ScanComponent],
)
{
    w.write_u16_be(MARKER_SOS);

    let ns = components.len() as u16;
    w.write_u16_be(6 + 2 * ns); // Ls

    w.write_raw_byte(ns as u8);

    for comp in components
    {
        w.write_raw_byte(comp.selector);
        w.write_raw_byte((comp.dc_table_id << 4) | comp.ac_table_id);
    }

    w.write_raw_byte(0);  // Ss
    w.write_raw_byte(63); // Se
    w.write_raw_byte(0);  // Ah=0, Al=0
}

pub fn write_dri(w: &mut BitWriter, restart_interval: u16)
{
    if restart_interval > 0
    {
        w.write_u16_be(MARKER_DRI);
        w.write_u16_be(4); // Lr = 4
        w.write_u16_be(restart_interval);
    }
}

pub fn write_rst(w: &mut BitWriter, restart_counter: u16)
{
    let m = (restart_counter % 8) as u16;
    w.write_u16_be(MARKER_RST0 + m);
}