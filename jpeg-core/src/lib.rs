//! # jpeg-core
//!
//! This crate implements a **baseline sequential DCT** JPEG encoder as
//! specified by ITU-T Recommendation T.81 (ISO/IEC 10918-1).
//!
//! ## What is JPEG?
//!
//! JPEG (Joint Photographic Experts Group) is a lossy compression method
//! for continuous-tone images. It achieves high compression
//! ratios by exploiting two properties of human vision:
//!
//! 1. **Luminance vs. chrominance sensitivity** - we perceive brightness
//!    detail more acutely than color detail. JPEG separates brightness (Y)
//!    from color (Cb, Cr) and can reduce the resolution of the color
//!    channels with little perceptible loss (*chroma subsampling*).
//!
//! 2. **Spatial frequency sensitivity** - we are less sensitive to fine
//!    detail (high spatial frequencies) than to broad patterns (low
//!    frequencies). JPEG transforms each 8*8 block into the frequency
//!    domain using the *Discrete Cosine Transform* (DCT), then *quantizes*
//!    the high-frequency coefficients more aggressively. This is where the
//!    lossy compression actually happens.
//!
//! ## Encoding pipeline
//!
//! The encoder processes an image through these stages:
//!
//! | Stage | Module | T.81 reference |
//! |-------|--------|----------------|
//! | Color conversion (RGB -> YCbCr) | [`color`] | JFIF 1.02 |
//! | Chroma subsampling | [`sampling`] | §A.1.1 |
//! | 8*8 block extraction + level shift | [`block`] | §A.1.3, §A.3.1 |
//! | Forward DCT | [`dct`] | §A.3.3 |
//! | Quantization + zig-zag reorder | [`quantize`] | §A.3.4, §A.3.6 |
//! | Huffman table construction | [`entropy::huffman_table`] | Annex C, §K.2 |
//! | Huffman encoding | [`entropy::huffman_encoder`] | §F.1.2 |
//! | Marker / bitstream assembly | [`marker`], [`bitstream`] | Annex B |
//!
//! ## Supported features
//!
//! - Baseline sequential DCT (SOF0) - the most widely supported JPEG mode.
//! - 8-bit sample precision.
//! - Optimised Huffman tables (built from actual image statistics).
//! - Chroma subsampling: 4:4:4, 4:2:2, 4:2:0.
//! - Restart markers with configurable interval.
//! - Grayscale (1-component) and color (3-component YCbCr) images.
//! - JFIF 1.02 interchange format.
//!
//! ## Not implemented
//!
//! - Progressive DCT (SOF2) - T.81 Annex G.
//! - Arithmetic entropy coding - T.81 Annex D.
//! - Lossless mode - T.81 Annex H.
//! - Hierarchical mode - T.81 Annex J.
//! - 12-bit sample precision (SOF1).
//! - EXIF / APP1 metadata.
//!
//! ## Quick start
//!
//! ```no_run
//! use jpeg_core::{encode, ColorSpace, EncoderConfig, RawImage};
//!
//! // Prepare a 64*64 mid-gray RGB image.
//! let pixels = vec![128u8; 64 * 64 * 3];
//! let image = RawImage 
//! {
//!     width: 64,
//!     height: 64,
//!     color_space: ColorSpace::Rgb,
//!     data: &pixels,
//! };
//!
//! let config = EncoderConfig 
//! {
//!     quality: 85,
//!     ..EncoderConfig::default()
//! };
//!
//! let jpeg_bytes = encode(&image, &config).expect("encoding failed");
//! // `jpeg_bytes` is a valid JPEG file that can be written to disk.
//! ```

pub mod bitstream;
pub mod block;
pub mod color;
pub mod dct;
pub mod encoder;
pub mod entropy;
pub mod error;
pub mod marker;
pub mod quantize;
pub mod sampling;
pub mod types;

pub use encoder::encode;
pub use error::Error;
pub use types::{ColorSpace, EncoderConfig, RawImage, Subsampling};
