//! Tilesets: CZONE*.MNI. See docs/FORMATS.md "Tilesets".

use alloc::vec::Vec;

use crate::AssetError;
use crate::gfx::{self, MASKED_TILE_BYTES, SOLID_TILE_BYTES, Tile};
use crate::reader::Reader;

bitflags::bitflags! {
    /// What a tile does (collision and actor code, `render_frame`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct TileAttributes: u16 {
        /// Solid from above: blocks downward movement (floors).
        const SOLID_TOP = 0x0001;
        /// Solid from below: blocks upward movement (ceilings).
        const SOLID_BOTTOM = 0x0002;
        /// Solid on its right side: blocks movement to the left.
        const SOLID_RIGHT = 0x0004;
        /// Solid on its left side: blocks movement to the right.
        const SOLID_LEFT = 0x0008;
        /// Animated: the next 3 tiles are its other frames.
        const ANIMATED = 0x0010;
        /// Masked tile drawn in front of the sprites.
        const IN_FRONT = 0x0020;
        /// Destroyed by explosions.
        const DESTRUCTIBLE = 0x0040;
        /// A pipe the player grabs and hangs from.
        const HANGABLE = 0x0080;
        /// Conveyor belt moving whatever stands on it left.
        const CONVEYOR_LEFT = 0x0100;
        /// Conveyor belt moving whatever stands on it right.
        const CONVEYOR_RIGHT = 0x0200;
        /// With `ANIMATED`: the frame changes only every second step.
        const SLOW_ANIMATION = 0x0400;
        /// A ladder or similar: the player can climb it.
        const CLIMBABLE = 0x4000;
        /// Solid on any side.
        const SOLID = Self::SOLID_TOP.bits()
            | Self::SOLID_BOTTOM.bits()
            | Self::SOLID_RIGHT.bits()
            | Self::SOLID_LEFT.bits();
        const _ = !0;
    }
}

const SOLID_TILES: usize = 1000;
const MASKED_TILES: usize = 160;
/// u16 attribute words per masked tile; only the first is used.
const MASKED_WORDS: usize = 5;

#[derive(Clone, Debug)]
pub struct Tileset {
    /// 1000 solid tiles.
    pub solid: Vec<Tile>,
    /// 160 masked tiles (transparent pixels are `gfx::TRANSPARENT`).
    pub masked: Vec<Tile>,
    /// One per solid tile.
    pub solid_attributes: Vec<TileAttributes>,
    /// One per masked tile: the first of its 5 attribute words.
    pub masked_attributes: Vec<TileAttributes>,
}

impl Tileset {
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut reader = Reader::new(file, bytes);
        let mut solid_attributes = Vec::with_capacity(SOLID_TILES);
        for _ in 0..SOLID_TILES {
            solid_attributes.push(TileAttributes::from_bits_retain(reader.u16()?));
        }
        // The game finds a masked tile's attributes at word 1000 + 5 * index
        // (map cell / 8), so the first word counts; the other four are
        // never read.
        let mut masked_attributes = Vec::with_capacity(MASKED_TILES);
        for _ in 0..MASKED_TILES {
            masked_attributes.push(TileAttributes::from_bits_retain(reader.u16()?));
            reader.bytes((MASKED_WORDS - 1) * 2)?;
        }
        let solid = gfx::decode_solid_tiles(reader.bytes(SOLID_TILES * SOLID_TILE_BYTES)?);
        let masked = gfx::decode_masked_tiles(reader.bytes(MASKED_TILES * MASKED_TILE_BYTES)?);
        Ok(Self {
            solid,
            masked,
            solid_attributes,
            masked_attributes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn file() -> Vec<u8> {
        let mut bytes = vec![0; 3600 + 32000 + 6400];
        bytes[2] = 0x11; // solid tile 1: SOLID_TOP | ANIMATED
        bytes[2020] = 0x20; // masked tile 2, word 0
        bytes[2021] = 0x40; // ... = 0x4020
        bytes[2022] = 0x01; // masked tile 2, word 1: ignored
        bytes[3600] = 0x80; // solid tile 0, row 0: blue, leftmost
        bytes[3600 + 32000] = 0xff; // masked tile 0, row 0: all transparent
        bytes
    }

    #[test]
    fn parses_attributes_and_tiles() {
        let t = Tileset::parse("T", &file()).unwrap();
        assert_eq!((t.solid.len(), t.masked.len()), (1000, 160));
        assert_eq!(
            t.solid_attributes[1],
            TileAttributes::SOLID_TOP | TileAttributes::ANIMATED
        );
        assert_eq!(
            t.masked_attributes[2],
            TileAttributes::IN_FRONT | TileAttributes::CLIMBABLE
        );
        assert_eq!(t.solid[0][0], 1);
        assert_eq!(t.masked[0][0], gfx::TRANSPARENT);
    }

    #[test]
    fn keeps_unknown_bits() {
        let mut bytes = file();
        bytes[1] = 0x80;
        let t = Tileset::parse("T", &bytes).unwrap();
        assert_eq!(t.solid_attributes[0].bits(), 0x8000);
    }

    #[test]
    fn short_file_is_an_error() {
        assert!(Tileset::parse("T", &file()[..10000]).is_err());
    }
}
