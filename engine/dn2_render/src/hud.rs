//! The status bar around and below the view: the frame, score, health,
//! weapon and ammunition, carried items, collected letters, the message
//! line and the boss health bar.
//!
//! The pieces come from STATUS.MNI. The game addresses them by video memory
//! offset, one byte per tile row: the sheet is a stream of tiles of 8 rows,
//! offset `8 * n` is tile `n`, offset + 0x140 is the tile 40 places on (the
//! one below in the sheet image) and an offset need not be a multiple of 8:
//! then the 8 rows run on into the next tile (see [`tile_at`]).

use alloc::vec::Vec;

use dn2_assets::gfx::{Image, Palette, Tile};
use dn2_assets::hd::HdArt;
use dn2_assets::images::TileSheet;
use dn2_assets::sprites::Sprites;

use crate::{HdPicture, ORIGINAL_WIDTH, Screen};

/// The weapons, in the order of the icons in the sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weapon {
    #[default]
    Regular,
    Laser,
    Rocket,
    FlameThrower,
}

/// Highest health the bar shows: it has 8 cells and the first point of
/// health is not drawn.
pub const MAX_HEALTH: u8 = 9;
/// Highest ammunition the bar shows.
pub const MAX_AMMO: u8 = 32;
/// Cells of the boss health bar, each of 8 health points.
pub const BOSS_CELLS: u8 = 30;

/// What the status bar shows. The default is the start of a level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hud {
    pub score: u32,
    /// Level number as shown (from 1); `None` in the bonus episode, which
    /// has none.
    pub level: Option<u8>,
    pub weapon: Weapon,
    /// Hit points, 0 to [`MAX_HEALTH`].
    pub health: u8,
    /// Rounds, 0 to [`MAX_AMMO`] (the bar moves in steps of two).
    pub ammo: u8,
    /// Actor sprite ids of the carried items, by slot (2 columns, 3 rows).
    pub items: [Option<u16>; 6],
    /// Items that are blinking off this frame: their slot stays blank.
    pub items_hidden: [bool; 6],
    /// The collected letters of the bonus word, one flag each.
    pub letters: [bool; 5],
    /// The message line: letters (either case), `,` `.` `!` `?`; spaces and
    /// other bytes leave their cell empty. At most 30 are shown.
    pub message: Vec<u8>,
    /// The boss's health, while a boss bar is up.
    pub boss_health: Option<u8>,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            score: 0,
            level: Some(1),
            weapon: Weapon::Regular,
            health: MAX_HEALTH,
            ammo: 32,
            items: [None; 6],
            items_hidden: [false; 6],
            letters: [false; 5],
            message: Vec::new(),
            boss_health: None,
        }
    }
}

/// The actor sprite whose frames 0 to 2 are the three parts of the frame.
const FRAME_SPRITE: u16 = 0x4d;
/// Frame parts: frame index, left column and bottom row in tiles.
const FRAME_PARTS: [(usize, i32, i32); 3] = [(0, 34, 20), (1, 2, 24), (2, 30, 24)];
/// Sprites of the five letters, which carry their own position.
const LETTER_SPRITES: [u16; 5] = [0x122, 0x123, 0x124, 0x125, 0x126];

// Offsets of the pieces in the sheet.
const HEALTH_FULL: u16 = 0x5e8;
const HEALTH_EMPTY: u16 = 0x5f0;
const BLANK: u16 = 0x5f8;
const WEAPON_ICONS: u16 = 0x660;
const DIGITS: u16 = 0x8c0;
const AMMO_BAR: u16 = 0x1d40;
const BOSS_EMPTY: u16 = 0xdc0;
const BOSS_FULL: u16 = 0xe00;
const BOSS_LABEL: [u16; 4] = [0x828, 0x890, 0x8b0, 0x8b0];
const FONT_A: u16 = 0x820;
const FONT_U: u16 = 0x1e88;
const FONT_PUNCTUATION: [(u8, u16); 4] = [
    (b',', 0x1eb8),
    (b'.', 0x1ec0),
    (b'!', 0x1ec8),
    (b'?', 0x1ed0),
];
/// Offset of the tile below.
const NEXT_ROW: u16 = 0x140;
/// Item slot positions in tiles, relative to the grid.
const ITEM_X: [i32; 6] = [0, 2, 0, 2, 0, 2];
const ITEM_Y: [i32; 6] = [0, 0, 2, 2, 4, 4];

/// The 8x8 picture at video memory offset `offset` of the sheet: 8 rows of
/// the tile stream starting at row `offset`. Past the sheet it is black.
pub fn tile_at(status: &TileSheet, offset: u16) -> Tile {
    let mut tile = [0; 64];
    for (k, row) in tile.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        let n = usize::from(offset) + k;
        if let Some(source) = status.tiles.get(n / 8) {
            row.copy_from_slice(&source[n % 8 * 8..][..8]);
        }
    }
    tile
}

/// The font tile offset of a message character.
fn font_offset(c: u8) -> Option<u16> {
    match c.to_ascii_uppercase() {
        c @ b'A'..=b'T' => Some(FONT_A + u16::from(c - b'A') * 8),
        c @ b'U'..=b'Z' => Some(FONT_U + u16::from(c - b'U') * 8),
        c => FONT_PUNCTUATION.iter().find(|p| p.0 == c).map(|p| p.1),
    }
}

/// Where a widescreen widens the status bar: the right panel moves right,
/// and the bottom bar's score box grows by repeating pixel columns that are
/// empty inside it, left and right of its label, half the width each.
const STRETCH_X: [i32; 2] = [32, 100];
/// The right panel's left edge and the bottom bar's top, in pixels.
const PANEL_X: i32 = 264;
const BAR_Y: i32 = 168;

/// The higher-detail picture of tile row `offset` of the status sheet on
/// (see [`tile_at`]), if `hd` has the sheet.
pub fn status_picture(hd: Option<&HdArt>, offset: u16) -> Option<HdPicture<'_>> {
    let hd = hd?;
    Some(HdPicture {
        image: hd.status.as_ref()?,
        x: 0,
        y: u32::from(offset) * hd.scale,
    })
}

/// The higher-detail picture of a sprite frame, if `hd` has it.
pub fn sprite_picture(hd: Option<&HdArt>, id: u16, frame: u16) -> Option<HdPicture<'_>> {
    hd?.sprites.get(&(id, frame)).map(HdPicture::new)
}

/// `hd` if `screen` shows pictures at its scale.
pub fn hd_for<'a>(screen: &Screen, hd: &'a HdArt) -> Option<&'a HdArt> {
    (screen.hd_scale() == Some(hd.scale)).then_some(hd)
}

/// Draws the whole status bar (everything outside the view), with the
/// pictures of `hd` where it has them. On a screen wider than the
/// original's it is widened to fit: see [`STRETCH_X`].
pub fn draw_hud(screen: &mut Screen, status: &TileSheet, sprites: &Sprites, hd: &HdArt, hud: &Hud) {
    let hd = hd_for(screen, hd);
    let left = core::mem::replace(&mut screen.left, 0);
    // Extra tile columns.
    let e = (screen.image.width.saturating_sub(ORIGINAL_WIDTH) / 8) as i32;
    if e == 0 {
        draw_frame(screen, sprites, hd);
    } else {
        let mut original = Screen::new(ORIGINAL_WIDTH, 200, Palette(Vec::new()));
        original.set_hd_scale(screen.hd_scale());
        draw_frame(&mut original, sprites, hd);
        widen(&original, screen, e * 8);
    }
    let mut pen = Pen { screen, status, hd };
    pen.number(hud.score, 15 + e, 22);
    if let Some(level) = hud.level {
        pen.number(u32::from(level), 0x23 + e + i32::from(level >= 10), 21);
    }
    let icons = WEAPON_ICONS + (hud.weapon as u16) * 32;
    pen.block(18 + e, 22, icons);
    pen.block(20 + e, 22, icons + 0x10);
    let full = hud.health.saturating_sub(1);
    for i in 0..8 {
        let offset = if i < full { HEALTH_FULL } else { HEALTH_EMPTY };
        pen.column(25 + e + i32::from(i), 22, offset);
    }
    // The bar picture slides up 4 rows for each round, in steps of 2.
    let rounds = u16::from(hud.ammo.min(MAX_AMMO)).next_multiple_of(2);
    pen.column(23 + e, 22, AMMO_BAR - rounds * 4);
    for (slot, item) in hud.items.iter().enumerate() {
        pen.block(0x22 + e + ITEM_X[slot], 3 + ITEM_Y[slot], BLANK);
        if let Some(id) = item.filter(|_| !hud.items_hidden[slot]) {
            let (x, y) = (0x23 + e + ITEM_X[slot], 4 + ITEM_Y[slot]);
            draw_sprite(pen.screen, sprites, hd, id, 0, x, y);
        }
    }
    for (&on, &id) in hud.letters.iter().zip(&LETTER_SPRITES) {
        if on {
            draw_sprite(pen.screen, sprites, hd, id, 0, e, 0);
        }
    }
    for (i, &c) in hud.message.iter().take(usize::from(BOSS_CELLS)).enumerate() {
        if let Some(offset) = font_offset(c) {
            pen.tile(1 + i as i32, 0, offset);
        }
    }
    if let Some(health) = hud.boss_health {
        pen.boss_bar(health);
    }
    screen.left = left;
}

/// The frame around the view and its labels.
fn draw_frame(screen: &mut Screen, sprites: &Sprites, hd: Option<&HdArt>) {
    for (frame, x, y) in FRAME_PARTS {
        draw_sprite(screen, sprites, hd, FRAME_SPRITE, frame, x, y);
    }
}

/// Copies the status bar of `original` (320 pixels wide) to the wider
/// `screen`, `extra` pixels wider: the right panel moved right, the bottom
/// bar stretched at [`STRETCH_X`]. The view's place is left alone.
fn widen(original: &Screen, screen: &mut Screen, extra: i32) {
    let width = screen.image.width as i32;
    let [a, b] = STRETCH_X;
    let first = extra / 2;
    for y in 0..original.image.height.min(screen.image.height) as i32 {
        for x in 0..width {
            let source = if y >= BAR_Y {
                if x < a {
                    x
                } else if x < a + first {
                    a
                } else if x < b + first {
                    x - first
                } else if x < b + extra {
                    b
                } else {
                    x - extra
                }
            } else if x >= PANEL_X + extra {
                x - extra
            } else {
                continue;
            };
            screen.copy_pixel(original, source as u32, y as u32, x, y);
        }
    }
}

/// Draws sheet tiles at tile positions of the screen.
struct Pen<'a> {
    screen: &'a mut Screen,
    status: &'a TileSheet,
    hd: Option<&'a HdArt>,
}

impl Pen<'_> {
    fn tile(&mut self, x: i32, y: i32, offset: u16) {
        let image = Image {
            width: 8,
            height: 8,
            pixels: tile_at(self.status, offset).to_vec(),
        };
        let hd = status_picture(self.hd, offset);
        self.screen.draw_hd(&image, hd, x * 8, y * 8);
    }

    /// A tile and the one below it.
    fn column(&mut self, x: i32, y: i32, offset: u16) {
        self.tile(x, y, offset);
        self.tile(x, y + 1, offset.wrapping_add(NEXT_ROW));
    }

    /// 2x2 tiles: offset, offset + 8 (right) and the ones below.
    fn block(&mut self, x: i32, y: i32, offset: u16) {
        self.column(x, y, offset);
        self.column(x + 1, y, offset + 8);
    }

    /// Decimal digits of 16x16 pixels, the last one starting in column `right`.
    fn number(&mut self, value: u32, right: i32, y: i32) {
        let mut rest = value;
        let mut x = right;
        loop {
            self.block(x, y, DIGITS + (rest % 10) as u16 * 16);
            rest /= 10;
            x -= 2;
            if rest == 0 {
                break;
            }
        }
    }

    /// The label BOSS and 30 cells in the top row.
    fn boss_bar(&mut self, health: u8) {
        for (i, offset) in BOSS_LABEL.into_iter().enumerate() {
            self.tile(1 + i as i32, 0, offset);
        }
        for i in 0..BOSS_CELLS {
            let offset = match i.cmp(&(health / 8)) {
                core::cmp::Ordering::Less => BOSS_FULL,
                core::cmp::Ordering::Equal => BOSS_EMPTY + u16::from(health % 8) * 8,
                core::cmp::Ordering::Greater => BOSS_EMPTY,
            };
            self.tile(6 + i32::from(i), 0, offset);
        }
    }
}

/// Draws a sprite frame the way the game does for the status bar: `x`, `y`
/// in tiles, `y` the bottom row; its tiles start one column left of `x`
/// plus the frame's offset.
fn draw_sprite(
    screen: &mut Screen,
    sprites: &Sprites,
    hd: Option<&HdArt>,
    id: u16,
    frame: usize,
    x: i32,
    y: i32,
) {
    let Some(f) = sprites.get(id).and_then(|s| s.frames.get(frame)) else {
        return;
    };
    let left = x + i32::from(f.x_offset) - 1;
    let top = y + i32::from(f.y_offset) - (f.image.height / 8) as i32 + 1;
    let hd = sprite_picture(hd, id, frame as u16);
    screen.draw_hd(&f.image, hd, left * 8, top * 8);
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use dn2_assets::gfx::Rgb;

    /// A sheet whose tile `n` is filled with colour `n % 15 + 1`.
    fn sheet() -> TileSheet {
        TileSheet {
            tiles: (0..1000).map(|n| [(n % 15 + 1) as u8; 64]).collect(),
        }
    }

    fn screen() -> Screen {
        Screen::new(320, 200, Palette(vec![Rgb::default(); 16]))
    }

    fn sprites() -> Sprites {
        Sprites::default()
    }

    fn colour(offset: u16) -> u8 {
        (offset / 8 % 15 + 1) as u8
    }

    #[test]
    fn offsets_are_rows() {
        let s = sheet();
        assert_eq!(tile_at(&s, 16), s.tiles[2]);
        assert_eq!(tile_at(&s, 16 + 0x140), s.tiles[42]);
        // Half way into tile 2: 4 rows of it, then 4 of tile 3.
        let t = tile_at(&s, 20);
        assert_eq!((t[0], t[31], t[32], t[63]), (3, 3, 4, 4));
        assert_eq!(tile_at(&s, 8 * 1000)[0], 0);
    }

    #[test]
    fn score_digits_end_at_column_16() {
        let (s, sp) = (sheet(), sprites());
        let mut screen = screen();
        let hud = Hud {
            score: 120,
            ..Hud::default()
        };
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hud);
        let at = |col: usize, row: usize| screen.image.pixels[row * 8 * 320 + col * 8];
        // Digits 1, 2, 0 in columns 11, 13, 15 (2 tiles wide each).
        for (col, digit) in [(11, 1u16), (13, 2), (15, 0)] {
            assert_eq!(at(col, 22), colour(DIGITS + digit * 16));
        }
    }

    #[test]
    fn health_cells() {
        let (s, sp) = (sheet(), sprites());
        let mut screen = screen();
        let hud = Hud {
            health: 4,
            ..Hud::default()
        };
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hud);
        let cell = |i: usize| screen.image.pixels[22 * 8 * 320 + (25 + i) * 8];
        assert_eq!(cell(2), colour(HEALTH_FULL));
        assert_eq!(cell(3), colour(HEALTH_EMPTY));
        assert_eq!(cell(7), colour(HEALTH_EMPTY));
    }

    #[test]
    fn hd_status_sheet() {
        let (s, sp) = (sheet(), sprites());
        let mut screen = screen();
        screen.set_hd_scale(Some(2));
        // 2x strip: tile n is green n + 1.
        let status = dn2_assets::gfx::RgbaImage {
            width: 16,
            height: 16000,
            pixels: (0..16 * 16000)
                .map(|i| 0xff00_0000 | (i / 16 / 16 + 1) << 8)
                .collect(),
        };
        let hd = HdArt {
            scale: 2,
            status: Some(status),
            ..HdArt::default()
        };
        let hud = Hud {
            health: 3,
            ..Hud::default()
        };
        draw_hud(&mut screen, &s, &sp, &hd, &hud);
        let mut frame = crate::FrameBuffer::new(0, 0);
        screen.present(&mut frame, 2, crate::Filter::Nearest);
        let cell = |i: u32| frame.pixels[(22 * 16 * 640 + (25 + i) * 16) as usize];
        assert_eq!(cell(1), u32::from(HEALTH_FULL / 8 + 1) << 8);
        assert_eq!(cell(2), u32::from(HEALTH_EMPTY / 8 + 1) << 8);
    }

    #[test]
    fn message_and_boss_bar() {
        let (s, sp) = (sheet(), sprites());
        let mut screen = screen();
        let hud = Hud {
            message: b"a  b".to_vec(),
            boss_health: Some(20),
            ..Hud::default()
        };
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hud);
        let at = |col: usize| screen.image.pixels[col * 8];
        // The boss label is drawn over the message's first columns.
        assert_eq!(at(1), colour(BOSS_LABEL[0]));
        assert_eq!(at(6), colour(BOSS_FULL));
        assert_eq!(at(8), colour(BOSS_EMPTY + 4 * 8));
        assert_eq!(at(9), colour(BOSS_EMPTY));
    }

    #[test]
    fn items_fill_slots_and_blink() {
        let (s, mut sp) = (sheet(), sprites());
        let mut screen = screen();
        sp.actors = vec![None; 0x35];
        let hud = Hud {
            items: [Some(0x34), None, None, None, None, None],
            ..Hud::default()
        };
        // A missing sprite draws nothing, but the slot is blanked.
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hud);
        let at =
            |screen: &Screen, col: usize, row: usize| screen.image.pixels[row * 8 * 320 + col * 8];
        assert_eq!(at(&screen, 0x22, 3), colour(BLANK));
        let hidden = Hud {
            items_hidden: [true; 6],
            ..hud
        };
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hidden);
        assert_eq!(at(&screen, 0x22, 3), colour(BLANK));
    }

    #[test]
    fn widescreen_moves_the_panel_and_widens_the_score_box() {
        let (s, sp) = (sheet(), sprites());
        let mut screen = Screen::new(320 + 13 * 8, 200, Palette(vec![Rgb::default(); 16]));
        let hud = Hud {
            score: 120,
            health: 4,
            ..Hud::default()
        };
        draw_hud(&mut screen, &s, &sp, &HdArt::default(), &hud);
        let width = screen.image.width as usize;
        let at = |col: usize, row: usize| screen.image.pixels[row * 8 * width + col * 8];
        for (col, digit) in [(11, 1u16), (13, 2), (15, 0)] {
            assert_eq!(at(col + 13, 22), colour(DIGITS + digit * 16));
        }
        assert_eq!(at(25 + 13 + 2, 22), colour(HEALTH_FULL));
        assert_eq!(at(25 + 13 + 3, 22), colour(HEALTH_EMPTY));
        assert_eq!(screen.left, 52);
    }

    #[test]
    fn fonts() {
        assert_eq!(font_offset(b'a'), Some(FONT_A));
        assert_eq!(font_offset(b'U'), Some(FONT_U));
        assert_eq!(font_offset(b'?'), Some(0x1ed0));
        assert_eq!(font_offset(b' '), None);
    }
}
