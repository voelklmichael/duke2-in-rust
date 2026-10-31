//! Actors: everything in a level that is not a tile or a shot, the player's
//! sprite included.
//!
//! The original keeps up to 448 actor slots. Each frame the engine goes through
//! them in order: an actor touching the view (or one that stays active) falls
//! if it has gravity, runs its update function, takes damage from the player's
//! shots, reacts to touching the player, and is drawn. The slot order comes
//! from the sprites' draw passes, so it decides what is in front.
//!
//! An actor has the fields every type uses, and a [`Kind`] with the state of
//! its update function. Behaviours are grouped by family in the submodules;
//! [`hits`] and [`contact`] hold the reactions to shots and to the player,
//! which the original keeps in one place for all types.

mod basic;
mod bosses;
pub mod contact;
mod creatures;
mod hazards;
pub mod hits;
mod items;
mod machines;
mod objects;
mod shooters;
pub mod spawn;
mod table;

use alloc::vec::Vec;

use dn2_assets::sprites::Sprites;

use crate::draw::{self, Draw, Placed, Sprite, SpriteLook};
use crate::effects::{Effects, Motion};
use crate::map::{Dir, Hit, Map};
use crate::particles::{Particles, Spread};
use crate::player::Player;
use crate::shots::Shot;
use crate::state::LevelState;
use crate::{Buttons, GameEvent, Random, Sound};

pub use basic::{Animate, BonusGlobe};
pub use items::ItemBox;
pub use spawn::Update;

/// Most actors at once.
pub const MAX_ACTORS: usize = 448;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ActorFlags: u8 {
        /// Updated every frame, even away from the view.
        const ALWAYS_UPDATE = 0x01;
        /// Becomes `ALWAYS_UPDATE` once it has been in the view.
        const STAY_ACTIVE = 0x02;
        /// Walks off ledges and up one-tile steps.
        const STEPS = 0x04;
        /// Falls when nothing is below.
        const GRAVITY = 0x08;
    }
}

/// The state of an actor's update function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The player's slot: the player is drawn here.
    Player,
    Animate(Animate),
    BonusGlobe(BonusGlobe),
    SecurityCamera,
    ItemBox(ItemBox),
    Creature(creatures::State),
    Shooter(shooters::State),
    Boss(bosses::State),
    Hazard(hazards::State),
    Machine(machines::State),
    /// An update function not written yet: the actor is drawn and stands
    /// still. `vars` are its starting variables.
    Unported {
        update: Update,
        vars: [u16; 5],
    },
}

impl Kind {
    /// The state a spawned actor starts with.
    fn new(s: &spawn::Spawn, vars: [u16; 5]) -> Self {
        match s.update {
            Update::Player => Self::Player,
            Update::Animate => Self::Animate(Animate::new(vars)),
            Update::BonusGlobe => Self::BonusGlobe(BonusGlobe::new(vars)),
            Update::SecurityCamera => Self::SecurityCamera,
            Update::ItemBox => Self::ItemBox(ItemBox::new(vars)),
            update => None
                .or_else(|| creatures::new(update, vars).map(Self::Creature))
                .or_else(|| shooters::new(update, vars).map(Self::Shooter))
                .or_else(|| bosses::new(update, vars).map(Self::Boss))
                .or_else(|| hazards::new(update, vars).map(Self::Hazard))
                .or_else(|| machines::new(update, vars).map(Self::Machine))
                .unwrap_or(Self::Unported { update, vars }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    /// The sprite, which is also the actor's type.
    pub id: u16,
    pub frame: u16,
    /// The left column, in tiles.
    pub x: i32,
    /// The bottom row, in tiles.
    pub y: i32,
    pub flags: ActorFlags,
    /// Frames into a fall: it sets the speed.
    pub fall: u8,
    pub health: i16,
    /// Points for killing it.
    pub score: u16,
    /// How it is drawn. An update may change it for one frame: it is put
    /// back after drawing.
    pub look: SpriteLook,
    /// Gone at the end of this frame's turn (the original frees the slot).
    pub removed: bool,
    pub kind: Kind,
}

impl Actor {
    pub fn placed(&self) -> Placed {
        Placed {
            id: self.id,
            frame: self.frame,
            x: self.x,
            y: self.y,
        }
    }

    pub fn remove(&mut self) {
        self.removed = true;
    }
}

/// The actor slots, in the order they are updated and drawn.
#[derive(Clone, Debug, Default)]
pub struct Actors {
    pub slots: Vec<Option<Actor>>,
    /// The slot whose actor is being updated: taken out meanwhile, but not
    /// free until the actor is removed.
    current: Option<usize>,
    /// The actor being updated was removed: its slot is free for a new one
    /// (which then gets the rest of the frame's checks in its place, as in
    /// the original).
    current_removed: bool,
}

impl Actors {
    pub fn iter(&self) -> impl Iterator<Item = &Actor> {
        self.slots.iter().flatten()
    }
}

/// What actors see and change during a frame.
pub struct Ctx<'a> {
    pub map: &'a mut Map,
    pub sprites: &'a Sprites,
    pub player: &'a mut Player,
    /// The view's top left tile: what is in the view is active.
    pub camera: (u16, u16),
    pub random: &'a mut Random,
    pub state: &'a mut LevelState,
    pub actors: &'a mut Actors,
    pub effects: &'a mut Effects,
    pub particles: &'a mut Particles,
    pub shots: &'a mut Vec<Shot>,
    pub draws: &'a mut Vec<Draw>,
    pub events: &'a mut Vec<GameEvent>,
    pub input: Buttons,
    /// Flips every frame (the original's draw page): a half-rate clock.
    pub draw_page: bool,
}

impl Ctx<'_> {
    pub fn sound(&mut self, id: Sound) {
        self.events.push(GameEvent::Sound(id));
    }

    /// One of the two explosion sounds, at random.
    pub fn explosion_sound(&mut self) {
        let id = crate::effects::explosion_sound(self.random);
        self.sound(id);
    }

    /// Removes the actor being updated; what is spawned after this may take
    /// its slot.
    pub fn remove(&mut self, a: &mut Actor) {
        a.remove();
        self.actors.current_removed = true;
    }

    pub fn add_score(&mut self, points: u16) {
        self.state.score += u32::from(points);
    }

    /// Starts an effect; false when fire found no floor.
    pub fn effect(&mut self, id: u16, x: i32, y: i32, motion: Motion, delay: u16) -> bool {
        self.effects
            .spawn(self.map, self.sprites, id, (x, y), motion, delay)
    }

    /// Rising `effect`s over the box of sprite `area`.
    pub fn cloud(&mut self, area: u16, x: i32, y: i32, effect: u16) {
        self.effects.cloud(self.sprites, area, x, y, effect);
    }

    pub fn particles(&mut self, color: u8, spread: Spread, x: i32, y: i32) {
        self.particles.spawn(self.random, color, spread, x, y);
    }

    /// Particles of a random colour.
    pub fn random_particles(&mut self, spread: Spread, x: i32, y: i32) {
        let color = self.random.next_byte() & 0x0f;
        self.particles(color, spread, x, y);
    }

    /// Spawns a placed type during play.
    pub fn spawn(&mut self, level_type: u16, x: i32, y: i32) {
        self.actors.spawn(level_type, x, y, self.state, self.random);
    }

    /// Draws a sprite now, after what was drawn before.
    pub fn draw(&mut self, id: u16, frame: u16, x: i32, y: i32, look: SpriteLook) {
        self.draws.push(Draw::Sprite(Sprite {
            id,
            frame,
            x,
            y,
            look,
        }));
    }

    /// Collision check for a sprite frame at `x`, `y`.
    pub fn collide(&self, id: u16, frame: u16, x: i32, y: i32, dir: Dir) -> Hit {
        self.map.collide(self.sprites, id, frame, x, y, dir)
    }

    /// Whether the actor's sprite is clear on `dir` where it is.
    pub fn clear(&self, a: &Actor, dir: Dir) -> bool {
        self.collide(a.id, a.frame, a.x, a.y, dir) == Hit::Free
    }

    pub fn on_screen(&self, p: Placed) -> bool {
        draw::on_screen(self.sprites, self.camera, p)
    }

    /// The player's sprite where it is.
    pub fn player_placed(&self) -> Placed {
        self.player.placed()
    }

    /// Whether `p` overlaps the player.
    pub fn touches_player(&self, p: Placed) -> bool {
        draw::overlap(self.sprites, self.map.width, p, self.player_placed())
    }

    pub fn hurt_player(&mut self) {
        let difficulty = self.state.difficulty;
        self.player.damage(difficulty, self.events);
    }

    /// Call after moving the actor one tile along `dir`. If the move was
    /// blocked, the actor goes back and this is true. Sideways, walking off a
    /// ledge is blocked too, unless the actor has `STEPS`; with `GRAVITY` and
    /// `STEPS`, a one-tile step does not block.
    pub fn move_blocked(&self, a: &mut Actor, dir: Dir) -> bool {
        match dir {
            Dir::Up | Dir::Down => {
                if self.clear(a, dir) {
                    return false;
                }
                a.y += if dir == Dir::Up { 1 } else { -1 };
                true
            }
            _ => {
                let steps = a.flags.contains(ActorFlags::STEPS);
                let lifted = steps && a.flags.contains(ActorFlags::GRAVITY);
                let (side, ahead, back) = if dir == Dir::Left {
                    (Dir::Left, -2, 1)
                } else {
                    (Dir::Right, 2, -1)
                };
                let y = a.y - i32::from(lifted);
                let mut free = self.collide(a.id, a.frame, a.x, y, side) == Hit::Free;
                if free {
                    let floor = self.collide(a.id, a.frame, a.x + ahead, y + 1, Dir::Down);
                    if floor == Hit::Free && !steps {
                        free = false;
                    }
                }
                if !free {
                    a.x += back;
                }
                !free
            }
        }
    }
}

/// One frame of every actor but the overlays.
pub fn update(c: &mut Ctx) {
    let count = c.actors.slots.len();
    for i in 0..count {
        let Some(mut a) = c.actors.slots[i].take() else {
            continue;
        };
        if is_overlay(&a) {
            c.actors.slots[i] = Some(a);
            continue;
        }
        c.actors.current = Some(i);
        c.actors.current_removed = false;
        step(&mut a, c);
        c.actors.current = None;
        if !a.removed {
            c.actors.slots[i] = Some(a);
        }
    }
    c.state.messages.update(c.events);
}

/// The water, drawn over everything.
pub fn update_overlays(c: &mut Ctx) {
    for i in 0..c.actors.slots.len() {
        let Some(mut a) = c.actors.slots[i].take() else {
            continue;
        };
        if is_overlay(&a) {
            c.actors.current = Some(i);
            c.actors.current_removed = false;
            behave(&mut a, c);
            c.actors.current = None;
        }
        if !a.removed {
            c.actors.slots[i] = Some(a);
        }
    }
}

/// The water blocks.
const OVERLAY: u16 = 0xdd;

fn is_overlay(a: &Actor) -> bool {
    a.id == OVERLAY
}

fn step(a: &mut Actor, c: &mut Ctx) {
    if !c.on_screen(a.placed()) {
        if !a.flags.contains(ActorFlags::ALWAYS_UPDATE) {
            return;
        }
    } else if a.flags.contains(ActorFlags::STAY_ACTIVE) {
        a.flags.insert(ActorFlags::ALWAYS_UPDATE);
    }
    let look = a.look;
    if a.flags.contains(ActorFlags::GRAVITY) {
        fall(a, c);
    }
    behave(a, c);
    take_replacement(a, c);
    let player = a.kind == Kind::Player;
    if !player && (a.y < 0 || a.y > i32::from(c.map.height)) {
        // Fell out of the map, or flew out of its top (the original's rows
        // are unsigned there).
        c.remove(a);
        return;
    }
    if a.look != SpriteLook::Hidden && a.health > 0 && !player {
        let damage = hits::damage(a, c);
        if damage > 0 {
            hits::react(a, c, damage);
            take_replacement(a, c);
        }
    }
    if a.look != SpriteLook::Hidden && !a.removed {
        contact::touch(a, c);
        take_replacement(a, c);
    }
    if a.look != SpriteLook::Hidden && !a.removed && c.on_screen(a.placed()) {
        c.draw(a.id, a.frame, a.x, a.y, a.look);
    }
    a.look = look;
}

/// If the actor being updated was removed and something spawned into its
/// slot, the rest of the frame's checks go to the new actor.
fn take_replacement(a: &mut Actor, c: &mut Ctx) {
    if !a.removed {
        return;
    }
    let Some(i) = c.actors.current else {
        return;
    };
    if let Some(new) = c.actors.slots[i].take() {
        *a = new;
        c.actors.current_removed = false;
    }
}

/// Runs the actor's update function.
fn behave(a: &mut Actor, c: &mut Ctx) {
    match a.kind {
        Kind::Player => basic::player(a, c),
        Kind::Animate(_) => basic::animate(a, c),
        Kind::BonusGlobe(_) => basic::bonus_globe(a, c),
        Kind::SecurityCamera => basic::security_camera(a, c),
        Kind::ItemBox(_) => items::item_box(a, c),
        Kind::Creature(_) => creatures::update(a, c),
        Kind::Shooter(_) => shooters::update(a, c),
        Kind::Boss(_) => bosses::update(a, c),
        Kind::Hazard(_) => hazards::update(a, c),
        Kind::Machine(_) => machines::update(a, c),
        Kind::Unported { .. } => {}
    }
}

/// The family's own reaction to a hit; false if it has none.
fn family_hit(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    match a.kind {
        Kind::Creature(_) => creatures::hit(a, c, damage),
        Kind::Shooter(_) => shooters::hit(a, c, damage),
        Kind::Boss(_) => bosses::hit(a, c, damage),
        Kind::Hazard(_) => hazards::hit(a, c, damage),
        Kind::Machine(_) => machines::hit(a, c, damage),
        _ => false,
    }
}

/// The family's own reaction to touching the player; false if it has none.
fn family_touch(a: &mut Actor, c: &mut Ctx) -> bool {
    match a.kind {
        Kind::Creature(_) => creatures::touch(a, c),
        Kind::Shooter(_) => shooters::touch(a, c),
        Kind::Boss(_) => bosses::touch(a, c),
        Kind::Hazard(_) => hazards::touch(a, c),
        Kind::Machine(_) => machines::touch(a, c),
        _ => false,
    }
}

/// The fall of an actor with gravity: it hangs for a frame, then falls a
/// tile per frame, then two. Standing on a belt carries it along.
fn fall(a: &mut Actor, c: &mut Ctx) {
    if !c.clear(a, Dir::Down) {
        // Sunk into the floor: back up.
        a.y -= 1;
        a.fall = 0;
    }
    let below = c.map.probe_sprite(c.sprites, a.id, a.frame, a.x, a.y + 1);
    if below.hit == Hit::Free {
        if a.fall < 4 {
            a.fall += 1;
        }
        if a.fall > 1 {
            a.y += 1;
        }
        if a.fall == 4 {
            if c.collide(a.id, a.frame, a.x, a.y + 1, Dir::Down) == Hit::Free {
                a.y += 1;
            } else {
                a.fall = 0;
            }
        }
    } else {
        a.fall = 0;
        let (dx, side) = match below.conveyor {
            Some(Dir::Left) => (-1, Dir::Left),
            Some(Dir::Right) => (1, Dir::Right),
            _ => return,
        };
        if c.collide(a.id, a.frame, a.x + dx, a.y, side) == Hit::Free {
            a.x += dx;
        }
    }
}
