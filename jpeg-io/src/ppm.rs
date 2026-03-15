
use std::io::{BufRead, BufReader, Read};

use crate::error::{Error, Result};

pub struct PpmImage
{
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn read_ppm<R: Read>(reader: R) -> Result<PpmImage>
{
    let mut reader = BufReader::new(reader);

    let magic = read_token(&mut reader)?;
    match magic.as_str()
    {
        "P3" => read_p3(&mut reader),
        "P6" => read_p6(&mut reader),
        _ => Err(Error::InvalidFormat(
            format!("expected P3 or P6, got '{}'", magic),
        )),
    }
}

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
        let val = read_token(reader)?.parse::<u32>()
            .map_err(|e| Error::InvalidFormat(format!("bad sample: {}", e)))?;

        if val > maxval
        {
            return Err(Error::ValueOutOfRange(
                format!("sample {} exceeds maxval {}", val, maxval),
            ));
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

fn read_header_value<R: BufRead>(reader: &mut R, name: &str) -> Result<u32>
{
    read_token(reader)?
        .parse::<u32>()
        .map_err(|e| Error::InvalidFormat(format!("bad {}: {}", name, e)))
}

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

fn validate_header(width: u32, height: u32, maxval: u32) -> Result<()>
{
    if width == 0 || height == 0
    {
        return Err(Error::InvalidFormat(
            format!("zero dimension: {}x{}", width, height),
        ));
    }
    if width > 65535 || height > 65535
    {
        return Err(Error::InvalidFormat(
            format!("dimensions too large: {}x{}", width, height),
        ));
    }
    if maxval == 0 || maxval > 65535
    {
        return Err(Error::ValueOutOfRange(
            format!("maxval must be 1..=65535, got {}", maxval),
        ));
    }
    Ok(())
}

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

        if byte == b'#'
        {
            reader.consume(1);
            let mut discard = Vec::new();
            reader.read_until(b'\n', &mut discard)?;
            continue;
        }

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
