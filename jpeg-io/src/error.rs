use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error
{
    Io(io::Error),

    InvalidFormat(String),

    ValueOutOfRange(String),
}

impl fmt::Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
            Error::ValueOutOfRange(msg) => write!(f, "value out of range: {}", msg),
        }
    }
}

impl std::error::Error for Error
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
    {
        match self
        {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error
{
    fn from(e: io::Error) -> Self
    {
        Error::Io(e)
    }
}

pub type Result<T> = core::result::Result<T, Error>;
