//! Text, boxes and pictures of the screens around the game.
//!
//! The original keeps the status sheet (STATUS.MNI) in video memory and draws
//! the menu fonts and the frames of text boxes by copying 8x8 tiles from it
//! (solid tiles and friends). Positions are in tiles as in the original: `row`,
//! `col`.

use alloc::format;
use alloc::string::String;

use dn2_assets::gfx::Tile;
use dn2_assets::hd::{FONT_GLYPH_WIDTH, HdArt};
use dn2_assets::images::TileSheet;
use dn2_assets::sprites::{ColourFont, Sprites};
use dn2_render::hud::{hd_for, sprite_picture, status_picture, tile_at};
use dn2_render::{HdPicture, Screen};

/// Offset of the tile below another in the status sheet.
const NEXT_ROW: u16 = 0x140;

/// The text box pieces in the status sheet: top left, top right, bottom left,
/// bottom right.
const BOX_CORNERS: [u16; 4] = [0x500, 0x510, 0x530, 0x520];
const BOX_TOP: u16 = 0x508;
const BOX_RIGHT: u16 = 0x518;
const BOX_BOTTOM: u16 = 0x528;
const BOX_LEFT: u16 = 0x538;
/// The blank fill of boxes.
pub const BOX_FILL: u16 = 0x540;
/// A blank 2x2 picture: the erased menu cursor.
pub const BLANK_2X2: u16 = 0x640;
/// The first of the 8 pictures of the spinning menu cursor, 10h apart.
pub const CURSOR: u16 = 0xb40;
/// The blinking text cursor.
pub const TEXT_CURSOR: u16 = 0x548;
/// Check boxes of the options menu, off and on (2x2).
pub const CHECK_OFF: u16 = 0x960;
pub const CHECK_ON: u16 = 0x970;

/// The glyph of the colour font drawn for characters it does not have.
const BLANK_GLYPH: usize = 0x28;

/// The row of the save slot `index` (from 0) in the slot list, and of its
/// name in the save and restore screens.
pub fn slot_row(index: i32) -> i32 {
    index * 2 + 6
}

/// What the screens draw with.
#[derive(Clone, Copy)]
pub struct Gfx<'a> {
    pub status: &'a TileSheet,
    pub sprites: &'a Sprites,
    pub font: &'a ColourFont,
    /// Higher-detail pictures of the status sheet and the sprites.
    pub hd: &'a HdArt,
}

/// Copies an 8x8 tile to pixel `x`, `y`, clipped to the screen, with its
/// higher-detail picture if there is one.
fn put_tile(screen: &mut Screen, tile: &Tile, hd: Option<HdPicture>, x: i32, y: i32) {
    let scale = screen.hd_scale().unwrap_or(1);
    for (dy, row) in tile.as_chunks::<8>().0.iter().enumerate() {
        for (dx, &pixel) in row.iter().enumerate() {
            let (px, py) = (x + dx as i32, y + dy as i32);
            match hd {
                Some(hd) => {
                    let block = dn2_render::HdBlock {
                        image: hd.image,
                        x: hd.x + dx as u32 * scale,
                        y: hd.y + dy as u32 * scale,
                        color: None,
                    };
                    screen.plot_hd(px + screen.left, py, Some(pixel), block);
                }
                None => screen.put(px, py, pixel),
            }
        }
    }
}

impl Gfx<'_> {
    /// The tile of the status sheet at video memory offset `offset`.
    pub fn solid(&self, screen: &mut Screen, row: i32, col: i32, offset: u16) {
        let hd = status_picture(hd_for(screen, self.hd), offset);
        put_tile(screen, &tile_at(self.status, offset), hd, col * 8, row * 8);
    }

    /// A 2x2 block of solid tiles.
    pub fn solid_2x2(&self, screen: &mut Screen, row: i32, col: i32, offset: u16) {
        for (dr, dc, o) in [
            (0, 0, offset),
            (1, 0, offset.wrapping_add(NEXT_ROW)),
            (0, 1, offset.wrapping_add(8)),
            (1, 1, offset.wrapping_add(8 + NEXT_ROW)),
        ] {
            self.solid(screen, row + dr, col + dc, o);
        }
    }

    /// A character of the orange 8x8 font.
    pub fn small_char(&self, screen: &mut Screen, ch: u8, row: i32, col: i32) {
        let ch = i32::from(if ch == b'_' { 0x1f } else { ch });
        let offset = match ch {
            ..0x3e => (ch - 0x16) * 8 + 0x1a40,
            0x3e..0x5b => (ch - 0x3e) * 8 + 0x1b80,
            0x5b..0x6c => (ch - 0x61) * 8 + 0x1c68,
            _ => (ch - 0x6c) * 8 + 0x1d48,
        };
        self.solid(screen, row, col, offset as u16);
    }

    /// The big 16x16 font (colour 10h), its bottom row at `row`.
    pub fn big_char(&self, screen: &mut Screen, ch: u8, row: i32, col: i32) {
        let ch16 = u16::from(ch);
        let offset = match ch {
            b'?' => 0x380,
            b',' => 0x390,
            b'.' => 0x3a0,
            b'!' => 0x3b0,
            b' ' => BLANK_2X2,
            0..=0x39 => ch16.wrapping_sub(0x30).wrapping_mul(0x10),
            0x3a..=0x4a => ch16
                .wrapping_sub(0x41)
                .wrapping_mul(0x10)
                .wrapping_add(0xa0),
            0x4b..=0x5a => (ch16 - 0x4b) * 0x10 + 0x280,
            _ => return,
        };
        self.solid_2x2(screen, row - 1, col, offset);
    }

    /// A string in the big font: two columns a letter.
    pub fn big_string(&self, screen: &mut Screen, text: &[u8], row: i32, col: i32) {
        for (i, &ch) in text.iter().enumerate() {
            self.big_char(screen, ch, row, col + 2 * i as i32);
        }
    }

    /// A number in the big font ending before column `right`, with a blank in
    /// front.
    pub fn number_text(&self, screen: &mut Screen, value: u32, row: i32, right: i32) {
        let text = format!("{value}");
        let width = 2 * text.len() as i32;
        self.big_char(screen, b' ', row, right - width - 2);
        self.big_string(screen, text.as_bytes(), row, right - width);
    }

    /// An 8x16 glyph of the colour font in the colour whose bit planes are set
    /// in `mask`, its bottom row at `row`. It is drawn one column right of
    /// `col`, from the row above. Where the glyph's mask is set, the planes in
    /// `mask` keep the background and the others are cleared; elsewhere the
    /// pixel is the glyph's ink. With the font's higher-detail picture, its ink
    /// is tinted with that colour and the rest shows what the glyph leaves
    /// there; without, where a pixel keeps its colour, the background's picture
    /// stays.
    pub fn colour_char(&self, screen: &mut Screen, mask: u8, ch: u8, row: i32, col: i32) {
        let glyph = match ch {
            b' ' => return,
            b'A'..=b'Z' => usize::from(ch - b'A'),
            b'a'..=b'z' => usize::from(ch - b'a') + 0x29,
            b'0'..=b'9' => usize::from(ch - b'0') + 0x1a,
            b'?' => 0x24,
            b',' => 0x25,
            b'.' => 0x26,
            b'!' => 0x27,
            _ => BLANK_GLYPH,
        };
        let Some(rows) = self.font.glyphs.get(glyph) else {
            return;
        };
        let mask = mask & 0x0f;
        let (left, top) = ((col + 1) * 8, (row - 1) * 8);
        let picture = hd_for(screen, self.hd).and_then(|hd| Some((hd.font.as_ref()?, hd.scale)));
        for (y, &(m, d)) in rows.iter().enumerate() {
            for x in 0..8 {
                let bit = 0x80u8 >> x;
                let (px, py) = (left + x, top + y as i32);
                let Some(pixel) = screen.pixel(px, py) else {
                    continue;
                };
                let new = if m & bit != 0 {
                    pixel & mask
                } else if d & bit != 0 {
                    mask
                } else {
                    0
                };
                if let Some((image, scale)) = picture {
                    let ink = dn2_render::HdBlock {
                        image,
                        x: (glyph as u32 * FONT_GLYPH_WIDTH + x as u32) * scale,
                        y: y as u32 * scale,
                        color: None,
                    };
                    let under = if (m | d) & bit != 0 { pixel & mask } else { 0 };
                    screen.plot_ink(px + screen.left, py, new, ink, mask, under);
                } else if new != pixel {
                    screen.put(px, py, new);
                }
            }
        }
    }

    /// A string with a colour mask: the colour font, one column a letter.
    pub fn colour_string(&self, screen: &mut Screen, mask: u8, text: &[u8], row: i32, col: i32) {
        for (i, &ch) in text.iter().enumerate() {
            self.colour_char(screen, mask, ch, row, col + i as i32);
        }
    }

    /// Small font text with the control codes of the scripts. F0h to FFh select
    /// a colour for the rest of the text (F0h + n is colour mask n - 1, drawn
    /// in the colour font); EFh followed by a 3 digit sprite id and a 2 digit
    /// frame draws that sprite.
    pub fn text(&self, screen: &mut Screen, text: &[u8], row: i32, col: i32) {
        let number = |digits: &[u8]| {
            digits
                .iter()
                .filter(|d| d.is_ascii_digit())
                .fold(0u16, |n, &d| n * 10 + u16::from(d - b'0'))
        };
        let mut colour = 0u8;
        let mut i = 0;
        while i < text.len() {
            let ch = text[i];
            let at = col + i as i32;
            if ch == 0xef {
                let id = number(text.get(i + 1..i + 4).unwrap_or_default());
                let frame = number(text.get(i + 4..i + 6).unwrap_or_default());
                self.sprite_at(screen, row + 1, at + 2, id, frame);
                i += 5;
            } else if ch >= 0xf0 {
                colour = ch - 0xef;
            } else if colour == 0 {
                self.small_char(screen, ch, row, at);
            } else {
                self.colour_char(screen, colour - 1, ch, row, at);
            }
            i += 1;
        }
    }

    /// The 8 saved game names ("Empty" for a slot without one) at rows 6,
    /// 8,..., the selected one lit.
    pub fn slot_list(&self, screen: &mut Screen, names: &[String], selected: usize) {
        for slot in 0..8 {
            let name = names.get(slot).map_or("", String::as_str);
            let text = if name.is_empty() { "Empty" } else { name };
            let mask = if slot == selected { 3 } else { 2 };
            self.colour_string(screen, mask, text.as_bytes(), slot_row(slot as i32), 0xd);
        }
    }

    /// Frame `frame` of sprite `id` with its bottom left at tile `row`, `col`
    /// of the screen (one column left of the sprite's own offset, which is
    /// one-based there).
    pub fn sprite_at(&self, screen: &mut Screen, row: i32, col: i32, id: u16, frame: u16) {
        let Some(f) = self
            .sprites
            .get(id)
            .and_then(|s| s.frames.get(usize::from(frame)))
        else {
            return;
        };
        let left = col + i32::from(f.x_offset) - 1;
        let top = row + i32::from(f.y_offset) - (f.image.height / 8) as i32 + 1;
        let hd = sprite_picture(hd_for(screen, self.hd), id, frame);
        screen.draw_hd(&f.image, hd, left * 8, top * 8);
    }

    /// A framed blank box of `width` by `height` tiles with its top left at
    /// `row`, `col`.
    pub fn text_box(&self, screen: &mut Screen, width: i32, height: i32, row: i32, col: i32) {
        for r in 1..height - 1 {
            for c in 1..width - 1 {
                self.solid(screen, row + r, col + c, BOX_FILL);
            }
        }
        for r in 0..height {
            self.solid(screen, row + r, col, BOX_LEFT);
            self.solid(screen, row + r, col + width - 1, BOX_RIGHT);
        }
        for c in 0..width {
            self.solid(screen, row, col + c, BOX_TOP);
            self.solid(screen, row + height - 1, col + c, BOX_BOTTOM);
        }
        let (right, bottom) = (col + width - 1, row + height - 1);
        self.solid(screen, row, col, BOX_CORNERS[0]);
        self.solid(screen, row, right, BOX_CORNERS[1]);
        self.solid(screen, bottom, col, BOX_CORNERS[2]);
        self.solid(screen, bottom, right, BOX_CORNERS[3]);
    }
}
