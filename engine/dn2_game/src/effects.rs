//! Effects: muzzle flashes, puffs, explosions, flying debris and the like.
//!
//! The original keeps 18 effect slots. Each frame the game draws every effect
//! and moves it on, after the actors and the player's shots, so an effect
//! started during a frame shows its first picture in that frame. Some effects
//! hurt the player they touch; the blast of an explosion destroys tiles around
//! it.

use dn2_assets::sprites::Sprites;
use dn2_assets::tileset::TileAttributes;

use crate::draw::{Draw, Placed, on_screen, overlap};
use crate::map::{Dir, Hit, Map};
use crate::{GameEvent, Random, Sound};

pub const MAX_EFFECTS: usize = 18;

/// Sprite ids of effects with their own rules.
pub const EXPLOSION_START: u16 = 1;
pub const EXPLOSION: u16 = 2;
pub const BLAST: u16 = 3;
pub const FIRE: u16 = 0x41;

/// Effects that hurt the player they touch: flames, sparks and the like.
const HARMFUL: [u16; 8] = [0x28, 0x29, 0x2b, FIRE, 0x45, 0x4b, 0x64, 0x12c];

/// Paths of flying debris: dx, dy per frame, each ended by -127.
const PATHS: [&[i8]; 7] = [
    &[
        3, 0, 3, 0, 3, 0, 2, 0, 2, 1, 2, 1, 2, 2, 1, 2, 1, 3, 1, 3, -127,
    ],
    &[
        3, -3, 2, -2, 2, -1, 1, 0, 1, 0, 1, 1, 1, 2, 1, 2, 1, 3, 1, 3, -127,
    ],
    &[
        0, -3, 0, -2, 0, -2, 0, -1, 0, 0, 0, 1, 0, 1, 0, 2, 0, 3, 0, 3, -127,
    ],
    &[
        -3, -3, -2, -2, -2, -1, -1, 0, -1, 0, -1, 1, -1, 2, -1, 3, -1, 4, -1, 4, -127,
    ],
    &[
        -3, 0, -3, 0, -3, 0, -2, 0, -2, 1, -2, 1, -2, 2, -1, 3, -1, 3, -1, 3, -127,
    ],
    &[
        0, 1, 0, 2, 0, 2, 0, 2, 0, 3, 0, 3, 0, 3, 0, 3, 0, 3, 0, 3, -127,
    ],
    &[
        -2, 1, -2, 1, -2, 1, -1, 1, 0, 1, 1, 1, 2, 0, 1, -1, -2, -1, -2, 1, -127,
    ],
];

/// Frames of a floating score number, by its frame counter.
const FLOAT_FRAMES: [u16; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 4, 3, 2, 1];

/// Tiles around an explosion's blast that it destroys, as dx, dy.
const BLAST_TILES: [(i32, i32); 8] = [
    (0, 0),
    (0, -1),
    (0, -2),
    (1, -2),
    (2, -2),
    (2, -1),
    (2, 0),
    (1, 0),
];

/// Frames an explosion cloud lasts; it adds a rising effect every second one.
const CLOUD_FRAMES: u16 = 18;

/// How an effect moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// Along one of the 7 debris paths, looping through its frames.
    Debris(u8),
    /// Plays once, rising a tile a frame.
    Rise,
    /// Plays once where it is.
    Still,
    /// Not drawn: starts a rising `effect` at a random spot of its box
    /// (`width` by `height` from its bottom left) every second frame.
    Cloud {
        effect: u16,
        width: i32,
        height: i32,
    },
    /// A score number: rises, waits, plays its frames, rises again.
    Float,
}

impl Motion {
    /// The original's movement codes: 0-8 debris paths, 0x61 rise, 0x62
    /// still, 0x64 float.
    pub fn from_code(code: u16) -> Self {
        match code {
            0..9 => Self::Debris(code as u8),
            0x61 => Self::Rise,
            0x64 => Self::Float,
            _ => Self::Still,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: u16,
    pub frame: u16,
    pub frames: u16,
    pub x: i32,
    /// The bottom row.
    pub y: i32,
    pub motion: Motion,
    /// Position in the debris path, frames of a float, or what a cloud has
    /// left.
    pub step: u16,
    /// Frames to wait before it shows.
    pub delay: u16,
}

/// What an effect frame works with.
pub struct EffectCtx<'a> {
    pub map: &'a mut Map,
    pub sprites: &'a Sprites,
    pub camera: (u16, u16),
    pub random: &'a mut Random,
    pub draws: &'a mut alloc::vec::Vec<Draw>,
    pub events: &'a mut alloc::vec::Vec<GameEvent>,
    /// The player's sprite, for harmful effects.
    pub player: Placed,
    /// Set when a harmful effect touched the player.
    pub hurt: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Effects {
    pub slots: [Option<Effect>; MAX_EFFECTS],
}

/// An effect's box, for the box tests: its sprite at its position.
fn placed(e: &Effect, frame: u16) -> Placed {
    Placed {
        id: e.id,
        frame,
        x: e.x,
        y: e.y,
    }
}

/// Whether an effect waits its delay: counts it down and says so.
fn waiting(e: &mut Effect) -> bool {
    if e.delay > 0 {
        e.delay -= 1;
        return true;
    }
    false
}

impl Effects {
    /// The effects playing, in slot order.
    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.slots.iter().flatten()
    }

    /// Starts sprite `id` in the first free slot, if there is one. Fire only
    /// starts over a floor; returns false when it did not for that reason (the
    /// fire of a fuse box stops spreading there).
    pub fn spawn(
        &mut self,
        map: &Map,
        sprites: &Sprites,
        id: u16,
        (x, y): (i32, i32),
        motion: Motion,
        delay: u16,
    ) -> bool {
        let Some(slot) = self.slots.iter_mut().find(|s| s.is_none()) else {
            return true;
        };
        if id == FIRE && map.collide(sprites, FIRE, 0, x, y + 1, Dir::Down) == Hit::Free {
            return false;
        }
        let frames = sprites.get(id).map_or(0, |s| s.frames.len()) as u16;
        *slot = Some(Effect {
            id,
            frame: 0,
            frames,
            x,
            y,
            motion,
            step: 0,
            delay,
        });
        true
    }

    /// An explosion over the box of frame 0 of sprite `area` at `x`, `y`, made
    /// of rising `effect`s.
    pub fn cloud(&mut self, sprites: &Sprites, area: u16, x: i32, y: i32, effect: u16) {
        let first = |id: u16| sprites.get(id).and_then(|s| s.frames.first());
        let Some(slot) = self.slots.iter_mut().find(|s| s.is_none()) else {
            return;
        };
        let (ax, ay, width, height) = first(area).map_or((0, 0, 0, 0), |f| {
            (
                i32::from(f.x_offset),
                i32::from(f.y_offset),
                (f.image.width / 8) as i32,
                (f.image.height / 8) as i32,
            )
        });
        let (ew, eh) = first(effect).map_or((0, 0), |f| {
            ((f.image.width / 8) as i32, (f.image.height / 8) as i32)
        });
        // The original swaps the effect's width and height here.
        *slot = Some(Effect {
            id: effect,
            frame: 0,
            frames: 0,
            x: x + ax - ew / 2,
            y: y + ay + eh / 2,
            motion: Motion::Cloud {
                effect,
                width,
                height,
            },
            step: CLOUD_FRAMES,
            delay: 0,
        });
    }

    /// Whether a burning fire overlaps `p` (what hurts actors).
    pub fn fire_touches(&self, sprites: &Sprites, map_width: u16, p: Placed) -> bool {
        self.slots.iter().flatten().any(|e| {
            e.id == FIRE && e.delay <= 1 && overlap(sprites, map_width, p, placed(e, e.frame))
        })
    }

    /// Draws every effect and moves it on.
    pub fn update(&mut self, c: &mut EffectCtx) {
        for i in 0..MAX_EFFECTS {
            // The slot stays taken while its effect steps, so effects it
            // starts go elsewhere.
            let Some(mut e) = self.slots[i] else {
                continue;
            };
            let keep = self.step(&mut e, c);
            self.slots[i] = keep.then_some(e);
        }
    }

    /// One frame of an effect; false when it is over.
    fn step(&mut self, e: &mut Effect, c: &mut EffectCtx) -> bool {
        match e.motion {
            Motion::Float => {
                e.step += 1;
                if e.step < 6 {
                    e.y -= 1;
                } else if e.step > 0x14 {
                    e.frame += 1;
                    if e.step > 0x22 {
                        e.y -= 1;
                    }
                }
                let frame = FLOAT_FRAMES[usize::from(e.frame % 14)];
                draw(c, e, frame);
                e.step != 0x3c
            }
            Motion::Cloud {
                effect,
                width,
                height,
            } => {
                if e.step % 2 == 1 {
                    let x = e.x + i32::from(c.random.next_byte()) % width.max(1);
                    let y = e.y - i32::from(c.random.next_byte()) % height.max(1);
                    self.spawn(c.map, c.sprites, effect, (x, y), Motion::Rise, 0);
                }
                e.step -= 1;
                e.step > 0
            }
            Motion::Still | Motion::Rise => {
                if !on_screen(c.sprites, c.camera, placed(e, e.frame)) {
                    return false;
                }
                if waiting(e) {
                    return true;
                }
                if e.id == EXPLOSION_START && e.frame == 0 {
                    let sound = explosion_sound(c.random);
                    c.events.push(GameEvent::Sound(sound));
                }
                draw(c, e, e.frame);
                if e.id == BLAST && e.frame == 1 {
                    self.blast_tiles(e, c);
                }
                e.frame += 1;
                if e.motion == Motion::Rise {
                    e.y -= 1;
                }
                if e.frame >= e.frames {
                    return false;
                }
                hurt_if_touching(e, c);
                true
            }
            Motion::Debris(path) => {
                if e.step > 0x11 && !on_screen(c.sprites, c.camera, placed(e, e.frame)) {
                    return false;
                }
                if waiting(e) {
                    return true;
                }
                draw(c, e, e.frame);
                e.frame += 1;
                if e.frame >= e.frames {
                    e.frame = 0;
                }
                let path = PATHS.get(usize::from(path)).copied().unwrap_or(&[]);
                let at = |i: u16| path.get(usize::from(i)).copied().unwrap_or(0);
                e.x += i32::from(at(e.step));
                e.y += i32::from(at(e.step + 1));
                e.step += 2;
                if at(e.step) == -127 {
                    e.step -= 2;
                }
                hurt_if_touching(e, c);
                true
            }
        }
    }

    /// The blast's second frame destroys the destructible tiles around it,
    /// each with a blast of its own.
    fn blast_tiles(&mut self, e: &Effect, c: &mut EffectCtx) {
        for (dx, dy) in BLAST_TILES {
            let (x, y) = (e.x + dx, e.y + dy);
            if c.map.cell(x, y) & 0x8000 == 0 && c.map.has(x, y, TileAttributes::DESTRUCTIBLE) {
                c.map.set_cell(x, y, 0);
                let delay = u16::from(c.random.next_byte() & 3);
                self.spawn(
                    c.map,
                    c.sprites,
                    BLAST,
                    (x - 1, y + 1),
                    Motion::Still,
                    delay,
                );
            }
        }
    }
}

/// One of the two explosion sounds, at random.
pub fn explosion_sound(random: &mut Random) -> Sound {
    if random.next_byte() & 1 != 0 {
        Sound::Explosion
    } else {
        Sound::Explosion2
    }
}

fn draw(c: &mut EffectCtx, e: &Effect, frame: u16) {
    c.draws.push(Draw::sprite(e.id, frame, e.x, e.y));
}

fn hurt_if_touching(e: &Effect, c: &mut EffectCtx) {
    if !HARMFUL.contains(&e.id) {
        return;
    }
    // The frame it shows next.
    if overlap(c.sprites, c.map.width, placed(e, e.frame), c.player) {
        c.hurt = true;
    }
}
