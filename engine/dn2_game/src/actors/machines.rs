//! Machines and level objects: doors, lifts, the respawn beacon, the
//! exit, key holes and card slots, force fields, water, map section
//! markers and the like. The simple objects are in [`super::objects`].

use super::objects;
use super::{Actor, Ctx, Kind, Update};
use crate::draw::{Placed, SpriteLook};
use crate::map::{Dir, Hit};
use crate::mechanics::MapSection;
use crate::messages::{once, text};
use crate::player::PlayerState;
use crate::state::{Checkpoint, LevelExit};
use crate::{Buttons, Sound};

/// The state of each update function of the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    VerticalDoor(VerticalDoor),
    HorizontalDoor(HorizontalDoor),
    RespawnBeacon(Beacon),
    TurboLift(Lift),
    LevelExit,
    SectionMarker(Marker),
    HoverbotGenerator(objects::HoverbotGenerator),
    MessengerDrone(objects::MessengerDrone),
    SlimePipe(objects::SlimePipe),
    SlimeDrop,
    ForceField(objects::ForceField),
    AccessCardSlot(objects::CardSlot),
    KeyHole(objects::KeyHole),
    Water(objects::Water),
    WaterDrop,
    DripSpawner,
    FireJet(objects::FireJetState),
    RadarCounter(objects::RadarCounter),
    HintPedestal(objects::HintPedestal),
    SpiderGenerator,
    UnicycleBot(objects::UnicycleBot),
    RandomCrawler(objects::RandomCrawler),
    Airlock,
    Prisoner(objects::Prisoner),
    ExplosionSpawner,
}

/// The starting state for `update` (from the spawn data's `vars`), if it
/// belongs to this family.
pub(super) fn new(update: Update, vars: [u16; 5]) -> Option<State> {
    Some(match update {
        Update::VerticalDoor => State::VerticalDoor(VerticalDoor::default()),
        Update::HorizontalDoor => State::HorizontalDoor(HorizontalDoor::default()),
        Update::RespawnBeacon => State::RespawnBeacon(Beacon::new(vars)),
        Update::TurboLift => State::TurboLift(Lift::new(vars)),
        Update::LevelExit => State::LevelExit,
        Update::MapSectionMarker => State::SectionMarker(Marker::new(vars)),
        Update::HoverbotGenerator => State::HoverbotGenerator(Default::default()),
        Update::MessengerDrone => State::MessengerDrone(objects::MessengerDrone::new(vars)),
        Update::SlimePipe => State::SlimePipe(Default::default()),
        Update::SlimeDrop => State::SlimeDrop,
        Update::ForceField => State::ForceField(Default::default()),
        Update::AccessCardSlot => State::AccessCardSlot(objects::CardSlot::new(vars)),
        Update::KeyHole => State::KeyHole(objects::KeyHole::new(vars)),
        Update::Water => State::Water(objects::Water::new(vars)),
        Update::WaterDrop => State::WaterDrop,
        Update::DripSpawner => State::DripSpawner,
        Update::FireJet => State::FireJet(Default::default()),
        Update::RadarCounterDisplay => State::RadarCounter(Default::default()),
        Update::HintGlobePedestal => State::HintPedestal(Default::default()),
        Update::WindblownSpiderGenerator => State::SpiderGenerator,
        Update::UnicycleBot => State::UnicycleBot(Default::default()),
        Update::RandomCrawler => State::RandomCrawler(objects::RandomCrawler::new(vars)),
        Update::Airlock => State::Airlock,
        Update::AggressivePrisoner => State::Prisoner(objects::Prisoner::new(vars)),
        Update::ExplosionSpawner => State::ExplosionSpawner,
        _ => return None,
    })
}

/// The actor's update function.
pub(super) fn update(a: &mut Actor, c: &mut Ctx) {
    let Kind::Machine(state) = &a.kind else {
        return;
    };
    match state {
        State::VerticalDoor(_) => vertical_door(a, c),
        State::HorizontalDoor(_) => horizontal_door(a, c),
        State::RespawnBeacon(_) => respawn_beacon(a, c),
        State::TurboLift(_) => turbo_lift(a, c),
        State::LevelExit => level_exit(a, c),
        State::SectionMarker(_) => section_marker(a, c),
        State::HoverbotGenerator(_) => objects::hoverbot_generator(a, c),
        State::MessengerDrone(_) => objects::messenger_drone(a, c),
        State::SlimePipe(_) => objects::slime_pipe(a, c),
        State::SlimeDrop => objects::slime_drop(a),
        State::ForceField(_) => objects::force_field(a, c),
        State::AccessCardSlot(_) => objects::access_card_slot(a),
        State::KeyHole(_) => objects::key_hole(a),
        State::Water(_) => objects::water(a, c),
        State::WaterDrop => objects::water_drop(a),
        State::DripSpawner => objects::drip_spawner(a, c),
        State::FireJet(_) => objects::fire_jet(a, c),
        State::RadarCounter(_) => objects::radar_counter_display(a, c),
        State::HintPedestal(_) => objects::hint_globe_pedestal(a, c),
        State::SpiderGenerator => objects::windblown_spider_generator(a, c),
        State::UnicycleBot(_) => objects::unicycle_bot(a, c),
        State::RandomCrawler(_) => objects::random_crawler(a, c),
        State::Airlock => objects::airlock(a, c),
        State::Prisoner(_) => objects::aggressive_prisoner(a, c),
        State::ExplosionSpawner => objects::explosion_spawner(a, c),
    }
}

/// The reaction to a hit for `damage`; false leaves it to the reactions by type
/// in [`super::hits`].
pub(super) fn hit(a: &mut Actor, c: &mut Ctx, _damage: u8) -> bool {
    match &a.kind {
        // Shot dead in one hit, and nothing else happens to it.
        Kind::Machine(State::Prisoner(_)) => {
            objects::prisoner_hit(a, c);
            true
        }
        _ => false,
    }
}

/// The reaction to touching the player; false leaves it to [`super::contact`].
pub(super) fn touch(a: &mut Actor, c: &mut Ctx) -> bool {
    let Kind::Machine(state) = &a.kind else {
        return false;
    };
    match state {
        State::AccessCardSlot(_) => card_slot_touch(a, c),
        State::KeyHole(_) => key_hole_touch(a, c),
        State::RespawnBeacon(_) => beacon_touch(a),
        State::HintPedestal(_) => objects::hint_pedestal_touch(a, c),
        State::Prisoner(_) => objects::prisoner_touch(a, c),
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------------
// Doors

/// Sprites: the vertical door's pieces and the horizontal hatch.
const VERTICAL_DOOR: u16 = 0x80;

/// Type 128 is a door 8 tiles tall standing on `x`, `y`. Its tiles are saved
/// and replaced by a solid tile; when the player is near, they open step by
/// step and are put back when he goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerticalDoor {
    /// The map cells the door covers, from the row under it up.
    saved: [u16; 8],
    /// The solid cell that stands in for the door; 0 before it is set up.
    solid: u16,
    /// How many tiles are open, 0 to 7.
    open: u16,
}

fn vertical_door(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, Machine(State::VerticalDoor));
    a.look = SpriteLook::Hidden;
    let (x, y) = (a.x, a.y);
    // The row of tile `u` of the door, counting up from the one below it.
    let row = |u: u16| y - i32::from(u) + 1;
    if s.solid == 0 {
        for u in 0..8 {
            s.saved[usize::from(u)] = c.map.cell(x, row(u));
        }
        s.solid = c.map.fully_solid_cell();
        for u in 1..8 {
            c.map.set_cell(x, row(u), s.solid);
        }
    }
    let solid = s.solid;
    let open = s.open;
    let (_, py) = c.player_at();
    let near = c.near_x(a, 7);
    let s = state!(a, Machine(State::VerticalDoor));
    let draw_from = |c: &mut Ctx, u: u16| {
        if u != 0 {
            c.draw(
                VERTICAL_DOOR,
                u - open,
                x,
                y - i32::from(u),
                SpriteLook::Normal,
            );
        }
    };
    if !near || y < py || py <= y - 7 {
        if open == 7 {
            c.sound(Sound::Door);
        }
        for u in open..9 {
            draw_from(c, u);
            if open == 1 && u != 0 {
                c.map.set_cell(x, row(u), solid);
            }
        }
        s.open = open.saturating_sub(1);
    } else {
        if open == 1 {
            c.sound(Sound::Door);
        }
        for u in open..9 {
            draw_from(c, u);
            if open == 1 && u < 8 {
                c.map.set_cell(x, row(u), s.saved[usize::from(u)]);
            }
        }
        if open < 7 {
            s.open += 1;
        } else {
            c.map.set_cell(x, y - 7, solid);
        }
    }
}

/// Type 132 is a hatch 5 tiles wide at `x`, `y`, done the same way. It opens
/// when the player is within 2 to the left, 5 to the right, 2 up and 6 down.
/// The frame is 0 closed, 1 half open, 2 open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HorizontalDoor {
    saved: [u16; 5],
    solid: u16,
}

/// Writes the hatch's five cells in its row, from `x` on.
fn hatch_row(c: &mut Ctx, x: i32, y: i32, cells: [u16; 5]) {
    for (n, cell) in cells.into_iter().enumerate() {
        c.map.set_cell(x + n as i32, y, cell);
    }
}

fn horizontal_door(a: &mut Actor, c: &mut Ctx) {
    let s = state!(a, Machine(State::HorizontalDoor));
    let (x, y) = (a.x, a.y);
    if s.solid == 0 {
        for n in 0..5 {
            s.saved[n] = c.map.cell(x + n as i32, y);
        }
        s.solid = c.map.fully_solid_cell();
        hatch_row(c, x, y, [s.solid; 5]);
    }
    let solid = s.solid;
    let (px, py) = c.player_at();
    let away = px < x - 2 || x + 6 <= px || py <= y - 3 || y + 7 <= py;
    if away {
        if a.frame != 0 {
            if a.frame == 2 {
                c.sound(Sound::Door);
            }
            a.frame -= 1;
            if a.frame == 0 {
                hatch_row(c, x, y, [solid; 5]);
            }
        }
    } else {
        if a.frame == 0 {
            hatch_row(c, x, y, s.saved);
            c.sound(Sound::Door);
        }
        if a.frame < 2 {
            a.frame += 1;
        }
        if a.frame == 2 {
            c.map.set_cell(x, y, solid);
            c.map.set_cell(x + 5, y, solid);
        }
    }
}

// ---------------------------------------------------------------------
// Respawn beacon

/// Type 133. Touched, it blinks for 14 frames, records the checkpoint at the
/// 10th and says "SECTOR SECURE". (A beacon in view when the level starts never
/// works: as in the original, it starts blinking on its own and the touch is
/// ignored.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beacon {
    /// Not seen yet: the first frame in view starts its idle blinking.
    fresh: bool,
    /// Plays its blinking frames (5 to 8): 0 not, 1 yes, 2 first time.
    blinking: u16,
    /// Counts down from 15 after a touch.
    timer: u16,
}

impl Beacon {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            fresh: vars[1] != 0,
            blinking: vars[0],
            timer: vars[2],
        }
    }
}

fn respawn_beacon(a: &mut Actor, c: &mut Ctx) {
    let on_screen = c.on_screen(a.placed());
    let s = state!(a, Machine(State::RespawnBeacon));
    if s.fresh {
        if on_screen {
            s.timer = 1;
            s.blinking = 2;
            a.frame = 5;
        }
        s.fresh = false;
    }
    if s.timer < 2 {
        if s.blinking != 0 {
            a.frame += 1;
            if a.frame == 9 {
                a.frame = 5;
            }
        }
        return;
    }
    s.timer -= 1;
    if s.timer & 1 != 0 {
        a.look = SpriteLook::WhiteFlash;
    }
    if s.timer == 10 {
        let p = &c.player;
        c.state.checkpoint = Some(Checkpoint {
            x: a.x,
            y: a.y,
            health: p.health,
            weapon: p.weapon,
            ammo: p.ammo,
            backdrop_alternate: c.state.backdrop_alternate,
        });
        c.state.messages.show(text::SECTOR_SECURE);
    }
    if s.timer == 1 {
        s.blinking = 1;
    }
}

/// Touching an unlit beacon lights it.
fn beacon_touch(a: &mut Actor) {
    let s = state!(a, Machine(State::RespawnBeacon));
    if s.timer == 0 && a.frame == 0 {
        s.timer = 0xf;
    }
}

// ---------------------------------------------------------------------
// Turbo lift

/// The lift's sprite: frame 5 is the platform, 0 to 3 the exhaust.
const LIFT: u16 = 0xd1;
/// The lift's tiles above the platform that it makes solid.
const LIFT_TILES: i32 = 2;

/// Type 209 is an elevator platform two tiles wide. While the player stands on
/// it he can press up or down to ride it. The two tiles above the platform are
/// solid tiles while it stands, and are swapped for the map's own while it
/// moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lift {
    /// Not set up yet.
    fresh: bool,
    /// The exhaust frame: 0 going down, 1 up, 3 standing still.
    exhaust: u16,
    /// Falling by itself (nobody on it and nothing below).
    falling: bool,
    solid: u16,
    saved: [u16; 2],
}

impl Lift {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            fresh: vars[4] != 0,
            exhaust: vars[0],
            falling: vars[3] != 0,
            solid: 0,
            saved: [0; 2],
        }
    }
}

/// Saves the two map tiles above the platform at `y`.
fn lift_remember(c: &Ctx, s: &mut Lift, x: i32, y: i32) {
    for n in 0..LIFT_TILES {
        s.saved[n as usize] = c.map.cell(x + n + 1, y - 2);
    }
}

/// Puts the saved tiles back.
fn lift_restore(c: &mut Ctx, s: &Lift, x: i32, y: i32) {
    for n in 0..LIFT_TILES {
        c.map.set_cell(x + n + 1, y - 2, s.saved[n as usize]);
    }
}

/// Fills the two tiles above the lift with the solid tile.
fn lift_close(c: &mut Ctx, s: &Lift, x: i32, y: i32) {
    for n in 0..LIFT_TILES {
        c.map.set_cell(x + n + 1, y - 2, s.solid);
    }
}

fn lift_floor(c: &Ctx, x: i32, y: i32) -> bool {
    c.collide(LIFT, 0, x, y + 1, Dir::Down) != Hit::Free
}

fn lift_ceiling(c: &Ctx, x: i32, y: i32) -> bool {
    c.collide(LIFT, 0, x, y - 6, Dir::Up) != crate::map::Hit::Free
}

fn turbo_lift(a: &mut Actor, c: &mut Ctx) {
    if matches!(
        c.player.state,
        PlayerState::Dying | PlayerState::AirlockLeft | PlayerState::AirlockRight
    ) {
        return;
    }
    let Kind::Machine(State::TurboLift(mut s)) =
        core::mem::replace(&mut a.kind, Kind::Machine(State::LevelExit))
    else {
        return;
    };
    lift_step(a, c, &mut s);
    a.kind = Kind::Machine(State::TurboLift(s));
}

fn lift_step(a: &mut Actor, c: &mut Ctx, s: &mut Lift) {
    let x = a.x;
    let draw_platform = |c: &mut Ctx, y: i32| c.draw(LIFT, 5, x, y, SpriteLook::Normal);
    if s.fresh {
        s.fresh = false;
        s.solid = c.map.fully_solid_cell();
        lift_remember(c, s, x, a.y);
    }
    if s.falling {
        for _ in 0..2 {
            if lift_floor(c, x, a.y) {
                s.falling = false;
            } else {
                a.y += 1;
            }
        }
        if !s.falling {
            lift_remember(c, s, x, a.y);
        }
        draw_platform(c, a.y);
        return;
    }
    let y = a.y;
    let (px, py) = c.player_at();
    let facing_right = c.player.facing == crate::player::Facing::Right;
    let on_it = y - 3 == py
        && ((facing_right && x <= px && px < x + 2) || (!facing_right && x - 1 <= px && px <= x));
    if on_it {
        c.player.on_lift = true;
        c.state
            .messages
            .show_once(once::TURBO_LIFT, text::TURBO_LIFT);
        let input = c.player.input;
        if !input.contains(Buttons::UP) || lift_ceiling(c, x, y) {
            if !input.contains(Buttons::DOWN) || lift_floor(c, x, y) {
                // Stands still.
                if lift_floor(c, x, y) {
                    c.player.on_lift = false;
                }
                if input.contains(Buttons::JUMP) {
                    c.player.on_lift = false;
                } else {
                    c.player.state = PlayerState::Standing;
                }
                s.exhaust = 3;
                lift_close(c, s, x, y);
            } else {
                // Down, two tiles a frame.
                s.exhaust = 0;
                lift_restore(c, s, x, y);
                for _ in 0..2 {
                    if !lift_floor(c, x, a.y) {
                        a.y += 1;
                        c.player.y = c.player.y.wrapping_add(1);
                        c.player.state = PlayerState::Lift;
                    }
                }
                lift_remember(c, s, x, a.y);
            }
        } else {
            // Up.
            s.exhaust = 1;
            if c.draw_page {
                c.sound(Sound::Roar);
            }
            lift_restore(c, s, x, y);
            for _ in 0..2 {
                if !lift_ceiling(c, x, a.y) {
                    a.y -= 1;
                    c.player.y = c.player.y.wrapping_sub(1);
                    c.player.state = PlayerState::Lift;
                }
            }
            lift_remember(c, s, x, a.y);
        }
    } else {
        c.player.on_lift = false;
        if !lift_floor(c, x, y) {
            // Nothing below: it falls, and the tiles above it go back.
            lift_restore(c, s, x, y);
            s.falling = true;
            draw_platform(c, y);
            return;
        }
        lift_close(c, s, x, y);
    }
    if s.exhaust != 0 && !lift_floor(c, x, a.y) {
        c.draw(
            LIFT,
            s.exhaust + u16::from(c.draw_page),
            x,
            a.y,
            SpriteLook::Normal,
        );
    }
    draw_platform(c, a.y);
}

// ---------------------------------------------------------------------
// Exit

/// Type 139 is invisible; when the player stands at or below it, within two
/// tiles to its left, the level ends, unless radar dishes are left.
fn level_exit(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    let (px, py) = c.player_at();
    if py <= a.y && px <= a.x && a.x - 2 <= px {
        if c.state.radar_dishes == 0 {
            c.state.exit = Some(LevelExit::Finished);
        } else {
            c.state
                .messages
                .show_once(once::EXIT_RADAR, text::EXIT_RADAR);
        }
    }
}

// ---------------------------------------------------------------------
// Card slot and key hole

/// A slot that is ready takes `item` from the player on up. True if it did (the
/// door step follows).
fn use_item(c: &mut Ctx, ready: &mut bool, item: u16) -> bool {
    if !*ready {
        c.player.item_slot_active = false;
        return false;
    }
    c.player.item_slot_active = true;
    if !c.player.input.contains(Buttons::UP) {
        return false;
    }
    let Some(i) = c.player.inventory.iter().position(|&i| i == item) else {
        return false;
    };
    c.player.inventory.remove(i);
    c.player.door_step = 1;
    *ready = false;
    true
}

/// A one-shot message that a successful slot uses up without showing it.
fn use_up(c: &mut Ctx, id: u8) {
    c.state.messages.shown |= 1 << id;
}

/// Items of the card slot and the key hole, and where the player stands:
/// two tiles under the slot's row.
const ACCESS_CARD: u16 = 0x25;
const KEY: u16 = 0x79;

/// Type 120: the access card turns the force fields off.
fn card_slot_touch(a: &mut Actor, c: &mut Ctx) {
    let (_, py) = c.player_at();
    if py - 2 != a.y {
        return;
    }
    let s = state!(a, Machine(State::AccessCardSlot));
    if use_item(c, &mut s.ready, ACCESS_CARD) {
        c.state.force_field_off = true;
    }
    c.state.messages.show_once(once::USE_CARD, text::USE_CARD);
    if !c.player.input.contains(Buttons::UP) {
        return;
    }
    if s.ready {
        c.state
            .messages
            .show_once(once::ACCESS_DENIED, text::ACCESS_DENIED);
        c.player.item_slot_active = false;
        return;
    }
    if !c.state.force_field_off {
        return;
    }
    use_up(c, once::ACCESS_DENIED);
    c.state.messages.show(text::ACCESS_GRANTED);
}

/// Type 122: the key opens the door of the next section.
fn key_hole_touch(a: &mut Actor, c: &mut Ctx) {
    let (_, py) = c.player_at();
    if py - 2 != a.y {
        return;
    }
    let s = state!(a, Machine(State::KeyHole));
    c.state.messages.show_once(once::USE_KEY, text::USE_KEY);
    if use_item(c, &mut s.ready, KEY) {
        c.state.key_used = true;
    }
    if !c.player.input.contains(Buttons::UP) {
        return;
    }
    if s.ready {
        c.state.messages.show_once(once::NEED_KEY, text::NEED_KEY);
        c.player.item_slot_active = false;
        return;
    }
    if !c.state.key_used {
        return;
    }
    c.state.messages.show(text::OPENING_DOOR);
    use_up(c, once::NEED_KEY);
}

// ---------------------------------------------------------------------
// Map section markers

/// Placed types of the corner markers: the right and the bottom edge.
const MARKER_RIGHT: u16 = 0x67;
const MARKER_BOTTOM: u16 = 0x68;

/// What starts a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// At once (after the delay).
    Always,
    /// Once a key or card was used on its slot.
    ItemUsed,
    /// During the level's earthquake.
    Earthquake,
}

/// Once its condition holds the marker records the map rectangle between itself
/// and the two corner markers that give the right and bottom edge, then removes
/// itself. Its second variable says what the section does (see
/// [`crate::mechanics`]) and when it starts, the first is a delay in frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    delay: u16,
    /// 0 sinks into the floor; else the section's kind minus 0x62.
    how: u16,
}

impl Marker {
    fn new(vars: [u16; 5]) -> Self {
        Self {
            delay: vars[0],
            how: vars[1],
        }
    }

    fn trigger(&self) -> Trigger {
        match self.how {
            2 => Trigger::ItemUsed,
            3 | 5 => Trigger::Earthquake,
            _ => Trigger::Always,
        }
    }
}

/// The first entry of the level's actor list from `start` with this type
/// and position (`None` matches any).
fn find_marker(c: &Ctx, start: usize, kind: u16, x: Option<i32>, y: Option<i32>) -> Option<usize> {
    (start..c.map.level.actors.len()).find(|&i| {
        let e = c.map.level.actors[i];
        e.kind == kind
            && x.is_none_or(|x| i32::from(e.x) == x)
            && y.is_none_or(|y| i32::from(e.y) == y)
    })
}

fn section_marker(a: &mut Actor, c: &mut Ctx) {
    a.look = SpriteLook::Hidden;
    let here = a.placed();
    let s = state!(a, Machine(State::SectionMarker));
    if c.state.key_used {
        // Only once the marker is in view (5 tiles lower).
        let at = Placed {
            y: here.y + 5,
            ..here
        };
        if !c.on_screen(at) {
            return;
        }
    }
    match s.trigger() {
        Trigger::ItemUsed => {
            if !c.state.key_used {
                return;
            }
            c.state.key_used = false;
        }
        Trigger::Earthquake => {
            if !c.state.earthquake_going(c.map.level.flags) {
                return;
            }
        }
        Trigger::Always => {}
    }
    if s.delay != 0 {
        s.delay -= 1;
        if s.delay == 0 {
            c.sound(Sound::SectionStart);
        }
        return;
    }
    let how = s.how;
    let (x, y) = (a.x, a.y);
    // The placed marker, then the right and bottom ones after it.
    let found = find_marker(c, 0, a.id, Some(x), Some(y)).and_then(|own| {
        let right = find_marker(c, own, MARKER_RIGHT, None, Some(y))?;
        let right_x = i32::from(c.map.level.actors[right].x);
        let bottom = find_marker(c, right, MARKER_BOTTOM, Some(right_x), None)?;
        Some((right_x, i32::from(c.map.level.actors[bottom].y)))
    });
    if let Some((right, bottom)) = found {
        c.state.sections.push(MapSection {
            left: x,
            top: y,
            right,
            bottom,
            kind: if how != 0 { how + 0x62 } else { 0 },
        });
    }
    c.remove(a);
}
