#[derive(Debug, Clone)]
pub struct HuffmanTable
{
    pub bits: [u8; 16],

    pub values: Vec<u8>,

    pub ehufco: [u32; 256],

    pub ehufsi: [u8; 256],
}

pub const MAX_DC_CATEGORIES: usize = 16;

pub struct DcFrequencies
{
    pub counts: [u32; MAX_DC_CATEGORIES],
}

impl DcFrequencies
{
    pub fn new() -> Self
    {
        Self { counts: [0; MAX_DC_CATEGORIES] }
    }

    pub fn record(&mut self, diff: i16)
    {
        let ssss = category(diff) as usize;
        self.counts[ssss] += 1;
    }
}

pub struct AcFrequencies
{
    pub counts: [u32; 256],
}

impl AcFrequencies
{
    pub fn new() -> Self
    {
        Self { counts: [0; 256] }
    }

    pub fn record_coefficient(&mut self, run: u8, value: i16)
    {
        let ssss = category(value);
        let rs = ((run as u16) << 4) | (ssss as u16);
        self.counts[rs as usize] += 1;
    }

    pub fn record_eob(&mut self)
    {
        self.counts[0x00] += 1;
    }

    pub fn record_zrl(&mut self)
    {
        self.counts[0xF0] += 1;
    }
}

#[inline]
pub fn category(value: i16) -> u8
{
    if value == 0
    {
        return 0;
    }
    let bits = (16 - value.unsigned_abs().leading_zeros()) as u8;
    bits.min(15)
}

pub fn build_table(freq: &[u32], max_symbol: usize) -> HuffmanTable
{
    let num_symbols = max_symbol + 1;

    let mut symbols: Vec<(u8, u32)> = Vec::new(); // (symbol_value, frequency)
    for i in 0..num_symbols.min(freq.len())
    {
        if freq[i] > 0
        {
            symbols.push((i as u8, freq[i]));
        }
    }

    if symbols.is_empty()
    {
        return build_empty_table();
    }

    if symbols.len() == 1
    {
        let sym = symbols[0].0;
        let mut bits = [0u8; 16];
        bits[0] = 1;
        let values = vec![sym];
        let (ehufco, ehufsi) = generate_encoder_tables(&bits, &values);
        return HuffmanTable { bits, values, ehufco, ehufsi };
    }

    let _real_count = symbols.len();

    let code_lengths = compute_code_lengths(&symbols);

    let mut bits = [0u32; 33]; // bits[1..=32]
    for &(_, len) in &code_lengths
    {
        if len > 0 && (len as usize) < bits.len()
        {
            bits[len as usize] += 1;
        }
    }

    adjust_bits(&mut bits, true);


    let mut sorted_symbols: Vec<(u8, u8)> = code_lengths.clone(); // (sym, len)
    sorted_symbols.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let total_codes: u32 = bits[1..=16].iter().sum();

    let selected: Vec<u8> = sorted_symbols
        .iter()
        .take(total_codes as usize)
        .map(|&(sym, _)| sym)
        .collect();


    let mut assigned: Vec<(u8, u8)> = Vec::with_capacity(selected.len()); // (sym, adjusted_len)
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
        slot.sort();
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

    let (ehufco, ehufsi) = generate_encoder_tables(&bits_out, &huffval);

    HuffmanTable
    {
        bits: bits_out,
        values: huffval,
        ehufco,
        ehufsi,
    }
}

fn compute_code_lengths(symbols: &[(u8, u32)]) -> Vec<(u8, u8)>
{
    let n = symbols.len();
    let total_leaves = n + 1; // +1 for reserved
    let max_nodes = 2 * total_leaves;
    let mut node_freq = vec![0u64; max_nodes];
    let mut parent = vec![0usize; max_nodes];
    let mut used = vec![false; max_nodes];

    for i in 0..n
    {
        node_freq[i] = symbols[i].1 as u64;
    }
    // Reserved code point: frequency 1, at the highest index among leaves
    node_freq[n] = 1;

    let mut next_internal = total_leaves;

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

    // Compute depths for REAL symbols only (not the reserved node).
    let mut result = Vec::with_capacity(n);
    for i in 0..n
    {
        let mut depth: u8 = 0;
        let mut node = i;
        while node != root
        {
            node = parent[node];
            depth += 1;
        }
        result.push((symbols[i].0, depth));
    }

    result
}

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

fn adjust_bits(bits: &mut [u32; 33], _remove_reserved: bool)
{
    // Step 1: Limit to 16 bits (standard algorithm from K.3)
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
                // Degenerate case: no shorter codes available.
                // Move all codes one level up.
                bits[i - 1] += bits[i];
                bits[i] = 0;
                break;
            }

            bits[i] -= 2;
            bits[i - 1] += 1;
            bits[j + 1] += 2;
            bits[j] -= 1;
        }
        i -= 1;
    }
}

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

fn build_empty_table() -> HuffmanTable
{
    let mut bits = [0u8; 16];
    bits[0] = 1; // One 1-bit code
    let values = vec![0];
    let (ehufco, ehufsi) = generate_encoder_tables(&bits, &values);
    HuffmanTable { bits, values, ehufco, ehufsi }
}

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
        let dc = block[0];
        let diff = dc - prev_dc;
        prev_dc = dc;
        dc_freq.record(diff);

        let mut zero_run: u8 = 0;
        for k in 1..64
        {
            if block[k] == 0
            {
                zero_run += 1;
            }
            else
            {
                while zero_run > 15
                {
                    ac_freq.record_zrl();
                    zero_run -= 16;
                }
                ac_freq.record_coefficient(zero_run, block[k]);
                zero_run = 0;
            }
        }

        if zero_run > 0
        {
            ac_freq.record_eob();
        }
    }

    (dc_freq, ac_freq)
}