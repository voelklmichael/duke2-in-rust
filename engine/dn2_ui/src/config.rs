//! The player's settings: the options menu, the key bindings and the last
//! skill level, kept in the platform's store under [`Config::KEY`] (the
//! original writes them to NUKEM2.-GT).

use alloc::vec::Vec;

use crate::Key;

/// The options menu's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Digitized sound effects (Sound Blaster), option `S`.
    pub sound_blaster: bool,
    /// AdLib sound effects, option `L`.
    pub adlib: bool,
    /// PC speaker sound effects, option `P`.
    pub pc_speaker: bool,
    /// Music, option `M`.
    pub music: bool,
    /// The game speed, 1 to 7.
    pub speed: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            sound_blaster: true,
            adlib: true,
            pc_speaker: false,
            music: true,
            speed: 4,
        }
    }
}

impl Options {
    /// Whether the option with this letter (of `//TOGGS`) is on.
    pub fn get(&self, letter: u8) -> bool {
        match letter {
            b'S' => self.sound_blaster,
            b'L' => self.adlib,
            b'P' => self.pc_speaker,
            b'M' => self.music,
            _ => false,
        }
    }

    /// Switches the option with this letter. The PC speaker excludes the
    /// sound cards and the other way round.
    pub fn toggle(&mut self, letter: u8) {
        match letter {
            b'S' => {
                self.sound_blaster = !self.sound_blaster;
                self.pc_speaker &= !self.sound_blaster;
            }
            b'L' => {
                self.adlib = !self.adlib;
                self.pc_speaker &= !self.adlib;
            }
            b'P' => {
                self.pc_speaker = !self.pc_speaker;
                if self.pc_speaker {
                    (self.adlib, self.sound_blaster) = (false, false);
                }
            }
            b'M' => self.music = !self.music,
            _ => {}
        }
    }

    /// Timer ticks of a game frame (12 minus the speed).
    pub fn frame_ticks(&self) -> u32 {
        12 - u32::from(self.speed.clamp(1, 7))
    }
}

/// The keys of the six game actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bindings {
    pub up: Key,
    pub down: Key,
    pub left: Key,
    pub right: Key,
    pub jump: Key,
    pub fire: Key,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            up: Key::Up,
            down: Key::Down,
            left: Key::Left,
            right: Key::Right,
            jump: Key::LeftCtrl,
            fire: Key::LeftAlt,
        }
    }
}

impl Bindings {
    /// How many there are.
    pub const COUNT: u8 = 6;

    /// Key `index` (1 to 6) in the order of the key configuration screen:
    /// fire, jump, up, down, left, right.
    pub fn get(&self, index: u8) -> Option<Key> {
        Some(*self.clone().slot(index)?)
    }

    pub fn slot(&mut self, index: u8) -> Option<&mut Key> {
        Some(match index {
            1 => &mut self.fire,
            2 => &mut self.jump,
            3 => &mut self.up,
            4 => &mut self.down,
            5 => &mut self.left,
            6 => &mut self.right,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub options: Options,
    pub bindings: Bindings,
    /// The skill level last chosen, 1 easy to 3 hard (0 before any).
    pub difficulty: u8,
}

impl Config {
    /// The store key.
    pub const KEY: &str = "config";

    /// Our file format: "DN2C", version 1, then the options (four flags,
    /// the speed), the six bindings as `u16` key codes (in the order of
    /// [`Bindings::get`]) and the difficulty.
    pub fn to_bytes(&self) -> Vec<u8> {
        let o = &self.options;
        let mut out = Vec::from(*b"DN2C");
        out.push(1);
        out.extend([
            u8::from(o.sound_blaster),
            u8::from(o.adlib),
            u8::from(o.pc_speaker),
            u8::from(o.music),
            o.speed,
        ]);
        for index in 1..=Bindings::COUNT {
            let key = self.bindings.get(index).unwrap_or(Key::Space);
            out.extend(key.code().to_le_bytes());
        }
        out.push(self.difficulty);
        out
    }

    /// Reads [`Config::to_bytes`]; `None` for a wrong header or version, a
    /// wrong length or a value out of range.
    pub fn parse(bytes: &[u8]) -> Option<Config> {
        let rest = bytes.strip_prefix(b"DN2C")?.strip_prefix(&[1])?;
        if rest.len() != 5 + 12 + 1 {
            return None;
        }
        let flag = |b: u8| (b <= 1).then_some(b == 1);
        let speed = rest[4];
        if !(1..=7).contains(&speed) || rest[17] > 3 {
            return None;
        }
        let options = Options {
            sound_blaster: flag(rest[0])?,
            adlib: flag(rest[1])?,
            pc_speaker: flag(rest[2])?,
            music: flag(rest[3])?,
            speed,
        };
        let mut bindings = Bindings::default();
        for index in 1..=Bindings::COUNT {
            let at = 5 + usize::from(index - 1) * 2;
            let code = u16::from_le_bytes([rest[at], rest[at + 1]]);
            *bindings.slot(index)? = Key::from_code(code)?;
        }
        Some(Config {
            options,
            bindings,
            difficulty: rest[17],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let mut config = Config::default();
        config.options.pc_speaker = true;
        config.options.speed = 7;
        config.bindings.fire = Key::F(12);
        config.bindings.jump = Key::Letter('Z');
        config.bindings.up = Key::Digit(0);
        config.difficulty = 3;
        assert_eq!(Config::parse(&config.to_bytes()), Some(config));
        let default = Config::default();
        assert_eq!(Config::parse(&default.to_bytes()), Some(default));
    }

    #[test]
    fn bad_config_is_none() {
        let bytes = Config::default().to_bytes();
        for len in 0..bytes.len() {
            assert_eq!(Config::parse(&bytes[..len]), None, "length {len}");
        }
        let mut long = bytes.clone();
        long.push(0);
        assert_eq!(Config::parse(&long), None);
        let mut version = bytes.clone();
        version[4] = 2;
        assert_eq!(Config::parse(&version), None);
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert_eq!(Config::parse(&magic), None);
        let mut key = bytes.clone();
        key[10] = 0xff;
        assert_eq!(Config::parse(&key), None);
        let mut speed = bytes;
        speed[9] = 0;
        assert_eq!(Config::parse(&speed), None);
    }
}
