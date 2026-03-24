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

#[cfg(test)]
mod tests
{
    use super::*;
    use std::io;

    #[test]
    fn display_io_error()
    {
        let inner = io::Error::new(io::ErrorKind::NotFound, "file gone");
        let e = Error::Io(inner);
        let msg = format!("{}", e);
        assert!(msg.contains("file gone"));
    }

    #[test]
    fn display_invalid_format()
    {
        let e = Error::InvalidFormat("bad magic".into());
        let msg = format!("{}", e);
        assert!(msg.contains("bad magic"));
    }

    #[test]
    fn display_value_out_of_range()
    {
        let e = Error::ValueOutOfRange("maxval 99999".into());
        let msg = format!("{}", e);
        assert!(msg.contains("maxval 99999"));
    }

    #[test]
    fn source_returns_inner_io_error()
    {
        let inner = io::Error::new(io::ErrorKind::BrokenPipe, "broken");
        let e = Error::Io(inner);
        let src = std::error::Error::source(&e);
        assert!(src.is_some());
    }

    #[test]
    fn source_returns_none_for_non_io()
    {
        let e = Error::InvalidFormat("x".into());
        assert!(std::error::Error::source(&e).is_none());

        let e = Error::ValueOutOfRange("y".into());
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn from_io_error()
    {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "nope");
        let e: Error = io_err.into();
        assert!(matches!(e, Error::Io(_)));
    }
}