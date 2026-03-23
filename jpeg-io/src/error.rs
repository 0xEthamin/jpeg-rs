//! Error types for image I/O operations (e.g. PPM parsing).

use std::fmt;
use std::io;

/// Errors that can occur when reading image source files.
#[derive(Debug)]
pub enum Error
{
    /// An underlying I/O error (e.g. file not found, read failure).
    Io(io::Error),

    /// The file format is invalid or unsupported.
    InvalidFormat(String),

    /// A numeric value in the file is outside the allowed range.
    ValueOutOfRange(String),
}

impl fmt::Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
            Self::ValueOutOfRange(msg) => write!(f, "value out of range: {}", msg),
        }
    }
}

impl std::error::Error for Error
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
    {
        match self
        {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error
{
    fn from(e: io::Error) -> Self
    {
        Self::Io(e)
    }
}

/// A specialized `Result` type for image I/O operations.
pub type Result<T> = core::result::Result<T, Error>;
