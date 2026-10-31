//! The play area: a 32x20-tile view at pixel (8, 8) with the backdrop,
//! solid and masked tiles, sprites, and the masked tiles marked
//! `IN_FRONT` drawn last, over the sprites.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use dn2_assets::gfx::{RgbaImage, TRANSPARENT, Tile};
use dn2_assets::hd::{HdArt, SHEET_COLUMNS};
use dn2_assets::images::TileSheet;
use dn2_assets::level::{Cell, Level, LevelFlags};
use dn2_assets::sprites::Sprites;
use dn2_assets::tileset::{TileAttributes, Tileset};

use crate::{HdBlock, Screen};

/// Where the view sits on the screen, in pixels.
pub const VIEW_X: i32 = 8;
pub const VIEW_Y: i32 = 8;
/// The original view's size in tiles.
pub const VIEW_COLUMNS: u16 = 32;
pub const VIEW_ROWS: u16 = 20;

/// The view's width in tiles on `screen`: 32 on the original's 320 pixels,
/// more on a wider screen (the status bar keeps its 8 columns).
pub fn view_columns(screen: &Screen) -> u16 {
    (screen.image.width / 8).saturating_sub(8).max(1) as u16
}

/// What a level is drawn from.
pub struct LevelGraphics<'a> {
    pub level: &'a Level,
    pub tileset: &'a Tileset,
    /// The backdrop in use (the level's, or its alternate one).
    pub backdrop: &'a TileSheet,
    pub sprites: &'a Sprites,
    pub hd: HdGraphics<'a>,
}

/// The higher-detail pictures of a level's graphics, where the pack has
/// them; drawn only on a screen with the overlay on at their scale.
#[derive(Clone, Copy, Debug, Default)]
pub struct HdGraphics<'a> {
    pub scale: u32,
    pub solid: Option<&'a RgbaImage>,
    pub masked: Option<&'a RgbaImage>,
    pub backdrop: Option<&'a RgbaImage>,
    pub sprites: Option<&'a BTreeMap<(u16, u16), RgbaImage>>,
}

impl<'a> HdGraphics<'a> {
    /// The pictures of `art` for tileset `tileset` and backdrop `backdrop`
    /// (file names).
    pub fn new(art: &'a HdArt, tileset: &str, backdrop: &str) -> Self {
        Self {
            scale: art.scale,
            solid: art.solid.get(tileset),
            masked: art.masked.get(tileset),
            backdrop: art.backdrops.get(backdrop),
            sprites: Some(&art.sprites),
        }
    }
}

/// The block of a tile sheet's picture for pixel `px`, `py` of tile
/// `index`.
fn sheet_block(image: &RgbaImage, scale: u32, index: usize, px: u32, py: u32) -> HdBlock<'_> {
    let (col, row) = (index as u32 % SHEET_COLUMNS, index as u32 / SHEET_COLUMNS);
    HdBlock::at(image, (col * 8 + px) * scale, (row * 8 + py) * scale)
}

/// Whether the view's tile `x`, `y` (counted from its top left) is in the
/// view: as many tiles wide as the screen gives it, and `VIEW_ROWS` high.
fn in_view(screen: &Screen, x: i32, y: i32) -> bool {
    (0..i32::from(view_columns(screen))).contains(&x) && (0..i32::from(VIEW_ROWS)).contains(&y)
}

/// What changes from frame to frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewState {
    /// Map tile at the view's top left.
    pub camera_x: u16,
    pub camera_y: u16,
    /// Game frames since the level started: drives tile animation and the
    /// backdrop's own scrolling.
    pub frame: u32,
}

/// How a sprite is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawMode {
    /// Draws nothing.
    Hidden,
    /// Opaque pixels as they are; hidden behind solid tiles marked
    /// `IN_FRONT`.
    #[default]
    Normal,
    /// Every opaque pixel white: the hit flash.
    WhiteFlash,
    /// Like `Normal`, but shows over `IN_FRONT` solid tiles too.
    OverForeground,
    /// Writes only colour bit 1 of the pixels, from the sprite's bit 0: a
    /// tint of the background.
    OnePlane,
}

/// One sprite to draw, in map tiles; `y` is the sprite's bottom row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteDraw {
    pub id: u16,
    pub frame: u16,
    pub x: i32,
    pub y: i32,
    pub mode: DrawMode,
}

/// Something drawn over the map, between the tiles and the foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewDraw {
    Sprite(SpriteDraw),
    /// One pixel at map pixel `x`, `y` (a particle).
    Pixel {
        x: i32,
        y: i32,
        color: u8,
    },
    /// A raw map cell drawn as its solid tile at map tile `x`, `y`.
    Tile {
        cell: u16,
        x: i32,
        y: i32,
    },
    /// A block of 2x2 water cells with its top left cell at map tile `x`, `y`:
    /// the pixels under it lose colour bit 2 and gain bit 3. `wave` 0 is still
    /// water, 1 to 4 the shapes of the surface in the top row.
    Water {
        wave: u8,
        x: i32,
        y: i32,
    },
}

/// Particles show only above the view's last pixel row of tiles 19 rows.
const PIXEL_ROWS: i32 = 19;

/// The backdrop is one 40x25-tile picture.
const BACKDROP_WIDTH: u32 = 320;
const BACKDROP_HEIGHT: u32 = 200;
/// Colour of `WhiteFlash`.
const WHITE: u8 = 15;

/// How far the backdrop is shifted left and up, in pixels, as the original does
/// it: the backdrop moves 4 pixels per camera tile with `PARALLAX_BOTH` (both
/// axes) and `PARALLAX_HORIZONTAL`; auto scrolling adds 2 pixels per frame to
/// the left, or 4 per frame up (plus the horizontal parallax).
fn backdrop_offset(flags: LevelFlags, state: &ViewState) -> (u32, u32) {
    let (mut x, mut y) = (0, 0);
    let frame = state.frame;
    let camera_x = u32::from(state.camera_x) * 4;
    let camera_y = u32::from(state.camera_y) * 4;
    if flags.contains(LevelFlags::AUTO_SCROLL) {
        x = frame % 160 * 2;
    }
    if flags.contains(LevelFlags::AUTO_SCROLL_VERTICAL) {
        (x, y) = (camera_x, (50 - frame % 50) % 50 * 4);
    }
    if flags.contains(LevelFlags::PARALLAX_BOTH) {
        (x, y) = (camera_x, camera_y);
    } else if flags.contains(LevelFlags::PARALLAX_HORIZONTAL) {
        (x, y) = (camera_x, 0);
    }
    (x % BACKDROP_WIDTH, y % BACKDROP_HEIGHT)
}

/// Draws an 8x8 `tile` at `x`, `y`, keeping the screen where it is
/// transparent; with `hd` (a sheet, its scale and the tile's index in it)
/// its higher-detail picture too.
fn draw_tile(
    screen: &mut Screen,
    tile: &Tile,
    hd: Option<(&RgbaImage, u32, usize)>,
    x: i32,
    y: i32,
) {
    for (i, &pixel) in tile.iter().enumerate() {
        let (px, py) = ((i % 8) as u32, (i / 8) as u32);
        let (sx, sy) = (x + px as i32, y + py as i32);
        let pixel = (pixel != TRANSPARENT).then_some(pixel);
        match (hd, pixel) {
            (Some((image, scale, index)), _) => {
                screen.plot_hd(sx, sy, pixel, sheet_block(image, scale, index, px, py));
            }
            (None, Some(pixel)) => screen.plot_at(sx, sy, |_| pixel),
            (None, None) => {}
        }
    }
}

/// Draws the part of the backdrop behind screen tile `tx`, `ty` of the view.
fn draw_backdrop_tile(
    screen: &mut Screen,
    backdrop: &TileSheet,
    hd: Option<(&RgbaImage, u32)>,
    offset: (u32, u32),
    tx: u32,
    ty: u32,
) {
    for py in 0..8 {
        for px in 0..8 {
            let x = (offset.0 + tx * 8 + px) % BACKDROP_WIDTH;
            let y = (offset.1 + ty * 8 + py) % BACKDROP_HEIGHT;
            let index = (y / 8 * TileSheet::COLUMNS + x / 8) as usize;
            if let Some(tile) = backdrop.tiles.get(index) {
                let pixel = tile[(y % 8 * 8 + x % 8) as usize];
                let (sx, sy) = (VIEW_X + (tx * 8 + px) as i32, VIEW_Y + (ty * 8 + py) as i32);
                match hd {
                    Some((image, scale)) => {
                        let block = HdBlock::at(image, x * scale, y * scale);
                        screen.plot_hd(sx, sy, Some(pixel), block);
                    }
                    None => screen.plot_at(sx, sy, |_| pixel),
                }
            }
        }
    }
}

/// The solid tile to show: animated ones step through the next 3 tiles,
/// every frame or (`SLOW_ANIMATION`) every second one.
fn animated(tileset: &Tileset, tile: u16, frame: u32) -> u16 {
    let attributes = tileset
        .solid_attributes
        .get(usize::from(tile))
        .copied()
        .unwrap_or_default();
    if !attributes.contains(TileAttributes::ANIMATED) {
        return tile;
    }
    let step = if attributes.contains(TileAttributes::SLOW_ANIMATION) {
        frame / 2
    } else {
        frame
    };
    tile + (step % 4) as u16
}

/// Whether a sprite tile over map cell `x`, `y` is covered by an `IN_FRONT`
/// solid tile.
fn covered(graphics: &LevelGraphics, x: i32, y: i32) -> bool {
    let level = graphics.level;
    if !(0..i32::from(level.width)).contains(&x) || !(0..i32::from(level.height)).contains(&y) {
        return false;
    }
    match level.cell(x as u16, y as u16) {
        Cell::Solid(tile) => graphics
            .tileset
            .solid_attributes
            .get(usize::from(tile))
            .is_some_and(|a| a.contains(TileAttributes::IN_FRONT)),
        _ => false,
    }
}

/// Draws one sprite, a tile at a time like the original: tiles outside the
/// view are left out.
fn draw_sprite(
    screen: &mut Screen,
    graphics: &LevelGraphics,
    hd: &HdGraphics,
    state: &ViewState,
    sprite: &SpriteDraw,
) {
    if sprite.mode == DrawMode::Hidden {
        return;
    }
    let Some(frame) = graphics
        .sprites
        .get(sprite.id)
        .and_then(|actor| actor.frames.get(usize::from(sprite.frame)))
    else {
        return;
    };
    let image = &frame.image;
    // The tint changes colour indices: it shows the original pixels.
    let hd_image = hd
        .sprites
        .and_then(|sprites| sprites.get(&(sprite.id, sprite.frame)))
        .filter(|_| sprite.mode != DrawMode::OnePlane);
    let white = screen.palette.get(WHITE).to_u32();
    let (columns, rows) = (image.width as i32 / 8, image.height as i32 / 8);
    let left = sprite.x + i32::from(frame.x_offset);
    let top = sprite.y + i32::from(frame.y_offset) - rows + 1;
    let (camera_x, camera_y) = (i32::from(state.camera_x), i32::from(state.camera_y));
    for row in 0..rows {
        for column in 0..columns {
            let (mx, my) = (left + column, top + row);
            let (vx, vy) = (mx - camera_x, my - camera_y);
            if !in_view(screen, vx, vy) {
                continue;
            }
            if sprite.mode != DrawMode::OverForeground && covered(graphics, mx, my) {
                continue;
            }
            for py in 0..8 {
                for px in 0..8 {
                    let (ix, iy) = ((column * 8 + px) as u32, (row * 8 + py) as u32);
                    let pixel = image.get(ix, iy);
                    let (x, y) = (VIEW_X + vx * 8 + px, VIEW_Y + vy * 8 + py);
                    if let Some(hd_image) = hd_image {
                        let flash = sprite.mode == DrawMode::WhiteFlash;
                        let block = HdBlock {
                            image: hd_image,
                            x: ix * hd.scale,
                            y: iy * hd.scale,
                            color: flash.then_some(white),
                        };
                        let pixel =
                            (pixel != TRANSPARENT).then_some(if flash { WHITE } else { pixel });
                        screen.plot_hd(x, y, pixel, block);
                        continue;
                    }
                    if pixel == TRANSPARENT {
                        continue;
                    }
                    match sprite.mode {
                        DrawMode::WhiteFlash => screen.plot_at(x, y, |_| WHITE),
                        DrawMode::OnePlane => {
                            screen.plot_at(x, y, |old| old & !2 | (pixel & 1) << 1)
                        }
                        _ => screen.plot_at(x, y, |_| pixel),
                    }
                }
            }
        }
    }
}

/// A particle, if it is inside the view.
fn draw_pixel(screen: &mut Screen, state: &ViewState, x: i32, y: i32, color: u8) {
    let vx = x - i32::from(state.camera_x) * 8;
    let vy = y - i32::from(state.camera_y) * 8;
    let columns = i32::from(view_columns(screen));
    if (0..columns * 8).contains(&vx) && (0..PIXEL_ROWS * 8).contains(&vy) {
        screen.plot_at(VIEW_X + vx, VIEW_Y + vy, |_| color);
    }
}

/// A raw solid cell at a map tile, if it is inside the view.
fn draw_cell(
    screen: &mut Screen,
    graphics: &LevelGraphics,
    hd: &HdGraphics,
    state: &ViewState,
    cell: u16,
    x: i32,
    y: i32,
) {
    let (vx, vy) = (x - i32::from(state.camera_x), y - i32::from(state.camera_y));
    if !in_view(screen, vx, vy) {
        return;
    }
    let index = usize::from(cell / 8);
    if let Some(tile) = graphics.tileset.solid.get(index) {
        let hd = hd.solid.map(|image| (image, hd.scale, index));
        draw_tile(screen, tile, hd, VIEW_X + vx * 8, VIEW_Y + vy * 8);
    }
}

/// Which pixels (bit 7 is the leftmost) of a pixel row of a block's top row of
/// cells turn to water, for the waves 0 (still) to 4 (the surface variants).
fn water_mask(wave: u8, row: i32) -> u8 {
    match (wave, row) {
        (1, 0) => 0x60,
        (1, 1) => 0xf9,
        (3, 0) => 0x9f,
        (3, 1) => 0x06,
        (2 | 4, 0) => 0x00,
        _ => 0xff,
    }
}

/// A block of water: the top row of cells in the shape of the wave, the
/// bottom row full. Cells outside the view are left out.
fn draw_water_block(screen: &mut Screen, state: &ViewState, wave: u8, x: i32, y: i32) {
    for (dy, dx) in [(0, 0), (0, 1), (1, 1), (1, 0)] {
        let (cx, cy) = (
            x + dx - i32::from(state.camera_x),
            y + dy - i32::from(state.camera_y),
        );
        if !in_view(screen, cx, cy) {
            continue;
        }
        for row in 0..8 {
            let mask = if dy == 0 { water_mask(wave, row) } else { 0xff };
            for p in 0..8 {
                if mask & (0x80 >> p) != 0 {
                    screen.plot_at(VIEW_X + cx * 8 + p, VIEW_Y + cy * 8 + row, |c| (c & !4) | 8);
                }
            }
        }
    }
}

/// Fills the whole view with one colour (the flash of a big explosion).
pub fn fill_view(screen: &mut Screen, color: u8) {
    let columns = i32::from(view_columns(screen));
    for y in 0..i32::from(VIEW_ROWS) * 8 {
        for x in 0..columns * 8 {
            screen.plot_at(VIEW_X + x, VIEW_Y + y, |_| color);
        }
    }
}

/// Draws the view: backdrop, tiles, then `draws` in the order given, then
/// the foreground tiles. Only the view's rectangle is touched.
pub fn draw_level(
    screen: &mut Screen,
    graphics: &LevelGraphics,
    state: &ViewState,
    draws: &[ViewDraw],
) {
    let (level, tileset) = (graphics.level, graphics.tileset);
    let offset = backdrop_offset(level.flags, state);
    let hd = if screen.hd_scale() == Some(graphics.hd.scale) {
        graphics.hd
    } else {
        HdGraphics::default()
    };
    let solid_hd = |index: usize| hd.solid.map(|image| (image, hd.scale, index));
    let masked_hd = |index: usize| hd.masked.map(|image| (image, hd.scale, index));
    let backdrop_hd = hd.backdrop.map(|image| (image, hd.scale));
    let mut foreground: Vec<(&Tile, usize, i32, i32)> = Vec::new();
    for ty in 0..u32::from(VIEW_ROWS) {
        for tx in 0..u32::from(view_columns(screen)) {
            let (mx, my) = (
                u32::from(state.camera_x) + tx,
                u32::from(state.camera_y) + ty,
            );
            // The camera may show one row below the map (looking down at
            // its bottom); the original draws the cells stored there, as
            // it reads the map row by row.
            let cell = if mx < u32::from(level.width) {
                level.cell(mx as u16, my as u16)
            } else {
                Cell::Empty
            };
            let (x, y) = (VIEW_X + tx as i32 * 8, VIEW_Y + ty as i32 * 8);
            let masked = match cell {
                Cell::Solid(tile) => {
                    let index = usize::from(animated(tileset, tile, state.frame));
                    if let Some(tile) = tileset.solid.get(index) {
                        draw_tile(screen, tile, solid_hd(index), x, y);
                    }
                    None
                }
                Cell::SolidAndMasked { solid, masked } => {
                    let index = usize::from(solid);
                    if let Some(tile) = tileset.solid.get(index) {
                        draw_tile(screen, tile, solid_hd(index), x, y);
                    }
                    Some(masked)
                }
                Cell::Masked(masked) => {
                    draw_backdrop_tile(screen, graphics.backdrop, backdrop_hd, offset, tx, ty);
                    Some(masked)
                }
                Cell::Empty => {
                    draw_backdrop_tile(screen, graphics.backdrop, backdrop_hd, offset, tx, ty);
                    None
                }
            };
            let Some(masked) = masked.map(usize::from) else {
                continue;
            };
            let Some(tile) = tileset.masked.get(masked) else {
                continue;
            };
            let in_front = tileset
                .masked_attributes
                .get(masked)
                .is_some_and(|a| a.contains(TileAttributes::IN_FRONT));
            if in_front {
                foreground.push((tile, masked, x, y));
            } else {
                draw_tile(screen, tile, masked_hd(masked), x, y);
            }
        }
    }
    for draw in draws {
        match *draw {
            ViewDraw::Sprite(ref sprite) => draw_sprite(screen, graphics, &hd, state, sprite),
            ViewDraw::Pixel { x, y, color } => draw_pixel(screen, state, x, y, color),
            ViewDraw::Tile { cell, x, y } => draw_cell(screen, graphics, &hd, state, cell, x, y),
            ViewDraw::Water { wave, x, y } => draw_water_block(screen, state, wave, x, y),
        }
    }
    for (tile, masked, x, y) in foreground {
        draw_tile(screen, tile, masked_hd(masked), x, y);
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use dn2_assets::gfx::{Image, Palette, Rgb};
    use dn2_assets::sprites::{ActorSprite, SpriteFrame};

    use super::*;

    fn tileset() -> Tileset {
        Tileset {
            solid: (0..16).map(|i| [i as u8 + 1; 64]).collect(),
            masked: (0..4).map(|i| [i as u8 + 100; 64]).collect(),
            solid_attributes: vec![TileAttributes::empty(); 16],
            masked_attributes: vec![TileAttributes::empty(); 4],
        }
    }

    struct Fixture {
        level: Level,
        tileset: Tileset,
        backdrop: TileSheet,
        sprites: Sprites,
    }

    impl Fixture {
        /// An empty level; backdrop tile `i` is filled with `i % 200`; the
        /// only sprite is 2 tiles wide, 1 tall, all colour 7.
        fn new(flags: LevelFlags) -> Self {
            let level = Level {
                tileset: String::new(),
                backdrop: String::new(),
                music: String::new(),
                flags,
                alternate_backdrop: 0,
                actors: Vec::new(),
                width: 64,
                height: 32,
                cells: vec![0; 32750],
                extra: Vec::new(),
            };
            let frame = SpriteFrame {
                x_offset: 0,
                y_offset: 0,
                image: Image::new(16, 8, 7),
            };
            Self {
                level,
                tileset: tileset(),
                backdrop: TileSheet {
                    tiles: (0..1000).map(|i| [(i % 200) as u8; 64]).collect(),
                },
                sprites: Sprites::new(
                    vec![Some(ActorSprite {
                        draw_pass: 0,
                        frames: vec![frame],
                    })],
                    vec![0],
                ),
            }
        }

        fn draw(&self, state: ViewState, draws: &[SpriteDraw]) -> Screen {
            self.draw_hd(state, draws, HdGraphics::default())
        }

        fn draw_hd(&self, state: ViewState, draws: &[SpriteDraw], hd: HdGraphics) -> Screen {
            let graphics = LevelGraphics {
                level: &self.level,
                tileset: &self.tileset,
                backdrop: &self.backdrop,
                sprites: &self.sprites,
                hd,
            };
            // Colour i is (i, 0, 0): the overlay's colours have green or blue.
            let palette = Palette((0..=255).map(|r| Rgb { r, g: 0, b: 0 }).collect());
            let mut screen = Screen::new(320, 200, palette);
            screen.set_hd_scale((hd.scale > 0).then_some(hd.scale));
            screen.clear(250);
            let draws: Vec<ViewDraw> = draws.iter().map(|&d| ViewDraw::Sprite(d)).collect();
            draw_level(&mut screen, &graphics, &state, &draws);
            screen
        }
    }

    /// The pixel at the top left of view tile `tx`, `ty`.
    fn at(screen: &Screen, tx: i32, ty: i32) -> u8 {
        screen
            .image
            .get((VIEW_X + tx * 8) as u32, (VIEW_Y + ty * 8) as u32)
    }

    fn sprite(x: i32, y: i32, mode: DrawMode) -> SpriteDraw {
        SpriteDraw {
            id: 0,
            frame: 0,
            x,
            y,
            mode,
        }
    }

    fn state(camera_x: u16, camera_y: u16, frame: u32) -> ViewState {
        ViewState {
            camera_x,
            camera_y,
            frame,
        }
    }

    #[test]
    fn backdrop_where_empty_and_nothing_outside_the_view() {
        let screen = Fixture::new(LevelFlags::empty()).draw(state(0, 0, 0), &[]);
        assert_eq!(at(&screen, 0, 0), 0);
        assert_eq!(at(&screen, 3, 2), 83);
        assert_eq!(screen.image.get(7, 8), 250);
        assert_eq!(screen.image.get(8, 7), 250);
        assert_eq!(screen.image.get(8 + 256, 8), 250);
        assert_eq!(screen.image.get(8, 8 + 160), 250);
    }

    #[test]
    fn solid_and_masked_tiles() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.tileset.masked[1][0] = TRANSPARENT;
        f.level.cells[2] = 3 * 8;
        f.level.cells[3] = 8000 + 40;
        f.level.cells[4] = 0x8000 | 5 | 2 << 10;
        let screen = f.draw(state(0, 0, 0), &[]);
        assert_eq!(at(&screen, 2, 0), 4);
        // Masked over the backdrop, which shows through its transparent pixel.
        assert_eq!(at(&screen, 3, 0), 3);
        assert_eq!(screen.image.get(VIEW_X as u32 + 25, VIEW_Y as u32), 101);
        // Masked over solid.
        assert_eq!(at(&screen, 4, 0), 102);
    }

    #[test]
    fn animation_steps() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.tileset.solid_attributes[4] = TileAttributes::ANIMATED;
        f.tileset.solid_attributes[8] = TileAttributes::ANIMATED | TileAttributes::SLOW_ANIMATION;
        f.level.cells[0] = 4 * 8;
        f.level.cells[1] = 8 * 8;
        // Solid tile `n` has colour n + 1.
        let shown = |frame| {
            let screen = f.draw(state(0, 0, frame), &[]);
            (at(&screen, 0, 0), at(&screen, 1, 0))
        };
        assert_eq!(shown(0), (5, 9));
        assert_eq!(shown(1), (6, 9));
        assert_eq!(shown(2), (7, 10));
        assert_eq!(shown(5), (6, 11));
        assert_eq!(shown(7), (8, 12));
    }

    #[test]
    fn sprite_is_placed_by_its_bottom_row_and_clipped() {
        let f = Fixture::new(LevelFlags::empty());
        let view = state(10, 10, 0);
        let screen = f.draw(view, &[sprite(10, 10, DrawMode::Normal)]);
        assert_eq!((at(&screen, 0, 0), at(&screen, 1, 0)), (7, 7));
        assert_ne!(at(&screen, 2, 0), 7);
        assert_ne!(at(&screen, 0, 1), 7);
        // At the right and bottom edges only the tiles inside the view show.
        let screen = f.draw(view, &[sprite(41, 29, DrawMode::Normal)]);
        assert_eq!(at(&screen, 31, 19), 7);
        assert_ne!(at(&screen, 30, 19), 7);
        assert_eq!(screen.image.get(8 + 256, 8 + 19 * 8), 250);
        let screen = f.draw(view, &[sprite(41, 30, DrawMode::Normal)]);
        assert!((0..32).all(|x| at(&screen, x, 19) != 7));
        assert_eq!(screen.image.get(8 + 256, 8 + 160), 250);
        // Above and left of the view.
        let screen = f.draw(view, &[sprite(9, 9, DrawMode::Normal)]);
        assert_eq!(at(&screen, 0, 0), at(&f.draw(view, &[]), 0, 0));
        let screen = f.draw(view, &[sprite(9, 10, DrawMode::Normal)]);
        assert_eq!((at(&screen, 0, 0), screen.image.get(7, 8)), (7, 250));
        // Unknown ids and frames are skipped.
        let skipped = [
            SpriteDraw {
                id: 9,
                ..sprite(10, 10, DrawMode::Normal)
            },
            SpriteDraw {
                frame: 3,
                ..sprite(10, 10, DrawMode::Normal)
            },
        ];
        assert_eq!(f.draw(view, &skipped).image, f.draw(view, &[]).image);
    }

    #[test]
    fn sprite_offsets() {
        let mut f = Fixture::new(LevelFlags::empty());
        let frame = &mut f.sprites.actors[0].as_mut().unwrap().frames[0];
        frame.x_offset = 2;
        frame.y_offset = -1;
        let screen = f.draw(state(0, 0, 0), &[sprite(1, 5, DrawMode::Normal)]);
        assert_eq!((at(&screen, 3, 4), at(&screen, 4, 4)), (7, 7));
        assert_ne!(at(&screen, 2, 4), 7);
        assert_ne!(at(&screen, 5, 4), 7);
        assert_ne!(at(&screen, 3, 5), 7);
    }

    #[test]
    fn sprite_modes() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.sprites.actors[0].as_mut().unwrap().frames[0].image.pixels[1] = TRANSPARENT;
        let view = state(0, 0, 0);
        let white = f.draw(view, &[sprite(0, 0, DrawMode::WhiteFlash)]);
        assert_eq!((white.image.get(8, 8), white.image.get(9, 8)), (15, 0));
        let hidden = f.draw(view, &[sprite(0, 0, DrawMode::Hidden)]);
        assert_eq!(hidden.image.get(8, 8), 0);
        // The background's bit 1 takes the sprite's bit 0.
        f.backdrop.tiles[0] = [0b1101; 64];
        let plane = f.draw(view, &[sprite(0, 0, DrawMode::OnePlane)]);
        assert_eq!(
            (plane.image.get(8, 8), plane.image.get(9, 8)),
            (0b1111, 0b1101)
        );
    }

    #[test]
    fn foreground_over_sprites() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.tileset.masked_attributes[0] = TileAttributes::IN_FRONT;
        f.level.cells[0] = 8000;
        f.level.cells[1] = 8000 + 40;
        let screen = f.draw(state(0, 0, 0), &[sprite(0, 0, DrawMode::Normal)]);
        assert_eq!((at(&screen, 0, 0), at(&screen, 1, 0)), (100, 7));
    }

    #[test]
    fn solid_in_front_hides_sprites_unless_over_foreground() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.tileset.solid_attributes[2] = TileAttributes::IN_FRONT;
        f.level.cells[1] = 2 * 8;
        let hidden = f.draw(state(0, 0, 0), &[sprite(0, 0, DrawMode::Normal)]);
        assert_eq!((at(&hidden, 0, 0), at(&hidden, 1, 0)), (7, 3));
        let over = f.draw(state(0, 0, 0), &[sprite(0, 0, DrawMode::OverForeground)]);
        assert_eq!((at(&over, 0, 0), at(&over, 1, 0)), (7, 7));
    }

    fn picture(width: u32, height: u32, pixel: impl Fn(u32, u32) -> u32) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels: (0..width * height)
                .map(|i| pixel(i % width, i / width))
                .collect(),
        }
    }

    #[test]
    fn hd_pictures_replace_tiles_backdrop_and_sprites() {
        let mut f = Fixture::new(LevelFlags::empty());
        f.level.cells[2] = 3 * 8;
        f.level.cells[4] = 3 * 8;
        // 2x: solid tile n is green n + 1, the backdrop blue, the sprite
        // green and blue except a see-through pixel at its top left.
        let solid = picture(640, 16, |x, _| 0xff00_0000 | (x / 16 + 1) << 8);
        let backdrop = picture(640, 400, |_, _| 0xff00_00ff);
        let frame_picture = picture(32, 16, |x, y| if x + y == 0 { 0 } else { 0xff00_ffff });
        let sprites = BTreeMap::from([((0, 0), frame_picture)]);
        let hd = HdGraphics {
            scale: 2,
            solid: Some(&solid),
            masked: None,
            backdrop: Some(&backdrop),
            sprites: Some(&sprites),
        };
        let mut frame = crate::FrameBuffer::new(0, 0);
        let shown = |screen: &Screen, frame: &mut crate::FrameBuffer, x: i32, y: i32| {
            screen.present(frame, 2, crate::Filter::Nearest);
            frame.pixels[((VIEW_Y * 2 + y) * 640 + VIEW_X * 2 + x) as usize]
        };
        let view = state(0, 0, 0);
        let screen = f.draw_hd(view, &[sprite(4, 0, DrawMode::Normal)], hd);
        assert_eq!(shown(&screen, &mut frame, 2 * 16, 0), 0x00_0400);
        assert_eq!(shown(&screen, &mut frame, 16, 0), 0x00_00ff);
        // The sprite over solid tile 3: its see-through pixel shows the
        // original sprite's pixel (colour 7), the rest its picture.
        assert_eq!(shown(&screen, &mut frame, 4 * 16, 0), 0x07_0000);
        assert_eq!(shown(&screen, &mut frame, 4 * 16 + 1, 0), 0x00_ffff);
        let white = f.draw_hd(view, &[sprite(4, 0, DrawMode::WhiteFlash)], hd);
        assert_eq!(shown(&white, &mut frame, 4 * 16 + 1, 0), 0x0f_0000);
        // The tint shows the original pixels.
        let plane = f.draw_hd(view, &[sprite(4, 0, DrawMode::OnePlane)], hd);
        assert_eq!(shown(&plane, &mut frame, 4 * 16 + 1, 0) & 0xffff, 0);
        // The pictures of palette indices are the same as without HD.
        let draws = [sprite(4, 0, DrawMode::Normal)];
        assert_eq!(f.draw(view, &draws).image, screen.image);
    }

    /// The backdrop pixel at the view's top left, as drawn with `flags`.
    fn first(flags: LevelFlags, camera_x: u16, camera_y: u16, frame: u32) -> u8 {
        let f = Fixture::new(flags);
        f.draw(state(camera_x, camera_y, frame), &[])
            .image
            .get(8, 8)
    }

    #[test]
    fn no_flag_fixes_the_backdrop_to_the_view() {
        assert_eq!(first(LevelFlags::empty(), 9, 9, 123), 0);
    }

    #[test]
    fn parallax_both_moves_half_speed_in_both_directions() {
        let flags = LevelFlags::PARALLAX_BOTH;
        // Backdrop tile (column, row) has colour row * 40 + column (mod 200).
        assert_eq!(first(flags, 2, 4, 0), 2 * 40 + 1);
        // An odd camera is half a tile further in: the view's first pixels
        // are the second half of tile 0, 4 pixels from its edge.
        let f = Fixture::new(flags);
        let screen = f.draw(state(1, 1, 0), &[]);
        assert_eq!(screen.image.get(8, 8), 0);
        assert_eq!(screen.image.get(8 + 4, 8), 1);
        assert_eq!(screen.image.get(8, 8 + 4), 40);
        // Wraps around the 40x25 tiles.
        assert_eq!(first(flags, 80, 50, 0), 0);
        assert_eq!(first(flags, 78, 48, 0), 199);
    }

    #[test]
    fn parallax_horizontal_keeps_the_rows() {
        let flags = LevelFlags::PARALLAX_HORIZONTAL;
        assert_eq!(first(flags, 4, 9, 0), 2);
        assert_eq!(first(flags, 80, 9, 0), 0);
    }

    #[test]
    fn auto_scroll_moves_two_pixels_a_frame() {
        let flags = LevelFlags::AUTO_SCROLL;
        assert_eq!(first(flags, 9, 5, 0), 0);
        let screen = Fixture::new(flags).draw(state(0, 0, 5), &[]);
        // 10 pixels in: tile 1.
        assert_eq!(screen.image.get(8, 8), 1);
        assert_eq!(first(flags, 0, 0, 4), 1);
        assert_eq!(first(flags, 0, 0, 160), 0);
    }

    #[test]
    fn auto_scroll_vertical_moves_up_four_pixels_a_frame_and_follows_the_camera() {
        let flags = LevelFlags::AUTO_SCROLL_VERTICAL;
        assert_eq!(first(flags, 0, 0, 0), 0);
        // One frame: backdrop row 196, in the last tile row.
        assert_eq!(first(flags, 0, 0, 1), 160);
        assert_eq!(first(flags, 0, 0, 2), 160);
        assert_eq!(first(flags, 0, 0, 3), 120);
        assert_eq!(first(flags, 0, 0, 50), 0);
        // The camera moves it horizontally only.
        assert_eq!(first(flags, 2, 7, 0), 1);
    }
}
