//! Player shots: they fly until they leave the view or hit a wall.
//!
//! Each frame a shot is drawn where it is, then moves on. Regular shots and
//! rockets stop at anything solid; lasers and flames fly through everything
//! until they leave the view. Actors look for shots over them before the shots
//! move (see `actors::hits`); a shot that hit is drawn once more, then gone.
//! The ship's cannon and the reactor's blasts are shots too.

use alloc::vec::Vec;

use dn2_assets::sprites::Sprites;

use crate::draw::{self, Draw, Placed};
use crate::effects::{BLAST, EXPLOSION, Effects, Motion};
use crate::map::{Dir, Hit, Map};
use crate::player::Weapon;
use crate::{GameEvent, Random, Sound};

/// Shots in the air at most.
pub const MAX_SHOTS: usize = 6;
/// Sprite ids of the effects.
const PUFF: u16 = 3;
const ROCKET_SMOKE: u16 = 0x0b;
/// Muzzle flash sprites: up, down, left, right.
const MUZZLE_FLASH: [u16; 4] = [0x21, 0x22, 0x23, 0x24];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotDirection {
    Left,
    Right,
    Up,
    Down,
}

impl ShotDirection {
    /// The muzzle flash for a shot this way.
    pub fn muzzle_flash(self) -> u16 {
        MUZZLE_FLASH[self.index()]
    }

    /// Index in the original's tables: up, down, left, right.
    fn index(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }

    /// The step along the shot's axis for a speed.
    fn step(self, speed: i32) -> (i32, i32) {
        match self {
            Self::Up => (0, -speed),
            Self::Down => (0, speed),
            Self::Left => (-speed, 0),
            Self::Right => (speed, 0),
        }
    }
}

/// What fired a shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotKind {
    Weapon(Weapon),
    /// A shot of this sprite: the ship's cannon (0x5b) or the reactor's
    /// blasts (0x55 to the left, 0x56 to the right). Animated, three tiles a
    /// frame, through everything.
    Blast(u16),
}

/// The sprite of the ship's shots.
pub const SHIP_SHOT: u16 = 0x5b;
/// The reactor's blasts to the left and the right.
pub const REACTOR_BLASTS: [u16; 2] = [0x55, 0x56];

/// A shot to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotRequest {
    pub kind: ShotKind,
    /// Where the shot starts, in tiles.
    pub x: i32,
    pub y: i32,
    pub direction: ShotDirection,
}

/// Starts a shot if fewer than [`MAX_SHOTS`] fly, with a muzzle flash where it
/// starts (a row higher for the ship's).
pub fn spawn(
    shots: &mut Vec<Shot>,
    effects: &mut Effects,
    map: &Map,
    sprites: &Sprites,
    request: &ShotRequest,
) {
    if shots.len() >= MAX_SHOTS {
        return;
    }
    shots.push(Shot::new(request));
    let (x, mut y) = (request.x, request.y);
    if request.kind == ShotKind::Blast(SHIP_SHOT) {
        y -= 1;
    }
    let flash = request.direction.muzzle_flash();
    effects.spawn(map, sprites, flash, (x, y), Motion::Still, 0);
}

/// Shot sprites by weapon, then direction up, down, left, right.
const SPRITES: [[u16; 4]; 4] = [
    [27, 27, 26, 26],
    [25, 25, 24, 24],
    [7, 8, 9, 10],
    [21, 204, 205, 206],
];
/// Where a rocket's smoke is, relative to the rocket, by direction.
const SMOKE: [(i32, i32); 4] = [(0, 0), (0, -2), (2, 0), (0, 0)];

#[derive(Clone, Debug)]
pub struct Shot {
    pub kind: ShotKind,
    pub x: i32,
    pub y: i32,
    pub direction: ShotDirection,
    /// Sprite id and frame to draw.
    pub sprite: u16,
    pub frame: u16,
    /// It hit an actor: drawn one last time.
    pub hit: bool,
}

/// What a shot's frame needs besides the map.
pub struct Out<'a> {
    pub random: &'a mut Random,
    pub effects: &'a mut Effects,
    pub events: &'a mut Vec<GameEvent>,
    pub draws: &'a mut Vec<Draw>,
}

impl Shot {
    pub fn new(request: &ShotRequest) -> Self {
        let sprite = match request.kind {
            ShotKind::Weapon(w) => SPRITES[w as usize][request.direction.index()],
            ShotKind::Blast(id) => id,
        };
        let (mut x, mut y) = (request.x, request.y);
        let down = request.direction == ShotDirection::Down;
        let ShotKind::Weapon(weapon) = request.kind else {
            return Self::at(request, sprite, x, y);
        };
        match weapon {
            // Flames start beside the player's line, laser and shot sprites
            // are drawn from another corner.
            Weapon::FlameThrower => {
                if matches!(request.direction, ShotDirection::Up | ShotDirection::Down) {
                    x -= 1;
                } else {
                    y += 1;
                }
            }
            Weapon::Regular if down => y += 1,
            Weapon::Laser if down => y += 3,
            Weapon::Laser if request.direction == ShotDirection::Left => x -= 2,
            _ => {}
        }
        Self::at(request, sprite, x, y)
    }

    fn at(request: &ShotRequest, sprite: u16, x: i32, y: i32) -> Self {
        Self {
            kind: request.kind,
            x,
            y,
            direction: request.direction,
            sprite,
            frame: 0,
            hit: false,
        }
    }

    /// Tiles per frame.
    fn speed(&self) -> i32 {
        match self.kind {
            ShotKind::Weapon(Weapon::Regular | Weapon::Rocket) => 2,
            ShotKind::Weapon(Weapon::Laser | Weapon::FlameThrower) => 5,
            ShotKind::Blast(_) => 3,
        }
    }

    fn placed(&self) -> Placed {
        Placed {
            id: self.sprite,
            frame: self.frame,
            x: self.x,
            y: self.y,
        }
    }

    /// One game frame: draws the shot, then moves it on. False once it is
    /// gone.
    pub fn update(
        &mut self,
        map: &Map,
        sprites: &Sprites,
        camera: (u16, u16),
        out: &mut Out,
    ) -> bool {
        if !self.hit && !draw::on_screen(sprites, camera, self.placed()) {
            return false;
        }
        let (x, y) = (self.x, self.y);
        out.draws.push(Draw::sprite(self.sprite, self.frame, x, y));
        if self.hit {
            return false;
        }
        let blocked = map.collide(sprites, self.sprite, self.frame, x, y, Dir::Any) != Hit::Free;
        match self.kind {
            ShotKind::Weapon(Weapon::Regular) if blocked => {
                let puff_x = x - i32::from(self.sprite == 27);
                out.effects
                    .spawn(map, sprites, PUFF, (puff_x, y + 1), Motion::Rise, 0);
                return false;
            }
            ShotKind::Weapon(Weapon::Rocket) if blocked => {
                self.explode(map, sprites, out);
                return false;
            }
            ShotKind::Weapon(Weapon::Rocket) => {
                let (dx, dy) = SMOKE[self.direction.index()];
                out.effects.spawn(
                    map,
                    sprites,
                    ROCKET_SMOKE,
                    (x + dx, y + dy),
                    Motion::Still,
                    0,
                );
            }
            _ => {}
        }
        self.advance(sprites);
        true
    }

    /// Moves the shot on by one frame.
    fn advance(&mut self, sprites: &Sprites) {
        let (dx, dy) = self.direction.step(self.speed());
        self.x += dx;
        self.y += dy;
        if matches!(
            self.kind,
            ShotKind::Weapon(Weapon::Regular) | ShotKind::Blast(_)
        ) {
            let frames = sprites.get(self.sprite).map_or(1, |s| s.frames.len()) as u16;
            self.frame = (self.frame + 1) % frames.max(1);
        }
    }

    /// A rocket hits a wall: an explosion where it is, and a cloud of
    /// blasts over its box.
    fn explode(&mut self, map: &Map, sprites: &Sprites, out: &mut Out) {
        let (x, y) = (self.x, self.y);
        let (bx, by) = match self.direction {
            ShotDirection::Up | ShotDirection::Down => (x - 2, y + 1),
            _ => (x - 1, y + 2),
        };
        out.effects
            .spawn(map, sprites, EXPLOSION, (bx, by), Motion::Still, 0);
        out.events.push(GameEvent::Sound(Sound::Explosion));
        out.effects.cloud(sprites, self.sprite, x, y, BLAST);
    }
}
