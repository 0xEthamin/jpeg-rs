//! Image source file readers for the jpeg-rs encoder.
//!
//! Currently supports the PPM (Portable Pixmap) format, which is a simple
//! uncompressed image format commonly used as an intermediate representation
//! in image processing pipelines.

pub mod error;
pub mod ppm;

pub use error::Error;
