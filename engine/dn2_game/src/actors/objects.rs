//! Level objects that mostly stand where they are: generators, drones,
//! slime pipes, force fields, card slots, water, fire jets and the like.
//! They animate, spawn something now and then, or hurt the player.

use super::machines::State;
use super::{Actor, ActorFlags, Ctx, Kind, hits};
use crate::Sound;
use crate::draw::{Draw, Placed, SpriteLook};
use crate::effects::{EXPLOSION_START, Motion};
use crate::map::Dir;
use crate::messages::{once, text};
use crate::particles::Spread;
use crate::player::{Facing, PlayerState};
use crate::shots::ShotDirection;

/// Placed types and sprites.
const HOVERBOT: u16 = 0;
/// Placed types (which are also their sprites).
const SLIME_DROP: u16 = 0x76;
const WATER_DROP: u16 = 0xe2;
const FORCE_FIELD: u16 = 0x77;
const FIRE_JET: u16 = 0xe7;
const RADAR_DISPLAY: u16 = 0xed;
pub(super) const HINT_GLOBE: u16 = 0xee;
const PRISONER: u16 = 0xfd;
const AIRLOCK_LEFT: u16 = 0xfa;
const HOVERBOT_GENERATOR: u16 = 0x73;
/// The dust a unicycle bot kicks up, and the prisoner's flung body.
const DUST: u16 = 0x0b;
const PRISONER_BODY: u16 = 0xff;
/// The parts a messenger drone is drawn from, the picture screen last.
const DRONE_JET_LEFT: u16 = 0x71;
const DRONE_BODY_LEFT: u16 = 0x6c;
const DRONE_JET_RIGHT: u16 = 0x70;
const DRONE_BODY_RIGHT: u16 = 0x6d;
const DRONE_DOME: u16 = 0x6e;
const DRONE_HULL: u16 = 0x6b;
const DRONE_SCREEN: u16 = 0x6f;

/// `Kind` of an actor of this family: its state, or nothing.
macro_rules! state {
    ($a:expr, $variant:ident) => {
        match &mut $a.kind {
            Kind::Machine(State::$variant(s)) => s,
            _ => return,
        }
    };
}

// ---------------------------------------------------------------------
// Hoverbot generator

/// Hoverbots a generator makes at most, and the frames between two.
const HOVERBOTS: u16 = 0x1e;
const HOVERBOT_PERIOD: u16 = 0x24;

/// Type 115 makes a hoverbot every 36 frames, up to 30 times.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HoverbotGenerator {
    made: u16,
    timer: u16,
}

pub(super) fn hoverbot_generator(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, HoverbotGenerator);
    let mut make = false;
    if s.made < HOVERBOTS {
        s.timer += 1;
        if s.timer == HOVERBOT_PERIOD {
            s.made += 1;
            s.timer = 0;
            make = true;
        }
    }
    if make {
        c.spawn(HOVERBOT, a.x + 1, a.y);
    }
    a.frame = (a.frame + 1) % 4;
    c.draw(HOVERBOT_GENERATOR, 4, a.x, a.y, SpriteLook::Normal);
}

// ---------------------------------------------------------------------
// Messenger drone

/// The drones' messages: which picture frame to show and for how many frames
/// more, one row per drone type. (The original's rows are 50 bytes with a pair
/// of unused bytes at the start.)
const MESSAGES: [&[(u8, u8)]; 5] = [
    &[
        (0, 10),
        (1, 10),
        (2, 10),
        (3, 14),
        (0, 10),
        (1, 10),
        (2, 10),
        (3, 14),
    ],
    &[
        (0, 8),
        (1, 8),
        (2, 8),
        (3, 14),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 4),
        (9, 1),
        (8, 4),
        (9, 1),
    ],
    &[(0, 4), (1, 4), (2, 3), (3, 6), (4, 3), (5, 5), (6, 15)],
    &[(0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (5, 15)],
    &[(0, 8), (1, 8), (2, 8), (3, 8), (4, 8), (5, 8), (6, 8)],
];

/// The picture sprites of the five message rows.
const MESSAGE_PICTURES: [u16; 5] = [0xd5, 0xd6, 0xd7, 0xd8, 0xdc];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Heading {
    /// Not started: it picks its side on its first frame.
    Unset,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Talk {
    /// Flying, until the player is near.
    Flying,
    /// Showing pair `step` of the message; `wait` frames left of it (0:
    /// not started).
    Showing { step: usize, wait: u8 },
    /// Done: flies on until off screen.
    Done,
}

/// Types 213 to 216 and 220 fly at the player, stop within 6 tiles to show
/// their message, and fly on until they are off screen. They can be shot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessengerDrone {
    heading: Heading,
    talk: Talk,
    /// The picture sprite (the placed type): it says which message.
    picture: u16,
}

impl MessengerDrone {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            heading: Heading::Unset,
            talk: Talk::Flying,
            picture: vars[3],
        }
    }
}

pub(super) fn messenger_drone(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, MessengerDrone);
    let flap = u16::from(c.draw_page);
    if s.heading == Heading::Unset {
        s.heading = if a.x < c.player.x {
            Heading::Right
        } else {
            Heading::Left
        };
        // The original keeps the message's number in the score: shot, the
        // drone is worth that many points.
        a.score = match s.picture {
            0xd5 => 0,
            0xd6 => 1,
            0xd7 => 2,
            0xd8 => 3,
            0xdc => 4,
            _ => a.score,
        };
    }
    let (heading, picture) = (s.heading, s.picture);
    // It flies, but not while it shows its message.
    let flying = !matches!(s.talk, Talk::Showing { .. });
    let waiting = s.talk == Talk::Flying;
    match heading {
        Heading::Left => {
            if flying {
                a.x -= 2;
                c.draw(DRONE_JET_LEFT, flap, a.x, a.y, SpriteLook::Normal);
            }
            c.draw(DRONE_BODY_LEFT, 0, a.x, a.y, SpriteLook::Normal);
        }
        _ => {
            if flying {
                a.x += 2;
                c.draw(DRONE_JET_RIGHT, flap, a.x, a.y, SpriteLook::Normal);
            }
            c.draw(DRONE_BODY_RIGHT, 0, a.x, a.y, SpriteLook::Normal);
        }
    }
    c.draw(DRONE_DOME, 0, a.x, a.y, SpriteLook::Normal);
    c.draw(DRONE_HULL, 0, a.x, a.y, SpriteLook::Normal);
    if waiting && near_x(c, a, 6) {
        let s = state!(a, MessengerDrone);
        s.talk = Talk::Showing { step: 0, wait: 0 };
    }
    let s = state!(a, MessengerDrone);
    if let Talk::Showing { step, wait } = s.talk {
        c.draw(DRONE_SCREEN, flap, a.x, a.y, SpriteLook::Normal);
        let row = MESSAGE_PICTURES
            .iter()
            .position(|&p| p == picture)
            .unwrap_or(0);
        match MESSAGES[row].get(step) {
            None => s.talk = Talk::Done,
            Some(&(frame, duration)) => {
                s.talk = if wait == 0 {
                    Talk::Showing {
                        step,
                        wait: duration,
                    }
                } else if wait == 1 {
                    Talk::Showing {
                        step: step + 1,
                        wait: 0,
                    }
                } else {
                    Talk::Showing {
                        step,
                        wait: wait - 1,
                    }
                };
                c.draw(picture, u16::from(frame), a.x, a.y, SpriteLook::Normal);
            }
        }
    }
    let done = matches!(state!(a, MessengerDrone).talk, Talk::Done);
    if done && !c.on_screen(a.placed()) {
        c.remove(a);
    }
    // The drone is drawn from parts, so it takes its hits here, not in
    // the common code.
    if !a.removed {
        let damage = hits::damage(a, c);
        if damage > 0 {
            hits::react(a, c, damage);
        }
    }
    a.look = SpriteLook::Hidden;
}

/// Whether the horizontal distance between the middle of the actor's sprite and
/// the player is at most `dist`.
pub(super) fn near_x(c: &Ctx, a: &Actor, dist: i32) -> bool {
    let width = crate::map::Map::sprite_body(c.sprites, a.id, a.frame, 0, 0).map_or(0, |b| b.width);
    let player = c.player.x
        + if c.player.facing == Facing::Left {
            2
        } else {
            1
        };
    (a.x + (width >> 1) - player).abs() <= dist
}

// ---------------------------------------------------------------------
// Slime and water drips

/// Frames between two slime drops.
const DRIP_PERIOD: u16 = 0x19;

/// Type 117 lets a slime drop fall every 25 frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlimePipe {
    timer: u16,
}

pub(super) fn slime_pipe(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, SlimePipe);
    a.look = SpriteLook::OverForeground;
    s.timer = s.timer.wrapping_add(1);
    if s.timer % DRIP_PERIOD == 0 {
        c.spawn(SLIME_DROP, a.x + 1, a.y + 1);
        c.sound(Sound::Drip);
    }
    a.frame = u16::from(a.frame == 0);
}

/// Type 118 falls; once it landed it shows a splash frame for one frame and
/// goes.
pub(super) fn slime_drop(a: &mut Actor) {
    if a.fall == 0 {
        a.frame += 1;
        if a.frame == 2 {
            a.remove();
        }
    }
}

/// Type 226 falls and goes when it lands.
pub(super) fn water_drop(a: &mut Actor) {
    if a.fall == 0 {
        a.remove();
    }
}

/// Type 227 is invisible and, on every second frame, lets a drop fall now and
/// then.
pub(super) fn drip_spawner(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    if c.draw_page && c.random.next_byte() > 0xdc {
        c.spawn(WATER_DROP, a.x, a.y);
        if c.on_screen(a.placed()) {
            c.sound(Sound::Drip);
        }
    }
}

// ---------------------------------------------------------------------
// Force field

/// Type 119 is a flickering beam that kills the player on touch. When the
/// access card turns the force fields off, the first one to notice goes dark
/// for good.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForceField {
    /// Frames flickered, for the frame to show.
    flicker: u16,
    off: bool,
}

pub(super) fn force_field(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, ForceField);
    let at = |frame| Placed {
        id: FORCE_FIELD,
        frame,
        x: a.x,
        y: a.y,
    };
    if !c.on_screen(at(1)) && !c.on_screen(at(0)) {
        return;
    }
    c.draw(FORCE_FIELD, 1, a.x, a.y, SpriteLook::Normal);
    if s.off {
        return;
    }
    if c.state.force_field_off {
        c.state.force_field_off = false;
        s.off = true;
        return;
    }
    if c.touches_player(at(2)) {
        // Deadly whatever the health: the cloak does not help either.
        c.player.health = 1;
        c.player.hurt_timer = 0;
        c.player.cloak_timer = 0;
        c.hurt_player();
    }
    s.flicker = s.flicker.wrapping_add(1);
    let frame = s.flicker % 3 + 2;
    let look = if c.random.next_byte() & 0x20 == 0 {
        SpriteLook::Normal
    } else {
        c.sound(Sound::Spark);
        SpriteLook::WhiteFlash
    };
    c.draw(FORCE_FIELD, frame, a.x, a.y, look);
}

// ---------------------------------------------------------------------
// Card slot and key hole

/// Type 120 blinks while it waits for the card, then shows frame 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardSlot {
    /// Waiting for the card.
    pub ready: bool,
}

impl CardSlot {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            ready: vars[0] != 0,
        }
    }
}

pub(super) fn access_card_slot(a: &mut Actor) {
    let s = state!(a, AccessCardSlot);
    if s.ready {
        a.frame += 1;
        if a.frame != 4 {
            return;
        }
    }
    a.frame = 0;
}

/// Frames of a blinking key hole.
const BLINK: [u16; 8] = [0, 1, 2, 3, 4, 3, 2, 1];

/// Type 122 blinks while it waits for the key, then shows frame 5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyHole {
    /// Waiting for the key.
    pub ready: bool,
    step: usize,
}

impl KeyHole {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            ready: vars[0] != 0,
            step: 0,
        }
    }
}

pub(super) fn key_hole(a: &mut Actor) {
    let s = state!(a, KeyHole);
    if !s.ready {
        a.frame = 5;
    } else {
        s.step = (s.step + 1) % BLINK.len();
        a.frame = BLINK[s.step];
    }
}

// ---------------------------------------------------------------------
// Water

/// The type 0xdd actor tints the screen where it is: a block of 2x2 cells of
/// still water; or with a surface (types 233, 234) the top row is the wave,
/// which steps through four shapes; the wide one (234) is a block of 4x4 cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Water {
    surface: bool,
    wide: bool,
    wave: u8,
}

impl Water {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            surface: vars[0] != 0,
            wide: vars[1] != 0,
            wave: vars[0] as u8,
        }
    }
}

pub(super) fn water(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, Water);
    a.look = SpriteLook::Hidden;
    let (x, y) = (a.x, a.y);
    if !s.surface {
        c.draws.push(Draw::Water { wave: 0, x, y });
        return;
    }
    s.wave += 1;
    if s.wave == 5 {
        s.wave = 1;
    }
    let (wave, wide) = (s.wave, s.wide);
    c.draws.push(Draw::Water { wave, x, y });
    if wide {
        c.draws.push(Draw::Water { wave, x: x + 2, y });
        c.draws.push(Draw::Water {
            wave: 0,
            x,
            y: y + 2,
        });
        c.draws.push(Draw::Water {
            wave: 0,
            x: x + 2,
            y: y + 2,
        });
    }
}

// ---------------------------------------------------------------------
// Fire jet

/// The flames of a fire jet: for each frame of the jet a list of (picture, y
/// offset) pairs; `FLAMES_END` ends a frame, `JET_END` the last one.
const FIRE_JET_FLAMES: [i8; 72] = [
    3, 0, 127, 4, -3, 1, 1, 127, 5, -6, 2, -2, 0, 2, 127, 3, -8, 0, -4, 1, 0, 127, 4, -9, 1, -5, 2,
    -1, 0, 3, 127, 5, -10, 2, -6, 0, -2, 1, 2, 127, 3, -9, 0, -5, 1, -1, 2, 3, 127, 3, -8, 0, -4,
    1, 0, 127, 4, -6, 1, -2, 2, 2, 127, 5, -3, 2, 1, 127, 3, 0, 127, -127,
];
const FLAMES_END: i8 = 127;
const JET_END: i8 = -127;
/// Frames between two bursts.
const JET_PAUSE: u16 = 0xf;

/// Type 231 is invisible. Every 15 frames a flame rises from it, which hurts
/// the player where it touches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FireJetState {
    wait: u16,
    /// Position in `FIRE_JET_FLAMES`.
    step: usize,
}

pub(super) fn fire_jet(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, FireJet);
    a.look = SpriteLook::Hidden;
    if s.wait < JET_PAUSE {
        s.wait += 1;
        return;
    }
    while FIRE_JET_FLAMES[s.step] != FLAMES_END {
        let frame = FIRE_JET_FLAMES[s.step] as u16;
        let y = a.y + i32::from(FIRE_JET_FLAMES[s.step + 1]);
        let flame = Placed {
            id: FIRE_JET,
            frame,
            x: a.x,
            y,
        };
        c.draw(FIRE_JET, frame, a.x, y, SpriteLook::Normal);
        if c.touches_player(flame) {
            c.hurt_player();
        }
        if s.step < 5 {
            c.sound(Sound::FireJet);
        }
        s.step += 2;
    }
    s.step += 1;
    if FIRE_JET_FLAMES[s.step] == JET_END {
        s.step = 0;
        s.wait = 0;
        if !c.on_screen(a.placed()) {
            // Sleeps until it is in view again.
            a.flags.remove(ActorFlags::ALWAYS_UPDATE);
            a.flags.insert(ActorFlags::STAY_ACTIVE);
        }
    }
}

// ---------------------------------------------------------------------
// Radar counter and hint pedestal

/// Frames of the radar counter display while dishes are left, and when all are
/// gone; 5 shows the number.
const RADAR_COUNTING: [u16; 31] = [
    4, 4, 4, 0, 4, 4, 4, 0, 4, 4, 4, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6,
];
const RADAR_DONE: [u16; 30] = [
    6, 6, 6, 0, 6, 6, 6, 0, 6, 6, 6, 0, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 0,
];

/// Type 237 is a screen that shows the number of radar dishes left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RadarCounter {
    step: usize,
}

pub(super) fn radar_counter_display(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, RadarCounter);
    for frame in 1..4 {
        c.draw(RADAR_DISPLAY, frame, a.x, a.y, SpriteLook::Normal);
    }
    let dishes = c.state.radar_dishes;
    let (frame, number) = if dishes == 0 {
        (RADAR_DONE.get(s.step).copied().unwrap_or(0), false)
    } else {
        let f = RADAR_COUNTING.get(s.step).copied().unwrap_or(0);
        (f, f == 5)
    };
    a.frame = frame;
    if c.draw_page {
        s.step += 1;
    }
    if s.step == 0x1d {
        s.step = 0;
    }
    if number {
        a.look = SpriteLook::Hidden;
        c.draw(RADAR_DISPLAY, 5, a.x, a.y, SpriteLook::Normal);
        c.draw(
            RADAR_DISPLAY,
            u16::from(dishes) + 7,
            a.x - 1,
            a.y,
            SpriteLook::Normal,
        );
    }
}

/// Type 240 shows the crystal globe on top of it once it was brought back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintPedestal {
    pub globe_placed: bool,
}

pub(super) fn hint_globe_pedestal(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, HintPedestal);
    if s.globe_placed {
        c.draw(HINT_GLOBE, 0, a.x + 1, a.y - 4, SpriteLook::Normal);
    }
}

/// The crystal globe carried to the pedestal: the engine types the
/// level's hint (`state.hint_requested`), and other messages wait until it
/// is done.
pub(super) fn hint_pedestal_touch(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, HintPedestal);
    if s.globe_placed {
        return;
    }
    if let Some(i) = c.player.inventory.iter().position(|&i| i == HINT_GLOBE) {
        c.player.inventory.remove(i);
        c.add_score(50000);
        c.sound(Sound::Pickup);
        for up in 0..5 {
            c.effect(0x7f, a.x, a.y - up, Motion::Float, 0);
        }
        s.globe_placed = true;
        c.state.hint_requested = true;
        c.state.messages.blocked = true;
    } else {
        c.state
            .messages
            .show_once(once::HINT_MACHINE, text::HINT_MACHINE);
    }
}

// ---------------------------------------------------------------------
// Windblown spiders

/// Type 241 is invisible; while the player is above it, it now and then blows a
/// spider across the right edge of the view.
pub(super) fn windblown_spider_generator(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    let above = c.player.y < a.y;
    if above && c.random.next_byte() & 1 != 0 && c.draw_page {
        let kind = u16::from(c.random.next_byte() % 3) + 0xf1;
        let x = i32::from(c.camera.0) + 0x1f;
        let y = i32::from(c.camera.1) + i32::from(c.random.next_byte() & 0xf);
        let path = 6 - u16::from(c.random.next_byte() & 2);
        c.effect(kind, x, y, Motion::from_code(path), 0);
    }
}

// ---------------------------------------------------------------------
// Unicycle bot

/// Frames a unicycle bot waits, and spins its wheel before it races off.
const UNICYCLE_WAIT: u16 = 0xf;
const UNICYCLE_SPIN: u16 = 10;

/// Type 244 idles for 15 frames, picks a direction and a time (15 to 46
/// frames), spins its wheel for 10 frames kicking up dust, and races off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnicycleBot {
    idle: u16,
    /// 0 none yet, 1 left, 2 right.
    heading: u8,
    spin: u16,
    /// Frames it still drives.
    time: u16,
}

pub(super) fn unicycle_bot(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, UnicycleBot);
    if s.idle < UNICYCLE_WAIT {
        s.idle += 1;
        if s.idle == UNICYCLE_WAIT {
            s.heading = if a.x < c.player.x { 2 } else { 1 };
            s.time = u16::from(c.random.next_byte() & 0x1f) + 0xf;
            s.spin = 0;
        }
        a.frame = u16::from(c.random.next_byte() & 1) * 5;
        return;
    }
    s.time = s.time.saturating_sub(1);
    let odd = u16::from(c.draw_page);
    let (heading, mut spin) = (s.heading, s.spin);
    let (dir, base, dust_x, dust_path, step) = match heading {
        1 => (Dir::Left, 3, a.x + 1, 1, -1),
        2 => (Dir::Right, 1, a.x, 3, 1),
        _ => return,
    };
    a.frame = base + odd;
    if spin < UNICYCLE_SPIN {
        spin += 1;
        if c.draw_page {
            c.effect(DUST, dust_x, a.y, Motion::Debris(dust_path), 0);
        }
    } else {
        a.x += step;
    }
    let blocked = c.move_blocked(a, dir);
    let s = state!(a, UnicycleBot);
    s.spin = spin;
    if blocked || s.time == 0 {
        s.idle = 0;
    }
}

// ---------------------------------------------------------------------
// Random crawler

/// Type 97, a spiked orb. On every second frame it moves one tile in its
/// direction for a while; when blocked or when its time is up it turns at a
/// right angle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomCrawler {
    /// 0 up, 1 down, 2 left, 3 right.
    heading: u8,
    /// Frames it still goes this way.
    time: u16,
    /// The two clocks of its steps: moves on every second frame, and its
    /// animation flips on those.
    skip: bool,
    flip: bool,
}

impl RandomCrawler {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            heading: vars[0] as u8,
            time: vars[2],
            skip: false,
            flip: false,
        }
    }
}

pub(super) fn random_crawler(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, RandomCrawler);
    s.skip = !s.skip;
    if s.skip {
        return;
    }
    s.flip = !s.flip;
    s.time = s.time.saturating_sub(1);
    let flip = u16::from(s.flip);
    match s.heading {
        0 => {
            a.frame = flip << 1;
            if a.frame != 0 {
                a.y -= 1;
            }
        }
        1 => {
            a.frame = flip << 1;
            if a.frame == 0 {
                a.y += 1;
            }
        }
        2 => {
            a.frame = flip;
            if a.frame == 0 {
                a.x -= 1;
            }
        }
        _ => {
            a.frame = flip;
            if a.frame != 0 {
                a.x += 1;
            }
        }
    }
    // A bound only for safety: a crawler boxed in on all sides would
    // otherwise never leave the loop.
    for _ in 0..64 {
        let s = state!(a, RandomCrawler);
        let dir = match s.heading {
            0 => Dir::Up,
            1 => Dir::Down,
            2 => Dir::Left,
            _ => Dir::Right,
        };
        let heading = s.heading;
        let time = s.time;
        if !(c.move_blocked(a, dir) || time == 0) {
            break;
        }
        let turn = if heading < 2 {
            2 + (c.random.next_byte() & 1)
        } else {
            c.random.next_byte() & 1
        };
        let s = state!(a, RandomCrawler);
        s.heading = turn;
        s.time = u16::from(c.random.next_byte() & 0x1f) + 10;
    }
}

// ---------------------------------------------------------------------
// Airlock

/// Types 250 (the player is sucked out to the left) and 251 (to the right) are
/// invisible; once the wall tile 3 tiles to the side is gone the player is
/// blown out.
pub(super) fn airlock(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    let left = a.id == AIRLOCK_LEFT;
    let wall = if left { a.x - 3 } else { a.x + 3 };
    if c.map.cell(wall, a.y) != 0 {
        return;
    }
    c.remove(a);
    c.player.state = if left {
        PlayerState::AirlockLeft
    } else {
        PlayerState::AirlockRight
    };
    c.player.frame = 8;
}

// ---------------------------------------------------------------------
// Aggressive prisoner

/// The arm's frames as it reaches out and back.
const REACH: [u16; 5] = [1, 2, 3, 4, 0];
/// The prisoner's frame when shot, and the one at which its body is gone.
const PRISONER_SHOT: u16 = 5;
const PRISONER_GONE: u16 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrisonerPhase {
    /// Behind bars, waiting for the player to come close.
    Idle,
    /// The arm reaches out: it hurts.
    Reaching,
    /// Shot: falls apart.
    Dying,
}

/// Type 253 stands behind bars. When the player comes within reach it grabs
/// out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prisoner {
    pub phase: PrisonerPhase,
    /// The step of the reach.
    step: usize,
    /// Frames of the death animation.
    dying: u16,
}

impl Prisoner {
    pub(super) fn new(_vars: [u16; 5]) -> Self {
        Self {
            phase: PrisonerPhase::Idle,
            step: 0,
            dying: 0,
        }
    }
}

pub(super) fn aggressive_prisoner(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, Prisoner);
    if s.phase == PrisonerPhase::Dying {
        s.dying += 1;
        if s.dying & 2 != 0 {
            a.frame += 1;
        }
        if a.frame == PRISONER_GONE {
            c.remove(a);
        }
        return;
    }
    c.draw(PRISONER, 0, a.x, a.y, SpriteLook::Normal);
    a.look = SpriteLook::Hidden;
    let px = c.player.x;
    let near = a.x - 4 < px && px < a.x + 7;
    if near && s.phase == PrisonerPhase::Idle && c.random.next_byte() & 0x10 != 0 && c.draw_page {
        s.step = 0;
        s.phase = PrisonerPhase::Reaching;
    }
    if s.phase == PrisonerPhase::Reaching {
        a.look = SpriteLook::Normal;
        a.frame = REACH[s.step];
        if s.step == 4 {
            s.phase = PrisonerPhase::Idle;
        } else if c.draw_page {
            s.step += 1;
        }
    }
}

/// Shot, a prisoner dies at once and flies back.
pub(super) fn prisoner_hit(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, Prisoner);
    if s.phase == PrisonerPhase::Dying {
        return;
    }
    s.phase = PrisonerPhase::Dying;
    c.sound(Sound::Splat);
    match c.state.last_shot {
        Some(ShotDirection::Left) => {
            c.effect(PRISONER_BODY, a.x, a.y, Motion::Debris(3), 0);
        }
        Some(ShotDirection::Right) => {
            c.effect(PRISONER_BODY, a.x, a.y, Motion::Debris(1), 0);
        }
        _ => {}
    }
    a.frame = PRISONER_SHOT;
    c.particles(5, Spread::Both, a.x + 3, a.y);
    c.add_score(500);
}

/// Touching the arm that reaches out hurts.
pub(super) fn prisoner_touch(a: &mut Actor, c: &mut Ctx) {
    if state!(a, Prisoner).phase == PrisonerPhase::Reaching {
        c.hurt_player();
    }
}

// ---------------------------------------------------------------------
// Explosion spawner

/// Type 254 starts three explosions and goes.
pub(super) fn explosion_spawner(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    c.effect(EXPLOSION_START, a.x, a.y, Motion::Still, 0);
    c.effect(EXPLOSION_START, a.x - 1, a.y - 2, Motion::Still, 1);
    c.effect(EXPLOSION_START, a.x + 1, a.y - 3, Motion::Still, 2);
    c.remove(a);
}
