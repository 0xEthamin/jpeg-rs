//! # Types
//!
//! Public types that form the encoder's API surface.
//!
//! These types describe the *input* to the encoder: the raw image and the
//! parameters that control how it is compressed. They are intentionally
//! decoupled from the internal representation so that the encoder
//! implementation can evolve without breaking callers.

use crate::error::{Error, Result};

/// The color model of the source image pixels.
///
/// # Background - why color space matters
///
/// A digital image is a grid of *samples*. Each pixel is described by one or
/// more *components* (channels). The JPEG standard (ITU-T T.81) is
/// component-agnostic: it compresses each component independently after an
/// optional subsampling step. However, the *choice* of color space has a
/// dramatic impact on compression efficiency.
///
/// Human vision is far more sensitive to brightness (luminance) than to color
/// (chrominance). Converting RGB to YCbCr separates luminance (Y) from
/// chrominance (Cb, Cr), allowing the chrominance planes to be subsampled
/// (reduced in resolution) with little perceptible loss. This is one of the key
/// insight that makes JPEG efficient for photographic images.
///
/// The encoder performs the RGB -> YCbCr conversion internally using the
/// coefficients defined by CCIR 601 (the same ones referenced in the JFIF
/// specification, version 1.02):
///
/// ```text
///   Y  = 0.299*R + 0.587*G + 0.114*B
///   Cb = -0.1687*R - 0.3313*G + 0.5*B + 128
///   Cr = 0.5*R - 0.4187*G - 0.0813*B + 128
/// ```
///
/// The +128 offset shifts Cb/Cr from the signed range [ -128, 127] into the
/// unsigned range [0, 255], which is what the encoder expects before the
/// level shift (T.81 §A.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace
{
    /// Three-component RGB.
    ///
    /// The pixel buffer must contain interleaved R, G, B triplets, one byte
    /// per component, in row-major order. The encoder converts to YCbCr
    /// internally.
    Rgb,

    /// Single-component grayscale.
    ///
    /// The pixel buffer contains one byte per pixel. No color conversion
    /// is performed; the samples are treated directly as the Y (luminance)
    /// component.
    Grayscale,
}

impl ColorSpace
{
    /// Number of components (channels) for this color space.
    #[inline]
    #[must_use]
    pub const fn num_components(self) -> u8
    {
        match self
        {
            Self::Rgb => 3,
            Self::Grayscale => 1,
        }
    }
}

/// Chroma subsampling mode.
///
/// # Background - what is chroma subsampling?
///
/// After converting to YCbCr, the two chrominance planes (Cb and Cr) can be
/// reduced in spatial resolution before compression. This is called *chroma
/// subsampling* and exploits the fact that human vision resolves color detail
/// at roughly half the spatial frequency of luminance detail.
///
/// The notation `J:a:b` (e.g. 4:2:0) is an industry convention:
///
/// | Mode  | Cb/Cr horizontal | Cb/Cr vertical | Compression gain |
/// |-------|------------------|----------------|------------------|
/// | 4:4:4 | full             | full           | none             |
/// | 4:2:2 | half             | full           | ~33 %            |
/// | 4:2:0 | half             | half           | ~50 %            |
///
/// In the JPEG bitstream, subsampling is expressed through *sampling factors*
/// (T.81 §A.1.1). Each component has a horizontal factor H and a vertical
/// factor V. The luminance (Y) component always has the largest factors.
/// The chrominance factors are smaller by the subsampling ratio.
///
/// # How sampling factors map to the bitstream
///
/// The encoder writes these factors into the SOF0 frame header (T.81 §B.2.2)
/// and uses them to determine the structure of each Minimum Coded Unit (MCU,
/// T.81 §A.2.1).
///
/// For example, with 4:2:0 subsampling the MCU contains:
/// - 4 luminance blocks (2 horizontal * 2 vertical)
/// - 1 Cb block
/// - 1 Cr block
///
/// giving a total of 6 blocks per MCU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling
{
    /// 4:4:4 - no subsampling. All components at full resolution.
    ///
    /// Sampling factors: Y(1*1), Cb(1*1), Cr(1*1).
    S444,

    /// 4:2:2 - horizontal subsampling only.
    ///
    /// Cb and Cr have half the horizontal resolution of Y.
    /// Sampling factors: Y(2*1), Cb(1*1), Cr(1*1).
    S422,

    /// 4:2:0 - horizontal and vertical subsampling.
    ///
    /// Cb and Cr have half the resolution in both dimensions.
    /// Sampling factors: Y(2*2), Cb(1*1), Cr(1*1).
    /// This is the most common mode for photographic JPEG.
    S420,
}

impl Subsampling
{
    /// Returns the sampling factors for all three components as a tuple:
    /// `(H_y, V_y, H_cb, V_cb, H_cr, V_cr)`.
    ///
    /// These correspond to the H and V parameters written into the SOF0
    /// frame header (T.81 §B.2.2, Figure B.3).
    #[inline]
    #[must_use]
    pub const fn factors(self) -> (u8, u8, u8, u8, u8, u8)
    {
        match self
        {
            Self::S444 => (1, 1, 1, 1, 1, 1),
            Self::S422 => (2, 1, 1, 1, 1, 1),
            Self::S420 => (2, 2, 1, 1, 1, 1),
        }
    }

    /// Maximum horizontal sampling factor across all components (Hmax).
    ///
    /// Used to compute component dimensions per T.81 §A.1.1:
    /// x = ⌈X * H / Hmax⌉
    #[inline]
    #[must_use]
    pub const fn h_max(self) -> u8
    {
        match self
        {
            Self::S444 => 1,
            Self::S422 | Self::S420 => 2,
        }
    }

    /// Maximum vertical sampling factor across all components (Vmax).
    ///
    /// Used to compute component dimensions per T.81 §A.1.1:
    /// y = ⌈Y * V / Vmax⌉
    #[inline]
    #[must_use]
    pub const fn v_max(self) -> u8
    {
        match self
        {
            Self::S444 | Self::S422 => 1,
            Self::S420 => 2,
        }
    }
}

/// Parameters that control the JPEG encoding process.
///
/// All fields have sensible defaults via [`Default`]. The most commonly
/// adjusted parameter is [`quality`](Self::quality).
///
/// # Example
///
/// ```
/// use jpeg_core::EncoderConfig;
///
/// let config = EncoderConfig 
/// {
///     quality: 75,
///     ..EncoderConfig::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct EncoderConfig
{
    /// Quality factor in the range 1..=100.
    ///
    /// Controls the quantization step sizes. Higher values produce larger
    /// files with fewer compression artefacts. The relationship between
    /// quality and step size follows the IJG formula:
    ///
    /// ```text
    ///   if quality < 50: scale = 5000 / quality
    ///   if quality >= 50: scale = 200 - 2 * quality
    ///
    ///   Q_scaled[i] = clamp((Q_base[i] * scale + 50) / 100, 1, 255)
    /// ```
    ///
    /// where `Q_base` is one of the example tables from T.81 Annex K (Tables
    /// K.1 and K.2).
    pub quality: u8,

    /// Chroma subsampling mode. See [`Subsampling`] for details.
    pub subsampling: Subsampling,

    /// Restart interval in MCUs (0 = disabled).
    ///
    /// When non-zero, the encoder inserts restart markers (RSTm, T.81
    /// §B.2.1) every `restart_interval` MCUs. Restart markers allow a
    /// decoder to resynchronise after a transmission error and enable
    /// parallel decoding of independent restart intervals.
    ///
    /// The restart marker index m cycles from 0 to 7 (T.81 Table B.1).
    /// At each restart boundary the DC predictions for all components are
    /// reset to zero.
    pub restart_interval: u16,

    /// JFIF density unit (0 = no unit / aspect ratio only, 1 = dots/inch,
    /// 2 = dots/cm).
    ///
    /// Written into the APP0/JFIF marker segment.
    pub density_units: u8,

    /// Horizontal pixel density. Interpretation depends on `density_units`.
    pub x_density: u16,

    /// Vertical pixel density. Interpretation depends on `density_units`.
    pub y_density: u16,
}

impl Default for EncoderConfig
{
    fn default() -> Self
    {
        Self
        {
            quality: 85,
            subsampling: Subsampling::S420,
            restart_interval: 0,
            density_units: 0,
            x_density: 1,
            y_density: 1,
        }
    }
}

impl EncoderConfig
{
    /// Validates the configuration, returning an error for out-of-range
    /// values.
    pub fn validate(&self) -> Result<()>
    {
        if self.quality == 0 || self.quality > 100
        {
            return Err(Error::InvalidQuality(self.quality));
        }
        Ok(())
    }
}

/// A raw (uncompressed) image ready to be encoded.
///
/// This is a *borrowed* view over caller-owned pixel data - the encoder does
/// not take ownership of the buffer.
///
/// # Layout
///
/// * **RGB**: the buffer contains interleaved R, G, B bytes in row-major
///   order. For an image of width W and height H, the buffer must be
///   exactly W * H * 3 bytes long.
///
/// * **Grayscale**: one byte per pixel, W * H bytes total.
///
/// Row 0 is the top of the image; column 0 is the left edge. This matches
/// the orientation defined in T.81 §A.1.4 and Figure A.1.
#[derive(Debug, Clone)]
pub struct RawImage<'a>
{
    /// Image width in pixels (number of samples per line).
    ///
    /// Stored as `u32` for ergonomic arithmetic but must fit in 16 bits
    /// (1..=65535) per T.81 §B.2.2 parameter X.
    pub width: u32,

    /// Image height in pixels (number of lines).
    ///
    /// Must fit in 16 bits (1..=65535) per T.81 §B.2.2 parameter Y.
    pub height: u32,

    /// The color model of the pixel data.
    pub color_space: ColorSpace,

    /// The raw pixel data. See [Layout](#layout) above.
    pub data: &'a [u8],
}

impl<'a> RawImage<'a>
{
    /// Validates dimensions and buffer size.
    ///
    /// # Errors
    ///
    /// * [`Error::InvalidDimensions`] if width or height is 0 or > 65535.
    /// * [`Error::BufferSizeMismatch`] if the buffer length does not match
    ///   width * height * components.
    pub fn validate(&self) -> Result<()>
    {
        if self.width == 0
            || self.height == 0
            || self.width > 65535
            || self.height > 65535
        {
            return Err
            (
                Error::InvalidDimensions 
                {
                    width: self.width,
                    height: self.height,
                }
            );
        }

        let expected = self.width as usize
            * self.height as usize
            * self.color_space.num_components() as usize;

        if self.data.len() != expected
        {
            return Err
            (
                Error::BufferSizeMismatch 
                {
                    expected,
                    actual: self.data.len(),
                }
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn rgb_has_3_components()
    {
        assert_eq!(ColorSpace::Rgb.num_components(), 3);
    }

    #[test]
    fn grayscale_has_1_component()
    {
        assert_eq!(ColorSpace::Grayscale.num_components(), 1);
    }

    #[test]
    fn s444_factors()
    {
        let (hy, vy, hcb, vcb, hcr, vcr) = Subsampling::S444.factors();
        assert_eq!((hy, vy), (1, 1));
        assert_eq!((hcb, vcb), (1, 1));
        assert_eq!((hcr, vcr), (1, 1));
    }

    #[test]
    fn s422_factors()
    {
        let (hy, vy, hcb, vcb, hcr, vcr) = Subsampling::S422.factors();
        assert_eq!((hy, vy), (2, 1));
        assert_eq!((hcb, vcb), (1, 1));
        assert_eq!((hcr, vcr), (1, 1));
    }

    #[test]
    fn s420_factors()
    {
        let (hy, vy, hcb, vcb, hcr, vcr) = Subsampling::S420.factors();
        assert_eq!((hy, vy), (2, 2));
        assert_eq!((hcb, vcb), (1, 1));
        assert_eq!((hcr, vcr), (1, 1));
    }

    #[test]
    fn h_max_values()
    {
        assert_eq!(Subsampling::S444.h_max(), 1);
        assert_eq!(Subsampling::S422.h_max(), 2);
        assert_eq!(Subsampling::S420.h_max(), 2);
    }

    #[test]
    fn v_max_values()
    {
        assert_eq!(Subsampling::S444.v_max(), 1);
        assert_eq!(Subsampling::S422.v_max(), 1);
        assert_eq!(Subsampling::S420.v_max(), 2);
    }

    #[test]
    fn default_config()
    {
        let c = EncoderConfig::default();
        assert_eq!(c.quality, 85);
        assert_eq!(c.subsampling, Subsampling::S420);
        assert_eq!(c.restart_interval, 0);
    }

    #[test]
    fn validate_quality_bounds()
    {
        assert!(EncoderConfig { quality: 0, ..EncoderConfig::default() }.validate().is_err());
        assert!(EncoderConfig { quality: 1, ..EncoderConfig::default() }.validate().is_ok());
        assert!(EncoderConfig { quality: 100, ..EncoderConfig::default() }.validate().is_ok());
        assert!(EncoderConfig { quality: 101, ..EncoderConfig::default() }.validate().is_err());
    }

    #[test]
    fn validate_correct_rgb()
    {
        let data = vec![0u8; 10 * 10 * 3];
        let img = RawImage { width: 10, height: 10, color_space: ColorSpace::Rgb, data: &data };
        assert!(img.validate().is_ok());
    }

    #[test]
    fn validate_correct_grayscale()
    {
        let data = vec![0u8; 10 * 10];
        let img = RawImage { width: 10, height: 10, color_space: ColorSpace::Grayscale, data: &data };
        assert!(img.validate().is_ok());
    }

    #[test]
    fn validate_zero_width()
    {
        let img = RawImage { width: 0, height: 10, color_space: ColorSpace::Rgb, data: &[] };
        assert!(matches!(img.validate(), Err(Error::InvalidDimensions { .. })));
    }

    #[test]
    fn validate_zero_height()
    {
        let img = RawImage { width: 10, height: 0, color_space: ColorSpace::Rgb, data: &[] };
        assert!(matches!(img.validate(), Err(Error::InvalidDimensions { .. })));
    }

    #[test]
    fn validate_too_large()
    {
        let img = RawImage { width: 65536, height: 1, color_space: ColorSpace::Rgb, data: &[] };
        assert!(matches!(img.validate(), Err(Error::InvalidDimensions { .. })));
    }

    #[test]
    fn validate_buffer_too_small()
    {
        let data = vec![0u8; 10]; // Way too small for 10*10 RGB
        let img = RawImage { width: 10, height: 10, color_space: ColorSpace::Rgb, data: &data };
        assert!(matches!(img.validate(), Err(Error::BufferSizeMismatch { .. })));
    }

    #[test]
    fn validate_buffer_too_large()
    {
        let data = vec![0u8; 1000]; // Too large for 10*10 RGB (=300)
        let img = RawImage { width: 10, height: 10, color_space: ColorSpace::Rgb, data: &data };
        assert!(matches!(img.validate(), Err(Error::BufferSizeMismatch { .. })));
    }

    #[test]
    fn validate_max_dimensions()
    {
        // 65535*1 grayscale should be valid (if we had that much memory).
        let data = vec![0u8; 65535];
        let img = RawImage { width: 65535, height: 1, color_space: ColorSpace::Grayscale, data: &data };
        assert!(img.validate().is_ok());
    }
}
