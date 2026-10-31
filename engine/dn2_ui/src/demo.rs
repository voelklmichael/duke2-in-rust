//! The demo of the attract mode: its input is the file NUKEM2.MNI.

use alloc::vec::Vec;

use dn2_game::Buttons;

/// The level files the demo plays, in order: the first level of episode 1 and
/// its levels 3, 5 and 7. The demo runs at difficulty 3.
pub fn demo_levels() -> [&'static str; 4] {
    ["L1.MNI", "L3.MNI", "L5.MNI", "L7.MNI"]
}

/// What a demo byte says about a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoFrame {
    /// The buttons of the frame; bit 7 ends the level.
    Input { buttons: Buttons, exit_level: bool },
    /// The demo is over: the byte FFh, or the end of the file.
    End,
}

/// The input of NUKEM2.MNI, a byte per frame.
pub struct Demo {
    bytes: Vec<u8>,
    pos: usize,
}

impl Demo {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.into(),
            pos: 0,
        }
    }

    /// The next frame. Once the demo has ended it stays ended.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> DemoFrame {
        match self.bytes.get(self.pos).copied() {
            None | Some(0xff) => DemoFrame::End,
            Some(b) => {
                self.pos += 1;
                DemoFrame::Input {
                    buttons: Buttons::from_bits_truncate(b),
                    exit_level: b & 0x80 != 0,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_buttons() {
        let mut demo = Demo::new(&[0x00, 0x0c, 0x30, 0x81, 0xff, 0x01]);
        let input = |buttons, exit_level| DemoFrame::Input {
            buttons,
            exit_level,
        };
        assert_eq!(demo.next(), input(Buttons::empty(), false));
        assert_eq!(demo.next(), input(Buttons::LEFT | Buttons::RIGHT, false));
        assert_eq!(demo.next(), input(Buttons::JUMP | Buttons::FIRE, false));
        assert_eq!(demo.next(), input(Buttons::UP, true));
        // FFh ends it, and what follows is never read.
        assert_eq!(demo.next(), DemoFrame::End);
        assert_eq!(demo.next(), DemoFrame::End);
    }

    #[test]
    fn the_end_of_the_bytes_ends_the_demo() {
        let mut demo = Demo::new(&[0x02]);
        assert!(matches!(demo.next(), DemoFrame::Input { .. }));
        assert_eq!(demo.next(), DemoFrame::End);
        assert_eq!(Demo::new(&[]).next(), DemoFrame::End);
    }
}
