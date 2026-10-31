//! Walking and flying creatures and robots: soldiers, walkers, bots,
//! skeletons, flyers (the original's walker, bot and flyer update functions).

use core::mem;

use dn2_assets::tileset::TileAttributes;

use super::{Actor, ActorFlags, Ctx, Kind, Update, contact, hits};
use crate::draw::SpriteLook;
use crate::effects::{BLAST, Motion};
use crate::map::{Dir, Hit};
use crate::particles::Spread;
use crate::player::PlayerState;
use crate::{Buttons, Sound};

/// Sprites of the family (they are also the types).
const HOVERBOT: u16 = 0;
const FAST_WALKER_LEFT: u16 = 31;
const FAST_WALKER_RIGHT: u16 = 32;
const SLIME_CRAWL: u16 = 67;
const SLIME_STRETCHED: u16 = 70;
const GREEN_LEFT: u16 = 189;
const GREEN_RIGHT: u16 = 190;
/// The enemy laser bolt: type 135 flies right, 136 (also the sprite) left.
const LASER_RIGHT: u16 = 135;
const LASER_LEFT: u16 = 136;
const LASER_FLASH_RIGHT: u16 = 0x94;
const LASER_FLASH_LEFT: u16 = 0x93;
/// The gunner's shot (an effect that hurts the player).
const GUNNER_SHOT: u16 = 300;
/// The flash of a hoverbot beaming in.
const BEAM_FLASH: u16 = 0x45;
const SHELL_DUST_LEFT: u16 = 4;
const SHELL_DUST_RIGHT: u16 = 0xbf;
const SHELL_BITS_LEFT: u16 = 0xc0;
const SHELL_BITS_RIGHT: u16 = 0xc4;
/// The steam of a turkey that was shot.
const STEAM: u16 = 0x54;

/// Walk cycle of the fast walker, the patrol walker, the ceiling dropper and
/// the flyers.
const WALK_CYCLE: [u16; 4] = [0, 1, 2, 1];

/// The state of each update function of the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Hoverbot(Hoverbot),
    Watchbot(Watchbot),
    SlimeBlob(SlimeBlob),
    Skeleton(Skeleton),
    Soldier(Soldier),
    SpikedGreenCreature(SpikedGreen),
    FastWalker(FastWalker),
    Turkey(Turkey),
    PatrolWalker(PatrolWalker),
    CeilingDropper(CeilingDropper),
    EnemyLaserShot(LaserShot),
    HomingDrone(HomingDrone),
    TerrainFollowingFlyer(TerrainFlyer),
    JumpingGunner(JumpingGunner),
}

/// The starting state for `update` (from the spawn data's `vars`), if it
/// belongs to this family.
pub(super) fn new(update: Update, vars: [u16; 5]) -> Option<State> {
    Some(match update {
        Update::Hoverbot => State::Hoverbot(Hoverbot {
            right: vars[0] != 0,
            beam: vars[1] as u8,
            turn: Turn::None,
            head: vars[3] as u8,
            delay: vars[4] as u8,
        }),
        Update::Watchbot => State::Watchbot(Watchbot {
            right: vars[0] != 0,
            hop: vars[1] as u8,
            idle: vars[2] as u8,
            blink: vars[3] as u8,
            blink_kind: vars[4] as u8,
        }),
        Update::SlimeBlob => State::SlimeBlob(SlimeBlob {
            count: vars[0] as u8,
            side: vars[1] as u8,
            step: vars[2] as u8,
        }),
        Update::Skeleton => State::Skeleton(Skeleton {
            right: vars[0] != 0,
            started: vars[4] != 0,
        }),
        Update::Soldier => State::Soldier(Soldier {
            left: vars[0] != 0,
            walked: vars[1] as u8,
            aim_delay: vars[2] as u8,
            sleeping: vars[4] > 1,
        }),
        Update::SpikedGreenCreature => State::SpikedGreenCreature(SpikedGreen {
            phase: vars[0] as u8,
            hop: vars[1] as u8,
        }),
        Update::FastWalker => State::FastWalker(FastWalker {
            pause: vars[0] as u8,
            step: vars[1] as u8,
        }),
        Update::Turkey => State::Turkey(Turkey {
            started: vars[0] != 0,
            walk: match vars[1] {
                0 => TurkeyWalk::Left,
                1 => TurkeyWalk::Right,
                _ => TurkeyWalk::Cooked,
            },
            steam: vars[2] as u8,
        }),
        // The spawn data's first variable (200) is overwritten when it
        // starts.
        Update::PatrolWalker => State::PatrolWalker(PatrolWalker {
            right: false,
            steps: 0,
            started: false,
        }),
        Update::CeilingDropper => State::CeilingDropper(CeilingDropper {
            mode: DropperMode::Left,
            step: vars[1] as u8,
            phase: vars[2] as u8,
            top: i32::from(vars[3]),
        }),
        Update::EnemyLaserShot => State::EnemyLaserShot(LaserShot {
            right: vars[0] == LASER_RIGHT,
            flashed: vars[1] != 0,
        }),
        Update::HomingDrone => State::HomingDrone(HomingDrone {
            unfolded: vars[0] as u8,
            flip: vars[1] != 0,
        }),
        Update::TerrainFollowingFlyer => State::TerrainFollowingFlyer(TerrainFlyer {
            height: vars[0] as u8,
            step: vars[1] as u8,
        }),
        Update::JumpingGunner => State::JumpingGunner(JumpingGunner {
            right: vars[0] != 0,
            rising: vars[1] == 1,
            rest: vars[2] as u8,
            reach: vars[3] as i8,
            jump_step: vars[4] as u8,
        }),
        _ => return None,
    })
}

/// The actor's update function.
pub(super) fn update(a: &mut Actor, c: &mut Ctx) {
    if !matches!(a.kind, Kind::Creature(_)) {
        return;
    }
    // The state is taken out so the function can use the whole actor.
    let Kind::Creature(mut state) = mem::replace(&mut a.kind, Kind::SecurityCamera) else {
        return;
    };
    match &mut state {
        State::Hoverbot(s) => hoverbot(a, c, s),
        State::Watchbot(s) => watchbot(a, c, s),
        State::SlimeBlob(s) => slime_blob(a, c, s),
        State::Skeleton(s) => skeleton(a, c, s),
        State::Soldier(s) => soldier(a, c, s),
        State::SpikedGreenCreature(s) => spiked_green(a, c, s),
        State::FastWalker(s) => fast_walker(a, c, s),
        State::Turkey(s) => turkey(a, c, s),
        State::PatrolWalker(s) => patrol_walker(a, c, s),
        State::CeilingDropper(s) => ceiling_dropper(a, c, s),
        State::EnemyLaserShot(s) => enemy_laser_shot(a, c, s),
        State::HomingDrone(s) => homing_drone(a, c, s),
        State::TerrainFollowingFlyer(s) => terrain_flyer(a, c, s),
        State::JumpingGunner(s) => jumping_gunner(a, c, s),
    }
    a.kind = Kind::Creature(state);
}

/// The reaction to a hit for `damage`; false leaves it to the reactions by type
/// in [`super::hits`].
pub(super) fn hit(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    let (x, y) = (a.x, a.y);
    match &mut a.kind {
        // The soldier turns to face the player when shot (and wakes).
        Kind::Creature(State::Soldier(s)) => {
            if s.sleeping {
                s.sleeping = false;
                s.left = c.player.x < x;
            }
            if !hits::hurt(a, c, damage) {
                return true;
            }
            c.particles(0x0b, Spread::Both, x + 1, y);
            c.explosion_sound();
            c.cloud(a.id, x, y, BLAST);
            c.remove(a);
            true
        }
        // The turkey is cooked: it takes no more shots and gives more
        // health.
        Kind::Creature(State::Turkey(s)) => {
            c.sound(Sound::Splat);
            if s.walk != TurkeyWalk::Cooked {
                c.effect(STEAM, x, y, Motion::Still, 0);
            }
            s.walk = TurkeyWalk::Cooked;
            a.health = 0;
            true
        }
        _ => false,
    }
}

/// The reaction to touching the player; false leaves it to [`super::contact`].
pub(super) fn touch(a: &mut Actor, c: &mut Ctx) -> bool {
    // A laser bolt hurts and is gone.
    if let Kind::Creature(State::EnemyLaserShot(_)) = &a.kind {
        c.remove(a);
        c.hurt_player();
        return true;
    }
    let Kind::Creature(State::Turkey(s)) = &a.kind else {
        return false;
    };
    // Eaten, raw or cooked.
    let cooked = s.walk == TurkeyWalk::Cooked;
    c.sound(Sound::Food);
    contact::heal(c, 1);
    if cooked {
        contact::heal(c, 1);
    }
    contact::sparkle(c, contact::SPARKLE_100, a.x, a.y);
    c.remove(a);
    true
}

/// Steps one tile sideways; true if that was blocked (the actor is back
/// where it was).
fn step_side(a: &mut Actor, c: &Ctx, right: bool) -> bool {
    a.x += if right { 1 } else { -1 };
    c.move_blocked(a, if right { Dir::Right } else { Dir::Left })
}

// ---------------------------------------------------------------------
// Hoverbot

/// How the hoverbot's head is turning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    None,
    /// Looking back from right to left: the head frame counts down.
    ToLeft,
    /// Looking back from left to right: the head frame counts up.
    ToRight,
}

/// Beams in (flashing, hidden for 8 frames), then patrols towards the player.
/// When the player is behind it, it turns its head and then turns around.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hoverbot {
    pub right: bool,
    /// Counts 9 down to 1 while beaming in.
    beam: u8,
    turn: Turn,
    /// Head frame 0 to 5, drawn over the body.
    head: u8,
    /// Frames after the beam in in which only the head shows.
    delay: u8,
}

/// Draws the hoverbot's head (6 to 11) over its body.
fn draw_head(a: &Actor, c: &mut Ctx, head: u8) {
    c.draw_at(HOVERBOT, u16::from(head) + 6, a);
}

fn hoverbot(a: &mut Actor, c: &mut Ctx, s: &mut Hoverbot) {
    if s.delay == 0 {
        if s.beam < 10 && s.beam > 1 {
            if s.beam == 8 {
                c.effect(BEAM_FLASH, a.x, a.y, Motion::Still, 0);
            }
            s.beam -= 1;
            if s.beam == 1 {
                s.delay = 10;
            }
            a.look = SpriteLook::Hidden;
            return;
        }
    } else {
        s.delay -= 1;
        draw_head(a, c, s.head);
    }
    a.frame += 1;
    if a.frame == 6 {
        a.frame = 0;
    }
    if s.delay != 0 {
        return;
    }
    if s.turn == Turn::None {
        if !s.right {
            step_side(a, c, false);
            if a.x < c.player.x {
                s.turn = Turn::ToRight;
                s.head = 0;
            }
        } else {
            step_side(a, c, true);
            if c.player.x < a.x {
                s.turn = Turn::ToLeft;
                s.head = 5;
            }
        }
    }
    if s.turn == Turn::ToLeft {
        if c.draw_page {
            s.head = s.head.saturating_sub(1);
        }
        if s.head == 0 {
            s.turn = Turn::None;
            s.right = false;
        }
    }
    if s.turn == Turn::ToRight {
        if c.draw_page {
            s.head += 1;
        }
        if s.head == 5 {
            s.turn = Turn::None;
            s.right = true;
        }
    }
    draw_head(a, c, s.head);
}

// ---------------------------------------------------------------------
// Watchbot

/// Frames of the watchbot's idle cycle.
const WATCHBOT_IDLE: [u16; 4] = [1, 2, 1, 0];
/// Frames of its two blinking animations: 32 each.
const WATCHBOT_BLINK: [u16; 64] = [
    1, 1, 1, 3, 3, 1, 6, 6, 7, 8, 7, 6, 6, 6, 7, 8, 7, 6, 6, 6, 1, 1, 3, 3, 3, 1, 1, 1, 6, 6, 1, 1,
    1, 1, 6, 6, 7, 8, 7, 6, 6, 1, 1, 3, 3, 1, 6, 6, 1, 1, 1, 3, 4, 5, 4, 3, 3, 3, 4, 5, 4, 3, 1, 1,
];

/// Hops towards the player. Now and then it stops to blink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watchbot {
    pub right: bool,
    /// The step of the hop: 0 none, 1 to 5 rising, 6 landing.
    hop: u8,
    /// The step of the idle cycle between hops.
    idle: u8,
    /// The frame of a blinking animation, 1 to 32; 0 when not blinking.
    blink: u8,
    /// Which of the two animations.
    blink_kind: u8,
}

fn watchbot(a: &mut Actor, c: &mut Ctx, s: &mut Watchbot) {
    if s.blink != 0 {
        let row = usize::from(s.blink_kind) * 32 + usize::from(s.blink) - 1;
        a.frame = WATCHBOT_BLINK[row];
        if c.draw_page {
            s.blink += 1;
        }
        if s.blink == 0x21 {
            s.blink = 0;
        }
        return;
    }
    if s.hop != 0 && a.frame == 0 {
        step_side(a, c, s.right);
    }
    if s.hop != 0 {
        if s.hop < 6 {
            a.y -= 1;
            if c.move_blocked(a, Dir::Up) {
                s.hop = 5;
            }
            if s.hop < 3 {
                a.y -= 1;
                if c.move_blocked(a, Dir::Up) {
                    s.hop = 5;
                }
            }
            s.hop += 1;
            if s.hop > 5 {
                a.flags.insert(ActorFlags::GRAVITY);
                a.fall = 0;
                return;
            }
        }
        if a.fall != 0 || s.hop != 6 {
            return;
        }
        s.hop = 0;
        a.flags.remove(ActorFlags::GRAVITY);
        s.idle = 0;
        c.sound_if_visible(a, Sound::Jump);
    }
    a.frame = WATCHBOT_IDLE[usize::from(s.idle & 3)];
    s.idle += 1;
    let r = c.random.next_byte();
    if r & 0x21 != 0 && s.idle == 2 && !c.move_blocked(a, Dir::Down) {
        s.blink = 1;
        s.blink_kind = c.random.next_byte() & 1;
    } else if s.idle == 4 {
        s.hop += 1;
        s.right = c.player.x >= a.x;
    }
}

// ---------------------------------------------------------------------
// Slime blob

/// The blob (sprite 67) crawls towards the player and now and then stretches up
/// (sprite 70) for a leap, then comes down again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlimeBlob {
    /// Crawling: frames counted up to 10, before it moves. Stretched: 100
    /// once the leap is over and it comes down.
    count: u8,
    /// The first frame of the direction: 0 left, 5 right (stretched: a
    /// flip between 0 and 1).
    side: u8,
    /// Flips the two frames of the crawl.
    step: u8,
}

fn slime_blob(a: &mut Actor, c: &mut Ctx, s: &mut SlimeBlob) {
    let px = c.player.x;
    if a.id == SLIME_STRETCHED {
        if s.count != 100 {
            if a.frame == 3 {
                a.y -= 1;
                if c.clear(a, Dir::Up) {
                    return;
                }
            }
            if a.frame < 6 {
                a.frame += 1;
                return;
            }
            if a.x != px {
                s.side = u8::from(s.side == 0);
                a.frame = u16::from(s.side) + if a.x > px { 7 } else { 9 };
                if s.side & 1 == 0 {
                    return;
                }
                if a.x > px {
                    a.x -= 1;
                    if c.clear(a, Dir::Left)
                        && c.collide(a.id, a.frame, a.x - 4, a.y - 1, Dir::Up) != Hit::Free
                    {
                        return;
                    }
                } else {
                    a.x += 1;
                    if c.clear(a, Dir::Right)
                        && c.collide(a.id, a.frame, a.x + 4, a.y - 1, Dir::Up) != Hit::Free
                    {
                        return;
                    }
                }
            }
            s.count = 100;
            return;
        }
        if a.frame == 4 {
            a.y += 1;
        }
        if a.frame == 3 {
            a.y += 1;
            if c.collide(a.id, a.frame, a.x, a.y + 3, Dir::Down) == Hit::Free {
                return;
            }
        }
        a.frame = a.frame.wrapping_sub(1);
        if a.frame != 0 {
            return;
        }
        a.id = SLIME_CRAWL;
        s.count = 0;
    } else {
        if s.count >= 10 {
            s.step = s.step.wrapping_add(1);
            a.frame = u16::from(s.side) + u16::from(s.step & 1) + 3;
            if (a.x > px && s.side != 0) || (a.x < px && s.side == 0) {
                s.count = 0;
                return;
            }
            if a.frame & 1 != 0 {
                return;
            }
            if step_side(a, c, s.side != 0) {
                s.count = 0;
            }
            return;
        }
        s.count += 1;
        if c.random.next_byte() & 0x1f == 0 {
            a.id = SLIME_STRETCHED;
            a.frame = 0;
        } else {
            a.frame = u16::from(s.side) + u16::from(c.random.next_byte() & 3);
            if s.count != 10 {
                return;
            }
            if a.x <= px {
                s.side = 5;
                return;
            }
        }
    }
    s.side = 0;
}

// ---------------------------------------------------------------------
// Skeleton

/// Walks towards where the player was at the start and turns at walls and
/// ledges, on every second frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skeleton {
    pub right: bool,
    started: bool,
}

fn skeleton(a: &mut Actor, c: &mut Ctx, s: &mut Skeleton) {
    if !s.started {
        s.started = true;
        if c.player.x < a.x {
            s.right = false;
            a.frame = 0;
        } else {
            s.right = true;
            a.frame = 4;
        }
    }
    if c.draw_page {
        return;
    }
    if !s.right {
        if step_side(a, c, false) {
            s.right = true;
            a.frame = 4;
            return;
        }
        a.frame += 1;
        if a.frame != 4 {
            return;
        }
    } else {
        if !step_side(a, c, true) {
            a.frame += 1;
            if a.frame == 8 {
                a.frame = 4;
            }
            return;
        }
        s.right = false;
    }
    a.frame = 0;
}

// ---------------------------------------------------------------------
// Soldier

/// The soldier (types 159, 171 and 217, all sprite 159): walks up to 20
/// steps one way, turns, and shoots at the player when it sees him. Type 217
/// sleeps until the player is within 6 tiles on its row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Soldier {
    pub left: bool,
    /// Steps walked in this direction.
    walked: u8,
    /// A delay between two aims.
    aim_delay: u8,
    /// Waits for the player (see above).
    pub sleeping: bool,
}

/// Whether the player is in sight: in front of the soldier, within two rows,
/// standing on the ground and not cloaked. Then the soldier aims (standing, or
/// crouching when the player ducks or is lower) and fires one frame in eight.
fn soldier_shoots(a: &mut Actor, c: &mut Ctx, s: &mut Soldier) -> bool {
    let (px, py) = (c.player.x, c.player.y);
    if c.player.cloak_timer != 0
        || (a.frame < 6 && px < a.x)
        || (a.frame > 5 && a.x < px)
        || a.y + 3 <= py
        || py <= a.y - 3
        || c.player.state != PlayerState::Standing
    {
        return false;
    }
    if s.aim_delay == 0 {
        if !c.input.contains(Buttons::DOWN) && py <= a.y {
            a.frame = if s.left { 10 } else { 4 };
        } else {
            a.frame = if s.left { 0xb } else { 5 };
            s.aim_delay = c.random.next_byte() & 0xf;
        }
    } else {
        s.aim_delay -= 1;
    }
    if c.random.next_byte() & 7 != 0 {
        return true;
    }
    let (x, y) = (a.x, a.y);
    // Frames 4 and 5 shoot right, 10 and 11 left (standing, crouching);
    // the others do not fire.
    match a.frame.wrapping_sub(4) {
        0 => {
            c.spawn(LASER_RIGHT, x + 3, y - 2);
            a.frame = 0xe;
        }
        1 => c.spawn(LASER_RIGHT, x + 3, y - 1),
        6 => {
            c.spawn(LASER_LEFT, x - 2, y - 2);
            a.frame = 0xf;
        }
        7 => c.spawn(LASER_LEFT, x - 2, y - 1),
        _ => {}
    }
    true
}

fn soldier(a: &mut Actor, c: &mut Ctx, s: &mut Soldier) {
    // Back from the shot frames.
    if a.frame == 0xf {
        a.frame = 10;
    } else if a.frame == 0xe {
        a.frame = 4;
    }
    loop {
        if s.sleeping {
            if a.y != c.player.y || !c.near_x(a, 6) {
                let shift = c.random.next_byte() & 4;
                a.frame = (u16::from(c.draw_page) >> shift) + 0xc;
                return;
            }
            s.sleeping = false;
            s.left = c.player.x < a.x;
        }
        if soldier_shoots(a, c, s) {
            return;
        }
        s.aim_delay = 0;
        if !s.left && c.draw_page {
            s.walked += 1;
            if step_side(a, c, true) || s.walked == 0x14 {
                s.left = true;
                s.walked = 0;
                a.frame = 6;
            } else {
                a.frame += 1;
                if a.frame > 3 {
                    a.frame = 0;
                }
            }
        }
        if !s.left || !c.draw_page {
            return;
        }
        s.walked += 1;
        if !step_side(a, c, false) && s.walked != 0x14 {
            a.frame += 1;
            if !(6..=9).contains(&a.frame) {
                a.frame = 6;
            }
            return;
        }
        s.left = false;
        s.walked = 0;
    }
}

// ---------------------------------------------------------------------
// Enemy laser shot

/// Flies two tiles a frame, right (type 135) or left (136), and is removed when
/// it hits a wall or leaves the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaserShot {
    right: bool,
    /// The muzzle flash was drawn.
    flashed: bool,
}

fn enemy_laser_shot(a: &mut Actor, c: &mut Ctx, s: &mut LaserShot) {
    if c.on_screen(a.placed()) {
        if !s.flashed {
            s.flashed = true;
            let flash = if s.right {
                LASER_FLASH_RIGHT
            } else {
                LASER_FLASH_LEFT
            };
            c.draw_at(flash, 0, a);
            c.sound(Sound::EnemyLaser);
        }
        let dir = if s.right { Dir::Right } else { Dir::Left };
        a.x += if s.right { 2 } else { -2 };
        if c.collide(LASER_LEFT, 0, a.x, a.y, dir) == Hit::Free {
            return;
        }
    }
    c.remove(a);
}

// ---------------------------------------------------------------------
// Spiked green creature

/// Frames of the spiked green creature's break-out hop: frame, then x and y
/// offsets (x in its facing direction).
const GREEN_HOP: [(u16, i32, i32); 6] = [
    (3, 0, 0),
    (3, 0, 0),
    (4, 2, -2),
    (4, 2, -1),
    (4, 2, 0),
    (5, 2, 0),
];

/// The phases of the spiked green creature, in `SpikedGreen::phase`.
const SHELL_BREAKS: u8 = 0xf;
const TURNS_UNTIL: u8 = 0x1e;
const HOPS: u8 = 0x1f;
const LANDS: u8 = 0x20;

/// Types 189 (facing left) and 190 break out of a stone shell (shell bits fly),
/// turn to the player, hop out and then hop towards him. The sprite says which
/// way it faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpikedGreen {
    /// Frames counted up to 0xf (the shell breaks), then up to 0x1e while it
    /// turns; 0x1f hopping out, 0x20 landing.
    phase: u8,
    /// The position in the hop table.
    hop: u8,
}

fn spiked_green(a: &mut Actor, c: &mut Ctx, s: &mut SpikedGreen) {
    if s.phase < SHELL_BREAKS {
        s.phase += 1;
        if s.phase == 5 {
            let dust = if a.id == GREEN_LEFT {
                SHELL_DUST_LEFT
            } else {
                SHELL_DUST_RIGHT
            };
            c.effect(dust, a.x, a.y, Motion::Still, 0);
            c.effect(dust, a.x, a.y, Motion::Still, 4);
        }
        if s.phase == SHELL_BREAKS {
            let first = if a.id == GREEN_LEFT {
                SHELL_BITS_LEFT
            } else {
                SHELL_BITS_RIGHT
            };
            for n in 0..4 {
                c.effect(first + n, a.x, a.y, Motion::Debris(n as u8), 0);
            }
            c.sound(Sound::Break);
            a.frame += 1;
        }
        return;
    }
    if s.phase < TURNS_UNTIL {
        a.id = if a.x > c.player.x {
            GREEN_LEFT
        } else {
            GREEN_RIGHT
        };
        s.phase += 1;
        if s.phase == 0x1a {
            a.frame += 1;
        }
        return;
    }
    let left = a.id == GREEN_LEFT;
    let forward = |a: &mut Actor, dx: i32| a.x += if left { -dx } else { dx };
    if s.phase < LANDS {
        if s.phase == TURNS_UNTIL {
            a.flags.remove(ActorFlags::GRAVITY);
            s.hop = 0;
            s.phase = HOPS;
        }
        match GREEN_HOP.get(usize::from(s.hop)) {
            Some(&(frame, dx, dy)) => {
                a.frame = frame;
                forward(a, dx);
                a.y += dy;
                s.hop += 1;
            }
            None => {
                s.phase = LANDS;
                a.frame = 5;
                a.flags.insert(ActorFlags::GRAVITY);
                forward(a, 2);
                a.fall = 2;
            }
        }
    } else if s.phase == LANDS {
        if a.fall == 0 {
            s.phase = SHELL_BREAKS;
            a.flags.remove(ActorFlags::GRAVITY);
            a.frame = 2;
        }
        forward(a, 2);
    }
    if left && c.move_blocked(a, Dir::Left) {
        a.id = GREEN_RIGHT;
        return;
    }
    if !left && c.move_blocked(a, Dir::Right) {
        a.id = GREEN_LEFT;
    }
}

// ---------------------------------------------------------------------
// Fast walker

/// Types 31 (left) and 32 (right) walk two tiles per frame and pause for 10
/// frames when they turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FastWalker {
    /// Frames left of the pause.
    pause: u8,
    /// The step of the walk cycle.
    step: u8,
}

fn fast_walker(a: &mut Actor, c: &mut Ctx, s: &mut FastWalker) {
    if s.pause != 0 {
        s.pause -= 1;
        a.frame = 0;
        if s.pause == 0 {
            s.step = 0;
        }
        return;
    }
    let right = a.id == FAST_WALKER_RIGHT;
    for _ in 0..2 {
        if step_side(a, c, right) {
            a.id = if right {
                FAST_WALKER_LEFT
            } else {
                FAST_WALKER_RIGHT
            };
            s.pause = 10;
        }
    }
    s.step = (s.step + 1) % 4;
    a.frame = WALK_CYCLE[usize::from(s.step)];
    if a.fall != 0 {
        a.frame = 2;
    }
}

// ---------------------------------------------------------------------
// Turkey

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurkeyWalk {
    Left,
    Right,
    /// Shot: it steams, and is worth more health.
    Cooked,
}

/// Type 202 walks and turns at walls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turkey {
    started: bool,
    pub walk: TurkeyWalk,
    /// Frames of steam when cooked.
    steam: u8,
}

fn turkey(a: &mut Actor, c: &mut Ctx, s: &mut Turkey) {
    if !s.started {
        s.started = true;
        if a.x < c.player.x {
            s.walk = TurkeyWalk::Right;
        }
    }
    let odd = u16::from(c.draw_page);
    match s.walk {
        TurkeyWalk::Right => {
            if step_side(a, c, true) {
                s.walk = TurkeyWalk::Left;
                return;
            }
            a.frame = odd + 2;
        }
        TurkeyWalk::Left => {
            let blocked = step_side(a, c, false);
            a.frame = odd;
            if blocked {
                s.walk = TurkeyWalk::Right;
            }
        }
        TurkeyWalk::Cooked => {
            a.frame = u16::from(s.steam & 3) + 4;
            s.steam = s.steam.wrapping_add(1);
        }
    }
}

// ---------------------------------------------------------------------
// Patrol walker

/// Type 176 walks and turns at walls and ledges; it starts towards the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatrolWalker {
    right: bool,
    steps: u8,
    started: bool,
}

fn patrol_walker(a: &mut Actor, c: &mut Ctx, s: &mut PatrolWalker) {
    if !s.started {
        s.right = c.player.x >= a.x;
        s.started = true;
    }
    if step_side(a, c, s.right) {
        s.right = !s.right;
    }
    s.steps = s.steps.wrapping_add(1);
    a.frame = if s.right { 3 } else { 0 } + WALK_CYCLE[usize::from(s.steps & 3)];
}

// ---------------------------------------------------------------------
// Ceiling dropper

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropperMode {
    Left,
    Right,
    /// Falling (and climbing back).
    Drop,
}

/// Type 203 walks along a ceiling and drops when the player is below it, then
/// climbs back up to the height it dropped from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CeilingDropper {
    mode: DropperMode,
    /// The step of the walk cycle.
    step: u8,
    /// The phase of the drop: 0 to 6 hangs, 7 falls, 8 climbs, 9 done.
    phase: u8,
    /// The row it drops from.
    top: i32,
}

fn ceiling_dropper(a: &mut Actor, c: &mut Ctx, s: &mut CeilingDropper) {
    let (px, py) = (c.player.x, c.player.y);
    if s.mode != DropperMode::Drop && a.y + 2 < py && px < a.x && a.x < px + 2 {
        s.mode = DropperMode::Drop;
    }
    if s.mode == DropperMode::Right {
        if !step_side(a, c, true) {
            s.step = s.step.wrapping_add(1);
            a.frame = WALK_CYCLE[usize::from(s.step & 3)] + 3;
        } else {
            s.mode = DropperMode::Left;
        }
    }
    if s.mode == DropperMode::Left {
        if step_side(a, c, false) {
            s.mode = DropperMode::Right;
            return;
        }
        s.step = s.step.wrapping_add(1);
        a.frame = WALK_CYCLE[usize::from(s.step & 3)];
    }
    if s.mode == DropperMode::Drop {
        let odd = u16::from(c.draw_page);
        match s.phase {
            0..7 => {
                s.top = a.y;
                a.frame = odd + 6;
                s.phase += 1;
            }
            7 => {
                if !a.flags.contains(ActorFlags::GRAVITY) {
                    a.fall = 1;
                }
                a.flags.insert(ActorFlags::GRAVITY);
                a.frame = 6;
                if a.fall == 0 {
                    s.phase = 8;
                    a.flags.remove(ActorFlags::GRAVITY);
                }
            }
            8 => {
                a.frame = odd + 6;
                if s.top < a.y {
                    a.y -= 1;
                } else {
                    s.phase = 9;
                }
            }
            _ => {
                s.mode = if c.draw_page {
                    DropperMode::Right
                } else {
                    DropperMode::Left
                };
                s.phase = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------
// Flyers

/// Type 280, released by the mine layer boss. It unfolds over 4 steps on every
/// second frame, then homes in on the player on three frames out of four.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomingDrone {
    unfolded: u8,
    /// Flips every frame.
    flip: bool,
}

fn homing_drone(a: &mut Actor, c: &mut Ctx, s: &mut HomingDrone) {
    s.flip = !s.flip;
    if s.unfolded < 4 {
        if s.flip {
            s.unfolded += 1;
            a.frame += 1;
        }
    } else {
        a.frame = 4;
        if c.random.next_byte() & 3 != 0 {
            // A step in x and y.
            a.x += (c.player.x + 1 - a.x).signum();
            a.y += (c.player.y - 1 - a.y).signum();
        }
    }
}

/// Types 271 to 273, small ships that fly left and keep the height above the
/// ground they had at the start. They explode when they hit a tile that hurts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainFlyer {
    /// The distance to the ground to keep (found at the first frame).
    height: u8,
    /// The step of the 4 animation frames.
    step: u8,
}

fn terrain_flyer(a: &mut Actor, c: &mut Ctx, s: &mut TerrainFlyer) {
    let ground = |c: &Ctx, x: i32, y: i32| c.map.has(x, y, TileAttributes::SOLID_TOP);
    // At the map's left edge this is the last cell of the row above,
    // normally wall.
    if c.map.has(a.x - 1, a.y, TileAttributes::SOLID_RIGHT) {
        hits::react(a, c, 1);
        return;
    }
    if s.height == 0 {
        for n in 0..0xf {
            if ground(c, a.x, a.y + n) {
                s.height = n as u8;
                break;
            }
        }
    }
    let mut n = 0;
    while n < i32::from(s.height) {
        if ground(c, a.x, a.y + n) {
            a.y -= 1;
            break;
        }
        n += 1;
    }
    if i32::from(s.height) == n && !ground(c, a.x, a.y + n) {
        a.y += 1;
    }
    a.x -= 1;
    if !c.on_screen(a.placed()) && a.x - 0x14 == c.player.x {
        c.remove(a);
    } else {
        s.step = (s.step + 1) % 4;
        a.frame = WALK_CYCLE[usize::from(s.step)];
    }
}

/// Rise of the jumping gunner's hop, per frame.
const JUMP: [i32; 4] = [-2, -2, -1, 0];

/// Type 299 hops towards the player and fires sideways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpingGunner {
    pub right: bool,
    /// Rising in a hop (else on the ground or falling).
    rising: bool,
    /// A rest: it fires when the count is at 20.
    rest: u8,
    /// How far it has hopped one way (1 to 5); at 6 it turns back.
    reach: i8,
    /// The step of the hop's rise.
    jump_step: u8,
}

fn jumping_gunner(a: &mut Actor, c: &mut Ctx, s: &mut JumpingGunner) {
    if s.rest != 0 {
        s.rest -= 1;
        if s.rest > 0x10 {
            return;
        }
        a.frame = if s.right { 4 } else { 0 };
        return;
    }
    if s.rising {
        a.y += JUMP.get(usize::from(s.jump_step)).copied().unwrap_or(0);
        s.jump_step += 1;
        if s.jump_step == 4 {
            s.rising = false;
            a.flags.insert(ActorFlags::GRAVITY);
            a.fall = 0;
        }
    }
    let (id, x, y) = (a.id, a.x, a.y);
    if c.collide(id, 0, x, y + 1, Dir::Down) == Hit::Free {
        // In the air: on sideways.
        if !s.right {
            if c.collide(id, 0, x - 2, y, Dir::Left) == Hit::Free {
                a.x -= 2;
            }
        } else if c.collide(id, 0, x + 2, y, Dir::Right) == Hit::Free {
            a.x += 2;
        }
    } else {
        s.right = x < c.player.x;
        let mut rests = true;
        if c.random.next_byte() & 1 != 0 {
            s.reach += if s.right { 1 } else { -1 };
            if s.reach == 0 {
                s.reach += 1;
            } else if s.reach != 6 {
                // Starts a hop.
                a.flags.remove(ActorFlags::GRAVITY);
                s.rising = true;
                a.fall = 0;
                s.jump_step = 0;
                rests = false;
            } else {
                s.reach -= 1;
            }
            if rests {
                rests = c.random.next_byte() & 1 != 0;
            }
        }
        if rests {
            s.rest = 0x14;
        }
    }
    a.frame = if s.right { 4 } else { 0 };
    if s.rest == 0x14 {
        a.frame += 3;
        let (code, shot_x) = if s.right { (0, a.x + 4) } else { (4, a.x) };
        c.effect(GUNNER_SHOT, shot_x, a.y - 4, Motion::from_code(code), 0);
    } else if s.rising {
        a.frame += 1;
    } else {
        a.frame += 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::spawn::lookup;

    fn start(level_type: u16) -> State {
        let s = lookup(level_type).unwrap();
        new(s.update, s.vars).unwrap()
    }

    #[test]
    fn soldiers_start_by_their_type() {
        let State::Soldier(a) = start(159) else {
            panic!()
        };
        let State::Soldier(b) = start(171) else {
            panic!()
        };
        let State::Soldier(c) = start(217) else {
            panic!()
        };
        assert!(!a.left && !a.sleeping);
        assert!(b.left && !b.sleeping);
        assert!(c.sleeping);
    }

    #[test]
    fn laser_bolts_fly_by_their_type() {
        let State::EnemyLaserShot(r) = start(135) else {
            panic!()
        };
        let State::EnemyLaserShot(l) = start(136) else {
            panic!()
        };
        assert!(r.right && !l.right);
    }

    #[test]
    fn walkers_and_bots_start_as_in_the_spawn_data() {
        let State::FastWalker(w) = start(31) else {
            panic!()
        };
        assert_eq!(w.pause, 10);
        let State::Hoverbot(h) = start(0) else {
            panic!()
        };
        assert_eq!(h.beam, 9);
        let State::JumpingGunner(g) = start(299) else {
            panic!()
        };
        assert_eq!(g.reach, 3);
        assert!(new(Update::Animate, [0; 5]).is_none());
    }
}
