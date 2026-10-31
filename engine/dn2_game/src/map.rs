//! The level map as the game sees it: cells and what their tiles do.

use alloc::vec::Vec;

use dn2_assets::level::Level;
use dn2_assets::sprites::Sprites;
use dn2_assets::tileset::{TileAttributes, Tileset};

/// What a collision check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Free,
    Solid,
    /// Moving up into a pipe the player can hang from.
    Hang,
    /// A ladder.
    Climb,
}

/// Sides of a box to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
    /// The box's top row and left column, any side of a tile (what shots use).
    Any,
}

/// A box of tiles: `x` is the left column, `y` the bottom row. Signed, so
/// a position above or left of the map is plain negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Who is asking; the player's checks see more than a shot's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mover {
    Thing,
    Player {
        /// The player stands (not jumping or falling): a ledge one tile
        /// high is a step.
        standing: bool,
        /// Up is held without left or right: a ladder beside the head
        /// pulls the player in line.
        grab_ladder: bool,
    },
}

/// The outcome of [`Map::probe`], with what the player's checks note on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub hit: Hit,
    /// Down: a conveyor belt under the box (`Left` or `Right`).
    pub conveyor: Option<Dir>,
    /// Left/right of a standing player: only the bottom row is in the way,
    /// so the player steps up one tile.
    pub step_up: bool,
    /// Up: columns to move to line up with a ladder.
    pub shift: i8,
}

impl Probe {
    fn of(hit: Hit) -> Self {
        Self {
            hit,
            conveyor: None,
            step_up: false,
            shift: 0,
        }
    }
}

/// The map of the running level: its own copy of the level (the game
/// changes the cells: destroyed walls, doors) and the tileset's attributes.
#[derive(Clone, Debug)]
pub struct Map {
    pub width: u16,
    pub height: u16,
    /// The level as it is now; the renderer draws its cells.
    pub level: Level,
    solid_attributes: Vec<TileAttributes>,
    masked_attributes: Vec<TileAttributes>,
}

impl Map {
    pub fn new(level: &Level, tileset: &Tileset) -> Self {
        Self {
            width: level.width,
            height: level.height,
            level: level.clone(),
            solid_attributes: tileset.solid_attributes.clone(),
            masked_attributes: tileset.masked_attributes.clone(),
        }
    }

    /// The raw cell at tile `x`, `y`; 0 outside the stored cells.
    pub fn cell(&self, x: i32, y: i32) -> u16 {
        self.index(x, y)
            .and_then(|i| self.level.cells.get(i))
            .copied()
            .unwrap_or(0)
    }

    /// Changes the raw cell at tile `x`, `y`, if it is in the map.
    pub fn set_cell(&mut self, x: i32, y: i32, cell: u16) {
        if let Some(c) = self.index(x, y).and_then(|i| self.level.cells.get_mut(i)) {
            *c = cell;
        }
    }

    /// Where the cell at tile `x`, `y` is stored, as the original computes
    /// it: row by row, so a column right of the map is the next row's
    /// start (and one left of it the row before's end), and wrapping at
    /// 32768 cells (its 64 KB map segment), so the rows below the map are
    /// its top rows again. Nothing above the map.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let i = y.checked_mul(i32::from(self.width))?.checked_add(x)?;
        (y >= 0 && i >= 0).then_some(i as usize & 0x7fff)
    }

    /// The attributes of the cell at tile `x`, `y` the way the collision
    /// code reads them: word `cell / 8` of the attribute block. That is the
    /// solid tile, or for a masked cell (8000 + 40 * tile) the first of the
    /// tile's 5 words. Layered cells (bit 15) read past the block: none.
    pub fn attributes_at(&self, x: i32, y: i32) -> TileAttributes {
        let cell = self.cell(x, y);
        if cell & 0x8000 != 0 {
            return TileAttributes::empty();
        }
        let word = usize::from(cell / 8);
        let found = match word.checked_sub(1000) {
            None => self.solid_attributes.get(word),
            Some(m) if m % 5 == 0 => self.masked_attributes.get(m / 5),
            Some(_) => None,
        };
        found.copied().unwrap_or_default()
    }

    /// The cell value of the first tile that is solid on all sides, or 0.
    pub fn fully_solid_cell(&self) -> u16 {
        self.solid_attributes
            .iter()
            .position(|&a| a == TileAttributes::SOLID)
            .map_or(0, |n| (n << 3) as u16)
    }

    /// Whether the cell at tile `x`, `y` has any of `flag`.
    pub fn has(&self, x: i32, y: i32, flag: TileAttributes) -> bool {
        self.attributes_at(x, y).intersects(flag)
    }

    /// A sprite frame's box for [`Map::probe`]: its size in tiles, moved by
    /// the frame's offsets, at `x`, `y`. `None` for a missing frame.
    pub fn sprite_body(sprites: &Sprites, id: u16, frame: u16, x: i32, y: i32) -> Option<Body> {
        let f = sprites.frame_box(id, frame)?;
        Some(Body {
            x: x + i32::from(f.x_offset),
            y: y + i32::from(f.y_offset),
            width: i32::from(f.width),
            height: i32::from(f.height),
        })
    }

    /// Whether the frame of sprite `id` at `x`, `y` is clear on `dir` (the
    /// collision test for anything but the player).
    pub fn collide(&self, sprites: &Sprites, id: u16, frame: u16, x: i32, y: i32, dir: Dir) -> Hit {
        self.probe_body(sprites, id, frame, x, y, dir).hit
    }

    /// The downward check of a sprite frame at `x`, `y`, with the belt it
    /// finds; free for a missing frame.
    pub fn probe_sprite(&self, sprites: &Sprites, id: u16, frame: u16, x: i32, y: i32) -> Probe {
        self.probe_body(sprites, id, frame, x, y, Dir::Down)
    }

    /// [`Map::probe`] for the box of a sprite frame at `x`, `y`; free for a
    /// missing frame.
    fn probe_body(
        &self,
        sprites: &Sprites,
        id: u16,
        frame: u16,
        x: i32,
        y: i32,
        dir: Dir,
    ) -> Probe {
        match Self::sprite_body(sprites, id, frame, x, y) {
            Some(body) => self.probe(body, dir, Mover::Thing),
            None => Probe::of(Hit::Free),
        }
    }

    /// The collision test for a box: what blocks it on `dir`. Outside the map:
    /// above the top it is free; sideways, left of column 0 and right of the
    /// last column is solid; up and down read the cells the way the original
    /// does (see [`Map::cell`]); below the last row moving down is free
    /// (falling out is the level logic's business).
    pub fn probe(&self, body: Body, dir: Dir, mover: Mover) -> Probe {
        let Body {
            x,
            y,
            width,
            height,
        } = body;
        let top = y - height + 1;
        let player = matches!(mover, Mover::Player { .. });
        match dir {
            Dir::Any => Probe::of(if top >= 0 && y != 0 {
                self.any_hit(body, top)
            } else {
                Hit::Free
            }),
            Dir::Up => {
                if top < 0 {
                    return Probe::of(Hit::Free);
                }
                if player && self.has(x + 1, top, TileAttributes::HANGABLE) {
                    return Probe::of(Hit::Hang);
                }
                if (0..width).any(|i| self.has(x + i, top, TileAttributes::SOLID_BOTTOM)) {
                    return Probe::of(Hit::Solid);
                }
                let mut probe = Probe::of(Hit::Free);
                if let Mover::Player { grab_ladder, .. } = mover {
                    let ladder = |dx| self.has(x + dx, top, TileAttributes::CLIMBABLE);
                    if ladder(1) {
                        probe.hit = Hit::Climb;
                    } else if grab_ladder && ladder(0) {
                        probe.hit = Hit::Climb;
                        probe.shift = -1;
                    } else if grab_ladder && ladder(2) {
                        probe.hit = Hit::Climb;
                        probe.shift = 1;
                    }
                }
                probe
            }
            Dir::Down => {
                let mut probe = Probe::of(Hit::Free);
                if y < 0 || y > i32::from(self.height) {
                    return probe;
                }
                for i in 0..width {
                    let a = self.attributes_at(x + i, y);
                    if a.contains(TileAttributes::CONVEYOR_LEFT) {
                        probe.conveyor = Some(Dir::Left);
                    }
                    if a.contains(TileAttributes::CONVEYOR_RIGHT) {
                        // A right belt only pushes if the box's last column
                        // is on it or over no floor at all.
                        let last = self.attributes_at(x + width - 1, y);
                        if last.contains(TileAttributes::CONVEYOR_RIGHT)
                            || !last.contains(TileAttributes::SOLID_TOP)
                        {
                            probe.conveyor = Some(Dir::Right);
                        }
                    }
                    if a.contains(TileAttributes::SOLID_TOP) {
                        probe.hit = Hit::Solid;
                        return probe;
                    }
                }
                if player && self.has(x + 1, y, TileAttributes::CLIMBABLE) {
                    probe.hit = Hit::Climb;
                }
                probe
            }
            Dir::Left | Dir::Right => {
                if top < 0 {
                    return Probe::of(Hit::Free);
                }
                let (column, side) = if dir == Dir::Left {
                    (x, TileAttributes::SOLID_RIGHT)
                } else {
                    (x + width - 1, TileAttributes::SOLID_LEFT)
                };
                if column < 0 || column >= i32::from(self.width) {
                    return Probe::of(Hit::Solid);
                }
                let mut probe = Probe::of(Hit::Free);
                for i in 0..height {
                    if self.has(column, y - i, side) {
                        match mover {
                            Mover::Player { standing: true, .. } if i == 0 => probe.step_up = true,
                            _ => return Probe::of(Hit::Solid),
                        }
                    }
                }
                probe
            }
        }
    }

    /// The all-sides check: any solid cell in the top row, then in the left
    /// column. A layered cell ends the row as free, the column as it is.
    fn any_hit(&self, body: Body, top: i32) -> Hit {
        let Body {
            x,
            y,
            width,
            height,
        } = body;
        let solid = |x, y| self.has(x, y, TileAttributes::SOLID);
        for i in 0..width {
            if self.cell(x + i, top) & 0x8000 != 0 {
                return Hit::Free;
            }
            if solid(x + i, top) {
                return Hit::Solid;
            }
        }
        for i in 0..height {
            if self.cell(x, y - i) & 0x8000 != 0 {
                break;
            }
            if solid(x, y - i) {
                return Hit::Solid;
            }
        }
        Hit::Free
    }
}
