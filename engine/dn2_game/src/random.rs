/// The game's random numbers: a small deterministic generator, started
/// again for every level, so a level plays the same for the same input.
/// (The original steps through a fixed table; this one only needs to look
/// random.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Random {
    state: u32,
    /// Bytes to step through instead, like the original: for comparing
    /// with it (the table is the caller's, from the original's data).
    table: Option<&'static [u8; 256]>,
}

const SEED: u32 = 0x2bad_cafe;

impl Random {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed | 1,
            table: None,
        }
    }

    /// Steps through `table` as the original does: the index is incremented
    /// before each read, starting from 0.
    pub fn from_table(table: &'static [u8; 256]) -> Self {
        Self {
            state: 0,
            table: Some(table),
        }
    }

    /// Where a table generator is in its table (for comparing).
    pub fn table_index(&self) -> Option<u8> {
        self.table.map(|_| self.state as u8)
    }

    /// The generator as at the start of a level.
    pub fn restarted(&self) -> Self {
        match self.table {
            Some(table) => Self::from_table(table),
            None => Self::default(),
        }
    }

    /// The next byte (xorshift32, or the table's).
    pub fn next_byte(&mut self) -> u8 {
        if let Some(table) = self.table {
            self.state = (self.state + 1) & 0xff;
            return table[self.state as usize];
        }
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x >> 24) as u8
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new(SEED)
    }
}
