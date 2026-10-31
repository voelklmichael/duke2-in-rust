//! The player: movement states, collision with the map, and the camera.
//!
//! One frame is: the conveyor under the feet, a ladder to grab, the
//! sideways step, then the state's own vertical step (jump, fall, climb, hang
//! or stand), then the weapon. States hand over to each other within the
//! frame the way the original's code does, because that decides things like
//! whether a landing already starts a jump. Sprite frames are the original's
//! frame numbers of the player sprites (ids 5 and 6). In the ship, the
//! player is the ship's sprite (87 or 88).

use alloc::vec::Vec;

use dn2_assets::sprites::Sprites;

use crate::map::{Dir, Hit, Map, Mover};
use crate::shots::{SHIP_SHOT, ShotDirection, ShotKind, ShotRequest};
use crate::{Buttons, GameEvent, Random, Sound};

/// The three places on the player a spider clings to: the original's array of
/// them, in the order of its slots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpiderPlaces<T> {
    /// The second spider to crawl on sits on the back.
    pub back: T,
    /// The first to crawl on sits on the face, in front of the weapon.
    pub face: T,
    /// One that fell on the player sits on the head.
    pub head: T,
}

impl<T: Copy + PartialEq> SpiderPlaces<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U) -> SpiderPlaces<U> {
        SpiderPlaces {
            back: f(self.back),
            face: f(self.face),
            head: f(self.head),
        }
    }

    pub fn contains(&self, value: T) -> bool {
        [self.back, self.face, self.head].contains(&value)
    }

    /// Clears (to `empty`) the places that hold `value`.
    pub fn clear(&mut self, value: T, empty: T) {
        for place in [&mut self.back, &mut self.face, &mut self.head] {
            if *place == value {
                *place = empty;
            }
        }
    }
}

pub mod frame {
    /// Standing; also the box used for walking, jumping and falling.
    pub const STAND: u16 = 0;
    /// Walking: 1 to 4.
    pub const WALK_LAST: u16 = 4;
    /// Landing, and the first jump step.
    pub const CROUCH: u16 = 5;
    /// Rising in a jump, and the first falling pose.
    pub const AIR: u16 = 6;
    pub const FALL_SLOW: u16 = 7;
    pub const FALL_FAST: u16 = 8;
    /// Somersault: 8 to 15.
    pub const SOMERSAULT_END: u16 = 0x10;
    pub const LOOK_UP: u16 = 0x10;
    pub const LOOK_DOWN: u16 = 0x11;
    /// Standing and firing forwards.
    pub const FIRE: u16 = 0x12;
    pub const FIRE_UP: u16 = 0x13;
    pub const HANG: u16 = 0x14;
    /// Hand over hand along a pipe: 0x15 to 0x18.
    pub const HANG_FIRST: u16 = 0x15;
    pub const HANG_LAST: u16 = 0x18;
    pub const HANG_DOWN: u16 = 0x19;
    pub const FIRE_HANG_DOWN: u16 = 0x1a;
    pub const FIRE_HANG: u16 = 0x1b;
    pub const HANG_UP: u16 = 0x1c;
    pub const FIRE_LOOK_DOWN: u16 = 0x22;
    /// Climbing: 0x23 and 0x24.
    pub const CLIMB_A: u16 = 0x23;
    pub const CLIMB_B: u16 = 0x24;
    /// Hovering on the flame thrower's flames, and firing down from there.
    pub const HOVER: u16 = 0x25;
    pub const FIRE_HOVER: u16 = 0x26;
    /// The dying player's hop is frames 29 to 31; then the body lies.
    pub const LYING: u16 = 0x20;
    /// The body is gone (the explosion hides it).
    pub const GONE: u16 = 0xff;
}

/// The dying player's hop: the frame and the rows it moves, per step.
const DEATH_HOP: [(u16, i16); 6] = [(29, -2), (29, -1), (29, 0), (29, 0), (30, 1), (31, 1)];
/// Frames an item blinks before it is taken away.
const FADE_FRAMES: u8 = 10;
/// The body must have lain this many frames for the explosion.
const EXPLODE_AT: u8 = 10;
/// Frames of lying, with the explosion, until the level restarts.
pub const DEATH_FRAMES: u8 = 0x23;
/// The sprite of the explosion over the body.
const DEATH_EXPLOSION: u16 = 0x47;

/// Tiles up per jump step; the jump is over after the last.
const JUMP_STEPS: [i32; 9] = [0, 2, 2, 1, 1, 1, 0, 0, 0];
/// The jump step after the key is let go early (a short hop).
const SHORT_HOP_STEP: u8 = 6;
/// A fall moves two tiles a frame once it has gone on this many steps.
const FALL_FAST_STEP: u8 = 4;
/// Ammunition a fresh weapon starts with.
pub const START_AMMO: u8 = 0x20;
/// Frames the camera waits before it runs ahead of a player looking down
/// and firing.
const LOOK_DOWN_DELAY: u8 = 5;
/// The somersault happens in a jump when a random byte is a multiple of this.
const SOMERSAULT_ODDS: u8 = 6;

/// The ship's sprites, facing right and left.
pub const SHIP_RIGHT: u16 = 87;
pub const SHIP_LEFT: u16 = 88;
/// The level types of the ship the player leaves behind, facing left and
/// right.
const LEFT_SHIP_LEFT: u16 = 89;
const LEFT_SHIP_RIGHT: u16 = 90;
/// Frames the ship flies one way before it goes two tiles a frame.
const SHIP_THRUST: u8 = 4;

/// Player sprite frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayerState {
    /// On the ground: standing, walking, looking up or down.
    #[default]
    Standing,
    Jumping,
    Falling,
    /// The frame after a long fall, crouching.
    Landing,
    /// Hanging from a pipe.
    Hanging,
    /// On a ladder.
    Climbing,
    /// Health ran out.
    Dying,
    /// Rising on the flames of the flame thrower, firing down.
    FlameHover,
    /// Held by a snake or a ceiling sucker: no control, the actor moves the
    /// player (`GRABBER`); the sprite may be hidden (frame 0xff).
    Grabbed,
    /// Carried up by an air fan, two tiles a frame.
    AirFan,
    /// Carried by a turbo lift: the lift moves him.
    Lift,
    /// Blown out of an airlock, to the left or the right.
    AirlockLeft,
    AirlockRight,
    /// Flying the ship: frame 1 of the ship's sprite.
    Ship,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    Left,
    #[default]
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weapon {
    #[default]
    Regular,
    Laser,
    Rocket,
    FlameThrower,
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    /// The left column, in tiles; left of the map is negative.
    pub x: i32,
    /// The bottom row, in tiles; above the map is negative.
    pub y: i32,
    pub facing: Facing,
    pub state: PlayerState,
    /// Sprite frame of the facing's sprite.
    pub frame: u16,
    pub health: u8,
    pub weapon: Weapon,
    pub ammo: u8,
    /// Frames of rapid fire left.
    pub rapid_fire_timer: u16,
    /// Frames the player cannot be hurt again after a hit; blinks meanwhile.
    pub hurt_timer: u8,
    /// Frames of the cloaking device left: invisible to some actors,
    /// cannot be hurt.
    pub cloak_timer: u16,
    /// The player was hurt in this level (the bonus screen asks).
    pub was_hurt: bool,
    /// Frames the dead player has lain there.
    pub death_timer: u8,
    /// Steps of the hop into the dead pose.
    death_step: u8,
    /// The places a spider clings to (the game sets this before every
    /// update). One on the face blocks the weapon, one on the head shortens
    /// jumps, and any stops somersaults.
    pub clinging: SpiderPlaces<bool>,
    /// Items carried (their actor types), in the order picked up.
    pub inventory: Vec<u16>,
    /// Items on their way out of the inventory and the frames they blink for
    /// yet.
    pub fading: Vec<(u16, u8)>,
    /// Where the cloak was found: it comes back there when it wears off.
    pub cloak_origin: (i32, i32),
    /// The buttons of this frame, after opposite directions cancelled.
    pub input: Buttons,
    /// Steps into the current jump.
    pub jump_step: u8,
    /// Steps into the current fall.
    pub fall_step: u8,
    /// A jump or grab used the jump button; it must be let go first.
    jump_latch: bool,
    /// Flips every frame: walking and hand-over-hand step on every other one.
    anim_toggle: bool,
    /// The jump turns somersaults.
    pub somersault: bool,
    climb_toggle: bool,
    /// The sideways step of this frame happened.
    moved: bool,
    /// The belt the last collision check found under the player.
    pub conveyor: Option<Dir>,
    camera_delay: u8,
    /// Standing at a card slot or key hole that takes an item: no looking
    /// up (set by the slot, cleared every frame).
    pub item_slot_active: bool,
    /// On a turbo lift (set by it).
    pub on_lift: bool,
    /// Steps of walking through a door after using a key or card, 1 to 8;
    /// he is held still meanwhile.
    pub door_step: u8,
    /// Step of the airlock hop.
    airlock_step: u8,
    /// A shot was fired and the fire button is not let go yet.
    fire_latch: bool,
    /// Lets rapid weapons shoot on every other frame.
    fire_toggle: bool,
    /// Frames the ship has flown the same way, up to [`SHIP_THRUST`].
    ship_thrust: u8,
    /// Died in the ship: it is left behind on the next update.
    ship_died: bool,
    /// The ship the player got out of this frame, for the game to spawn:
    /// its level type and position.
    pub left_ship: Option<(u16, i32, i32)>,
}

/// What a frame of the player works with.
struct Ctx<'a> {
    map: &'a Map,
    sprites: &'a Sprites,
    random: &'a mut Random,
    shots: &'a mut Vec<ShotRequest>,
    events: &'a mut Vec<GameEvent>,
}

/// Frames the player cannot be hurt at the start of a level or after a
/// respawn.
const START_HURT_TIMER: u8 = 0x14;

impl Player {
    /// A player standing at a placed start position, with the regular
    /// weapon, who cannot be hurt for a moment.
    pub fn new(x: i32, y: i32, facing: Facing) -> Self {
        Self {
            x,
            y,
            facing,
            health: 9,
            ammo: START_AMMO,
            hurt_timer: START_HURT_TIMER,
            ..Self::default()
        }
    }

    /// One point of health, then a while (shorter on harder levels) without
    /// further harm; at 0 the player dies. Nothing while cloaked, still
    /// blinking or dying.
    pub fn damage(&mut self, difficulty: u8, events: &mut Vec<GameEvent>) {
        if self.cloak_timer != 0 || self.hurt_timer != 0 || self.state == PlayerState::Dying {
            return;
        }
        self.health = self.health.saturating_sub(1);
        self.was_hurt = true;
        if self.health > 0 {
            self.hurt_timer = 50 - difficulty * 10;
            events.push(GameEvent::Sound(Sound::Hurt));
        } else {
            self.ship_died = self.state == PlayerState::Ship;
            self.state = PlayerState::Dying;
            self.death_step = 0;
            self.death_timer = 0;
            events.push(GameEvent::Sound(Sound::Die));
        }
    }

    /// Adds an item that is carried once: a second one replaces the first.
    pub fn give_item(&mut self, item: u16) {
        self.inventory.retain(|&i| i != item);
        self.fading.retain(|&(i, _)| i != item);
        self.inventory.push(item);
    }

    /// The item blinks for a moment ([`Self::update_fading`]) before it goes.
    pub fn take_item(&mut self, item: u16) {
        if self.inventory.contains(&item) && !self.fading.iter().any(|&(i, _)| i == item) {
            self.fading.push((item, FADE_FRAMES));
        }
    }

    /// Once a frame, the items on their way out count down and are gone.
    pub fn update_fading(&mut self) {
        for (_, t) in &mut self.fading {
            *t -= 1;
        }
        let inventory = &mut self.inventory;
        self.fading.retain(|&(item, t)| {
            if t == 0 {
                inventory.retain(|&i| i != item);
            }
            t != 0
        });
    }

    /// Whether the item blinks off now (every other frame while fading).
    pub fn item_hidden(&self, item: u16, draw_page: bool) -> bool {
        draw_page && self.fading.iter().any(|&(i, _)| i == item)
    }

    /// The explosion over the body and the sparks of its first moments: called
    /// after each frame of the dying player.
    pub fn death_effects(
        &self,
        effects: &mut crate::effects::Effects,
        particles: &mut crate::particles::Particles,
        map: &Map,
        sprites: &Sprites,
        random: &mut Random,
    ) {
        use crate::particles::Spread;
        if self.state != PlayerState::Dying || self.frame != frame::GONE {
            return;
        }
        let (x, y) = (self.x, self.y);
        let burst = match self.death_timer {
            EXPLODE_AT => Some((6, Spread::Both, 2)),
            12 => Some((3, Spread::Right, 1)),
            14 => Some((0xa, Spread::Left, 2)),
            _ => None,
        };
        if self.death_timer == EXPLODE_AT {
            effects.spawn(
                map,
                sprites,
                DEATH_EXPLOSION,
                (x, y),
                crate::effects::Motion::Still,
                0,
            );
        }
        if let Some((color, spread, dx)) = burst {
            particles.spawn(random, color, spread, x + dx, y);
        }
    }

    /// The player's sprite where it is.
    pub fn placed(&self) -> crate::draw::Placed {
        crate::draw::Placed {
            id: self.sprite_id(),
            frame: self.frame,
            x: self.x,
            y: self.y,
        }
    }

    /// The sprite id: the facing (5 left, 6 right, as the placed starts),
    /// or the ship's.
    pub fn sprite_id(&self) -> u16 {
        let ship = self.state == PlayerState::Ship || self.ship_died;
        match (self.facing, ship) {
            (Facing::Left, false) => 5,
            (Facing::Right, false) => 6,
            (Facing::Left, true) => SHIP_LEFT,
            (Facing::Right, true) => SHIP_RIGHT,
        }
    }

    /// Climbs into the ship at `x`, `y`.
    pub fn enter_ship(&mut self, x: i32, y: i32, facing: Facing) {
        self.state = PlayerState::Ship;
        self.facing = facing;
        self.frame = 1;
        (self.x, self.y) = (x, y);
    }

    /// The ship stays where it is, facing the same way, and the player is
    /// beside it.
    fn leave_ship(&mut self) {
        let (kind, dx) = match self.facing {
            Facing::Left => (LEFT_SHIP_LEFT, 3),
            Facing::Right => (LEFT_SHIP_RIGHT, 1),
        };
        self.left_ship = Some((kind, self.x, self.y));
        self.ship_died = false;
        self.x += dx;
    }

    /// One game frame. Shots the weapon fires go to `shots`.
    pub fn update(
        &mut self,
        input: Buttons,
        map: &Map,
        sprites: &Sprites,
        random: &mut Random,
        shots: &mut Vec<ShotRequest>,
        events: &mut Vec<GameEvent>,
    ) {
        let mut c = Ctx {
            map,
            sprites,
            random,
            shots,
            events,
        };
        let mut input = input;
        if self.clinging.face {
            input.remove(Buttons::FIRE);
        }
        if self.state == PlayerState::Grabbed {
            // The grabbing actor moves the player; only the camera looks on.
            self.input = input;
            return;
        }
        match self.state {
            PlayerState::AirlockLeft | PlayerState::AirlockRight => {
                self.airlock(&mut c);
                return;
            }
            // The lift carries him: no movement of his own, and the weapon
            // does not fire (but keeps its rhythm).
            PlayerState::Lift => {
                self.input = input;
                if input.contains(Buttons::UP | Buttons::DOWN) {
                    self.input.remove(Buttons::UP | Buttons::DOWN);
                }
                self.weapon_step(&mut c);
                return;
            }
            PlayerState::Standing if self.door_step != 0 => return,
            _ => {}
        }
        self.anim_toggle = !self.anim_toggle;
        self.moved = false;
        self.input = input;
        self.ride_conveyor(&c);
        if self.state == PlayerState::Dying {
            self.die(&mut c);
            return;
        }
        // The ladder check comes before opposite directions cancel; grabbing
        // one goes straight to climbing, facing the way it did.
        if !self.grab_ladder(&c) {
            for (a, b) in [
                (Buttons::LEFT, Buttons::RIGHT),
                (Buttons::UP, Buttons::DOWN),
            ] {
                if input.contains(a) && input.contains(b) {
                    self.input.remove(a | b);
                }
            }
            if self.state == PlayerState::Ship {
                if self.ship(&mut c) {
                    self.vertical(&mut c);
                }
                self.weapon_step(&mut c);
                return;
            }
            if self.input.contains(Buttons::LEFT) {
                self.facing = Facing::Left;
            }
            if self.input.contains(Buttons::RIGHT) {
                self.facing = Facing::Right;
            }
        }
        let mut hovered = false;
        if self.state != PlayerState::Climbing {
            self.walk(&c);
            hovered = self.flame_hover(&c);
            if self.state == PlayerState::Landing {
                self.state = PlayerState::Standing;
                c.events.push(GameEvent::Sound(Sound::Land));
            }
        }
        if !hovered {
            self.vertical(&mut c);
        }
        self.weapon_step(&mut c);
    }

    /// The end of the frame: the weapon.
    fn weapon_step(&mut self, c: &mut Ctx) {
        if self.input.contains(Buttons::FIRE) {
            self.camera_delay = if self.conveyor.is_none() {
                LOOK_DOWN_DELAY
            } else {
                0
            };
        } else {
            self.fire_toggle = false;
        }
        if self.state != PlayerState::Climbing {
            self.fire(c);
        }
    }

    /// Flying the ship: a tile a frame the way it is steered, two once it has
    /// flown that way for [`SHIP_THRUST`] frames, sliding up or down along what
    /// is in the way; up and down a tile a frame. Jump gets out. Returns
    /// whether the player jumped out.
    fn ship(&mut self, c: &mut Ctx) -> bool {
        self.frame = 1;
        let steer = if self.pressed(Buttons::LEFT) {
            Some(Facing::Left)
        } else if self.pressed(Buttons::RIGHT) {
            Some(Facing::Right)
        } else {
            None
        };
        match steer {
            Some(facing) => {
                if self.facing != facing {
                    self.ship_thrust = 0;
                }
                self.facing = facing;
                self.ship_thrust = (self.ship_thrust + 1).min(SHIP_THRUST);
                self.ship_step(c);
                if self.ship_thrust == SHIP_THRUST {
                    self.ship_step(c);
                }
            }
            None => self.ship_thrust = 0,
        }
        let free = |p: &Self, id, dy, dir| {
            c.map.collide(c.sprites, id, 0, p.x, p.y + dy, dir) == Hit::Free
        };
        if self.pressed(Buttons::UP) && free(self, SHIP_RIGHT, -1, Dir::Up) {
            self.y -= 1;
        }
        if self.pressed(Buttons::DOWN) && free(self, SHIP_RIGHT, 1, Dir::Down) {
            self.y += 1;
        }
        self.release_jump();
        if !self.pressed(Buttons::JUMP) || self.jump_latch {
            return false;
        }
        self.leave_ship();
        self.jump_latch = true;
        Self::sound(c, Sound::Jump);
        self.state = PlayerState::Jumping;
        self.jump_step = 0;
        true
    }

    /// One tile of the ship's flight the way it faces, or a slide up or down
    /// a tile when that is blocked.
    fn ship_step(&mut self, c: &Ctx) {
        let (dx, dir) = match self.facing {
            Facing::Left => (-1, Dir::Left),
            Facing::Right => (1, Dir::Right),
        };
        let id = self.sprite_id();
        let hit = |x, y, id, dir| c.map.collide(c.sprites, id, 0, x, y, dir);
        if hit(self.x + dx, self.y, id, dir) == Hit::Free {
            self.x += dx;
        } else if hit(self.x, self.y + 1, SHIP_LEFT, Dir::Down) != Hit::Free {
            self.y -= 1;
        } else if hit(self.x, self.y - 1, SHIP_LEFT, Dir::Up) != Hit::Free {
            self.y += 1;
        }
    }

    /// The dying player: a short hop, a fall to the floor, then the body lies
    /// there; from [`EXPLODE_AT`] frames on it is gone (see
    /// [`Self::death_effects`]), and [`DEATH_FRAMES`] end the level.
    fn die(&mut self, c: &mut Ctx) {
        if self.ship_died {
            self.leave_ship();
        }
        if self.frame == frame::LYING || self.frame == frame::GONE {
            self.death_timer = self.death_timer.saturating_add(1);
            if self.death_timer < EXPLODE_AT {
                return;
            }
            self.frame = frame::GONE;
            if self.death_timer & 1 != 0 && self.death_timer > 8 && self.death_timer < 0x10 {
                let id = crate::effects::explosion_sound(c.random);
                Self::sound(c, id);
            }
            return;
        }
        let step = usize::from(self.death_step);
        if step == DEATH_HOP.len() {
            // Fall to the floor, up to two tiles.
            for _ in 0..2 {
                self.move_by(0, 1);
                let frame = self.frame;
                if self.check(c, 0, 1, frame, Dir::Down) == Hit::Solid {
                    self.frame = frame::LYING;
                    break;
                }
            }
            return;
        }
        let (frame, dy) = DEATH_HOP[step];
        self.frame = frame;
        self.move_by(0, i32::from(dy));
        self.death_step += 1;
    }

    /// The walk out of an airlock: two tiles sideways a frame, with a hop. (The
    /// camera follows in [`follow_camera`].)
    fn airlock(&mut self, c: &mut Ctx) {
        const HOP: [i32; 5] = [-2, -2, -1, -1, 0];
        self.y += HOP[usize::from(self.airlock_step)];
        self.frame += 1;
        if self.frame == 0x10 {
            self.frame = 8;
        }
        if self.state == PlayerState::AirlockLeft {
            self.facing = Facing::Left;
            self.x -= 2;
        } else {
            self.facing = Facing::Right;
            self.x += 2;
        }
        if self.airlock_step == 0 {
            Self::sound(c, Sound::Hurt);
        }
        if self.airlock_step < 4 {
            self.airlock_step += 1;
        }
    }

    fn pressed(&self, button: Buttons) -> bool {
        self.input.contains(button)
    }

    /// Whether sprite frame `frame` of the player at `x`, `y` is clear on
    /// `dir`. The standing frame's box is a column narrower: the blank column
    /// is left out on the side it is on. Also notes the belt, steps up a one
    /// tile ledge and lines up with a ladder.
    fn probe(&mut self, c: &Ctx, x: i32, y: i32, frame: u16, dir: Dir) -> Hit {
        let id = self.sprite_id();
        self.probe_as(c, id, x, y, frame, dir)
    }

    /// [`Self::probe`] with the box of sprite `id`.
    fn probe_as(&mut self, c: &Ctx, id: u16, x: i32, y: i32, frame: u16, dir: Dir) -> Hit {
        self.conveyor = None;
        if id == SHIP_LEFT || id == SHIP_RIGHT {
            // The ship is an ordinary sprite to the map.
            return c.map.collide(c.sprites, id, frame, x, y, dir);
        }
        let Some(mut body) = Map::sprite_body(c.sprites, id, frame, 0, 0) else {
            return Hit::Free;
        };
        // The player's own offsets are not part of the box.
        (body.x, body.y) = (x, y);
        let left = id == 5;
        if left && self.x == 0 && dir == Dir::Left {
            return Hit::Solid;
        }
        if frame == frame::STAND || (left && frame == frame::HOVER) {
            body.width -= 1;
            if left {
                body.x += 1;
            }
        }
        let mover = Mover::Player {
            standing: self.state == PlayerState::Standing,
            grab_ladder: self.pressed(Buttons::UP) && !self.moves_sideways(),
        };
        let probe = c.map.probe(body, dir, mover);
        self.conveyor = probe.conveyor;
        if probe.step_up {
            self.y -= 1;
        }
        self.x += i32::from(probe.shift);
        probe.hit
    }

    fn moves_sideways(&self) -> bool {
        self.input.intersects(Buttons::LEFT | Buttons::RIGHT)
    }

    /// `probe` for the player's own position moved by `dx`, `dy`.
    fn check(&mut self, c: &Ctx, dx: i32, dy: i32, frame: u16, dir: Dir) -> Hit {
        let (x, y) = (self.x + dx, self.y + dy);
        self.probe(c, x, y, frame, dir)
    }

    fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    fn sound(c: &mut Ctx, id: Sound) {
        c.events.push(GameEvent::Sound(id));
    }

    /// The belt under the feet moves the player one tile a frame.
    fn ride_conveyor(&mut self, c: &Ctx) {
        self.check(c, 0, 1, frame::STAND, Dir::Down);
        let (dx, dir) = match self.conveyor {
            Some(Dir::Left) => (-1, Dir::Left),
            Some(Dir::Right) => (1, Dir::Right),
            _ => return,
        };
        if self.check(c, dx, 0, frame::STAND, dir) == Hit::Free {
            self.move_by(dx, 0);
        }
    }

    /// Up with a ladder at the head: start climbing (a jump still rising
    /// fast goes past it). Returns whether the player took hold.
    fn grab_ladder(&mut self, c: &Ctx) -> bool {
        if self.state == PlayerState::Climbing || !self.pressed(Buttons::UP) {
            return false;
        }
        let ladder = self.check(c, 0, 0, frame::CLIMB_B, Dir::Up) == Hit::Climb;
        if ladder && !(self.state == PlayerState::Jumping && self.jump_step < 4) {
            self.state = PlayerState::Climbing;
            self.frame = frame::CLIMB_B;
            return true;
        }
        false
    }

    /// The step left or right. Not while crouching after a landing, looking
    /// up or down, on the second frame of a jump, or looking up from a pipe.
    fn walk(&mut self, c: &Ctx) {
        if !self.moves_sideways() || self.state == PlayerState::Landing {
            return;
        }
        let up_or_down = self.pressed(Buttons::UP) || self.pressed(Buttons::DOWN);
        let blocked = match self.state {
            PlayerState::Jumping => self.jump_step == 1,
            PlayerState::Standing => up_or_down,
            PlayerState::Hanging => self.pressed(Buttons::DOWN) || self.pressed(Buttons::FIRE),
            _ => false,
        };
        if blocked || self.frame == frame::HANG_UP {
            return;
        }
        let free = if self.state == PlayerState::Hanging {
            // A hanging player's box sits a row higher on the right.
            match self.facing {
                Facing::Right => self.check(c, 1, -1, frame::STAND, Dir::Right),
                Facing::Left => {
                    let frame = self.frame;
                    self.check(c, 0, 0, frame, Dir::Left)
                }
            }
        } else {
            match self.facing {
                Facing::Right => self.check(c, 1, 0, frame::STAND, Dir::Right),
                Facing::Left => self.check(c, -1, 0, frame::STAND, Dir::Left),
            }
        } == Hit::Free;
        if free {
            self.move_by(if self.facing == Facing::Right { 1 } else { -1 }, 0);
            self.moved = true;
        }
    }

    /// Down and fire with the flame thrower: the player hovers on the flames,
    /// rising a tile a frame unless a ceiling is in the way. Letting go of
    /// either button starts a fall. Not while the sideways step was skipped for
    /// being pressed together with up or down, jumping off, or hanging, as in
    /// the original.
    fn flame_hover(&mut self, c: &Ctx) -> bool {
        let skipped = self.moves_sideways()
            && self.state != PlayerState::Landing
            && self.frame != frame::HANG_UP
            && match self.state {
                PlayerState::Jumping => self.jump_step == 1,
                PlayerState::Standing => self.pressed(Buttons::UP) || self.pressed(Buttons::DOWN),
                PlayerState::Hanging => self.pressed(Buttons::DOWN) || self.pressed(Buttons::FIRE),
                _ => false,
            };
        if skipped {
            return false;
        }
        let pressed = self.weapon == Weapon::FlameThrower
            && self.pressed(Buttons::DOWN)
            && self.pressed(Buttons::FIRE);
        if pressed {
            self.state = PlayerState::FlameHover;
            self.frame = frame::HOVER;
        }
        if self.state != PlayerState::FlameHover {
            return false;
        }
        if pressed {
            // The original tests the box facing left, whichever way he faces.
            let (x, y) = (self.x, self.y - 1);
            if self.probe_as(c, 5, x, y, frame::HOVER, Dir::Up) != Hit::Solid {
                self.move_by(0, -1);
            }
        } else {
            // Let go of the flames: fall.
            self.jump_latch = true;
            self.fall_off();
            self.frame = frame::AIR;
        }
        true
    }

    /// The state's vertical movement. A state may pass the frame on to the
    /// next one (landing on feet goes on standing, the end of a jump falls).
    fn vertical(&mut self, c: &mut Ctx) {
        for _ in 0..8 {
            let done = match self.state {
                PlayerState::Standing => self.stand(c),
                PlayerState::Hanging => self.hang(c),
                PlayerState::Falling => self.fall(c),
                PlayerState::Jumping => self.jump(c),
                PlayerState::Climbing => self.climb(c),
                PlayerState::AirFan => self.air_fan(c),
                PlayerState::Landing
                | PlayerState::Dying
                | PlayerState::FlameHover
                | PlayerState::Grabbed
                | PlayerState::Lift
                | PlayerState::AirlockLeft
                | PlayerState::AirlockRight
                | PlayerState::Ship => true,
            };
            if done {
                return;
            }
        }
        debug_assert!(false, "player state loop");
    }

    /// Carried up by an air fan: two tiles a frame while the way is free.
    fn air_fan(&mut self, c: &mut Ctx) -> bool {
        for _ in 0..2 {
            if self.check(c, 0, -1, frame::STAND, Dir::Up) != Hit::Solid {
                self.move_by(0, -1);
            }
        }
        true
    }

    fn release_jump(&mut self) {
        if !self.pressed(Buttons::JUMP) {
            self.jump_latch = false;
        }
    }

    /// Falls from a jump or off a ladder, ending any somersault (`FallStart`).
    fn start_fall(&mut self) {
        self.fall_off();
        self.somersault = false;
    }

    /// Falls from the ground or the flame-thrower hover. The somersault flag
    /// stays as it was: a later jump goes on rolling from its first frame.
    fn fall_off(&mut self) {
        self.state = PlayerState::Falling;
        self.fall_step = 0;
    }

    /// On the ground: jump, or stand, walk and look, or step off an edge.
    fn stand(&mut self, c: &mut Ctx) -> bool {
        self.release_jump();
        if self.pressed(Buttons::JUMP)
            && !self.jump_latch
            && self.check(c, 0, -1, frame::STAND, Dir::Up) != Hit::Solid
        {
            self.jump_latch = true;
            Self::sound(c, Sound::Jump);
            self.state = PlayerState::Jumping;
            self.jump_step = 0;
            return false;
        }
        let below = self.check(c, 0, 1, frame::STAND, Dir::Down);
        if below == Hit::Free || below == Hit::Climb {
            self.fall_off();
            return false;
        }
        self.frame = if self.pressed(Buttons::UP) {
            frame::LOOK_UP
        } else if self.pressed(Buttons::DOWN) {
            frame::LOOK_DOWN
        } else if !self.moves_sideways() || !self.moved {
            frame::STAND
        } else {
            let next = self.frame + u16::from(self.anim_toggle);
            if next > frame::WALK_LAST { 1 } else { next }
        };
        true
    }

    /// Hanging from a pipe: pull up, let go, or shuffle along.
    fn hang(&mut self, c: &mut Ctx) -> bool {
        if self.check(c, 0, 0, frame::STAND, Dir::Up) == Hit::Hang {
            self.move_by(0, 1);
        }
        let above = self.check(c, 0, -1, frame::STAND, Dir::Up);
        self.release_jump();
        let pull_up = self.pressed(Buttons::JUMP)
            && !self.jump_latch
            && !self.pressed(Buttons::DOWN)
            && self.check(c, 0, -2, frame::STAND, Dir::Up) == Hit::Free;
        if pull_up {
            self.jump_latch = true;
            Self::sound(c, Sound::Jump);
            self.state = PlayerState::Jumping;
            self.move_by(0, -1);
            self.jump_step = 1;
            return false;
        }
        if (self.pressed(Buttons::DOWN) && self.pressed(Buttons::JUMP)) || above != Hit::Hang {
            // Let go; the fall starts next frame.
            self.jump_latch = true;
            self.state = PlayerState::Falling;
            self.fall_step = 0;
            self.frame = frame::AIR;
            return true;
        }
        if self.pressed(Buttons::DOWN) {
            self.frame = frame::HANG_DOWN;
        } else if !self.pressed(Buttons::FIRE) && self.moves_sideways() && self.moved {
            let next = self.frame + u16::from(self.anim_toggle);
            self.frame = if next > frame::HANG_LAST {
                frame::HANG_FIRST
            } else {
                next
            };
        } else {
            self.frame = frame::HANG;
        }
        if self.pressed(Buttons::UP) {
            self.frame = frame::HANG_UP;
        }
        true
    }

    fn grab_pipe(&mut self, c: &mut Ctx) {
        self.frame = frame::HANG;
        self.state = PlayerState::Hanging;
        Self::sound(c, Sound::Clank);
    }

    /// One frame of a fall: one tile, then two a frame from the fourth step.
    fn fall(&mut self, c: &mut Ctx) -> bool {
        self.release_jump();
        for tile in 0..2 {
            if tile == 0 {
                self.fall_step = (self.fall_step + 1).min(FALL_FAST_STEP);
            } else if self.fall_step != FALL_FAST_STEP {
                break;
            }
            if self.check(c, 0, 0, frame::STAND, Dir::Up) == Hit::Hang {
                self.grab_pipe(c);
                self.move_by(0, 1);
                return true;
            }
            if self.check(c, 0, 1, frame::STAND, Dir::Down) == Hit::Solid {
                if self.fall_step == FALL_FAST_STEP {
                    self.state = PlayerState::Landing;
                    self.frame = frame::CROUCH;
                    return true;
                }
                self.state = PlayerState::Standing;
                return false;
            }
            self.move_by(0, 1);
            self.frame = if tile == 0 {
                frame::FALL_SLOW
            } else {
                frame::FALL_FAST
            };
        }
        true
    }

    /// One jump step. Returns whether the frame is done.
    fn jump(&mut self, c: &mut Ctx) -> bool {
        self.release_jump();
        if self.jump_step == 1 || self.jump_step == 2 {
            // Rising two tiles: look a second tile ahead for a pipe or the ceiling.
            match self.check(c, 0, -2, frame::STAND, Dir::Up) {
                Hit::Hang => {
                    self.move_by(0, -1);
                    return self.grab_pipe_in_jump(c);
                }
                Hit::Solid => {
                    self.jump_step = 4;
                    self.somersault = false;
                    self.frame = frame::AIR;
                }
                _ => self.frame = frame::AIR,
            }
        }
        if self.jump_step > 8 {
            self.start_fall();
            return false;
        }
        match self.check(c, 0, -1, frame::STAND, Dir::Up) {
            Hit::Hang => return self.grab_pipe_in_jump(c),
            Hit::Solid if self.jump_step < 6 => {
                self.start_fall();
                return false;
            }
            _ => {}
        }
        self.move_by(0, -JUMP_STEPS[usize::from(self.jump_step)]);
        if self.jump_step == 3 && (!self.pressed(Buttons::JUMP) || self.clinging.head) {
            self.jump_step = SHORT_HOP_STEP;
        }
        if self.jump_step == 0 {
            self.frame = frame::CROUCH;
        }
        if self.jump_step == 2 && !self.somersault && !self.clinging.contains(true) {
            self.somersault = c.random.next_byte().is_multiple_of(SOMERSAULT_ODDS);
            self.frame = if self.somersault {
                frame::FALL_FAST
            } else {
                frame::AIR
            };
        }
        if self.somersault {
            self.frame += 1;
            if self.frame == frame::SOMERSAULT_END || !self.moves_sideways() {
                self.frame = frame::AIR;
                self.somersault = false;
            }
        }
        self.jump_step += 1;
        true
    }

    /// A pipe in reach while jumping: hang from it right away.
    fn grab_pipe_in_jump(&mut self, c: &mut Ctx) -> bool {
        self.grab_pipe(c);
        false
    }

    /// On a ladder: jump off, climb two frames to the tile, or drop off the foot.
    fn climb(&mut self, c: &mut Ctx) -> bool {
        self.fall_step = 0;
        self.release_jump();
        if self.pressed(Buttons::JUMP)
            && !self.jump_latch
            && self.check(c, 0, -1, frame::CLIMB_B, Dir::Up) != Hit::Solid
        {
            // later: the original lets the jump start sideways into a wall.
            if self.pressed(Buttons::LEFT) {
                self.move_by(-1, 0);
            }
            if self.pressed(Buttons::RIGHT) {
                self.move_by(1, 0);
            }
            self.jump_latch = true;
            self.state = PlayerState::Jumping;
            Self::sound(c, Sound::Jump);
            self.jump_step = 1;
            return false;
        }
        if self.pressed(Buttons::UP) && self.check(c, 0, -1, frame::CLIMB_B, Dir::Up) == Hit::Climb
        {
            self.move_by(0, -1);
            self.climb_frame();
        }
        if self.pressed(Buttons::DOWN) {
            if self.check(c, 0, 1, frame::CLIMB_B, Dir::Down) != Hit::Climb {
                self.start_fall();
                return false;
            }
            self.move_by(0, 1);
            self.climb_frame();
        }
        true
    }

    /// The climbing frame flips on every second row.
    fn climb_frame(&mut self) {
        if self.y & 1 != 0 {
            self.climb_toggle = !self.climb_toggle;
            self.frame = if self.climb_toggle {
                frame::CLIMB_B
            } else {
                frame::CLIMB_A
            };
        }
    }

    /// The fire button: one shot per press (every other frame while held for
    /// the flame thrower and rapid fire), from the pose of the moment.
    fn fire(&mut self, c: &mut Ctx) {
        self.fire_toggle = !self.fire_toggle;
        if self.frame == frame::HANG_UP
            || self.frame == frame::CROUCH
            || self.state == PlayerState::Lift
        {
            return;
        }
        if !self.pressed(Buttons::FIRE) {
            self.fire_latch = false;
        }
        let ship = self.state == PlayerState::Ship;
        if self.weapon == Weapon::FlameThrower || self.rapid_fire_timer != 0 || ship {
            self.fire_latch = self.fire_toggle;
        }
        if !self.pressed(Buttons::FIRE) || self.fire_latch {
            return;
        }
        self.fire_latch = true;
        if self.frame == frame::STAND {
            self.frame = frame::FIRE;
        }
        let right = self.facing == Facing::Right;
        if ship {
            // The ship's cannon, from its nose; no ammunition used.
            c.shots.push(ShotRequest {
                kind: ShotKind::Blast(SHIP_SHOT),
                x: self.x + if right { 8 } else { -3 },
                y: self.y,
                direction: if right {
                    ShotDirection::Right
                } else {
                    ShotDirection::Left
                },
            });
            Self::sound(c, Sound::Laser);
            return;
        }
        // The shot's direction, where it starts relative to the player, and
        // the pose to show for the shot.
        let (direction, dx, dy, pose) = match (self.frame, right) {
            (frame::LOOK_UP, true) => (ShotDirection::Up, 2, -5, Some(frame::FIRE_UP)),
            (frame::LOOK_UP, false) => (ShotDirection::Up, 1, -5, Some(frame::FIRE_UP)),
            (frame::LOOK_DOWN, true) => (ShotDirection::Right, 3, -1, Some(frame::FIRE_LOOK_DOWN)),
            (frame::LOOK_DOWN, false) => (ShotDirection::Left, -2, -1, Some(frame::FIRE_LOOK_DOWN)),
            (frame::HANG, true) => (ShotDirection::Right, 3, -2, Some(frame::FIRE_HANG)),
            (frame::HANG, false) => (ShotDirection::Left, -2, -2, Some(frame::FIRE_HANG)),
            (frame::HANG_DOWN, true) => (ShotDirection::Down, 0, 1, Some(frame::FIRE_HANG_DOWN)),
            (frame::HANG_DOWN, false) => (ShotDirection::Down, 3, 1, Some(frame::FIRE_HANG_DOWN)),
            (frame::HOVER, true) => (ShotDirection::Down, 1, 1, Some(frame::FIRE_HOVER)),
            (frame::HOVER, false) => (ShotDirection::Down, 2, 1, Some(frame::FIRE_HOVER)),
            (_, true) => (ShotDirection::Right, 3, -2, None),
            (_, false) => (ShotDirection::Left, -2, -2, None),
        };
        c.shots.push(ShotRequest {
            kind: ShotKind::Weapon(self.weapon),
            x: self.x + dx,
            y: self.y + dy,
            direction,
        });
        Self::sound(
            c,
            match self.weapon {
                Weapon::Regular | Weapon::Rocket => Sound::Shot,
                Weapon::Laser => Sound::Laser,
                Weapon::FlameThrower => Sound::Roar,
            },
        );
        if let Some(pose) = pose {
            self.frame = pose;
        }
        if self.weapon != Weapon::Regular {
            self.ammo = self.ammo.saturating_sub(1);
            if self.ammo == 0 {
                self.weapon = Weapon::Regular;
                self.ammo = START_AMMO;
            }
        }
    }
}

/// The view in tiles.
pub const VIEW_WIDTH: i32 = 32;
pub const VIEW_HEIGHT: i32 = 20;

/// The camera for a level's start: the player 15 columns and 19 rows in, kept
/// inside the map.
pub fn center_camera(player: &Player, map: &Map) -> (u16, u16) {
    let (max_x, _) = max_camera(map);
    let x = (player.x - 15).clamp(0, max_x);
    let y = (player.y - 19).clamp(0, (i32::from(map.height) - 0x15).max(0));
    (x as u16, y as u16)
}

/// The furthest camera position: the view's bottom row may be the map's
/// last one.
fn max_camera(map: &Map) -> (i32, i32) {
    (
        (i32::from(map.width) - VIEW_WIDTH).max(0),
        (i32::from(map.height) - VIEW_HEIGHT + 1).max(0),
    )
}

/// The camera's top left map tile for the player, kept inside the map.
///
/// Sideways it keeps the player 10 to 18 columns from the left edge, moving
/// up to two a frame. Up and down: the player may be 6 to 18 rows from the
/// top; below that the camera follows falls at up to two rows a frame. Looking
/// up scrolls the view up until the player is 18 rows down; looking down
/// (standing or hanging) runs ahead down to 5 rows from the top, after a
/// short wait when firing. A jump pulls the camera up two rows at a time if
/// the player gets within 2 of the top edge. On a ladder or a belt, and in
/// the ship, it follows gently, keeping the player 11 to 12 rows down; the
/// ship keeps 11 to 13 columns from the left edge.
pub fn follow_camera(camera: (u16, u16), player: &mut Player, map: &Map) -> (u16, u16) {
    let (max_x, max_y) = max_camera(map);
    let (mut cx, mut cy) = (i32::from(camera.0), i32::from(camera.1));
    let (px, py) = (player.x, player.y);
    let input = player.input;
    let state = player.state;
    let down = input.contains(Buttons::DOWN);
    let up = input.contains(Buttons::UP);
    let map_height = i32::from(map.height);

    // Blown out of an airlock: the view runs along, two columns a frame.
    match state {
        PlayerState::AirlockLeft => {
            if cx > 2 {
                cx -= 2;
            }
            return finish_camera(cx, cy, max_x, max_y);
        }
        PlayerState::AirlockRight => {
            if i32::from(map.width) - 0x22 > cx {
                cx += 2;
            }
            return finish_camera(cx, cy, max_x, max_y);
        }
        _ => {}
    }

    // The ship keeps 11 to 13 columns from the left edge instead.
    let (near, far) = if state == PlayerState::Ship {
        (0xb, 0xd)
    } else {
        (10, 0x12)
    };
    for _ in 0..2 {
        if cx != 0 && px - cx < near {
            cx -= 1;
        } else if cx < max_x && px - cx > far {
            cx += 1;
        }
    }
    let dy = |cy: i32| py - cy;

    if state == PlayerState::Standing
        && up
        && cy != 0
        && dy(cy) < 0x13
        && !player.item_slot_active
        && !player.on_lift
    {
        if cy < 2 {
            cy -= 1;
        } else if dy(cy) < 0x12 {
            cy -= 2;
        }
        if dy(cy) == 0x12 {
            cy -= 1;
        }
    }

    if matches!(
        state,
        PlayerState::Climbing
            | PlayerState::FlameHover
            | PlayerState::AirFan
            | PlayerState::Lift
            | PlayerState::Ship
    ) || (player.conveyor.is_some() && !up && !down)
    {
        // Two gentle steps, the second with a bound a row sooner; the lift
        // takes one more down in the first.
        let downs = if state == PlayerState::Lift { 2 } else { 1 };
        if cy != 0 && dy(cy) < 0xb {
            cy -= 1;
        } else {
            for _ in 0..downs {
                if cy < map_height - 0x13 && dy(cy) > 0xc {
                    cy += 1;
                }
            }
        }
        if cy != 0 && dy(cy) < 0xb {
            cy -= 1;
        } else if cy < map_height - 0x14 && dy(cy) > 0xc {
            cy += 1;
        }
        return finish_camera(cx, cy, max_x, max_y);
    }

    let looking_down =
        down && matches!(state, PlayerState::Standing | PlayerState::Hanging) && !player.on_lift;
    if looking_down {
        if state == PlayerState::Standing && player.camera_delay != 0 {
            player.camera_delay -= 1;
            return finish_camera(cx, cy, max_x, max_y);
        }
        for _ in 0..2 {
            if dy(cy) > 4 && cy + 0x13 < map_height {
                cy += 1;
            }
        }
    } else if up && state == PlayerState::Hanging && cy != 0 {
        if player.camera_delay != 0 {
            player.camera_delay -= 1;
            return finish_camera(cx, cy, max_x, max_y);
        }
        if cy < 2 {
            cy -= 1;
        } else {
            if dy(cy) < 0x12 {
                cy -= 2;
            }
            if dy(cy) == 0x13 {
                cy += 1;
            }
        }
    }

    if py < 0 {
        cy = 0;
    } else if state == PlayerState::Jumping && cy > 2 && py - 2 < cy {
        cy -= 2;
    } else if cy != 0 && dy(cy) < 6 {
        cy -= 1;
    } else if map_height - 0x12 > cy {
        for _ in 0..2 {
            if dy(cy) > 0x12 && map_height - 0x13 > cy {
                cy += 1;
            }
        }
    } else if dy(cy) >= 0x13 {
        cy += 1;
    }
    finish_camera(cx, cy, max_x, max_y)
}

fn finish_camera(cx: i32, cy: i32, max_x: i32, max_y: i32) -> (u16, u16) {
    (cx.clamp(0, max_x) as u16, cy.clamp(0, max_y) as u16)
}
