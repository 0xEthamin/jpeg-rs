//! Bitstream
//!
//! # Bit ordering (T.81 §C.3)
//!
//! JPEG packs Huffman codes and additional bits into bytes with the most
//! significant bit (MSB) first:
//!
//! This module accumulates bits in a 32-bit buffer and flushes complete
//! bytes to the output vector.
//!
//! # Byte stuffing (T.81 §F.1.2.3)
//!
//! Inside entropy-coded data, the byte value 0xFF is reserved as the first
//! byte of a two-byte marker code. To prevent a data byte of 0xFF from
//! being misinterpreted as a marker, the encoder must insert a zero byte
//! (0x00) immediately after every 0xFF data byte. This is called *byte
//! stuffing*:
//!
//! The decoder reverses this by discarding any 0x00 that follows 0xFF in
//! the entropy-coded segment.
//!
//! # Padding at segment boundaries (T.81 §B.1.1.5, Note 1)
//!
//! At the end of an entropy-coded segment (before a marker), any partially
//! filled byte is padded with 1-bits

use crate::error::{Error, Result};

/// A bit-oriented writer that accumulates JPEG entropy-coded data.
///
/// Bits are pushed MSB-first into an internal 32-bit buffer and flushed
/// to an output byte vector whenever 8 or more bits have accumulated.
/// Byte stuffing (0xFF -> 0xFF 0x00) is applied automatically.
pub struct BitWriter
{
    /// The output byte buffer.
    data: Vec<u8>,

    /// Accumulator for bits not yet flushed. Pending bits occupy the
    /// most-significant positions.
    bit_buffer: u32,

    /// Number of bits currently in `bit_buffer` (0..=31).
    bits_pending: u8,
}

impl BitWriter
{
    /// Create a new writer with a pre-allocated output buffer.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self
    {
        Self
        {
            data: Vec::with_capacity(capacity),
            bit_buffer: 0,
            bits_pending: 0,
        }
    }

    /// Write `count` bits from the least-significant end of `value`.
    ///
    /// `count` must be in the range 0..=16. If `count` is 0, this is a
    /// no-op.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BitstreamError`] if `count > 16`.
    #[inline]
    pub fn write_bits(&mut self, value: u32, count: u8) -> Result<()>
    {
        if count > 16
        {
            return Err
            (
                Error::BitstreamError
                (
                    format!("write_bits: count must be in 0..=16, got {}", count),
                )
            );
        }
        if count == 0
        {
            return Ok(());
        }

        // Mask off any extraneous high bits.
        let masked = value & ((1u32 << count) - 1);

        // Shift into the accumulator.
        self.bit_buffer = (self.bit_buffer << count) | masked;
        self.bits_pending += count;

        // Emit complete bytes.
        while self.bits_pending >= 8
        {
            self.bits_pending -= 8;
            let byte = ((self.bit_buffer >> self.bits_pending) & 0xFF) as u8;
            self.emit_byte(byte);
        }

        Ok(())
    }

    /// Emit a single byte to the output, applying byte stuffing.
    #[inline]
    fn emit_byte(&mut self, byte: u8)
    {
        self.data.push(byte);
        if byte == 0xFF
        {
            self.data.push(0x00);
        }
    }

    /// Pad the remaining bits with 1s and flush.
    ///
    /// Called at the end of each entropy-coded segment (before a restart
    /// marker or the EOI marker) per T.81 §B.1.1.5 Note 1.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`write_bits`](Self::write_bits).
    pub fn flush_with_ones(&mut self) -> Result<()>
    {
        if self.bits_pending > 0
        {
            let pad_bits = 8 - self.bits_pending;
            let ones = (1u32 << pad_bits) - 1;
            self.write_bits(ones, pad_bits)?;
        }
        Ok(())
    }

    /// Ensure the bit buffer is empty before writing raw bytes.
    ///
    /// Raw bytes (markers, headers) must only be written when there are
    /// no pending entropy-coded bits, because raw bytes bypass byte
    /// stuffing.
    #[inline]
    fn require_aligned(&self) -> Result<()>
    {
        if self.bits_pending != 0
        {
            return Err
            (
                Error::BitstreamError
                (
                    format!
                    (
                        "cannot write raw bytes with {} unflushed bits in the accumulator",
                        self.bits_pending,
                    ),
                )
            );
        }
        Ok(())
    }

    /// Write a single raw byte *outside* entropy-coded data.
    ///
    /// This does **not** apply byte stuffing and must only be used for
    /// marker segments and header data.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BitstreamError`] if there are unflushed bits in
    /// the accumulator.
    #[inline]
    pub fn write_raw_byte(&mut self, byte: u8) -> Result<()>
    {
        self.require_aligned()?;
        self.data.push(byte);
        Ok(())
    }

    /// Write a slice of raw bytes *outside* entropy-coded data.
    ///
    /// No byte stuffing is applied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BitstreamError`] if there are unflushed bits in
    /// the accumulator.
    pub fn write_raw_bytes(&mut self, bytes: &[u8]) -> Result<()>
    {
        self.require_aligned()?;
        self.data.extend_from_slice(bytes);
        Ok(())
    }

    /// Write a 16-bit unsigned integer in big-endian byte order.
    ///
    /// Used for marker codes, segment lengths, and other 16-bit parameters
    /// in the JPEG header. All multi-byte integers in JPEG are big-endian
    /// (T.81 §B.1.1.1).
    ///
    /// # Errors
    ///
    /// Returns [`Error::BitstreamError`] if there are unflushed bits in
    /// the accumulator.
    pub fn write_u16_be(&mut self, value: u16) -> Result<()>
    {
        self.write_raw_byte((value >> 8) as u8)?;
        self.write_raw_byte((value & 0xFF) as u8)?;
        Ok(())
    }

    /// Consume the writer and return the accumulated byte vector.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8>
    {
        self.data
    }

    /// Current length of the output buffer in bytes.
    #[must_use]
    pub fn len(&self) -> usize
    {
        self.data.len()
    }

    /// Returns `true` if the output buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool
    {
        self.data.is_empty()
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn write_single_byte()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0xAB, 8).unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0xAB]);
    }

    #[test]
    fn byte_stuffing_on_ff()
    {
        // Writing 0xFF in entropy data must produce 0xFF 0x00.
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0xFF, 8).unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0xFF, 0x00]);
    }

    #[test]
    fn no_stuffing_on_raw_byte()
    {
        // Raw bytes (for markers) do NOT get stuffed.
        let mut w = BitWriter::with_capacity(16);
        w.write_raw_byte(0xFF).unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0xFF]);
    }

    #[test]
    fn flush_pads_with_ones()
    {
        // Write 4 bits = 0b1010, then flush.
        // Expected: 0b1010_1111 = 0xAF.
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0b1010, 4).unwrap();
        w.flush_with_ones().unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0xAF]);
    }

    #[test]
    fn flush_noop_when_aligned()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0x42, 8).unwrap();
        w.flush_with_ones().unwrap(); // No pending bits -> no-op.
        let data = w.into_bytes();
        assert_eq!(data, [0x42]);
    }

    #[test]
    fn write_u16_be()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_u16_be(0xFFD8).unwrap();
        let data = w.into_bytes();
        // Raw bytes, no stuffing: 0xFF, 0xD8.
        assert_eq!(data, [0xFF, 0xD8]);
    }

    #[test]
    fn write_bits_spanning_bytes()
    {
        // Write 12 bits = 0xABC, then flush remaining 4 bits.
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0xABC, 12).unwrap();
        w.flush_with_ones().unwrap();
        // First 8 bits: 0xAB. Remaining 4 bits: 0xC = 0b1100, padded
        // with 1s -> 0b1100_1111 = 0xCF.
        let data = w.into_bytes();
        assert_eq!(data, [0xAB, 0xCF]);
    }

    #[test]
    fn write_zero_bits_is_noop()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0xFFFF, 0).unwrap();
        assert!(w.is_empty());
    }

    #[test]
    fn len_and_is_empty()
    {
        let mut w = BitWriter::with_capacity(16);
        assert!(w.is_empty());
        assert_eq!(w.len(), 0);
        w.write_bits(0xFF, 8).unwrap();
        assert!(!w.is_empty());
        assert_eq!(w.len(), 2); // 0xFF + stuffed 0x00
    }

    #[test]
    fn multiple_ff_bytes_each_stuffed()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0xFF, 8).unwrap();
        w.write_bits(0xFF, 8).unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0xFF, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn write_raw_bytes_slice()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_raw_bytes(&[0x4A, 0x46, 0x49, 0x46, 0x00]).unwrap();
        let data = w.into_bytes();
        assert_eq!(data, [0x4A, 0x46, 0x49, 0x46, 0x00]);
    }

    #[test]
    fn write_bits_count_too_large_returns_error()
    {
        let mut w = BitWriter::with_capacity(16);
        let result = w.write_bits(0, 17);
        assert!(result.is_err());
        assert!(w.is_empty(), "no bytes should have been written");
    }

    #[test]
    fn write_raw_byte_with_pending_bits_returns_error()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0b1010, 4).unwrap(); // 4 pending bits
        let result = w.write_raw_byte(0x42);
        assert!(result.is_err());
    }

    #[test]
    fn write_raw_bytes_with_pending_bits_returns_error()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0b1010, 4).unwrap();
        let result = w.write_raw_bytes(&[0x42]);
        assert!(result.is_err());
    }

    #[test]
    fn write_u16_be_with_pending_bits_returns_error()
    {
        let mut w = BitWriter::with_capacity(16);
        w.write_bits(0b1010, 4).unwrap();
        let result = w.write_u16_be(0xFFD8);
        assert!(result.is_err());
    }
}