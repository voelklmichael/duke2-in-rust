/// The game's random numbers: a small deterministic generator, started
/// again for every level, so a level plays the same for the same input.
/// (The original steps through a fixed table; this one only needs to look
/// random.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Random {
    state: u32,
}

const SEED: u32 = 0x2bad_cafe;

impl Random {
    /// The next byte (xorshift32).
    pub fn next_byte(&mut self) -> u8 {
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
        Self { state: SEED | 1 }
    }
}
