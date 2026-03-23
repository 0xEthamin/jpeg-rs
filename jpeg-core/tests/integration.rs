use jpeg_core::{encode, ColorSpace, EncoderConfig, RawImage, Subsampling};

#[test]
fn encode_color_all_configs() 
{
    let w = 16u32;
    let h = 16u32;
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h 
    {
        for x in 0..w 
        {
            let r = (x * 255 / (w - 1)) as u8;
            let g = (y * 255 / (h - 1)) as u8;
            pixels.push(r);
            pixels.push(g);
            pixels.push(128u8);
        }
    }
    let image = RawImage { width: w, height: h, color_space: ColorSpace::Rgb, data: &pixels };

    for quality in [1, 50, 85, 100] 
    {
        for sub in [Subsampling::S444, Subsampling::S422, Subsampling::S420] 
        {
            let config = EncoderConfig { quality, subsampling: sub, ..EncoderConfig::default() };
            let data = encode(&image, &config).unwrap();
            assert_eq!(data[0..2], [0xFF, 0xD8], "SOI marker");
            assert_eq!(data[data.len()-2..], [0xFF, 0xD9], "EOI marker");
        }
    }
}

#[test]
fn encode_grayscale() 
{
    let w = 16u32;
    let h = 16u32;
    let pixels: Vec<u8> = (0..w*h).map(|i| (i % 256) as u8).collect();
    let image = RawImage { width: w, height: h, color_space: ColorSpace::Grayscale, data: &pixels };
    let data = encode(&image, &EncoderConfig::default()).unwrap();
    assert_eq!(data[0..2], [0xFF, 0xD8]);
    assert_eq!(data[data.len()-2..], [0xFF, 0xD9]);
}

#[test]
fn encode_with_restart_markers() 
{
    let w = 32u32;
    let h = 32u32;
    let pixels = vec![100u8; (w * h * 3) as usize];
    let image = RawImage { width: w, height: h, color_space: ColorSpace::Rgb, data: &pixels };
    let config = EncoderConfig { restart_interval: 2, ..EncoderConfig::default() };
    let data = encode(&image, &config).unwrap();
    assert_eq!(data[0..2], [0xFF, 0xD8]);
    assert_eq!(data[data.len()-2..], [0xFF, 0xD9]);
    // Verify at least one RST marker is present (0xFFD0..0xFFD7)
    let has_rst = data.windows(2).any(|w| w[0] == 0xFF && (0xD0..=0xD7).contains(&w[1]));
    assert!(has_rst, "restart markers should be present when restart_interval > 0");
}

#[test]
fn encode_non_multiple_of_8() 
{
    // Test with dimensions that are not multiples of 8
    for (w, h) in [(1, 1), (7, 7), (9, 9), (13, 11), (15, 17), (1, 100), (100, 1)] 
    {
        let pixels = vec![128u8; (w * h * 3) as usize];
        let image = RawImage { width: w, height: h, color_space: ColorSpace::Rgb, data: &pixels };
        let data = encode(&image, &EncoderConfig::default()).unwrap();
        assert_eq!(data[0..2], [0xFF, 0xD8], "SOI for {}x{}", w, h);
        assert_eq!(data[data.len()-2..], [0xFF, 0xD9], "EOI for {}x{}", w, h);
    }
}

#[test]
fn validation_rejects_invalid_input() 
{
    let bad = RawImage { width: 0, height: 10, color_space: ColorSpace::Rgb, data: &[] };
    assert!(bad.validate().is_err());

    let bad = RawImage { width: 10, height: 10, color_space: ColorSpace::Rgb, data: &[0; 10] };
    assert!(bad.validate().is_err());

    let config = EncoderConfig { quality: 0, ..EncoderConfig::default() };
    assert!(config.validate().is_err());

    let config = EncoderConfig { quality: 101, ..EncoderConfig::default() };
    assert!(config.validate().is_err());
}

#[test]
fn encode_full_pipeline_validation() 
{
    // Verify that encode() itself rejects invalid input.
    let bad_image = RawImage { width: 0, height: 10, color_space: ColorSpace::Rgb, data: &[] };
    assert!(encode(&bad_image, &EncoderConfig::default()).is_err());

    let pixels = vec![0u8; 10 * 10 * 3];
    let image = RawImage { width: 10, height: 10, color_space: ColorSpace::Rgb, data: &pixels };
    let bad_config = EncoderConfig { quality: 0, ..EncoderConfig::default() };
    assert!(encode(&image, &bad_config).is_err());
}

#[test]
fn encode_solid_color_images() 
{
    // Solid color images are a good stress test for Huffman tables
    // because most AC coefficients will be zero.
    let w = 32u32;
    let h = 32u32;
    for color in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (0, 0, 0), (255, 255, 255)] 
    {
        let mut pixels = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..w*h 
        {
            pixels.push(color.0);
            pixels.push(color.1);
            pixels.push(color.2);
        }
        let image = RawImage { width: w, height: h, color_space: ColorSpace::Rgb, data: &pixels };
        let data = encode(&image, &EncoderConfig::default()).unwrap();
        assert_eq!(data[0..2], [0xFF, 0xD8]);
        assert_eq!(data[data.len()-2..], [0xFF, 0xD9]);
    }
}

#[test]
fn encode_grayscale_with_restart() 
{
    let w = 24u32;
    let h = 24u32;
    let pixels: Vec<u8> = (0..w*h).map(|i| (i % 256) as u8).collect();
    let image = RawImage { width: w, height: h, color_space: ColorSpace::Grayscale, data: &pixels };
    let config = EncoderConfig { restart_interval: 3, ..EncoderConfig::default() };
    let data = encode(&image, &config).unwrap();
    assert_eq!(data[0..2], [0xFF, 0xD8]);
    assert_eq!(data[data.len()-2..], [0xFF, 0xD9]);
}

#[test]
fn higher_quality_produces_larger_file() 
{
    let w = 32u32;
    let h = 32u32;
    let mut pixels = Vec::new();
    for y in 0..h 
    {
        for x in 0..w 
        {
            pixels.push((x * 8) as u8);
            pixels.push((y * 8) as u8);
            pixels.push(128u8);
        }
    }
    let image = RawImage { width: w, height: h, color_space: ColorSpace::Rgb, data: &pixels };

    let low_q = encode(&image, &EncoderConfig { quality: 10, ..EncoderConfig::default() }).unwrap();
    let high_q = encode(&image, &EncoderConfig { quality: 95, ..EncoderConfig::default() }).unwrap();

    assert!
    (
        high_q.len() > low_q.len(),
        "quality 95 ({} bytes) should be larger than quality 10 ({} bytes)",
        high_q.len(), low_q.len(),
    );
}

#[test]
fn jfif_marker_present() 
{
    let pixels = vec![128u8; 8 * 8 * 3];
    let image = RawImage { width: 8, height: 8, color_space: ColorSpace::Rgb, data: &pixels };
    let data = encode(&image, &EncoderConfig::default()).unwrap();

    // After SOI (2 bytes), APP0 marker should be next: 0xFF 0xE0
    assert_eq!(data[2], 0xFF);
    assert_eq!(data[3], 0xE0);
    // JFIF identifier at offset 6..11
    assert_eq!(&data[6..11], b"JFIF\0");
}
