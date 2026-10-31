//! Actor sprites: ACTRINFO.MNI + ACTORS.MNI. See docs/FORMATS.md "Actor sprites".

use alloc::vec::Vec;

use crate::AssetError;
use crate::gfx::{self, Image, MASKED_TILE_BYTES};
use crate::reader::Reader;

/// The sprites of all actor ids (actors, shots and effects share ids).
#[derive(Clone, Debug, Default)]
pub struct Sprites {
    /// Indexed by actor id; `None` for ids without frames.
    pub actors: Vec<Option<ActorSprite>>,
    /// The draw pass (-1..3) of every id, also of those without frames
    /// (the water, the exit and other invisible objects): the level's actors
    /// are spawned pass by pass.
    pub passes: Vec<i8>,
    /// ACTRINFO.MNI and where each id's record starts in it, for
    /// [`Sprites::frame_box`]. Empty for sprites made in code.
    info: Vec<u8>,
    records: Vec<usize>,
}

/// A frame's offsets and size in tiles: what the game logic sees of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameBox {
    pub x_offset: i16,
    pub y_offset: i16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug)]
pub struct ActorSprite {
    /// When the actor is drawn relative to the others (-1..3).
    pub draw_pass: i8,
    pub frames: Vec<SpriteFrame>,
}

#[derive(Clone, Debug)]
pub struct SpriteFrame {
    /// Added to the actor's position, in tiles.
    pub x_offset: i16,
    pub y_offset: i16,
    /// Width and height are whole tiles x 8; transparent pixels are
    /// `gfx::TRANSPARENT`.
    pub image: Image,
}

impl Sprites {
    /// `info` is ACTRINFO.MNI, `data` is ACTORS.MNI. An id whose frame count
    /// is 0 has no sprite.
    pub fn parse(info: &[u8], data: &[u8]) -> Result<Self, AssetError> {
        let mut table = Reader::new("ACTRINFO.MNI", info);
        // The offset table ends where the first actor's records start.
        let ids = table.u16()? as usize;
        table.seek(0)?;
        let mut actors = Vec::with_capacity(ids);
        let mut passes = Vec::with_capacity(ids);
        let mut records = Vec::with_capacity(ids);
        for _ in 0..ids {
            let offset = table.u16()? as usize * 2;
            records.push(offset);
            actors.push(Self::actor(info, data, offset)?);
            // The record's second word, whether there are frames or not.
            let mut record = Reader::new("ACTRINFO.MNI", info);
            record.seek(offset + 2)?;
            passes.push(record.i16()?.clamp(-1, 3) as i8);
        }
        Ok(Self {
            actors,
            passes,
            info: info.to_vec(),
            records,
        })
    }

    /// Sprites made in code (tests, tools): no ACTRINFO to read boxes from,
    /// so [`Sprites::frame_box`] uses the frames.
    pub fn new(actors: Vec<Option<ActorSprite>>, passes: Vec<i8>) -> Self {
        Self {
            actors,
            passes,
            info: Vec::new(),
            records: Vec::new(),
        }
    }

    /// The box of frame `frame` of `id` as the original's game logic reads
    /// it: straight from the frame records, without checking the frame
    /// count. So an id without frames, or a frame past the last, gets the
    /// box of whatever record follows (the next id's), and the record's
    /// word index wraps at 16 bits. `None` only past the file's end.
    pub fn frame_box(&self, id: u16, frame: u16) -> Option<FrameBox> {
        if self.info.is_empty() {
            let f = self.get(id)?.frames.get(usize::from(frame))?;
            return Some(FrameBox {
                x_offset: f.x_offset,
                y_offset: f.y_offset,
                width: (f.image.width / 8) as u16,
                height: (f.image.height / 8) as u16,
            });
        }
        let start = *self.records.get(usize::from(id))?;
        let index = ((start / 2) as u16).wrapping_add(frame.wrapping_mul(8));
        let pos = usize::from(index) * 2;
        let word = |i: usize| {
            let at = pos + i * 2;
            Some(u16::from_le_bytes([
                *self.info.get(at)?,
                *self.info.get(at + 1)?,
            ]))
        };
        Some(FrameBox {
            x_offset: word(2)? as i16,
            y_offset: word(3)? as i16,
            height: word(4)?,
            width: word(5)?,
        })
    }

    /// The draw pass of an id, also if it has no sprite.
    pub fn draw_pass(&self, id: u16) -> Option<i8> {
        self.passes.get(usize::from(id)).copied()
    }

    pub fn get(&self, id: u16) -> Option<&ActorSprite> {
        self.actors.get(id as usize)?.as_ref()
    }

    /// The actor whose frame records start at byte `offset` of `info`.
    fn actor(info: &[u8], data: &[u8], offset: usize) -> Result<Option<ActorSprite>, AssetError> {
        let mut records = Reader::new("ACTRINFO.MNI", info);
        records.seek(offset)?;
        let count = records.u16()? as usize;
        let draw_pass = records.i16()?;
        if count == 0 {
            return Ok(None);
        }
        if !(-1..=3).contains(&draw_pass) {
            return Err(records.error("draw pass out of range"));
        }
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            records.seek(offset + i * 16 + 4)?;
            let (x_offset, y_offset) = (records.i16()?, records.i16()?);
            let (height, width) = (records.u16()? as u32, records.u16()? as u32);
            let start = records.u32()? as usize;
            let mut tiles = Reader::new("ACTORS.MNI", data);
            tiles.seek(start)?;
            let bytes = tiles.bytes((width * height) as usize * MASKED_TILE_BYTES)?;
            let mut image = Image::new(width * 8, height * 8, gfx::TRANSPARENT);
            for (n, tile) in gfx::decode_masked_tiles(bytes).iter().enumerate() {
                image.put_tile(n as u32 % width, n as u32 / width, tile);
            }
            frames.push(SpriteFrame {
                x_offset,
                y_offset,
                image,
            });
        }
        Ok(Some(ActorSprite {
            draw_pass: draw_pass as i8,
            frames,
        }))
    }
}

/// The colour font of the menus: the frames of sprite 1Dh, which are not masked
/// tiles but 8x16 glyphs of one colour plane, a mask byte and a data byte per
/// row.
#[derive(Clone, Debug, Default)]
pub struct ColourFont {
    /// Per glyph and row: (mask, data); a set mask bit keeps the background.
    pub glyphs: Vec<[(u8, u8); 16]>,
}

impl ColourFont {
    /// The font's sprite id.
    pub const SPRITE: usize = 0x1d;

    /// Reads the glyphs from ACTRINFO.MNI (`info`) and ACTORS.MNI (`data`).
    pub fn parse(info: &[u8], data: &[u8]) -> Result<Self, AssetError> {
        let mut table = Reader::new("ACTRINFO.MNI", info);
        table.seek(Self::SPRITE * 2)?;
        let offset = table.u16()? as usize * 2;
        table.seek(offset)?;
        let count = table.u16()? as usize;
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            table.seek(offset + i * 16 + 12)?;
            let mut bytes = Reader::new("ACTORS.MNI", data);
            bytes.seek(table.u32()? as usize)?;
            let raw = bytes.bytes(32)?;
            glyphs.push(core::array::from_fn(|row| (raw[row * 2], raw[row * 2 + 1])));
        }
        Ok(Self { glyphs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn words(words: &[u16]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn empty_ids_have_no_sprite() {
        // 2 ids; id 0 has no frames, id 1 one 2x1 frame (height 1, width 2).
        // Word offsets: id 0 -> word 2 (the empty record), id 1 -> word 10.
        let mut info = words(&[2, 10]);
        info.extend(words(&[0; 8]));
        info.extend(words(&[1, 2, 0xffff, 3, 1, 2, 0, 0]));
        let mut data = vec![0xff; 40]; // transparent
        data.extend(vec![0; 40]); // black
        let sprites = Sprites::parse(&info, &data).unwrap();
        assert_eq!(sprites.actors.len(), 2);
        assert!(sprites.get(0).is_none() && sprites.get(5).is_none());
        let sprite = sprites.get(1).unwrap();
        assert_eq!(sprite.draw_pass, 2);
        let frame = &sprite.frames[0];
        assert_eq!((frame.x_offset, frame.y_offset), (-1, 3));
        assert_eq!((frame.image.width, frame.image.height), (16, 8));
        assert_eq!(frame.image.get(0, 0), gfx::TRANSPARENT);
        assert_eq!(frame.image.get(8, 0), 0);
    }

    #[test]
    fn tiles_go_row_by_row() {
        // One id, one frame of 2 tiles wide, 2 high; only tile 1 is opaque.
        let info = words(&[1, 1, 0, 0, 0, 2, 2, 0, 0]);
        let mut data = vec![0xff; 4 * MASKED_TILE_BYTES];
        data[MASKED_TILE_BYTES..2 * MASKED_TILE_BYTES].fill(0);
        let sprites = Sprites::parse(&info, &data).unwrap();
        let image = &sprites.get(0).unwrap().frames[0].image;
        assert_eq!((image.width, image.height), (16, 16));
        assert_eq!(image.get(8, 0), 0); // tile 1 is top right
        assert_eq!(image.get(0, 8), gfx::TRANSPARENT);
    }

    #[test]
    fn tiles_past_the_end_are_an_error() {
        let info = words(&[1, 1, 0, 0, 0, 1, 1, 0, 0]);
        assert!(Sprites::parse(&info, &[0; 39]).is_err());
    }
}
