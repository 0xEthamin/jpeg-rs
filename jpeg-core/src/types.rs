use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace
{
    Rgb,
    Grayscale,
}

impl ColorSpace
{
    #[inline]
    pub const fn num_components(self) -> u8
    {
        match self
        {
            ColorSpace::Rgb => 3,
            ColorSpace::Grayscale => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling
{
    S444,
    S422,
    S420,
}

impl Subsampling
{
    #[inline]
    pub const fn factors(self) -> (u8, u8, u8, u8, u8, u8)
    {
        match self
        {
            Subsampling::S444 => (1, 1, 1, 1, 1, 1),
            Subsampling::S422 => (2, 1, 1, 1, 1, 1),
            Subsampling::S420 => (2, 2, 1, 1, 1, 1),
        }
    }

    #[inline]
    pub const fn h_max(self) -> u8
    {
        match self
        {
            Subsampling::S444 => 1,
            Subsampling::S422 | Subsampling::S420 => 2,
        }
    }

    #[inline]
    pub const fn v_max(self) -> u8
    {
        match self
        {
            Subsampling::S444 | Subsampling::S422 => 1,
            Subsampling::S420 => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncoderConfig
{
    pub quality: u8,
    pub subsampling: Subsampling,
    pub restart_interval: u16,
    pub density_units: u8,
    pub x_density: u16,
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
    pub fn validate(&self) -> Result<()>
    {
        if self.quality == 0 || self.quality > 100
        {
            return Err(Error::InvalidQuality(self.quality));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RawImage<'a>
{
    pub width: u32,
    pub height: u32,
    pub color_space: ColorSpace,
    pub data: &'a [u8],
}

impl<'a> RawImage<'a>
{
    pub fn validate(&self) -> Result<()>
    {
        if self.width == 0
            || self.height == 0
            || self.width > 65535
            || self.height > 65535
        {
            return Err(Error::InvalidDimensions {
                width: self.width,
                height: self.height,
            });
        }

        let expected = self.width as usize
            * self.height as usize
            * self.color_space.num_components() as usize;

        if self.data.len() != expected
        {
            return Err(Error::BufferSizeMismatch {
                expected,
                actual: self.data.len(),
            });
        }

        Ok(())
    }
}
