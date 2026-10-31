//! The song player, run by the 280 Hz timer: it sends every record that is due
//! and starts over at the end.

use alloc::vec::Vec;

use dn2_assets::sound::ImfCommand;

pub struct Song {
    commands: Vec<ImfCommand>,
    next: usize,
    /// The tick counter and the tick the next record is due at.
    now: u32,
    due: u32,
}

impl Song {
    pub fn new(commands: Vec<ImfCommand>) -> Self {
        Self {
            commands,
            next: 0,
            now: 0,
            due: 0,
        }
    }

    /// Whether fewer than 41 bytes (a record is 4) are left.
    pub fn near_end(&self) -> bool {
        self.commands.len().saturating_sub(self.next) * 4 < 41
    }

    /// One timer tick: `write(reg, value)` for each record that is due.
    /// The next due time is "now + delay", and the song restarts as soon as
    /// its last record is sent, without waiting for that record's delay.
    pub fn tick(&mut self, mut write: impl FnMut(u8, u8)) {
        while self.next < self.commands.len() && self.due <= self.now {
            let c = self.commands[self.next];
            self.next += 1;
            self.due = self.now + u32::from(c.delay);
            write(c.reg, c.value);
        }
        self.now += 1;
        if self.next >= self.commands.len() {
            (self.next, self.now, self.due) = (0, 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dn2_assets::sound::parse_imf;

    #[test]
    fn plays_with_delays_and_restarts() {
        let data = [1, 2, 3, 0, 4, 5, 0, 0, 6, 7, 1, 0, 9];
        let mut song = Song::new(parse_imf(&data));
        let mut log = Vec::new();
        for tick in 0..9 {
            song.tick(|r, v| log.push((tick, r, v)));
        }
        assert_eq!(
            log,
            [
                (0, 1, 2),
                (3, 4, 5),
                (3, 6, 7),
                (4, 1, 2),
                (7, 4, 5),
                (7, 6, 7),
                (8, 1, 2)
            ]
        );
    }
}
