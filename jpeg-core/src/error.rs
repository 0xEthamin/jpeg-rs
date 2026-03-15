use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error
{
    InvalidDimensions
    {
        width: u32,
        height: u32,
    },

    InvalidQuality(u8),

    BufferSizeMismatch
    {
        expected: usize,
        actual: usize,
    },

    InvalidSamplingFactor
    {
        component: u8,
        horizontal: u8,
        vertical: u8,
    },

    Internal(&'static str),
}

impl fmt::Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Error::InvalidDimensions { width, height } =>
            {
                write!(f, "invalid image dimensions: {}x{}", width, height)
            }
            Error::InvalidQuality(q) =>
            {
                write!(f, "quality must be 1..=100, got {}", q)
            }
            Error::BufferSizeMismatch { expected, actual } =>
            {
                write!
                (
                    f,
                    "buffer size mismatch: expected {} bytes, got {}",
                    expected, actual
                )
            }
            Error::InvalidSamplingFactor { component, horizontal, vertical } =>
            {
                write!
                (
                    f,
                    "invalid sampling factor for component {}: {}x{}",
                    component, horizontal, vertical
                )
            }
            Error::Internal(msg) =>
            {
                write!(f, "internal encoder error: {}", msg)
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
