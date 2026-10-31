//! What the player holds during a game frame.

bitflags::bitflags! {
    /// The game's controls; the bits are those of the original demo file.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Buttons: u8 {
        const UP = 1 << 0;
        const DOWN = 1 << 1;
        const LEFT = 1 << 2;
        const RIGHT = 1 << 3;
        const JUMP = 1 << 4;
        const FIRE = 1 << 5;
    }
}
