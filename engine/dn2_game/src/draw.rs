//! What a game frame draws, in order, and the sprite boxes the game tests
//! against each other and the view.

use dn2_assets::sprites::Sprites;

use crate::player::{VIEW_HEIGHT, VIEW_WIDTH};

/// How a sprite is drawn (mirrors the renderer's draw modes the game uses).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpriteLook {
    /// Not drawn.
    Hidden,
    #[default]
    Normal,
    /// Every opaque pixel white: the hit flash.
    WhiteFlash,
    /// Shows over the foreground tiles too.
    OverForeground,
    /// A tint of what is behind it (the cloaked player).
    OnePlane,
}

/// A sprite to draw, in map tiles; `y` is the bottom row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub id: u16,
    pub frame: u16,
    pub x: i32,
    pub y: i32,
    pub look: SpriteLook,
}

/// One thing drawn over the map, in the order the frame made them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    Sprite(Sprite),
    /// A particle: one pixel at map pixel `x`, `y`, a palette colour.
    Pixel {
        x: i32,
        y: i32,
        color: u8,
    },
    /// A map cell drawn as a solid tile at map tile `x`, `y` (falling
    /// pieces of the map).
    Tile {
        cell: u16,
        x: i32,
        y: i32,
    },
    /// A block of 2x2 cells of water tinting what is behind it, its top left
    /// cell at map tile `x`, `y`. `wave` 0 is still water, 1 to 4 the shapes of
    /// the surface in the top row.
    Water {
        wave: u8,
        x: i32,
        y: i32,
    },
}

impl Draw {
    /// A sprite drawn the normal way.
    pub fn sprite(id: u16, frame: u16, x: i32, y: i32) -> Self {
        Self::Sprite(Sprite {
            id,
            frame,
            x,
            y,
            look: SpriteLook::Normal,
        })
    }
}

/// A sprite frame placed in the map, for the box tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub id: u16,
    pub frame: u16,
    pub x: i32,
    pub y: i32,
}

/// The box of a placed frame: left column, bottom row, width, height in
/// tiles; `None` for a missing frame.
fn bounds(sprites: &Sprites, p: Placed) -> Option<(i32, i32, i32, i32)> {
    let f = sprites.frame_box(p.id, p.frame)?;
    Some((
        p.x + i32::from(f.x_offset),
        p.y + i32::from(f.y_offset),
        i32::from(f.width),
        i32::from(f.height),
    ))
}

/// Player sprites (facing left and right).
const PLAYER: [u16; 2] = [5, 6];

/// Whether the boxes of two placed frames share a tile. The player's box is a
/// tile narrower in most poses and a tile lower when ducking, but only as `b`,
/// as in the original. The map's width matters for boxes outside it (see
/// below).
pub fn overlap(sprites: &Sprites, map_width: u16, a: Placed, b: Placed) -> bool {
    let (Some((mut ax, ay, mut aw, ah)), Some((mut bx, by, mut bw, mut bh))) =
        (bounds(sprites, a), bounds(sprites, b))
    else {
        return false;
    };
    if PLAYER.contains(&b.id) {
        if b.frame == 0x11 || b.frame == 0x22 {
            bh -= 1;
        }
        if b.frame < 9 || matches!(b.frame, 0x11 | 0x12 | 0x14 | 0x1b | 0x1c | 0x22) {
            bw -= 1;
            if b.id == PLAYER[0] {
                bx += 1;
            }
        }
    }
    // The original's columns are unsigned: the second box never overlaps
    // if it starts left of the map. The first, if it starts outside the
    // map, becomes the box from column 0 to its right edge (wrapping in 16
    // bits): cut at the left edge, or reaching all the way from it.
    if bx < 0 {
        return false;
    }
    if ax < 0 || ax > i32::from(map_width) {
        aw += ax;
        ax = 0;
    }
    let x = ax < bx + bw && bx < ax + aw;
    let y = ay - ah < by && by - bh < ay;
    x && y
}

/// Whether a placed frame's box touches the view at `camera` (the original's
/// 32x20 tiles, whatever the screen shows: this decides what is active). A box
/// left of the map is not: the original's columns are unsigned there.
pub fn on_screen(sprites: &Sprites, camera: (u16, u16), p: Placed) -> bool {
    in_view(sprites, camera, (VIEW_WIDTH, VIEW_HEIGHT), p)
}

/// [`on_screen`] for a view of `size` columns and rows.
pub fn in_view(sprites: &Sprites, camera: (u16, u16), size: (i32, i32), p: Placed) -> bool {
    let Some((x, y, width, height)) = bounds(sprites, p) else {
        return false;
    };
    if x < 0 {
        return false;
    }
    let (cx, cy) = (i32::from(camera.0), i32::from(camera.1));
    let (columns, rows) = size;
    let top = y - height + 1;
    // A box of no height still counts when its bottom row is in view.
    let bottom_in = cy <= y && y < cy + rows;
    let across = top < cy + rows && cy + rows <= y;
    x < cx + columns && cx < x + width && (bottom_in || across)
}
