//! # Entropy
//!
//! JPEG supports two entropy coding methods (T.81 §1):
//!
//! 1. **Huffman coding** - variable-length prefix codes. Supported by
//!    all JPEG decoders (required for baseline). Implemented here.
//!
//! 2. **Arithmetic coding** - adaptive binary arithmetic coding. Offers
//!    ~5–10 % better compression but historically encumbered by patents
//!    (now expired). Not implemented in this encoder.
//!
//! This module provides Huffman table construction and Huffman encoding.

pub mod huffman_encoder;
pub mod huffman_table;
