//! PPM (Portable Pixmap) file reader.
//!
//! PPM is part of the Netpbm family of formats. It stores uncompressed
//! RGB pixel data in a simple header + data layout, making it ideal as an
//! intermediate format for testing image encoders.
//!
//! Two sub-formats are supported:
//!
//!   - **P6** (binary): the most common variant. After the ASCII header,
//!     pixel data is stored as raw bytes (1 or 2 bytes per sample depending
//!     on maxval).
//!
//!   - **P3** (ASCII): pixel values are written as decimal numbers separated
//!     by whitespace. Less efficient but human-readable.
//!
//! # Header format
//!
//! ```text
//!   <magic>         "P3" or "P6"
//!   <width>         decimal integer
//!   <height>        decimal integer
//!   <maxval>        decimal integer (1..65535)
//!   <pixel data>    (format depends on magic)
//! ```
//!
//! Comments (lines starting with `#`) may appear between any tokens in the
//! header.

use std::io::{BufRead, BufReader, Read};

use crate::error::{Error, Result};

/// A decoded PPM image.
pub struct PpmImage
{
    /// Image width in pixels.
    pub width: u32,

    /// Image height in pixels.
    pub height: u32,

    /// Interleaved RGB pixel data, 8 bits per component.
    ///
    /// If the source file had a maxval other than 255, the samples have
    /// been normalised to the [0, 255] range.
    pub data: Vec<u8>,
}

/// Read a PPM image from any `Read` source.
///
/// Detects whether the file is P3 (ASCII) or P6 (binary) from the magic
/// number and dispatches to the appropriate parser.
///
/// # Errors
///
/// Returns an error if the magic number is not `P3` or `P6`, if any
/// header field is missing or invalid, or if the pixel data is incomplete
/// or out of range.
pub fn read_ppm<R: Read>(reader: R) -> Result<PpmImage>
{
    let mut reader = BufReader::new(reader);

    let magic = read_token(&mut reader)?;
    match magic.as_str()
    {
        "P3" => read_p3(&mut reader),
        "P6" => read_p6(&mut reader),
        _ => Err
        (
            Error::InvalidFormat
            (
                format!("expected P3 or P6, got '{}'", magic),
            )
        ),
    }
}

/// Read a binary (P6) PPM file.
fn read_p6<R: BufRead>(reader: &mut R) -> Result<PpmImage>
{
    let width = read_header_value(reader, "width")?;
    let height = read_header_value(reader, "height")?;
    let maxval = read_header_value(reader, "maxval")?;

    validate_header(width, height, maxval)?;

    let num_pixels = width as usize * height as usize;
    let bytes_per_sample = if maxval < 256 { 1 } else { 2 };
    let raw_size = num_pixels * 3 * bytes_per_sample;

    let mut raw = vec![0u8; raw_size];
    reader.read_exact(&mut raw)?;

    let data = normalize_samples(&raw, maxval, bytes_per_sample);

    Ok(PpmImage { width, height, data })
}

/// Read an ASCII (P3) PPM file.
fn read_p3<R: BufRead>(reader: &mut R) -> Result<PpmImage>
{
    let width = read_header_value(reader, "width")?;
    let height = read_header_value(reader, "height")?;
    let maxval = read_header_value(reader, "maxval")?;

    validate_header(width, height, maxval)?;

    let num_samples = width as usize * height as usize * 3;
    let mut data = Vec::with_capacity(num_samples);

    for _ in 0..num_samples
    {
        let val = read_token(reader)?
            .parse::<u32>()
            .map_err(|e| Error::InvalidFormat(format!("bad sample: {}", e)))?;

        if val > maxval
        {
            return Err
            (
                Error::ValueOutOfRange
                (
                    format!("sample {} exceeds maxval {}", val, maxval),
                )
            );
        }

        let normalized = if maxval == 255
        {
            val as u8
        }
        else
        {
            ((val * 255 + maxval / 2) / maxval) as u8
        };
        data.push(normalized);
    }

    Ok(PpmImage { width, height, data })
}

/// Parse a single decimal integer from the header.
fn read_header_value<R: BufRead>(reader: &mut R, name: &str) -> Result<u32>
{
    read_token(reader)?
        .parse::<u32>()
        .map_err(|e| Error::InvalidFormat(format!("bad {}: {}", name, e)))
}

/// Normalise raw samples to the [0, 255] range.
fn normalize_samples(raw: &[u8], maxval: u32, bytes_per_sample: usize) -> Vec<u8>
{
    if maxval == 255
    {
        return raw.to_vec();
    }

    if bytes_per_sample == 1
    {
        raw.iter()
            .map(|&b| ((b as u32 * 255 + maxval / 2) / maxval) as u8)
            .collect()
    }
    else
    {
        raw.chunks_exact(2)
            .map(|chunk| 
            {
                let val = ((chunk[0] as u32) << 8) | (chunk[1] as u32);
                ((val * 255 + maxval / 2) / maxval) as u8
            })
            .collect()
    }
}

/// Validate PPM header values.
fn validate_header(width: u32, height: u32, maxval: u32) -> Result<()>
{
    if width == 0 || height == 0
    {
        return Err
        (
            Error::InvalidFormat
            (
                format!("zero dimension: {}x{}", width, height),
            )
        );
    }
    if width > 65535 || height > 65535
    {
        return Err
        (
            Error::InvalidFormat
            (
                format!("dimensions too large: {}x{}", width, height),
            )
        );
    }
    if maxval == 0 || maxval > 65535
    {
        return Err
        (
            Error::ValueOutOfRange
            (
                format!("maxval must be 1..=65535, got {}", maxval),
            )
        );
    }
    Ok(())
}

/// Read the next whitespace-delimited token from the stream, skipping
/// comments (lines starting with `#`).
fn read_token<R: BufRead>(reader: &mut R) -> Result<String>
{
    let mut token = String::new();

    loop
    {
        let buf = reader.fill_buf()?;
        if buf.is_empty()
        {
            if token.is_empty()
            {
                return Err(Error::InvalidFormat("unexpected end of file".into()));
            }
            return Ok(token);
        }

        let byte = buf[0];

        // Skip comment lines.
        if byte == b'#'
        {
            reader.consume(1);
            let mut discard = Vec::new();
            reader.read_until(b'\n', &mut discard)?;
            continue;
        }

        // Whitespace delimits tokens.
        if byte.is_ascii_whitespace()
        {
            reader.consume(1);
            if token.is_empty()
            {
                continue;
            }
            return Ok(token);
        }

        reader.consume(1);
        token.push(byte as char);
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_p6_basic()
    {
        // 2*2 P6 image, maxval=255
        let mut data = Vec::new();
        data.extend_from_slice(b"P6\n2 2\n255\n");
        // 4 pixels * 3 channels = 12 bytes
        data.extend_from_slice(&[
            255, 0, 0,   // red
            0, 255, 0,   // green
            0, 0, 255,   // blue
            128, 128, 128 // gray
        ]);

        let img = read_ppm(Cursor::new(data)).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.data.len(), 12);
        assert_eq!(img.data[0], 255); // R of first pixel
    }

    #[test]
    fn read_p3_basic()
    {
        let data = b"P3\n2 1\n255\n255 0 0 0 255 0\n";
        let img = read_ppm(Cursor::new(data.as_ref())).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        assert_eq!(img.data, [255, 0, 0, 0, 255, 0]);
    }

    #[test]
    fn read_p3_with_comments()
    {
        let data = b"P3\n# This is a comment\n2 1\n# Another comment\n255\n100 200 50 10 20 30\n";
        let img = read_ppm(Cursor::new(data.as_ref())).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        assert_eq!(img.data, [100, 200, 50, 10, 20, 30]);
    }

    #[test]
    fn read_p6_maxval_scaling()
    {
        // maxval=127: sample 127 should normalize to 255.
        let mut data = Vec::new();
        data.extend_from_slice(b"P6\n1 1\n127\n");
        data.extend_from_slice(&[127, 0, 64]);

        let img = read_ppm(Cursor::new(data)).unwrap();
        assert_eq!(img.data[0], 255); // 127 * 255 / 127 = 255
        assert_eq!(img.data[1], 0);
        // 64 * 255 / 127 = 128.5 -> rounded = 129 (with (64*255+63)/127)
        assert!((img.data[2] as i16 - 128).abs() <= 1);
    }

    #[test]
    fn reject_invalid_magic()
    {
        let data = b"P5\n1 1\n255\n\x80";
        let result = read_ppm(Cursor::new(data.as_ref()));
        assert!(result.is_err());
    }

    #[test]
    fn reject_zero_dimension()
    {
        let data = b"P3\n0 1\n255\n";
        let result = read_ppm(Cursor::new(data.as_ref()));
        assert!(result.is_err());
    }

    #[test]
    fn reject_zero_maxval()
    {
        let data = b"P3\n1 1\n0\n0 0 0\n";
        let result = read_ppm(Cursor::new(data.as_ref()));
        assert!(result.is_err());
    }

    #[test]
    fn reject_sample_exceeding_maxval()
    {
        let data = b"P3\n1 1\n100\n101 0 0\n";
        let result = read_ppm(Cursor::new(data.as_ref()));
        assert!(result.is_err());
    }

    #[test]
    fn read_p6_16bit_maxval()
    {
        // maxval=1000, 2 bytes per sample
        let mut data = Vec::new();
        data.extend_from_slice(b"P6\n1 1\n1000\n");
        // 1 pixel * 3 channels * 2 bytes = 6 bytes
        // Sample = 500 (big-endian: 0x01F4)
        data.extend_from_slice(&[0x01, 0xF4, 0x00, 0x00, 0x03, 0xE8]);

        let img = read_ppm(Cursor::new(data)).unwrap();
        assert_eq!(img.data.len(), 3);
        // 500 * 255 / 1000 = 127.5 -> 128
        assert!((img.data[0] as i16 - 128).abs() <= 1);
        // 0 * 255 / 1000 = 0
        assert_eq!(img.data[1], 0);
        // 1000 * 255 / 1000 = 255
        assert_eq!(img.data[2], 255);
    }
}
