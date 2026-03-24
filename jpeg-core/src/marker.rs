//! # Markers
//!
//! A JPEG file (in JFIF interchange format) is an ordered sequence of
//! *marker segments* and *entropy-coded data segments*.
//!
//! The high-level structure for baseline sequential JPEG is:
//!
//! ```text
//!   SOI                          <- Start Of Image
//!   APP0 (JFIF)                  <- Application marker: JFIF metadata
//!   DQT (table 0 - luminance)    <- Define Quantization Table
//!   DQT (table 1 - chrominance)
//!   SOF0 (frame header)          <- Start Of Frame - Baseline DCT
//!   DHT (DC table 0)             <- Define Huffman Table
//!   DHT (AC table 0)
//!   DHT (DC table 1)
//!   DHT (AC table 1)
//!   DRI (restart interval)       <- Define Restart Interval (optional)
//!   SOS (scan header)            <- Start Of Scan
//!   <entropy-coded data>         <- Huffman-coded DCT coefficients
//!   EOI                          <- End Of Image
//! ```
//!
//! Every marker is a two-byte code: 0xFF followed by a non-zero byte.
//! Most markers begin a *marker segment* whose second field is a 16-bit
//! length (counting from the length field itself, excluding the marker
//! bytes).

use crate::bitstream::BitWriter;
use crate::entropy::huffman_table::HuffmanTable;
use crate::error::Result;
use crate::quantize::QuantTable;
use crate::types::EncoderConfig;

/// Start Of Image - must be the first marker in the file.
pub const MARKER_SOI: u16 = 0xFFD8;

/// End Of Image - must be the last marker in the file.
pub const MARKER_EOI: u16 = 0xFFD9;

/// Application-specific marker 0 - used for the JFIF header.
pub const MARKER_APP0: u16 = 0xFFE0;

/// Define Quantization Table(s).
pub const MARKER_DQT: u16 = 0xFFDB;

/// Define Huffman Table(s).
pub const MARKER_DHT: u16 = 0xFFC4;

/// Start Of Frame - Baseline DCT (the only frame type this encoder produces).
pub const MARKER_SOF0: u16 = 0xFFC0;

/// Start Of Scan - begins the entropy-coded data.
pub const MARKER_SOS: u16 = 0xFFDA;

/// Define Restart Interval.
pub const MARKER_DRI: u16 = 0xFFDD;

/// Restart marker base (RST0). The actual marker is RST0 + (m % 8).
pub const MARKER_RST0: u16 = 0xFFD0;

/// Write the SOI (Start Of Image) marker.
///
/// This must be the very first thing in the JPEG file (T.81 §B.2.1).
pub fn write_soi(w: &mut BitWriter) -> Result<()>
{
    w.write_u16_be(MARKER_SOI)
}

/// Write the EOI (End Of Image) marker.
///
/// This must be the very last thing in the JPEG file (T.81 §B.2.1).
pub fn write_eoi(w: &mut BitWriter) -> Result<()>
{
    w.write_u16_be(MARKER_EOI)
}

/// Write the APP0/JFIF marker segment.
///
/// The JFIF marker (not part of T.81 itself, but defined by the JFIF
/// specification v1.02) identifies the file as a JFIF-format JPEG and
/// provides pixel density information.
///
/// The segment structure is:
///
/// ```text
///   APP0 marker (0xFFE0)
///   Length      (16)                    <- 16 bytes of payload
///   Identifier "JFIF\0"  (5 bytes)
///   Version     1.02     (2 bytes)
///   Units       0|1|2    (1 byte)       <- 0=aspect, 1=dpi, 2=dpcm
///   Xdensity             (2 bytes)
///   Ydensity             (2 bytes)
///   Xthumbnail 0         (1 byte)       <- no embedded thumbnail
///   Ythumbnail 0         (1 byte)
/// ```
pub fn write_app0_jfif(w: &mut BitWriter, config: &EncoderConfig) -> Result<()>
{
    w.write_u16_be(MARKER_APP0)?;
    w.write_u16_be(16)?; // Segment length

    // Identifier: "JFIF\0"
    w.write_raw_bytes(&[0x4A, 0x46, 0x49, 0x46, 0x00])?;

    // Version 1.02
    w.write_raw_byte(1)?; // Major
    w.write_raw_byte(2)?; // Minor

    // Pixel density
    w.write_raw_byte(config.density_units)?;
    w.write_u16_be(config.x_density)?;
    w.write_u16_be(config.y_density)?;

    // No thumbnail
    w.write_raw_byte(0)?;
    w.write_raw_byte(0)?;

    Ok(())
}

/// Write a DQT (Define Quantization Table) marker segment.
///
/// T.81 §B.2.4.1:
///
/// ```text
///   DQT marker (0xFFDB)
///   Length       (67)                 <- 2 + 1 + 64
///   Pq|Tq       (1 byte)              <- Pq=0 (8-bit), Tq=table_id
///   Q0..Q63     (64 bytes)            <- values in zig-zag order
/// ```
///
/// Pq (precision) is always 0 for baseline JPEG (8-bit quantization
/// values). The table entries are written in zig-zag order as required
/// by the standard.
pub fn write_dqt(w: &mut BitWriter, table_id: u8, table: &QuantTable) -> Result<()>
{
    w.write_u16_be(MARKER_DQT)?;
    w.write_u16_be(67)?; // Lq = 2 + 1 + 64
    w.write_raw_byte(table_id & 0x0F)?; // Pq=0 (8-bit) | Tq
    for &qk in &table.zigzag_values
    {
        w.write_raw_byte(qk as u8)?;
    }
    Ok(())
}

/// Write a DHT (Define Huffman Table) marker segment.
///
/// T.81 §B.2.4.2:
///
/// ```text
///   DHT marker  (0xFFC4)
///   Length      (2 + 1 + 16 + Mt)     <- Mt = total number of symbols
///   Tc|Th       (1 byte)              <- Tc=class (0=DC,1=AC), Th=table_id
///   L1..L16     (16 bytes)            <- BITS: codes of each length
///   V1..VMt     (Mt bytes)            <- HUFFVAL: symbol values
/// ```
///
/// # Arguments
///
/// * `class` - 0 for DC table, 1 for AC table.
/// * `table_id` - destination identifier (0 or 1 for baseline).
/// * `table` - the Huffman table to write.
pub fn write_dht
(
    w: &mut BitWriter,
    class: u8,
    table_id: u8,
    table: &HuffmanTable,
) -> Result<()>
{
    w.write_u16_be(MARKER_DHT)?;

    let mt: u16 = table.bits.iter().map(|&b| b as u16).sum();
    let length = 2 + 1 + 16 + mt;
    w.write_u16_be(length)?;

    // Tc (table class) in the upper nibble, Th (table destination) in lower.
    w.write_raw_byte((class << 4) | (table_id & 0x0F))?;

    // BITS: number of codes of each length 1..16.
    for &count in &table.bits
    {
        w.write_raw_byte(count)?;
    }

    // HUFFVAL: symbol values in order of increasing code length.
    for &val in &table.values
    {
        w.write_raw_byte(val)?;
    }

    Ok(())
}

/// Parameters for a single component in the frame header.
pub struct FrameComponent
{
    /// Component identifier (1 = Y, 2 = Cb, 3 = Cr by JFIF convention).
    pub id: u8,

    /// Horizontal sampling factor H (T.81 §A.1.1).
    pub h_sampling: u8,

    /// Vertical sampling factor V.
    pub v_sampling: u8,

    /// Index of the quantization table to use (0 or 1).
    pub quant_table_id: u8,
}

/// Write the SOF0 (Start Of Frame - Baseline DCT) marker segment.
///
/// T.81 §B.2.2, Figure B.3:
///
/// ```text
///   SOF0 marker  (0xFFC0)
///   Lf           (2 bytes)             <- 8 + 3 * Nf
///   P            (1 byte)              <- Sample precision (always 8)
///   Y            (2 bytes)             <- Number of lines (height)
///   X            (2 bytes)             <- Samples per line (width)
///   Nf           (1 byte)              <- Number of components
///   For each component:
///     Ci         (1 byte)              <- Component identifier
///     Hi|Vi      (1 byte)              <- Sampling factors (4+4 bits)
///     Tqi        (1 byte)              <- Quantization table selector
/// ```
pub fn write_sof0
(
    w: &mut BitWriter,
    width: u16,
    height: u16,
    components: &[FrameComponent],
) -> Result<()>
{
    w.write_u16_be(MARKER_SOF0)?;

    let nf = components.len() as u16;
    w.write_u16_be(8 + 3 * nf)?; // Lf

    w.write_raw_byte(8)?;      // P = 8-bit sample precision
    w.write_u16_be(height)?;   // Y = number of lines
    w.write_u16_be(width)?;    // X = samples per line
    w.write_raw_byte(nf as u8)?;

    for comp in components
    {
        w.write_raw_byte(comp.id)?;
        w.write_raw_byte((comp.h_sampling << 4) | comp.v_sampling)?;
        w.write_raw_byte(comp.quant_table_id)?;
    }

    Ok(())
}

/// Parameters for a single component in the scan header.
pub struct ScanComponent
{
    /// Component selector (must match a Ci from the frame header).
    pub selector: u8,

    /// DC entropy coding table selector (0 or 1).
    pub dc_table_id: u8,

    /// AC entropy coding table selector (0 or 1).
    pub ac_table_id: u8,
}

/// Write the SOS (Start Of Scan) marker segment.
///
/// T.81 §B.2.3, Figure B.4:
///
/// ```text
///   SOS marker (0xFFDA)
///   Ls           (2 bytes)             <- 6 + 2 * Ns
///   Ns           (1 byte)              <- Number of components in scan
///   For each component:
///     Csj        (1 byte)              <- Component selector
///     Tdj|Taj    (1 byte)              <- DC|AC table selectors (4+4 bits)
///   Ss           (1 byte)              <- Start of spectral selection (0)
///   Se           (1 byte)              <- End of spectral selection (63)
///   Ah|Al        (1 byte)              <- Successive approximation (0)
/// ```
///
/// For baseline sequential JPEG:
///   - Ss = 0, Se = 63 (all coefficients in one scan).
///   - Ah = 0, Al = 0 (no successive approximation).
pub fn write_sos
(
    w: &mut BitWriter,
    components: &[ScanComponent],
) -> Result<()>
{
    w.write_u16_be(MARKER_SOS)?;

    let ns = components.len() as u16;
    w.write_u16_be(6 + 2 * ns)?; // Ls

    w.write_raw_byte(ns as u8)?;

    for comp in components
    {
        w.write_raw_byte(comp.selector)?;
        w.write_raw_byte((comp.dc_table_id << 4) | comp.ac_table_id)?;
    }

    w.write_raw_byte(0)?;  // Ss = 0 (start of spectral selection)
    w.write_raw_byte(63)?; // Se = 63 (end of spectral selection)
    w.write_raw_byte(0)?;  // Ah = 0, Al = 0

    Ok(())
}

/// Write a DRI (Define Restart Interval) marker segment, if the interval
/// is non-zero.
///
/// T.81 §B.2.4.4:
///
/// ```text
///   DRI marker   (0xFFDD)
///   Lr           (4)                   <- always 4
///   Ri           (2 bytes)             <- restart interval in MCUs
/// ```
///
/// A restart interval of 0 means restart is disabled, in which case this
/// function emits nothing.
pub fn write_dri(w: &mut BitWriter, restart_interval: u16) -> Result<()>
{
    if restart_interval > 0
    {
        w.write_u16_be(MARKER_DRI)?;
        w.write_u16_be(4)?; // Lr
        w.write_u16_be(restart_interval)?;
    }
    Ok(())
}

/// Write a restart marker (RSTm).
///
/// Restart markers cycle from RST0 (0xFFD0) through RST7 (0xFFD7) and
/// then wrap around (T.81 §B.2.1: "modulo 8 restart interval count").
///
/// These stand-alone markers (no segment length) appear between
/// entropy-coded segments when restart is enabled.
pub fn write_rst(w: &mut BitWriter, restart_counter: u16) -> Result<()>
{
    let m = restart_counter % 8;
    w.write_u16_be(MARKER_RST0 + m)
}

#[cfg(test)]
mod tests
{
    use super::*;
    use crate::quantize::{QuantTable, STD_LUMINANCE_QUANT};

    #[test]
    fn soi_is_ffd8()
    {
        let mut w = BitWriter::with_capacity(16);
        write_soi(&mut w).unwrap();
        assert_eq!(w.into_bytes(), [0xFF, 0xD8]);
    }

    #[test]
    fn eoi_is_ffd9()
    {
        let mut w = BitWriter::with_capacity(16);
        write_eoi(&mut w).unwrap();
        assert_eq!(w.into_bytes(), [0xFF, 0xD9]);
    }

    #[test]
    fn app0_jfif_structure()
    {
        let config = EncoderConfig::default();
        let mut w = BitWriter::with_capacity(64);
        write_app0_jfif(&mut w, &config).unwrap();
        let data = w.into_bytes();

        // Marker: 0xFF, 0xE0
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xE0);
        // Length: 0x00, 0x10 = 16
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x10);
        // Identifier: "JFIF\0"
        assert_eq!(&data[4..9], b"JFIF\0");
        // Version: 1.02
        assert_eq!(data[9], 1);
        assert_eq!(data[10], 2);
    }

    #[test]
    fn dqt_segment_length()
    {
        let qt = QuantTable::from_standard(&STD_LUMINANCE_QUANT, 50);
        let mut w = BitWriter::with_capacity(128);
        write_dqt(&mut w, 0, &qt).unwrap();
        let data = w.into_bytes();

        // Marker: 0xFF, 0xDB
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xDB);
        // Length: 67 = 0x0043
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x43);
        // Pq=0, Tq=0
        assert_eq!(data[4], 0x00);
        // 64 table values follow
        assert_eq!(data.len(), 2 + 2 + 1 + 64); // marker + length + pq|tq + values
    }

    #[test]
    fn sof0_baseline_marker()
    {
        let components = 
        [
            FrameComponent { id: 1, h_sampling: 2, v_sampling: 2, quant_table_id: 0 },
            FrameComponent { id: 2, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
            FrameComponent { id: 3, h_sampling: 1, v_sampling: 1, quant_table_id: 1 },
        ];
        let mut w = BitWriter::with_capacity(64);
        write_sof0(&mut w, 640, 480, &components).unwrap();
        let data = w.into_bytes();

        // Marker: 0xFF, 0xC0
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xC0);
        // Precision: 8
        assert_eq!(data[4], 8);
        // Height: 480 = 0x01E0
        assert_eq!(data[5], 0x01);
        assert_eq!(data[6], 0xE0);
        // Width: 640 = 0x0280
        assert_eq!(data[7], 0x02);
        assert_eq!(data[8], 0x80);
        // Nf: 3
        assert_eq!(data[9], 3);
    }

    #[test]
    fn sos_baseline_params()
    {
        let components = 
        [
            ScanComponent { selector: 1, dc_table_id: 0, ac_table_id: 0 },
        ];
        let mut w = BitWriter::with_capacity(32);
        write_sos(&mut w, &components).unwrap();
        let data = w.into_bytes();

        // Marker: 0xFF, 0xDA
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xDA);
        // Ns = 1
        assert_eq!(data[4], 1);
        // Ss=0, Se=63, Ah|Al=0
        let tail = &data[data.len() - 3..];
        assert_eq!(tail, [0, 63, 0]);
    }

    #[test]
    fn dri_zero_interval_emits_nothing()
    {
        let mut w = BitWriter::with_capacity(16);
        write_dri(&mut w, 0).unwrap();
        assert!(w.into_bytes().is_empty());
    }

    #[test]
    fn dri_nonzero_emits_marker()
    {
        let mut w = BitWriter::with_capacity(16);
        write_dri(&mut w, 100).unwrap();
        let data = w.into_bytes();
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xDD);
        // Lr = 4
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x04);
        // Ri = 100 = 0x0064
        assert_eq!(data[4], 0x00);
        assert_eq!(data[5], 0x64);
    }

    #[test]
    fn rst_markers_cycle_modulo_8()
    {
        for counter in 0..16u16
        {
            let mut w = BitWriter::with_capacity(4);
            write_rst(&mut w, counter).unwrap();
            let data = w.into_bytes();
            let expected_m = (counter % 8) as u8;
            assert_eq!(data[0], 0xFF);
            assert_eq!(data[1], 0xD0 + expected_m);
        }
    }
}