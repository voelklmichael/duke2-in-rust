//! Keys by name. The engine maps them to the game's buttons with the
//! bindings of the configuration; a gamepad sets buttons directly.

/// The keys without a parameter, in the order of their [`Key::code`]s 1 to 21.
const NAMED: [Key; 21] = [
    Key::Up,
    Key::Down,
    Key::Left,
    Key::Right,
    Key::Enter,
    Key::Escape,
    Key::Space,
    Key::Backspace,
    Key::Tab,
    Key::LeftCtrl,
    Key::RightCtrl,
    Key::LeftAlt,
    Key::RightAlt,
    Key::LeftShift,
    Key::RightShift,
    Key::Insert,
    Key::Delete,
    Key::Home,
    Key::End,
    Key::PageUp,
    Key::PageDown,
];

/// The keys the game knows. Frontends translate their own key codes; keys
/// not listed here are not passed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    /// F1-F12.
    F(u8),
    /// 'A'-'Z', upper case.
    Letter(char),
    /// '0'-'9'.
    Digit(u8),
}

impl Key {
    /// The name the key configuration shows.
    pub fn name(self) -> &'static str {
        const LETTERS: [&str; 26] = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
            "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
        ];
        const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
        const F: [&str; 12] = [
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        ];
        match self {
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Enter => "ENTR",
            Key::Escape => "ESC",
            Key::Space => "SPACE",
            Key::Backspace => "BKSP",
            Key::Tab => "TAB",
            Key::LeftCtrl | Key::RightCtrl => "CTRL",
            Key::LeftAlt | Key::RightAlt => "ALT",
            Key::LeftShift => "LSHFT",
            Key::RightShift => "RSHFT",
            Key::Insert => "INS",
            Key::Delete => "DEL",
            Key::Home => "HOME",
            Key::End => "END",
            Key::PageUp => "PGUP",
            Key::PageDown => "PGDN",
            Key::F(n) => F.get(usize::from(n).wrapping_sub(1)).copied().unwrap_or(""),
            Key::Letter(c) => LETTERS
                .get((c as usize).wrapping_sub('A' as usize))
                .copied()
                .unwrap_or(""),
            Key::Digit(d) => DIGITS.get(usize::from(d)).copied().unwrap_or(""),
        }
    }

    /// A stable number for the key, for files: named keys 1 to 21, then
    /// 0x100 + n for F(n), 0x200 + c for a letter, 0x300 + d for a digit.
    pub fn code(self) -> u16 {
        match self {
            Key::F(n) => 0x100 + u16::from(n),
            Key::Letter(c) => 0x200 + c as u16,
            Key::Digit(d) => 0x300 + u16::from(d),
            named => NAMED
                .iter()
                .position(|&k| k == named)
                .map_or(0, |i| i as u16 + 1),
        }
    }

    /// The key of a [`Key::code`]; `None` for a number no key has.
    pub fn from_code(code: u16) -> Option<Key> {
        let low = (code & 0xff) as u8;
        match code >> 8 {
            0 => NAMED.get(usize::from(code).checked_sub(1)?).copied(),
            1 if (1..=12).contains(&low) => Some(Key::F(low)),
            2 if low.is_ascii_uppercase() => Some(Key::Letter(low as char)),
            3 if low <= 9 => Some(Key::Digit(low)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        let mut keys = NAMED.to_vec();
        keys.extend((1..=12).map(Key::F));
        keys.extend(('A'..='Z').map(Key::Letter));
        keys.extend((0..=9).map(Key::Digit));
        let mut codes = alloc::vec::Vec::new();
        for key in keys {
            assert_eq!(Key::from_code(key.code()), Some(key));
            codes.push(key.code());
        }
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), 21 + 12 + 26 + 10);
    }

    #[test]
    fn bad_codes_are_none() {
        for code in [0, 22, 0x100, 0x10d, 0x240, 0x20a, 0x30a, 0x400, 0xffff] {
            assert_eq!(Key::from_code(code), None, "{code:#x}");
        }
    }
}
