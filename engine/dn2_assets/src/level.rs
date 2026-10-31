//! Levels: L1-L8, M1-M8, N1-N8, O1-O8.MNI. See docs/FORMATS.md "Levels".

use alloc::string::String;
use alloc::vec::Vec;

use crate::AssetError;
use crate::reader::Reader;

bitflags::bitflags! {
    /// The header's flags byte.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct LevelFlags: u8 {
        /// The backdrop follows the camera at half speed in both directions.
        const PARALLAX_BOTH = 0x01;
        /// The backdrop follows the camera at half speed horizontally only.
        const PARALLAX_HORIZONTAL = 0x02;
        /// The backdrop scrolls by itself, wrapping around.
        const AUTO_SCROLL = 0x08;
        /// The backdrop scrolls by itself vertically and follows the camera.
        const AUTO_SCROLL_VERTICAL = 0x10;
        /// Random earthquakes shake the screen.
        const EARTHQUAKE = 0x20;
        /// A switch sets the backdrop's alternate state for good; with
        /// `PARALLAX_HORIZONTAL`, a "destroyed every ..." message follows.
        const BACKDROP_SWITCH_ONCE = 0x40;
        /// A switch toggles the backdrop between its two states.
        const BACKDROP_SWITCH_TOGGLE = 0x80;
        const _ = !0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelActor {
    pub kind: u16,
    /// In tiles.
    pub x: u16,
    pub y: u16,
}

/// What a map cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// The backdrop shows through.
    Empty,
    /// A solid tile, by index into `Tileset::solid`.
    Solid(u16),
    /// A masked tile over the backdrop, by index into `Tileset::masked`.
    Masked(u16),
    /// A masked tile over a solid tile.
    SolidAndMasked { solid: u16, masked: u16 },
}

#[derive(Clone, Debug)]
pub struct Level {
    /// File names from the header, trimmed and upper case like the
    /// archive's (e.g. "CZONE1.MNI").
    pub tileset: String,
    pub backdrop: String,
    pub music: String,
    pub flags: LevelFlags,
    pub alternate_backdrop: u8,
    pub actors: Vec<LevelActor>,
    /// In tiles.
    pub width: u16,
    pub height: u16,
    /// The 32750 raw map words, row by row.
    pub cells: Vec<u16>,
    /// The extra layer unpacked: 2 bits per cell, one byte per cell.
    pub extra: Vec<u8>,
}

/// Cells in the stored map.
const MAP_CELLS: usize = 32750;

/// A fixed-size name: up to the NUL, without trailing spaces, upper case.
fn name(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])
        .trim_end_matches(' ')
        .to_ascii_uppercase()
}

/// The extra layer's RLE: a signed count n; n > 0 repeats the next byte n
/// times, n < 0 copies the next -n bytes, 0 ends.
fn unpack(reader: &mut Reader) -> Result<Vec<u8>, AssetError> {
    let mut out = Vec::new();
    loop {
        match reader.u8()? as i8 {
            0 => return Ok(out),
            n @ 1.. => {
                let byte = reader.u8()?;
                out.resize(out.len() + n as usize, byte);
            }
            n => out.extend_from_slice(reader.bytes(n.unsigned_abs().into())?),
        }
    }
}

impl Level {
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut reader = Reader::new(file, bytes);
        let header_len = usize::from(reader.u16()?);
        let header = reader.bytes(header_len)?;
        if header_len < 0x2d + 2 {
            return Err(reader.error("level header too short"));
        }
        let mut head = Reader::new(file, header);
        let names = head.bytes(0x27)?;
        let flags = LevelFlags::from_bits_retain(head.u8()?);
        let alternate_backdrop = head.u8()?;
        head.seek(0x2b)?;
        let words = usize::from(head.u16()?);
        let actors = (0..words / 3)
            .map(|_| {
                Ok(LevelActor {
                    kind: head.u16()?,
                    x: head.u16()?,
                    y: head.u16()?,
                })
            })
            .collect::<Result<Vec<_>, AssetError>>()?;
        head.seek(header_len - 2)?;
        let width = head.u16()?;
        if !(32..=1024).contains(&width) || !width.is_power_of_two() {
            return Err(reader.error("bad level width"));
        }

        let cells = reader
            .bytes(MAP_CELLS * 2)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        let packed_len = usize::from(reader.u16()?);
        let packed = reader.bytes(packed_len)?;
        let mut extra = Vec::new();
        for byte in unpack(&mut Reader::new(file, packed))? {
            extra.extend((0..4).map(|i| (byte >> (2 * i)) & 3));
        }

        Ok(Self {
            tileset: name(&names[..13]),
            backdrop: name(&names[13..26]),
            music: name(&names[26..39]),
            flags,
            alternate_backdrop,
            actors,
            width,
            height: 0x8000 / width - 2,
            cells,
            extra,
        })
    }

    /// The cell at tile `x`, `y`; [`Cell::Empty`] outside the stored cells.
    /// Rows past `height` that still fit in the stored cells are returned
    /// (the game reads row `height` itself). The packed solid index of a
    /// layered cell has 10 bits; one level uses 1008, which is clamped.
    pub fn cell(&self, x: u16, y: u16) -> Cell {
        let i = usize::from(y) * usize::from(self.width) + usize::from(x);
        let value = self.cells.get(i).copied().unwrap_or(0);
        match value {
            0 => Cell::Empty,
            1..8000 => Cell::Solid(value / 8),
            8000..0x8000 => Cell::Masked((value - 8000) / 40),
            _ => {
                let extra = u16::from(self.extra.get(i).copied().unwrap_or(0));
                Cell::SolidAndMasked {
                    solid: (value & 0x3ff).min(999),
                    masked: ((value >> 10) & 0x1f) + extra * 0x20,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn file(width: u16, actors: &[[u16; 3]], cells: &[(usize, u16)], rle: &[u8]) -> Vec<u8> {
        let mut head = vec![0u8; 0x2d];
        head[..11].copy_from_slice(b"CZONE1.MNI\0");
        head[0x0d..0x14].copy_from_slice(b"DROP1  ");
        head[0x27] = 0x21;
        head[0x28] = 3;
        head[0x2b..0x2d].copy_from_slice(&(actors.len() as u16 * 3).to_le_bytes());
        for word in actors.iter().flatten() {
            head.extend(word.to_le_bytes());
        }
        head.extend(width.to_le_bytes());
        let mut map = vec![0u8; MAP_CELLS * 2];
        for &(i, v) in cells {
            map[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        let mut out = (head.len() as u16).to_le_bytes().to_vec();
        out.extend(head);
        out.extend(map);
        out.extend((rle.len() as u16).to_le_bytes());
        out.extend(rle);
        out
    }

    #[test]
    fn header_and_actors() {
        let bytes = file(64, &[[1, 2, 3], [4, 5, 6]], &[], &[0]);
        let level = Level::parse("T.MNI", &bytes).unwrap();
        assert_eq!(level.tileset, "CZONE1.MNI");
        assert_eq!(level.backdrop, "DROP1");
        assert_eq!(level.music, "");
        assert_eq!(
            level.flags,
            LevelFlags::EARTHQUAKE | LevelFlags::PARALLAX_BOTH
        );
        assert_eq!(level.alternate_backdrop, 3);
        assert_eq!(level.actors.len(), 2);
        assert_eq!(
            level.actors[1],
            LevelActor {
                kind: 4,
                x: 5,
                y: 6
            }
        );
        assert_eq!((level.width, level.height), (64, 510));
        assert_eq!(level.cells.len(), MAP_CELLS);
    }

    #[test]
    fn cell_kinds() {
        // Extra layer: a literal byte (2 bits per cell, low bits first), then
        // a run of two bytes.
        let rle = [0xff, 0b1110_0100, 2, 0b01, 0];
        let cells = [
            (1, 80),
            (2, 8000 + 40 * 5 + 39),
            (3, 0x8000 | 7 << 10 | 12),
            (4, 0x8000 | 7 << 10 | 12),
            (5, 0x8000 | 31 << 10 | 999),
        ];
        let level = Level::parse("T.MNI", &file(32, &[], &cells, &rle)).unwrap();
        assert_eq!(level.extra, [0, 1, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(level.cell(0, 0), Cell::Empty);
        assert_eq!(level.cell(1, 0), Cell::Solid(10));
        assert_eq!(level.cell(2, 0), Cell::Masked(5));
        assert_eq!(
            level.cell(3, 0),
            Cell::SolidAndMasked {
                solid: 12,
                masked: 7 + 96
            }
        );
        assert_eq!(
            level.cell(4, 0),
            Cell::SolidAndMasked {
                solid: 12,
                masked: 7 + 32
            }
        );
        assert_eq!(
            level.cell(5, 0),
            Cell::SolidAndMasked {
                solid: 999,
                masked: 31
            }
        );
    }

    #[test]
    fn rows_past_height() {
        // Width 1024: height 30, but 31 rows and a bit are stored.
        let level = Level::parse("T.MNI", &file(1024, &[], &[(30 * 1024, 8)], &[0])).unwrap();
        assert_eq!(level.height, 30);
        assert_eq!(level.cell(0, 30), Cell::Solid(1));
        assert_eq!(level.cell(0, 31), Cell::Empty);
        assert_eq!(level.cell(1023, 31), Cell::Empty);
        assert_eq!(level.cell(0, 32), Cell::Empty);
    }

    #[test]
    fn bad_input() {
        assert!(Level::parse("T.MNI", &[]).is_err());
        let mut bytes = file(64, &[], &[], &[0]);
        assert!(Level::parse("T.MNI", &bytes[..1000]).is_err());
        bytes[2 + 0x2d] = 48;
        assert!(Level::parse("T.MNI", &bytes).is_err());
    }
}
