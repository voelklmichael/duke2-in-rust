//! The status line's messages: hints the game types out when the player picks
//! something up or reaches something. Some show only once a game. The text is
//! typed out a letter a frame (at the end of `actors::update`); `*` breaks the
//! line.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{GameEvent, Sound};

/// Texts; `*` breaks the line.
pub mod text {
    pub const ROCKET_LAUNCHER: &str =
        "A ROCKET LAUNCHER IS VERY*DEADLY. IT IS THE STRONGEST OF*ANY WEAPON!";
    pub const FLAME_THROWER: &str =
        "THERE ARE MANY SECRETS WITH*THIS WEAPON. USE IT TO*YOUR ADVANTAGE!";
    pub const REGULAR_WEAPON: &str = "THIS IS YOUR REGULAR WEAPON!";
    pub const LASER: &str = "THIS WEAPON CAN SHOOT*THROUGH ANYTHING!";
    pub const SODA: &str = "THE CAFFEINE IN SODAS PROVIDES*ONE UNIT OF HEALTH.";
    pub const HEALTH: &str = "THIS ITEM WILL GIVE YOU ONE UNIT*OF HEALTH.";
    pub const LETTER_N: &str = "GOT THE N.  WHAT IS NEXT?";
    pub const LETTER_U: &str = "GOT THE U.";
    pub const LETTER_K: &str = "GOT THE K.";
    pub const LETTER_E: &str = "GOT THE E.";
    pub const ALL_LETTERS: &str = "EXCELLENT!  ONE HUNDRED THOUSAND*POINTS!!!!!!!!!!!";
    pub const CLOAK: &str = "YOU ARE NOW INVINCIBLE FOR A SHORT*PERIOD OF TIME...**USE THIS TO ALSO DISABLE*THE FORCE FIELD...";
    pub const KEY: &str = "FIND THE DOOR THAT*THIS KEY OPENS.";
    pub const ACCESS_CARD: &str = "USE THE CARD TO TURN OFF*FORCE FIELDS.";
    pub const RAPID_FIRE: &str = "HOLD DOWN YOUR FIRE BUTTON FOR*RAPID FIRE.";
    pub const CLOAK_ENDING: &str = "CLOAK IS DISABLING...";
    pub const RAPID_FIRE_ENDING: &str = "RAPID FIRE IS DISABLING...";
    pub const FORCE_FIELD_DESTROYED: &str = "FORCE FIELD DESTROYED... *GOOD WORK...";
    pub const EARTHQUAKE: &str = " E A R T H Q U A K E ! ! !*WAIT, THIS IS NOT EARTH.";
    pub const DESTROYED_EVERYTHING: &str = "DUKE... YOU HAVE DESTROYED*EVERYTHING.  EXCELLENT...";
    pub const SECTOR_SECURE: &str = "SECTOR SECURE!!!";
    pub const EXIT_RADAR: &str = " WAIT!!!!!!!!      *YOU NEED TO DESTROY ALL THE RADAR*DISHES FIRST BEFORE YOU CAN COMPLETE*THE LEVEL...";
    pub const TURBO_LIFT: &str = "PRESS UP OR DOWN TO USE THE*TURBO LIFT.";
    pub const USE_CARD: &str = "USE THE ACCESS CARD TO DISABLE*THIS FORCE FIELD.";
    pub const ACCESS_DENIED: &str = "ACCESS DENIED.";
    pub const ACCESS_GRANTED: &str = "ACCESS GRANTED.";
    pub const USE_KEY: &str = "USE A KEY TO OPEN THIS DOOR.";
    pub const NEED_KEY: &str = "YOU NEED A KEY TO OPEN*THE DOOR.";
    pub const OPENING_DOOR: &str = "OPENING DOOR.";
    pub const HINT_MACHINE: &str =
        "THIS DEVICE WILL GIVE SPECIFIC HINTS.*FIND THE SPECIAL BLUE GLOBE AND*BRING IT BACK HERE.";
    pub const TRANSPORTER: &str = "PRESS UP OR ENTER TO USE*THE TRANSPORTER.*";
    /// At the start of a level with radar dishes.
    pub const RADAR: &str = "DUKE, FIND AND DESTROY ALL THE*RADAR DISHES ON THIS LEVEL.";
    /// At the start of a game on its first level.
    pub const WELCOME: &str = "WELCOME TO DUKE NUKEM II!";
    pub const SHIP: &str = "THE SHIP. USE THIS TO GET OUT OF THIS*LEVEL.  YOU CAN FLY ANYWHERE, AND*YOUR WEAPON IS VERY POWERFUL.";
    pub const FORCE_FIELD: &str =
        "OUCH, YOU NEED TO FIND THE CLOAKING*DEVICE TO DISABLE THIS FORCEFIELD.";
    pub const HINT_GLOBE: &str = "BRING THIS SPECIAL CRYSTAL GLOBE*BACK TO THE PEDESTAL TO RECEIVE A*SPECIAL HINT FOR THIS LEVEL.";
}

/// Messages shown once per game, by the original's flag numbers.
pub mod once {
    pub const RAPID_FIRE: u8 = 0;
    pub const HEALTH: u8 = 1;
    pub const REGULAR_WEAPON: u8 = 2;
    pub const LASER: u8 = 3;
    pub const FLAME_THROWER: u8 = 4;
    pub const ROCKET_LAUNCHER: u8 = 5;
    pub const EARTHQUAKE: u8 = 6;
    pub const KEY: u8 = 7;
    pub const ACCESS_CARD: u8 = 8;
    pub const SHIP: u8 = 9;
    pub const LETTER_N: u8 = 0x0a;
    pub const LETTER_U: u8 = 0x0b;
    pub const LETTER_K: u8 = 0x0c;
    pub const LETTER_E: u8 = 0x0d;
    pub const NEED_KEY: u8 = 0x0e;
    pub const ACCESS_DENIED: u8 = 0x0f;
    pub const FORCE_FIELD: u8 = 0x10;
    pub const EXIT_RADAR: u8 = 0x11;
    pub const HINT_MACHINE: u8 = 0x12;
    pub const TURBO_LIFT: u8 = 0x13;
    pub const TRANSPORTER: u8 = 0x14;
    pub const ALL_LETTERS: u8 = 0x15;
    pub const SODA: u8 = 0x16;
    pub const USE_CARD: u8 = 0x17;
    pub const USE_KEY: u8 = 0x18;
}

/// A line holds this many letters; one that fills it pauses like one that
/// ends.
const LINE_LENGTH: u16 = 0x26;
/// Frames a finished line stays on screen.
const PAUSE: u16 = 0x15;
/// The messages only show on the first levels of an episode (counted from
/// 1).
pub const LAST_MESSAGE_LEVEL: u8 = 7;

/// The typing state of the status line.
#[derive(Clone, Debug, Default)]
pub struct Messages {
    /// The message being typed, or to type next.
    pub current: Option<Cow<'static, str>>,
    /// How much of `current` is typed.
    pos: usize,
    /// 0 when idle, else the column being typed, from 1.
    column: u16,
    /// Frames left of the pause after a line.
    pause: u16,
    /// Messages cannot start while this is set (the hint machine).
    pub blocked: bool,
    /// No messages at all: the level is past [`LAST_MESSAGE_LEVEL`].
    pub disabled: bool,
    /// One-shot messages already shown, one bit each.
    pub shown: u32,
}

impl Messages {
    /// Starts typing `text`. Does nothing while blocked or on the later levels.
    pub fn show(&mut self, text: &'static str) {
        if self.blocked || self.disabled {
            return;
        }
        self.start(Cow::Borrowed(text));
    }

    /// Types a text made at run time (the hint machine's), also while
    /// messages are blocked or disabled.
    pub fn show_text(&mut self, text: String) {
        self.start(Cow::Owned(text));
    }

    fn start(&mut self, text: Cow<'static, str>) {
        self.current = Some(text);
        self.pos = 0;
        self.column = 1;
        self.pause = 0;
    }

    /// Shows `text` unless the one-shot message `id` was shown before.
    pub fn show_once(&mut self, id: u8, text: &'static str) {
        if self.shown & (1 << id) == 0 {
            self.shown |= 1 << id;
            self.show(text);
        }
    }

    /// The letter at the typing position; 0 past the end.
    fn letter(&self) -> u8 {
        self.current
            .as_ref()
            .and_then(|t| t.as_bytes().get(self.pos))
            .copied()
            .unwrap_or(0)
    }

    /// The letters typed so far on the line, for the status line (`*` and
    /// spaces are blank tiles).
    pub fn line(&self) -> &[u8] {
        let Some(text) = &self.current else {
            return &[];
        };
        let typed = usize::from(self.column.saturating_sub(1));
        text.as_bytes()
            .get(self.pos.saturating_sub(typed)..self.pos)
            .unwrap_or(&[])
    }

    /// The typing part of the actors' update: one letter a frame, then a pause
    /// once the line is full or ends before a `*` or the end of the text.
    pub fn update(&mut self, events: &mut Vec<GameEvent>) {
        if self.column == 0 {
            return;
        }
        if self.pause != 0 {
            self.pause -= 1;
            if self.pause == 0 {
                self.column = 1;
                if self.letter() == 0 {
                    self.column = 0;
                    self.blocked = false;
                    self.current = None;
                }
            }
            return;
        }
        let letter = self.letter();
        if letter != 0 {
            if letter != b' ' && letter != b'*' {
                events.push(GameEvent::Sound(Sound::Type));
            }
            self.column += 1;
            self.pos += 1;
        }
        let next = self.letter();
        if self.column == LINE_LENGTH || next == 0 || next == b'*' {
            self.pause = PAUSE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(m: &mut Messages, frames: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        for _ in 0..frames {
            m.update(&mut events);
        }
        events
    }

    #[test]
    fn types_a_letter_a_frame_with_a_sound() {
        let mut m = Messages::default();
        m.show("AB C");
        let events = typed(&mut m, 1);
        assert_eq!(m.line(), b"A");
        assert_eq!(events, [GameEvent::Sound(Sound::Type)]);
        // The space makes no sound.
        let events = typed(&mut m, 2);
        assert_eq!(m.line(), b"AB ");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn a_star_breaks_the_line_after_a_pause() {
        let mut m = Messages::default();
        m.show("HI*YO");
        typed(&mut m, 2);
        assert_eq!(m.line(), b"HI");
        // The pause, then the star as a blank first column.
        typed(&mut m, usize::from(PAUSE));
        assert_eq!(m.line(), b"");
        typed(&mut m, 1);
        assert_eq!(m.line(), b"*");
        typed(&mut m, 2);
        assert_eq!(m.line(), b"*YO");
    }

    #[test]
    fn the_message_ends_and_frees_the_line() {
        let mut m = Messages::default();
        m.show("A");
        typed(&mut m, 1 + usize::from(PAUSE));
        assert_eq!(m.current, None);
        assert_eq!(m.line(), b"");
        typed(&mut m, 5);
        assert_eq!(m.line(), b"");
    }

    #[test]
    fn once_only_shows_the_first_time() {
        let mut m = Messages::default();
        m.show_once(once::KEY, "A");
        typed(&mut m, 1 + usize::from(PAUSE));
        m.show_once(once::KEY, "B");
        assert_eq!(m.current, None);
    }

    #[test]
    fn later_levels_have_no_messages() {
        let mut m = Messages {
            disabled: true,
            ..Messages::default()
        };
        m.show("A");
        assert_eq!(m.current, None);
    }
}
