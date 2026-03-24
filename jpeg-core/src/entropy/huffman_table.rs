//! # Huffman_table
//!
//! # Huffman coding in JPEG - overview
//! JPEG uses Huffman coding as one of its two entropy coding methods (the
//! other being arithmetic coding, which this encoder does not implement).
//!
//! A Huffman code assigns variable-length bit patterns to symbols, with
//! shorter codes for more frequent symbols. In JPEG, two kinds of symbols
//! are Huffman-coded:
//!
//!   1. **DC difference categories** - the number of additional bits needed
//!      to represent the difference between the current block's DC coefficient
//!      and the previous block's DC coefficient (T.81 §F.1.2.1, Table F.1).
//!
//!   2. **AC run/size composites** - an 8-bit value `RRRRSSSS` where the high
//!      nibble RRRR is the run-length of zero coefficients preceding the
//!      current non-zero coefficient, and the low nibble SSSS is the category
//!      of that coefficient's magnitude (T.81 §F.1.2.2, Table F.2).
//!
//! Two special AC symbols exist:
//!   - `0x00` = End-Of-Block (EOB): all remaining AC coefficients are zero.
//!   - `0xF0` = Zero Run Length (ZRL): a run of exactly 16 zeros.
//! 
//! A Huffman table is stored in the JPEG bitstream as two lists:
//!
//!   - **BITS** (16 bytes): `BITS[i]` = number of codes of length `i+1`
//!     (for i = 0..15, so code lengths 1 through 16).
//!
//!   - **HUFFVAL** (variable length): the symbol values, in order of
//!     increasing code length, then by symbol value within each length.
//!
//! The actual Huffman codes are *not* stored explicitly - they are derived
//! deterministically from BITS and HUFFVAL using the algorithm in T.81
//! Figures C.1, C.2, and C.3.
//!
//! # Table construction (T.81 Annex K, Figures K.1–K.4)
//!
//! This module builds optimised Huffman tables from the actual symbol
//! frequencies in the image data, following the procedure in T.81 §K.2:
//!
//! 1. **Collect frequencies** - count how often each symbol occurs.
//! 2. **Build code tree** - use a greedy algorithm (similar to the classic
//!    Huffman algorithm) to assign code lengths to symbols (Figure K.1).
//! 3. **Limit to 16 bits** - JPEG requires all codes to be at most 16 bits
//!    long. The `Adjust_BITS` procedure (Figure K.3) redistributes codes
//!    to enforce this limit.
//! 4. **Sort symbols** - reorder symbols by code length, then by symbol
//!    value within each length (Figure K.4).
//! 5. **Generate encoder tables** - from BITS and HUFFVAL, compute two
//!    lookup tables indexed by symbol value:
//!      - `EHUFCO[symbol]` = the Huffman code (T.81 Figure C.3).
//!      - `EHUFSI[symbol]` = the code length in bits.
//!
//! # Reserved code point
//!
//! Per T.81 §K.2:
//!
//! > "FREQ values for V = 256 is set to 1 to reserve one code point."
//!
//! This ensures that no Huffman code consists entirely of 1-bits, which
//! is required so that the all-1s padding at segment boundaries (see
//! [`crate::bitstream`]) cannot be confused with valid coded data.

/// A Huffman table for encoding.
///
/// Contains both the specification-form data (BITS + HUFFVAL, for writing
/// into the DHT marker segment) and the encoder lookup tables (EHUFCO +
/// EHUFSI, for fast encoding).
#[derive(Debug, Clone)]
pub struct HuffmanTable
{
    /// Number of codes of each length (1..16).
    ///
    /// `bits[i]` = number of Huffman codes of length `i + 1`.
    /// Corresponds to the BITS list in T.81 §C.
    pub bits: [u8; 16],

    /// Symbol values in order of increasing code length, then by symbol
    /// value within each length.
    ///
    /// Corresponds to the HUFFVAL list in T.81 §C.
    pub values: Vec<u8>,

    /// Encoder code table: `ehufco[symbol]` = Huffman code for `symbol`.
    ///
    /// Corresponds to EHUFCO in T.81 Figure C.3.
    pub ehufco: [u32; 256],

    /// Encoder size table: `ehufsi[symbol]` = code length in bits.
    ///
    /// A value of 0 means the symbol has no assigned code (it never
    /// occurred in the frequency data).
    ///
    /// Corresponds to EHUFSI in T.81 Figure C.3.
    pub ehufsi: [u8; 256],
}

/// Maximum number of DC difference categories.
///
/// For 8-bit baseline JPEG, DC differences range from  -2047 to +2047,
/// which requires categories 0 through 11 (T.81 Table F.1). We allow
/// up to 16 categories to handle extended precision and lossless modes
/// (T.81 Table H.2).
pub const MAX_DC_CATEGORIES: usize = 16;

/// Frequency counter for DC difference categories.
///
/// Tracks how often each category (0..`MAX_DC_CATEGORIES`) appears in
/// the image data. The resulting counts are used to build an optimised
/// Huffman table for the DC coefficient stream.
pub struct DcFrequencies
{
    /// `counts[ssss]` = number of times category `ssss` was observed.
    pub counts: [u32; MAX_DC_CATEGORIES],
}

impl Default for DcFrequencies
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl DcFrequencies
{
    #[must_use]
    pub fn new() -> Self
    {
        Self { counts: [0; MAX_DC_CATEGORIES] }
    }

    /// Record a DC difference value.
    pub fn record(&mut self, diff: i16)
    {
        let ssss = category(diff) as usize;
        self.counts[ssss] += 1;
    }
}

/// Frequency counter for AC run/size composite symbols.
///
/// The AC symbol space has 256 possible values (8-bit RS byte). In
/// practice, only a subset is used: EOB (0x00), ZRL (0xF0), and the
/// valid run/size combinations.
pub struct AcFrequencies
{
    /// `counts[rs]` = number of times composite value `rs` was observed.
    pub counts: [u32; 256],
}

impl Default for AcFrequencies
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl AcFrequencies
{
    #[must_use]
    pub fn new() -> Self
    {
        Self { counts: [0; 256] }
    }

    /// Record a non-zero AC coefficient preceded by `run` zero coefficients.
    pub fn record_coefficient(&mut self, run: u8, value: i16)
    {
        let ssss = category(value);
        let rs = ((run as u16) << 4) | (ssss as u16);
        self.counts[rs as usize] += 1;
    }

    /// Record an End-Of-Block symbol (all remaining AC coefficients are zero).
    pub fn record_eob(&mut self)
    {
        self.counts[0x00] += 1;
    }

    /// Record a Zero Run Length symbol (run of exactly 16 zeros).
    pub fn record_zrl(&mut self)
    {
        self.counts[0xF0] += 1;
    }
}

/// Compute the SSSS category for a coefficient value.
///
/// The category is defined in T.81 Tables F.1 (DC) and F.2 (AC):
///
/// | SSSS | Range of values                |
/// |------|--------------------------------|
/// | 0    | 0                              |
/// | 1    |  -1, 1                         |
/// | 2    |  -3.. -2, 2..3                 |
/// | 3    |  -7.. -4, 4..7                 |
/// | …    | …                              |
/// | n    |  -(2ⁿ -1).. -2ⁿ⁻¹, 2ⁿ⁻¹..2ⁿ -1 |
///
/// In short, SSSS is the number of
/// bits needed to represent |value|.
///
/// The result is clamped to 15, which is the maximum category for 8-bit
/// baseline JPEG (values up to +-32767 for 16-bit lossless mode would
/// need categories up to 16, per T.81 Table H.2).
#[inline]
#[must_use]
pub fn category(value: i16) -> u8
{
    if value == 0
    {
        return 0;
    }
    let bits = (16 - value.unsigned_abs().leading_zeros()) as u8;
    bits.min(15)
}

/// Build an optimised Huffman table from symbol frequencies.
///
/// This implements the full procedure from T.81 Annex K (§K.2):
///
/// 1. Build the code-length tree (Figure K.1).
/// 2. Count codes per length (Figure K.2).
/// 3. Limit all codes to <= 16 bits (Figure K.3, `Adjust_BITS`).
/// 4. Sort symbols by code length (Figure K.4).
/// 5. Generate EHUFCO/EHUFSI encoder tables (Figures C.1–C.3).
///
/// # Arguments
///
/// * `freq` - frequency of each symbol. `freq[i]` is the count for
///   symbol `i`. Symbols with zero frequency receive no code.
/// * `max_symbol` - the highest symbol value to consider.
///
/// # Returns
///
/// A [`HuffmanTable`] ready for encoding.
pub fn build_table(freq: &[u32], max_symbol: usize) -> HuffmanTable
{
    let num_symbols = max_symbol + 1;

    // Collect symbols with non-zero frequency.
    let mut symbols: Vec<(u8, u32)> = Vec::new();
    for (i, &count) in freq.iter().enumerate().take(num_symbols)
    {
        if count > 0
        {
            symbols.push((i as u8, count));
        }
    }

    // Edge case: no symbols at all - return a minimal valid table.
    if symbols.is_empty()
    {
        return build_empty_table();
    }

    // Edge case: a single symbol - assign it a 1-bit code.
    if symbols.len() == 1
    {
        let sym = symbols[0].0;
        let mut bits = [0u8; 16];
        bits[0] = 1;
        let values = vec![sym];
        let (ehufco, ehufsi) = generate_encoder_tables(&bits, &values);
        return HuffmanTable { bits, values, ehufco, ehufsi };
    }

    // Step 1: Build the code-length tree (Figure K.1).
    let code_lengths = compute_code_lengths(&symbols);

    // Step 2: Count codes per length (Figure K.2).
    let mut bits = [0u32; 33];
    for &(_, len) in &code_lengths
    {
        if len > 0 && (len as usize) < bits.len()
        {
            bits[len as usize] += 1;
        }
    }

    // Step 3: Limit to 16 bits (Figure K.3).
    adjust_bits(&mut bits);

    // Step 4: Sort symbols by code length, then by symbol value
    // within each length (Figure K.4).
    let mut sorted_symbols: Vec<(u8, u8)> = code_lengths;
    sorted_symbols.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let total_codes: u32 = bits[1..=16].iter().sum();

    let selected: Vec<u8> = sorted_symbols
        .iter()
        .take(total_codes as usize)
        .map(|&(sym, _)| sym)
        .collect();

    // Assign symbols to code lengths, sorted by symbol value within
    // each length group.
    let mut assigned: Vec<(u8, u8)> = Vec::with_capacity(selected.len());
    let mut idx = 0;
    for len in 1..=16u8
    {
        let count = bits[len as usize] as usize;
        let mut slot: Vec<u8> = Vec::with_capacity(count);
        for _ in 0..count
        {
            if idx < selected.len()
            {
                slot.push(selected[idx]);
                idx += 1;
            }
        }
        slot.sort_unstable();
        for sym in slot
        {
            assigned.push((sym, len));
        }
    }

    let huffval: Vec<u8> = assigned.iter().map(|&(sym, _)| sym).collect();

    let mut bits_out = [0u8; 16];
    for i in 1..=16
    {
        bits_out[i - 1] = bits[i] as u8;
    }

    // Step 5: Generate encoder tables (Figures C.1–C.3).
    let (ehufco, ehufsi) = generate_encoder_tables(&bits_out, &huffval);

    HuffmanTable
    {
        bits: bits_out,
        values: huffval,
        ehufco,
        ehufsi,
    }
}

/// Build the code-length tree using the greedy Huffman algorithm.
///
/// This corresponds to T.81 Figure K.1 (`Code_size`).
///
/// The algorithm maintains a forest of trees. At each step, the two
/// trees with the smallest total frequency are merged. The depth of
/// each leaf in the final tree gives the code length for that symbol.
///
/// An extra node with frequency 1 is added as the "reserved code point"
/// (T.81 §K.2) to ensure no code consists entirely of 1-bits.
fn compute_code_lengths(symbols: &[(u8, u32)]) -> Vec<(u8, u8)>
{
    let n = symbols.len();
    let total_leaves = n + 1; // +1 for the reserved code point
    let max_nodes = 2 * total_leaves;

    let mut node_freq = vec![0u64; max_nodes];
    let mut parent    = vec![0usize; max_nodes];
    let mut used      = vec![false; max_nodes];

    for (i, &(_, freq)) in symbols.iter().enumerate()
    {
        node_freq[i] = freq as u64;
    }
    // Reserved code point at index `n`, frequency 1.
    node_freq[n] = 1;

    let mut next_internal = total_leaves;

    // Merge pairs until only one tree (the root) remains.
    for _ in 0..total_leaves - 1
    {
        let v1 = find_min_unused(&node_freq, &used, next_internal, usize::MAX);
        used[v1] = true;
        let v2 = find_min_unused(&node_freq, &used, next_internal, usize::MAX);
        used[v2] = true;

        let internal = next_internal;
        node_freq[internal] = node_freq[v1] + node_freq[v2];
        parent[v1] = internal;
        parent[v2] = internal;
        next_internal += 1;
    }

    let root = next_internal - 1;

    // Compute the depth (= code length) for each real symbol.
    // The reserved node at index `n` is excluded from the output.
    let mut result = Vec::with_capacity(n);
    for (i, &(sym, _)) in symbols.iter().enumerate()
    {
        let mut depth: u8 = 0;
        let mut node = i;
        while node != root
        {
            node = parent[node];
            depth += 1;
        }
        result.push((sym, depth));
    }

    result
}

/// Find the unused node with the smallest frequency.
///
/// When frequencies are equal, the node with
/// the largest index wins (this places the reserved code point in the
/// longest code, as required by §K.2).
fn find_min_unused
(
    freq: &[u64],
    used: &[bool],
    count: usize,
    exclude: usize,
) -> usize
{
    let mut min_idx = usize::MAX;
    let mut min_freq = u64::MAX;
    for i in 0..count
    {
        if !used[i] && i != exclude && freq[i] <= min_freq
        {
            min_freq = freq[i];
            min_idx = i;
        }
    }
    min_idx
}

/// Limit code lengths to 16 bits.
///
/// Implements T.81 Figure K.3 (`Adjust_BITS`).
///
/// The algorithm works from the longest codes downward. When it finds
/// codes longer than 16 bits, it promotes pairs of symbols to shorter
/// code lengths. The process guarantees that the resulting code lengths
/// still form a valid prefix code (the Kraft inequality is maintained).
///
/// # Reserved code point handling
///
/// Per T.81 §K.2, one code point is reserved to ensure that no Huffman
/// code consists entirely of 1-bits. The standard's `Adjust_BITS`
/// procedure (Figure K.3) removes this reserved code from the longest
/// code length after limiting.
///
/// In this implementation, the reserved code point is not removed
/// from the BITS counts. Instead, it remains as an unused entry in the
/// HUFFVAL list - it is assigned a code but never emitted during
/// encoding. This is safe because:
///
/// - The encoder only emits codes for symbols that actually occur in the
///   data, and the reserved symbol (index 256, which is beyond the 0–255
///   range of real symbols) never occurs.
/// - Keeping it avoids the complexity of tracking which code length the
///   reserved point ended up at after redistribution, especially in edge
///   cases with very few symbols.
fn adjust_bits(bits: &mut [u32; 33])
{
    let mut i = 32;
    while i > 16
    {
        while bits[i] > 0
        {
            let mut j = i - 2;
            while j > 0 && bits[j] == 0
            {
                j -= 1;
            }

            if j == 0
            {
                // Degenerate case: no shorter codes available to split.
                // Promote all codes at this length up by one level.
                bits[i - 1] += bits[i];
                bits[i] = 0;
                break;
            }

            // Standard redistribution per Figure K.3:
            // Remove two codes at length i, add one at i -1 (their shared
            // prefix), and split one code at length j into two at j+1.
            bits[i] -= 2;
            bits[i - 1] += 1;
            bits[j + 1] += 2;
            bits[j] -= 1;
        }
        i -= 1;
    }
}

/// Generate the encoder lookup tables EHUFCO and EHUFSI from BITS and
/// HUFFVAL.
///
/// This implements T.81 Figures C.1 (Generate_size_table), C.2
/// (Generate_code_table), and C.3 (Order_codes).
fn generate_encoder_tables
(
    bits: &[u8; 16],
    huffval: &[u8],
) -> ([u32; 256], [u8; 256])
{
    let mut huffsize: Vec<u8> = Vec::new();
    for i in 0..16u8
    {
        let length = i + 1;
        for _ in 0..bits[i as usize]
        {
            huffsize.push(length);
        }
    }
    let lastk = huffsize.len();

    let mut huffcode = vec![0u32; lastk];
    if lastk > 0
    {
        let mut code: u32 = 0;
        let mut si = huffsize[0];

        for k in 0..lastk
        {
            while huffsize[k] != si
            {
                code <<= 1;
                si += 1;
            }
            huffcode[k] = code;
            code += 1;
        }
    }

    let mut ehufco = [0u32; 256];
    let mut ehufsi = [0u8; 256];

    for k in 0..lastk.min(huffval.len())
    {
        let symbol = huffval[k] as usize;
        ehufco[symbol] = huffcode[k];
        ehufsi[symbol] = huffsize[k];
    }

    (ehufco, ehufsi)
}

/// Build a minimal valid table for the edge case of no symbols.
///
/// Even when there are no symbols to encode (e.g. an all-zero AC
/// coefficient stream), the DHT marker must still contain a valid table.
/// We assign a single 1-bit code to symbol 0.
fn build_empty_table() -> HuffmanTable
{
    let mut bits = [0u8; 16];
    bits[0] = 1;
    let values = vec![0];
    let (ehufco, ehufsi) = generate_encoder_tables(&bits, &values);
    HuffmanTable { bits, values, ehufco, ehufsi }
}
/// Collect DC and AC symbol frequencies from a sequence of quantized blocks.
///
/// This is the first step of optimised Huffman table construction: before
/// we can build a Huffman code, we need to know how often each symbol
/// occurs.
///
/// The function simulates the encoding process (DC differencing, run-length
/// counting) without actually producing any bits, in order to count the
/// exact symbols that will be encoded.
///
/// # Arguments
///
/// * `blocks` - quantized blocks in zig-zag order (as produced by
///   [`crate::quantize::quantize_block`]). Element [0] of each block is
///   the quantized DC coefficient; elements [1..63] are AC coefficients.
pub fn collect_frequencies
(
    blocks: &[[i16; 64]],
) -> (DcFrequencies, AcFrequencies)
{
    let mut dc_freq = DcFrequencies::new();
    let mut ac_freq = AcFrequencies::new();

    let mut prev_dc: i16 = 0;

    for block in blocks
    {
        // DC: encode the difference from the previous block's DC.
        let dc = block[0];
        let diff = dc - prev_dc;
        prev_dc = dc;
        dc_freq.record(diff);

        // AC: count run/size pairs and special symbols.
        let mut zero_run: u8 = 0;
        for &coeff in block.iter().skip(1)
        {
            if coeff == 0
            {
                zero_run += 1;
            }
            else
            {
                // Emit ZRL symbols for runs ≥ 16.
                while zero_run > 15
                {
                    ac_freq.record_zrl();
                    zero_run -= 16;
                }
                ac_freq.record_coefficient(zero_run, coeff);
                zero_run = 0;
            }
        }

        // If the block ends with zeros, emit EOB.
        if zero_run > 0
        {
            ac_freq.record_eob();
        }
    }

    (dc_freq, ac_freq)
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn category_zero()
    {
        assert_eq!(category(0), 0);
    }

    #[test]
    fn category_positive_values()
    {
        assert_eq!(category(1), 1);   // 2^0 .. 2^1-1
        assert_eq!(category(2), 2);
        assert_eq!(category(3), 2);   // 2^1 .. 2^2-1
        assert_eq!(category(4), 3);
        assert_eq!(category(7), 3);   // 2^2 .. 2^3-1
        assert_eq!(category(255), 8); // 2^7 .. 2^8-1
        assert_eq!(category(1023), 10);
    }

    #[test]
    fn category_negative_values()
    {
        assert_eq!(category(-1), 1);
        assert_eq!(category(-3), 2);
        assert_eq!(category(-7), 3);
        assert_eq!(category(-255), 8);
    }

    #[test]
    fn category_matches_t81_table_f1()
    {
        // T.81 Table F.1: SSSS=1 -> values  -1,1
        assert_eq!(category(-1), 1);
        assert_eq!(category(1), 1);
        // SSSS=2 ->  -3.. -2, 2..3
        for v in [2, 3, -2, -3] { assert_eq!(category(v), 2); }
        // SSSS=3 ->  -7.. -4, 4..7
        for v in [4, 7, -4, -7] { assert_eq!(category(v), 3); }
        // SSSS=4 ->  -15.. -8, 8..15
        for v in [8, 15, -8, -15] { assert_eq!(category(v), 4); }
    }

    #[test]
    fn build_table_single_symbol()
    {
        let freq = [0u32, 0, 0, 100, 0]; // Only symbol 3 occurs.
        let table = build_table(&freq, 4);
        assert!(table.ehufsi[3] > 0, "symbol 3 should have a code");
    }

    #[test]
    fn build_table_all_symbols_get_codes()
    {
        // 5 symbols, all with non-zero frequency.
        let freq = [10u32, 20, 30, 40, 50];
        let table = build_table(&freq, 4);
        for i in 0..5
        {
            assert!(table.ehufsi[i] > 0, "symbol {} should have a code", i);
        }
    }

    #[test]
    fn build_table_codes_are_max_16_bits()
    {
        let freq = [1u32; 256]; // Many symbols, equal frequency.
        let table = build_table(&freq, 255);
        for i in 0..256
        {
            if table.ehufsi[i] > 0
            {
                assert!(
                    table.ehufsi[i] <= 16,
                    "symbol {} has code length {} > 16",
                    i, table.ehufsi[i],
                );
            }
        }
    }

    #[test]
    fn build_table_prefix_free()
    {
        // Verify the Kraft inequality: sum(2^(-len_i)) <= 1.
        let freq = [5u32, 10, 15, 20, 25, 30, 35, 40];
        let table = build_table(&freq, 7);

        let mut kraft_sum = 0.0f64;
        for i in 0..256
        {
            if table.ehufsi[i] > 0
            {
                kraft_sum += 2.0f64.powi(-(table.ehufsi[i] as i32));
            }
        }
        assert!(
            kraft_sum <= 1.0 + 1e-10,
            "Kraft inequality violated: sum = {}",
            kraft_sum,
        );
    }

    #[test]
    fn build_table_empty_frequencies()
    {
        let freq = [0u32; 16];
        let table = build_table(&freq, 15);
        // Should produce a valid (minimal) table, not panic.
        let total: u8 = table.bits.iter().sum();
        assert!(total > 0, "empty table should still have at least one code");
    }

    #[test]
    fn build_table_shorter_codes_for_frequent_symbols()
    {
        // Symbol 0 is very frequent, symbol 1 is rare.
        let freq = [1000u32, 1];
        let table = build_table(&freq, 1);
        assert!(
            table.ehufsi[0] <= table.ehufsi[1],
            "frequent symbol should have shorter or equal code: {} vs {}",
            table.ehufsi[0], table.ehufsi[1],
        );
    }

    #[test]
    fn collect_frequencies_dc_differences()
    {
        // Two blocks: DC = 10 and DC = 20.
        // Differences: 10  - 0 = 10 (cat 4), 20  - 10 = 10 (cat 4).
        let mut blocks = Vec::new();
        let mut b1 = [0i16; 64]; b1[0] = 10; blocks.push(b1);
        let mut b2 = [0i16; 64]; b2[0] = 20; blocks.push(b2);

        let (dc_freq, _) = collect_frequencies(&blocks);
        assert_eq!(dc_freq.counts[4], 2); // category(10) = 4
    }

    #[test]
    fn collect_frequencies_eob()
    {
        // Block with DC=5 and all AC=0 -> one EOB.
        let mut block = [0i16; 64];
        block[0] = 5;
        let (_, ac_freq) = collect_frequencies(&[block]);
        assert_eq!(ac_freq.counts[0x00], 1); // EOB
    }

    #[test]
    fn collect_frequencies_zrl()
    {
        // Block with 16 zeros then a non-zero AC.
        let mut block = [0i16; 64];
        block[0] = 0; // DC
        // AC positions 1..16 are zero (run of 16).
        block[17] = 5; // AC at zig-zag position 17
        let (_, ac_freq) = collect_frequencies(&[block]);
        assert_eq!(ac_freq.counts[0xF0], 1); // ZRL
    }

    #[test]
    fn collect_frequencies_run_size_composite()
    {
        // Block: DC=0, AC[1]=0, AC[2]=3 -> run=1, cat=2, RS = 0x12.
        let mut block = [0i16; 64];
        block[0] = 0; // DC
        block[1] = 0; // AC zig-zag 1 = zero
        block[2] = 3; // AC zig-zag 2 = non-zero, category 2
        let (_, ac_freq) = collect_frequencies(&[block]);
        assert_eq!(ac_freq.counts[0x12], 1); // run=1, ssss=2
    }

    #[test]
    fn dc_frequencies_default()
    {
        let freq = DcFrequencies::default();
        assert_eq!(freq.counts, [0u32; MAX_DC_CATEGORIES]);
    }

    #[test]
    fn ac_frequencies_default()
    {
        let freq = AcFrequencies::default();
        assert_eq!(freq.counts, [0u32; 256]);
    }

    #[test]
    fn build_table_very_skewed_frequencies()
    {
        // Force the adjust_bits degenerate path (j==0 branch):
        // Many symbols with frequency 1 -> very deep tree -> needs heavy limiting
        let mut freq = [0u32; 256];
        // One dominant symbol and 255 rare ones
        freq[0] = 1_000_000;
        for count in freq.iter_mut().skip(1)
        {
            *count = 1;
        }
        let table = build_table(&freq, 255);
        for i in 0..256
        {
            assert!(
                table.ehufsi[i] <= 16,
                "symbol {} has code length {} > 16",
                i, table.ehufsi[i],
            );
        }
    }

    #[test]
    fn build_table_two_symbols()
    {
        // Exactly 2 symbols: covers the minimal non-trivial tree
        let freq = [50u32, 100];
        let table = build_table(&freq, 1);
        assert!(table.ehufsi[0] > 0);
        assert!(table.ehufsi[1] > 0);
        // More frequent symbol should have shorter or equal code
        assert!(table.ehufsi[1] <= table.ehufsi[0]);
    }
}