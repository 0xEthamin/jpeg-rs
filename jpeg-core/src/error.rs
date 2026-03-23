//! #Errors
//!
//! Error types for the JPEG encoder.
//!
//! Every fallible operation in the encoder returns [`Result<T>`], which uses
//! [`Error`] as its error variant. In a production context these errors must
//! be propagated - never swallowed - so that callers can distinguish between
//! "the input was invalid" and "there is a bug in the encoder".

use core::fmt;

/// All errors that may occur during JPEG encoding.
///
/// Errors are split into two families:
///
/// * **Validation errors** - the caller provided invalid input. These are
///   expected in normal operation (bad image dimensions, out-of-range quality,
///   buffer size mismatch, …).
///
/// * **Internal errors** - invariants that should always hold were violated.
///   These indicate a bug in the encoder itself and should be reported
///   upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error
{
    /// The image dimensions are outside the range allowed by JPEG.
    ///
    /// Per T.81 §B.2.2 (Frame header), the number of lines Y and the number
    /// of samples per line X are each stored as 16-bit unsigned integers, so
    /// both width and height must be in the range 1..=65535.
    InvalidDimensions
    {
        width: u32,
        height: u32,
    },

    /// The quality factor is outside the allowed range 1..=100.
    ///
    /// The quality factor controls quantization step sizes. A value of 1
    /// produces the smallest file (lowest quality) and 100 the largest
    /// (highest quality). The scaling formula used is the one popularised
    /// by the Independent JPEG Group (IJG): for q < 50 the scale factor is
    /// 5000/q, for q >= 50 it is 200 - 2q.
    InvalidQuality(u8),

    /// The pixel buffer does not have the expected number of bytes.
    ///
    /// For an RGB image of width W and height H, exactly W * H * 3 bytes
    /// are required. For a grayscale image, W * H bytes are required.
    BufferSizeMismatch
    {
        expected: usize,
        actual: usize,
    },

    /// A Huffman code was requested for a symbol that has no code assigned.
    ///
    /// This should never happen if frequency collection and table construction
    /// are correct. Its presence signals an internal bug.
    MissingHuffmanCode
    {
        /// The symbol value (RS byte for AC, SSSS for DC).
        symbol: u16,
        /// A human-readable context, e.g. "DC category 5" or "AC run=3 cat=2".
        context: String,
    },

    /// A bitstream operation was called with invalid parameters or in an
    /// invalid state.
    ///
    /// This covers two situations:
    ///
    /// * `write_bits` was called with `count > 16`.
    /// * `write_raw_byte` / `write_raw_bytes` / `write_u16_be` was called
    ///   while there are unflushed entropy-coded bits in the accumulator.
    ///   Raw bytes must only be written when the bit buffer is empty (i.e.
    ///   between entropy-coded segments).
    BitstreamError(String),

    /// A generic internal error with a descriptive message.
    ///
    /// Used as a catch-all for invariant violations that do not warrant their
    /// own variant. The message should be precise enough to locate the bug.
    Internal(String),
}

impl fmt::Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Self::InvalidDimensions { width, height } =>
            {
                write!
                (
                    f,
                    "invalid image dimensions: {}x{} \
                     (each must be 1..=65535, per T.81 §B.2.2)",
                    width, height,
                )
            }
            Self::InvalidQuality(q) =>
            {
                write!(f, "quality must be 1..=100, got {}", q)
            }
            Self::BufferSizeMismatch { expected, actual } =>
            {
                write!
                (
                    f,
                    "buffer size mismatch: expected {} bytes, got {}",
                    expected, actual,
                )
            }
            Self::MissingHuffmanCode { symbol, context } =>
            {
                write!
                (
                    f,
                    "no Huffman code assigned for symbol 0x{:04X} ({})",
                    symbol, context,
                )
            }
            Self::BitstreamError(msg) =>
            {
                write!(f, "bitstream error: {}", msg)
            }
            Self::Internal(msg) =>
            {
                write!(f, "internal encoder error: {}", msg)
            }
        }
    }
}

impl std::error::Error for Error {}

/// A specialized `Result` type for JPEG encoding operations.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn display_invalid_dimensions()
    {
        let e = Error::InvalidDimensions { width: 0, height: 100 };
        let msg = format!("{}", e);
        assert!(msg.contains("0x100"));
        assert!(msg.contains("T.81"));
    }

    #[test]
    fn display_invalid_quality()
    {
        let e = Error::InvalidQuality(0);
        let msg = format!("{}", e);
        assert!(msg.contains("1..=100"));
        assert!(msg.contains("0"));
    }

    #[test]
    fn display_buffer_mismatch()
    {
        let e = Error::BufferSizeMismatch { expected: 300, actual: 100 };
        let msg = format!("{}", e);
        assert!(msg.contains("300"));
        assert!(msg.contains("100"));
    }

    #[test]
    fn display_missing_huffman()
    {
        let e = Error::MissingHuffmanCode 
        {
            symbol: 0xF0,
            context: "AC ZRL".to_string(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("00F0"));
        assert!(msg.contains("AC ZRL"));
    }

    #[test]
    fn display_bitstream_error()
    {
        let e = Error::BitstreamError("unflushed bits".to_string());
        let msg = format!("{}", e);
        assert!(msg.contains("bitstream error"));
        assert!(msg.contains("unflushed bits"));
    }

    #[test]
    fn display_internal()
    {
        let e = Error::Internal("something broke".to_string());
        assert!(format!("{}", e).contains("something broke"));
    }

    #[test]
    fn error_implements_std_error()
    {
        let e: Box<dyn std::error::Error> = Box::new(Error::InvalidQuality(42));
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn error_is_eq()
    {
        assert_eq!(Error::InvalidQuality(50), Error::InvalidQuality(50));
        assert_ne!(Error::InvalidQuality(50), Error::InvalidQuality(60));
    }
}