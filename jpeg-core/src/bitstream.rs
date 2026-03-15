pub struct BitWriter
{
    data: Vec<u8>,

    bit_buffer: u32,

    bits_pending: u8,
}

impl BitWriter
{
    pub fn with_capacity(capacity: usize) -> Self
    {
        Self
        {
            data: Vec::with_capacity(capacity),
            bit_buffer: 0,
            bits_pending: 0,
        }
    }

    #[inline]
    pub fn write_bits(&mut self, value: u32, count: u8)
    {
        debug_assert!(count <= 16);
        if count == 0
        {
            return;
        }

        let masked = value & ((1u32 << count) - 1);

        self.bit_buffer = (self.bit_buffer << count) | masked;
        self.bits_pending += count;

        while self.bits_pending >= 8
        {
            self.bits_pending -= 8;
            let byte = ((self.bit_buffer >> self.bits_pending) & 0xFF) as u8;
            self.emit_byte(byte);
        }
    }

    #[inline]
    fn emit_byte(&mut self, byte: u8)
    {
        self.data.push(byte);
        if byte == 0xFF
        {
            self.data.push(0x00);
        }
    }

    pub fn flush_with_ones(&mut self)
    {
        if self.bits_pending > 0
        {
            let pad_bits = 8 - self.bits_pending;
            let ones = (1u32 << pad_bits) - 1;
            self.write_bits(ones, pad_bits);
        }
    }

    #[inline]
    pub fn write_raw_byte(&mut self, byte: u8)
    {
        debug_assert_eq!(self.bits_pending, 0);
        self.data.push(byte);
    }

    pub fn write_raw_bytes(&mut self, bytes: &[u8])
    {
        debug_assert_eq!(self.bits_pending, 0);
        self.data.extend_from_slice(bytes);
    }

    pub fn write_u16_be(&mut self, value: u16)
    {
        self.write_raw_byte((value >> 8) as u8);
        self.write_raw_byte((value & 0xFF) as u8);
    }

    pub fn into_bytes(self) -> Vec<u8>
    {
        self.data
    }

    pub fn len(&self) -> usize
    {
        self.data.len()
    }
}
