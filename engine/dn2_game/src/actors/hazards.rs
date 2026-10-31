//! Hazards: spiders, snakes, ceiling suckers, crushers, laser turrets,
//! drones, force field generators, missiles, air fans and the like.
//!
//! Most of these go through stages, which the original keeps in the actor's
//! variables; here each update function has a state of its own. Several act
//! on the player: the snake and the ceiling sucker hold him
//! (`PlayerState::Grabbed`, [`LevelState::grabber`]), spiders cling to him
//! ([`LevelState::spiders`]) and the air fan lifts him
//! (`PlayerState::AirFan`, [`LevelState::fan`]). The parked ship takes him
//! aboard (`PlayerState::Ship`).

use super::hits::{self, ROBOT_DEATH};
use super::{Actor, ActorFlags, Ctx, Kind, Update};
use crate::draw::Placed;
use crate::effects::{BLAST, EXPLOSION, EXPLOSION_START, Motion};
use crate::map::{Dir, Hit};
use crate::messages::{once, text};
use crate::particles::Spread;
use crate::player::{Facing, PlayerState, Weapon, frame};
use crate::shots::ShotDirection;
use crate::{Buttons, Sound, SpriteLook};

/// Sprites (which are also the actors' types).
const FLAME_BOT_RIGHT: u16 = 38;
const EYEBALL_LEFT: u16 = 98;
const EYEBALL_RIGHT: u16 = 99;
const HOVER_DRONE_SPRITE: u16 = 151;
const SPIDER_SPRITE: u16 = 154;
/// Sprites that parts of these actors draw: the eyeball thrown, the hazard
/// column, the force field's overlay, the missile's flame, the fan's air
/// stream and the crusher's shaft.
const EYEBALL: u16 = 100;
const COLUMN: u16 = 150;
const FIELD_OVERLAY: u16 = 0x5d;
const MISSILE_FLAME: u16 = 0x95;
const AIR_STREAM: u16 = 0x104;
const SHAFT: u16 = 219;
/// Enemy laser shots, flying left and right (they are actors).
const LASER_SHOT_LEFT: u16 = 0x88;
const LASER_SHOT_RIGHT: u16 = 0x87;
/// Effects.
const FLAME_RIGHT: u16 = 0x28;
const FLAME_LEFT: u16 = 0x29;
const FIREBALL: u16 = 0x2b;
const FLAME: u16 = 0x4a;
const DUST: u16 = 0x54;
const DEBRIS_ROCK: u16 = 0x60;
const SPARKLE_500: u16 = 0x7c;
const TURRET_DEBRIS: u16 = 0x83;
const SPIDER_DEBRIS: u16 = 0xe8;
const COLUMN_DEBRIS_TOP: u16 = 0x98;
const COLUMN_DEBRIS_TIP: u16 = 0x99;
/// The player's frame that is not there: he is hidden.
const NO_FRAME: u16 = 0xff;

/// The state of each update function of the family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    FlameThrowerBot(FlameThrowerBot),
    ParkedShip(ParkedShip),
    BrokenMissile(BrokenMissile),
    EyeballThrower(EyeballThrower),
    LaserTurret(LaserTurret),
    ForceFieldGenerator(ForceFieldGenerator),
    ExtendingHazardColumn(ExtendingHazardColumn),
    HoverDrone(HoverDrone),
    Spider(Spider),
    LaunchableMissile(LaunchableMissile),
    AirFan(AirFan),
    Snake(Snake),
    CeilingSucker(CeilingSucker),
    CrusherPiston(CrusherPiston),
}

/// Types 38 and 39 slide up and down a wall and now and then breathe a flame to
/// the right (38) or left (39).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlameThrowerBot {
    /// Going up (every other frame) instead of down.
    rising: bool,
    /// Frames of breathing left, from 16; the flame comes at 8.
    breath: u16,
}

/// The player's ship waits (types 87, 89 and 90) for the player to fall into it
/// (see [`super::contact`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParkedShip {
    /// Frames before it can be entered again.
    pub cooldown: u16,
}

/// The way a ship sprite faces (87 right, 88 left).
fn ship_facing(id: u16) -> Facing {
    if id == crate::player::SHIP_LEFT {
        Facing::Left
    } else {
        Facing::Right
    }
}

/// Type 95. Shot, it tips over to the side away from the shot, and after 12
/// frames blows up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrokenMissile {
    /// 0 not shot, 1 tips left, 2 tips right.
    side: u8,
    /// Frames since it was shot.
    step: u8,
}

/// Where an eyeball thrower is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eyes {
    /// Looks where the player is.
    Face,
    /// Its start-up frames, 1 to 10.
    Start(u8),
    Walk,
    Throw,
}

/// Types 98 (facing left) and 99 (right) first look at the player, then walk,
/// and when the player is 10 to 14 tiles ahead throw an eyeball.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EyeballThrower {
    stage: Eyes,
    /// Steps in the walk stage: it moves on every fourth.
    steps: u16,
    /// Which walking frame.
    foot: bool,
    /// Frames of the throw so far.
    throw: u16,
}

/// Type 131 rises out of the wall (it cannot be hurt until it is out), then
/// turns to face the player and every 40 frames fires a laser (types 135 and
/// 136).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaserTurret {
    /// Frames of rising left; 0 when it is out.
    rise: u16,
    /// Frames to the next shot.
    timer: u16,
}

/// Types 93 and 218. When shot it flashes for 20 frames; touched while cloaked
/// it is destroyed: sparks and 500 points on every second frame for 10 frames,
/// then it blows up and says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForceFieldGenerator {
    /// Frames of flashing, from 1; 0 not shot.
    flash: u16,
    /// Frames since the destruction started; 0 not started.
    destroy: u16,
    /// The frame of the overlay sprite (the field's colour).
    overlay: u16,
}

/// What the hazard column is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Shrinking,
    Growing,
    Burning,
    /// Retracted for a while.
    Retracted,
}

/// Frames of the hazard column's burning phase, plus one.
const COLUMN_BURN: [u16; 20] = [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 3];

/// Type 150 hangs from the ceiling and grows downward. In the burning phase it
/// hurts the player. It can be shot for 250 points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendingHazardColumn {
    /// Tiles long (when retracted, frames of waiting).
    length: u16,
    phase: Column,
    /// Frames of burning so far.
    burn: u16,
}

/// Type 151 wakes up, drifts towards the player for 40 frames, opens, fires
/// lasers to both sides four times (every second frame), closes and starts
/// over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoverDrone {
    /// Frames of waking up, to 10.
    wake: u16,
    /// Frames into the cycle: drift to 40, open to 50, fire to 80, close.
    phase: u16,
}

/// Where a spider goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Heading {
    Left,
    Right,
    /// Falling on the player.
    Drop,
}

/// Type 154 crawls along floors and ceilings, drops on the player when he
/// passes below, and clings to him (the contact code lets it). Turning around
/// twice shakes it off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spider {
    heading: Heading,
    /// Which frames the spider shows on the player (0xc left, 0xe right).
    cling_frame: u16,
    /// Has looked at what is below it.
    started: bool,
    /// Walks on a floor, not under a ceiling. The original keeps it in the
    /// actor's score (which then also adds a point when it is shot).
    floor: bool,
    /// Clings to the player.
    clinging: bool,
    /// The player's facing when last looked at; `None` just after landing.
    last_facing: Option<Facing>,
    /// Turns of the player, which count down while he does not turn.
    turns: u8,
}

/// Type 144 sits in a silo. Once it was shot it ignites, then flies up (faster
/// after the 9th frame) with a flame until it hits the ceiling, where it blows
/// a hole in the rock and explodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchableMissile {
    /// 0 not shot; else frames since, to 9 (the shot sets 1 or 2, see `hit`).
    stage: u8,
    /// Hit the ceiling.
    blown: bool,
}

/// The frame of the fan actor and of its air stream sprite, by how far the fan
/// has come.
const FAN_FRAME: [u16; 62] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2,
    3, 0, 1, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2,
];
const FAN_STREAM: [u16; 62] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2,
    3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2,
];
/// Frames of the fan's cycle: it blows up to 60 and back down.
const FAN_TOP: u16 = 60;
/// The stream lifts from this long on, 25 rows high and 5 columns wide.
const FAN_LIFT_FROM: u16 = 25;
const FAN_REACH: i32 = 25;
const FAN_WIDTH: i32 = 5;

/// Type 130 blows in cycles. While the stream is at least 25 long, a player in
/// the 5 columns above the fan, up to 25 rows up, is lifted
/// (`PlayerState::AirFan`); when he leaves the stream he falls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirFan {
    /// Blowing down again.
    fading: bool,
    /// Where it is in the cycle.
    step: u16,
}

/// Type 78 walks and, when it has swallowed the player, carries him along,
/// hurting him every frame. Firing makes it spit him out and die.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snake {
    /// Walking right (otherwise left).
    right: bool,
    /// The player stepped next to it (the contact code): it grabs him.
    caught: bool,
    /// Frames of swallowing him, from 1 to 7.
    swallow: u16,
    /// He is inside.
    inside: bool,
    /// Frames walked.
    ticks: u16,
}

/// Where a crusher is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crusher {
    Idle,
    Extending,
    Retracting,
}

/// Type 81 is a plant on the ceiling. It extends when the player is within 4
/// tiles; when it caught him (the contact code) it lets go after a while and
/// hurts him. Then it rests for 40 frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CeilingSucker {
    /// Frames into the animation, from 1; from 100 it rests; 0 waits.
    timer: u16,
    caught: bool,
}

/// Type 219 waits until it is on screen, counts 20 frames, then extends down
/// until it hits something, slams, and retracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrusherPiston {
    /// Frames of waiting, to 20.
    wait: u16,
    /// Tiles it is extended.
    length: u16,
    phase: Crusher,
}

/// The starting state for `update` (from the spawn data's `vars`), if it
/// belongs to this family.
pub(super) fn new(update: Update, vars: [u16; 5]) -> Option<State> {
    Some(match update {
        Update::FlameThrowerBot => State::FlameThrowerBot(FlameThrowerBot {
            rising: vars[0] != 0,
            breath: vars[1],
        }),
        Update::ParkedShip => State::ParkedShip(ParkedShip { cooldown: vars[0] }),
        Update::BrokenMissile => State::BrokenMissile(BrokenMissile { side: 0, step: 0 }),
        Update::EyeballThrower => State::EyeballThrower(EyeballThrower {
            stage: Eyes::Face,
            steps: 0,
            foot: false,
            throw: 0,
        }),
        Update::LaserTurret => State::LaserTurret(LaserTurret {
            rise: vars[0],
            timer: vars[1],
        }),
        Update::ForceFieldGenerator => State::ForceFieldGenerator(ForceFieldGenerator {
            flash: vars[0],
            destroy: vars[2],
            overlay: vars[3],
        }),
        Update::ExtendingHazardColumn => State::ExtendingHazardColumn(ExtendingHazardColumn {
            length: vars[0],
            phase: match vars[1] {
                0 => Column::Shrinking,
                1 => Column::Growing,
                2 => Column::Burning,
                _ => Column::Retracted,
            },
            burn: vars[2],
        }),
        Update::HoverDrone => State::HoverDrone(HoverDrone {
            wake: vars[0],
            phase: vars[1],
        }),
        Update::Spider => State::Spider(Spider {
            heading: Heading::Left,
            cling_frame: 0,
            started: false,
            floor: false,
            clinging: false,
            last_facing: None,
            turns: 0,
        }),
        Update::LaunchableMissile => State::LaunchableMissile(LaunchableMissile {
            stage: 0,
            blown: false,
        }),
        Update::AirFan => State::AirFan(AirFan {
            fading: vars[0] != 0,
            step: vars[1],
        }),
        Update::Snake => State::Snake(Snake {
            right: vars[0] != 0,
            caught: vars[1] != 0,
            swallow: vars[2],
            inside: vars[3] != 0,
            ticks: vars[4],
        }),
        Update::CeilingSucker => State::CeilingSucker(CeilingSucker {
            timer: vars[0],
            caught: vars[1] != 0,
        }),
        Update::CrusherPiston => State::CrusherPiston(CrusherPiston {
            wait: vars[0],
            length: vars[1],
            phase: match vars[2] {
                1 => Crusher::Extending,
                2 => Crusher::Retracting,
                _ => Crusher::Idle,
            },
        }),
        _ => return None,
    })
}

/// The slot the actor being updated is in.
fn slot(c: &Ctx) -> Option<usize> {
    c.actors.current
}

/// The player's position in tiles.
fn player_at(c: &Ctx) -> (i32, i32) {
    (c.player.x, c.player.y)
}

/// Whether the middle of the actor and the player are at most `dist` columns
/// apart.
fn player_near_x(c: &Ctx, a: &Actor, dist: i32) -> bool {
    let width = c
        .sprites
        .frame_box(a.id, a.frame)
        .map_or(0, |f| i32::from(f.width));
    let player = player_at(c).0
        + if c.player.facing == Facing::Left {
            2
        } else {
            1
        };
    (a.x + width / 2 - player).abs() <= dist
}

/// Lets the player go from a snake or a ceiling sucker.
fn release_player(c: &mut Ctx) {
    // The frame stays (hidden, if he was inside) until he next stands.
    if c.player.state == PlayerState::Grabbed {
        c.player.state = PlayerState::Standing;
    }
    c.state.grabber = None;
}

/// The actor's update function.
pub(super) fn update(a: &mut Actor, c: &mut Ctx) {
    let Kind::Hazard(s) = &a.kind else {
        return;
    };
    let mut s = *s;
    match &mut s {
        State::FlameThrowerBot(s) => flame_thrower_bot(a, c, s),
        State::ParkedShip(s) => s.cooldown = s.cooldown.saturating_sub(1),
        State::BrokenMissile(s) => broken_missile(a, c, s),
        State::EyeballThrower(s) => eyeball_thrower(a, c, s),
        State::LaserTurret(s) => laser_turret(a, c, s),
        State::ForceFieldGenerator(s) => force_field_generator(a, c, s),
        State::ExtendingHazardColumn(s) => extending_hazard_column(a, c, s),
        State::HoverDrone(s) => hover_drone(a, c, s),
        State::Spider(s) => spider(a, c, s),
        State::LaunchableMissile(s) => launchable_missile(a, c, s),
        State::AirFan(s) => air_fan(a, c, s),
        State::Snake(s) => snake(a, c, s),
        State::CeilingSucker(s) => ceiling_sucker(a, c, s),
        State::CrusherPiston(s) => crusher_piston(a, c, s),
    }
    a.kind = Kind::Hazard(s);
}

fn flame_thrower_bot(a: &mut Actor, c: &mut Ctx, s: &mut FlameThrowerBot) {
    if c.random.next_byte() & 0x7f == 0 {
        s.breath = 0x10;
    }
    if s.breath == 0 {
        if !s.rising {
            a.y += 1;
            if c.move_blocked(a, Dir::Down) {
                s.rising = true;
            }
        } else if c.draw_page {
            a.y -= 1;
            if c.move_blocked(a, Dir::Up) {
                s.rising = false;
            }
        }
    } else {
        s.breath -= 1;
        if s.breath == 8 {
            let (flame, x) = if a.id == FLAME_BOT_RIGHT {
                (FLAME_RIGHT, a.x + 7)
            } else {
                (FLAME_LEFT, a.x - 7)
            };
            c.effect(flame, x, a.y - 3, Motion::Still, 0);
        }
    }
}

/// The frames of the missile tipping over, by step.
const TIP: [u16; 7] = [1, 2, 3, 2, 3, 4, 3];
/// Frames from the shot to the explosion.
const TIP_FRAMES: u8 = 12;

fn broken_missile(a: &mut Actor, c: &mut Ctx, s: &mut BrokenMissile) {
    if s.side == 0 || s.step >= TIP_FRAMES {
        return;
    }
    if let Some(&step) = TIP.get(usize::from(s.step)) {
        a.frame = if s.side == 1 { step } else { step + 4 };
        if step == 3 {
            c.sound(Sound::Clank);
        }
    }
    s.step += 1;
    if s.step == TIP_FRAMES {
        c.remove(a);
        c.state.flash = Some(1);
        c.explosion_sound();
        c.effect(FIREBALL, a.x, a.y, Motion::Still, 0);
        for n in 0..4u16 {
            let path = if n & 1 == 0 { 1 } else { 3 };
            let x = a.x + i32::from(n) * 2;
            c.effect(DEBRIS_ROCK, x, a.y, Motion::Debris(path), n);
        }
    }
}

fn eyeball_thrower(a: &mut Actor, c: &mut Ctx, s: &mut EyeballThrower) {
    const START: [u16; 11] = [0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5];
    const WALK: [u16; 2] = [5, 6];
    const THROW: [u16; 6] = [7, 7, 8, 8, 9, 9];
    let px = player_at(c).0;
    match s.stage {
        Eyes::Face => {
            a.id = if px < a.x {
                EYEBALL_LEFT
            } else {
                EYEBALL_RIGHT
            };
            s.stage = Eyes::Start(1);
        }
        Eyes::Start(n) => {
            a.frame = START[usize::from(n)];
            s.stage = if n == 10 {
                Eyes::Walk
            } else {
                Eyes::Start(n + 1)
            };
        }
        Eyes::Walk => {
            s.steps = s.steps.wrapping_add(1);
            let ahead = (a.id == EYEBALL_LEFT && px < a.x) || (a.id == EYEBALL_RIGHT && a.x < px);
            if ahead && player_near_x(c, a, 14) && !player_near_x(c, a, 9) {
                s.stage = Eyes::Throw;
                s.throw = 0;
            } else if s.steps & 3 == 0 {
                s.foot = !s.foot;
                a.frame = WALK[usize::from(s.foot)];
                let dir = if a.id == EYEBALL_LEFT {
                    Dir::Left
                } else {
                    Dir::Right
                };
                a.x += if dir == Dir::Left { -1 } else { 1 };
                if c.move_blocked(a, dir) {
                    s.stage = Eyes::Face;
                    a.frame = 1;
                }
            }
        }
        Eyes::Throw => {
            let step = usize::from(s.throw);
            s.throw += 1;
            a.frame = THROW.get(step).copied().unwrap_or(0);
            if s.throw == 4 {
                if a.id == EYEBALL_LEFT {
                    c.effect(EYEBALL, a.x, a.y - 6, Motion::Debris(3), 0);
                } else {
                    c.effect(EYEBALL, a.x + 3, a.y - 6, Motion::Debris(1), 0);
                }
            }
            if s.throw == 6 {
                s.stage = Eyes::Walk;
                // The original's counter is left at 6, which the next step
                // toggles to the first frame.
                s.foot = true;
            }
        }
    }
}

fn laser_turret(a: &mut Actor, c: &mut Ctx, s: &mut LaserTurret) {
    if s.rise != 0 {
        s.rise -= 1;
        if s.rise == 0 {
            a.health = 1;
            s.timer = 0x28;
        } else {
            a.health = 0;
        }
        if s.rise < 0x15 {
            if s.rise & 1 != 0 && s.rise < 10 {
                a.frame = a.frame.wrapping_sub(1);
            }
        } else {
            a.frame = a.frame.wrapping_add(1);
        }
        a.frame = a.frame.wrapping_add(1);
        if a.frame > 7 {
            a.frame = 0;
        }
        if a.frame == 5 || a.frame == 6 {
            c.sound(Sound::Whir);
        }
        return;
    }
    if s.timer < 7 && s.timer & 1 != 0 {
        a.look = SpriteLook::WhiteFlash;
    }
    // Frame 4 looks straight ahead to the right, frame 0 to the left.
    let (shot, x);
    if player_at(c).0 < a.x {
        if a.frame != 0 {
            a.frame -= 1;
            return;
        }
        s.timer = s.timer.wrapping_sub(1);
        if s.timer != 0 {
            return;
        }
        (shot, x) = (LASER_SHOT_LEFT, a.x - 3);
    } else {
        if a.frame < 4 {
            a.frame += 1;
            return;
        }
        if a.frame > 4 {
            a.frame -= 1;
            return;
        }
        s.timer = s.timer.wrapping_sub(1);
        if s.timer != 0 {
            return;
        }
        (shot, x) = (LASER_SHOT_RIGHT, a.x + 2);
    }
    c.spawn(shot, x, a.y);
    s.timer = 0x28;
}

fn force_field_generator(a: &mut Actor, c: &mut Ctx, s: &mut ForceFieldGenerator) {
    if s.flash != 0 {
        s.flash += 1;
        a.frame = u16::from(c.draw_page) + 1;
        if c.random.next_byte() & 8 != 0 {
            a.look = SpriteLook::WhiteFlash;
            c.sound(Sound::Spark);
        }
        if s.flash == 0x14 {
            s.flash = 0;
            a.frame = 0;
        }
    }
    c.draw(FIELD_OVERLAY, s.overlay, a.x, a.y, SpriteLook::Normal);
    if s.destroy != 0 {
        s.destroy += 1;
        if s.destroy & 1 != 0 {
            c.sound(Sound::Break);
            let up = a.y - i32::from(s.destroy);
            c.particles(0x0b, Spread::Both, a.x + 1, up + 0x0f);
            c.effect(SPARKLE_500, a.x, up + 0x13, Motion::Float, 0);
            c.add_score(500);
        }
        if s.destroy == 0x0b {
            c.remove(a);
            for path in [5, 3, 1] {
                c.effect(EXPLOSION, a.x - 1, a.y + 5, Motion::Debris(path), 0);
            }
            c.sound(Sound::BigExplosion);
            c.state.messages.show(text::FORCE_FIELD_DESTROYED);
        }
    }
}

/// A piece of the hazard column.
fn column_piece(c: &mut Ctx, frame: u16, x: i32, y: i32) {
    c.draw(COLUMN, frame, x, y, SpriteLook::Normal);
}

fn extending_hazard_column(a: &mut Actor, c: &mut Ctx, s: &mut ExtendingHazardColumn) {
    // The actor itself is not drawn: the column is.
    a.look = SpriteLook::Hidden;
    if s.phase == Column::Retracted {
        s.length += 1;
        column_piece(c, 0, a.x, a.y);
        column_piece(c, 1, a.x, a.y + 1);
        if s.length == 10 {
            s.length = 1;
            s.phase = Column::Growing;
        }
        return;
    }
    let length = i32::from(s.length);
    for n in 0..length {
        column_piece(c, 0, a.x, a.y + n);
    }
    let end = a.y + length;
    if s.phase == Column::Growing {
        column_piece(c, 1, a.x, end);
        s.length += 1;
        if s.length == 7 {
            s.phase = Column::Burning;
        }
    }
    if s.phase == Column::Shrinking {
        column_piece(c, 1, a.x, end);
        s.length -= 1;
        if s.length == 1 {
            s.phase = Column::Retracted;
        }
    }
    if s.phase == Column::Burning {
        a.frame = COLUMN_BURN[usize::from(s.burn)] + 1;
        column_piece(c, a.frame, a.x, end);
        let tip = Placed {
            id: a.id,
            frame: 2,
            x: a.x,
            y: end,
        };
        if c.touches_player(tip) {
            c.hurt_player();
        }
        s.burn += 1;
        if s.burn == 0x13 {
            s.burn = 0;
            s.phase = Column::Shrinking;
        }
    }
    // Shot at its tip.
    let top = a.y;
    a.y = end;
    if hits::damage(a, c) != 0 {
        c.remove(a);
        c.add_score(250);
        c.effect(COLUMN_DEBRIS_TOP, a.x, a.y, Motion::Debris(3), 0);
        c.effect(COLUMN_DEBRIS_TIP, a.x + 2, a.y, Motion::Debris(1), 0);
        c.explosion_sound();
        c.cloud(a.id, a.x, a.y, BLAST);
    }
    a.y = top;
}

fn hover_drone(a: &mut Actor, c: &mut Ctx, s: &mut HoverDrone) {
    if s.wake < 10 {
        s.wake += 1;
        if !c.on_screen(a.placed()) {
            a.flags.remove(ActorFlags::ALWAYS_UPDATE);
        }
        return;
    }
    if s.phase < 0x28 {
        if c.random.next_byte() & 3 == 0 {
            let (px, py) = player_at(c);
            let dx = (px - a.x + 1).signum();
            let dy = (py - a.y - 2).signum();
            a.x += dx;
            a.y += dy;
            if dx > 0 && c.collide(HOVER_DRONE_SPRITE, 0, a.x, a.y, Dir::Right) != Hit::Free {
                a.x -= 1;
            }
            if dx < 0 && c.collide(HOVER_DRONE_SPRITE, 0, a.x, a.y, Dir::Left) != Hit::Free {
                a.x += 1;
            }
            if dy > 0 {
                c.move_blocked(a, Dir::Down);
            }
            if dy < 0 {
                c.move_blocked(a, Dir::Up);
            }
        }
        s.phase += 1;
        return;
    }
    if s.phase < 0x32 {
        // Opens.
        s.phase += 1;
        if a.frame <= 4 {
            a.frame += 1;
        }
        return;
    }
    if s.phase > 0x4f {
        // Closes, then starts over.
        if s.phase > 99 {
            return;
        }
        if a.frame != 0 {
            a.frame -= 1;
            return;
        }
        s.wake = 0;
        s.phase = 0;
        return;
    }
    if c.draw_page {
        return;
    }
    let (x, y) = (a.x, a.y);
    match s.phase & 3 {
        0 => c.spawn(LASER_SHOT_LEFT, x - 2, y - 1),
        1 => c.spawn(LASER_SHOT_LEFT, x - 2, y),
        2 => c.spawn(LASER_SHOT_RIGHT, x + 2, y),
        _ => c.spawn(LASER_SHOT_RIGHT, x + 2, y - 1),
    }
    s.phase += 1;
}

fn crusher_piston(a: &mut Actor, c: &mut Ctx, s: &mut CrusherPiston) {
    if s.wait < 0x14 {
        if s.wait == 0 && !c.on_screen(a.placed()) {
            return;
        }
        s.wait += 1;
        if s.wait == 0x14 {
            s.phase = Crusher::Extending;
        }
    }
    match s.phase {
        Crusher::Idle => {}
        Crusher::Extending => {
            s.length += 1;
            a.y += 1;
            for n in 0..i32::from(s.length) {
                c.draw(SHAFT, 1, a.x, a.y - n, SpriteLook::Normal);
            }
            if c.move_blocked(a, Dir::Down) {
                c.effect(DUST, a.x, a.y + 4, Motion::Still, 0);
                c.sound(Sound::Thud);
                s.phase = Crusher::Retracting;
            }
        }
        Crusher::Retracting => {
            s.length = s.length.saturating_sub(1);
            a.y -= 1;
            for n in 0..i32::from(s.length) {
                c.draw(SHAFT, 1, a.x, a.y - n + 1, SpriteLook::Normal);
            }
            if s.length == 1 {
                *s = CrusherPiston {
                    wait: 0,
                    length: 0,
                    phase: Crusher::Idle,
                };
            }
        }
    }
}

fn spider(a: &mut Actor, c: &mut Ctx, s: &mut Spider) {
    let me = slot(c);
    let (px, py) = player_at(c);
    let facing = c.player.facing;
    if c.player.state == PlayerState::Grabbed && c.state.spiders.contains(me) {
        // Gone with the player into the jaws of a snake.
        c.state.spiders.clear(me, None);
        c.remove(a);
        return;
    }
    if !s.clinging {
        crawl(a, c, s, px, py);
        return;
    }
    if !s.floor {
        a.y = py - 3;
        a.x = if facing == Facing::Left { px + 1 } else { px };
        a.frame = u16::from(c.random.next_byte() & 1) + 8;
    } else {
        if c.state.spiders.face == me {
            a.y = py - 1;
            if facing == Facing::Left {
                a.x = px - 1;
                s.cling_frame = 0xc;
            } else {
                a.x = px + 2;
                s.cling_frame = 0xe;
            }
        } else if c.state.spiders.back == me {
            a.y = py - 2;
            if facing != Facing::Right {
                a.x = px + 3;
                s.cling_frame = 0xe;
            } else {
                a.x = px - 2;
                s.cling_frame = 0xc;
            }
        }
        a.frame = s.cling_frame + u16::from(c.random.next_byte() & 1);
    }
    if Some(facing) == s.last_facing {
        if c.draw_page && s.turns != 0 {
            s.turns -= 1;
        }
    } else {
        s.turns += 1;
        s.last_facing = Some(facing);
        if s.turns == 2 {
            // Shaken off.
            let path = if c.random.next_byte() & 2 == 0 { 1 } else { 3 };
            c.effect(SPIDER_DEBRIS, a.x, a.y, Motion::Debris(path), 0);
            c.state.spiders.clear(me, None);
            c.remove(a);
        }
    }
    if c.player.state == PlayerState::Dying {
        let path = if c.random.next_byte() & 2 == 0 { 3 } else { 1 };
        c.effect(SPIDER_DEBRIS, a.x, a.y, Motion::Debris(path), 0);
        c.state.spiders.clear(me, None);
        c.remove(a);
    }
}

/// The spider on its own: crawls, and drops on the player below it.
fn crawl(a: &mut Actor, c: &mut Ctx, s: &mut Spider, px: i32, py: i32) {
    if !s.started {
        s.started = true;
        if c.collide(SPIDER_SPRITE, 0, a.x, a.y + 1, Dir::Down) != Hit::Free {
            s.floor = true;
            a.score = 1;
            a.frame = 9;
        }
    }
    // It moves on every other frame, but on every one while it falls.
    if s.heading == Heading::Drop || !c.draw_page {
        if s.heading == Heading::Right {
            a.x += 1;
            if s.floor {
                if c.move_blocked(a, Dir::Right) {
                    a.frame = 9;
                    s.heading = Heading::Left;
                    return;
                }
                a.frame += 1;
                if a.frame == 9 {
                    a.frame = 6;
                }
                return;
            }
            if c.collide(SPIDER_SPRITE, 0, a.x, a.y, Dir::Right) == Hit::Free
                && c.collide(SPIDER_SPRITE, 0, a.x + 2, a.y - 1, Dir::Up) != Hit::Free
            {
                a.frame += 1;
                if a.frame == 3 {
                    a.frame = 0;
                }
            } else {
                a.x -= 1;
                s.heading = Heading::Left;
                a.frame = 3;
            }
        }
        // (Turned round under a ceiling, it goes on at once, as in the
        // original.)
        if s.heading == Heading::Left {
            a.x -= 1;
            if s.floor {
                if !c.move_blocked(a, Dir::Left) {
                    a.frame += 1;
                    if a.frame == 12 {
                        a.frame = 9;
                    }
                    return;
                }
                a.frame = 6;
                s.heading = Heading::Right;
                return;
            }
            if c.collide(SPIDER_SPRITE, 0, a.x, a.y, Dir::Left) == Hit::Free
                && c.collide(SPIDER_SPRITE, 0, a.x - 2, a.y - 1, Dir::Up) != Hit::Free
            {
                a.frame += 1;
                if a.frame == 6 {
                    a.frame = 3;
                }
            } else {
                a.x += 1;
                s.heading = Heading::Right;
                a.frame = 0;
            }
        }
    }
    if a.x == px && s.heading != Heading::Drop && a.frame < 6 && a.y < py - 3 {
        s.heading = Heading::Drop;
        a.frame = 6;
        a.flags.insert(ActorFlags::GRAVITY);
        return;
    }
    if s.heading != Heading::Drop || a.fall != 0 {
        return;
    }
    // Landed.
    s.floor = true;
    a.score = 1;
    s.heading = Heading::Right;
}

fn launchable_missile(a: &mut Actor, c: &mut Ctx, s: &mut LaunchableMissile) {
    if s.stage != 0 {
        let flame = u16::from(c.draw_page);
        c.draw(MISSILE_FLAME, flame, a.x, a.y, SpriteLook::Normal);
        if s.stage == 1 {
            c.effect(FLAME, a.x - 2, a.y + 1, Motion::Still, 0);
            c.effect(FLAME, a.x + 1, a.y + 1, Motion::Still, 0);
        }
        if s.stage > 5 {
            a.y -= 1;
            if c.move_blocked(a, Dir::Up) {
                s.blown = true;
            }
            if s.stage > 8 {
                a.y -= 1;
                if c.move_blocked(a, Dir::Up) {
                    s.blown = true;
                }
            }
            c.sound(Sound::Roar);
        }
        if s.stage < 9 {
            s.stage += 1;
        }
    }
    if s.blown {
        c.remove(a);
        c.state.flash = Some(1);
        c.explosion_sound();
        crate::mechanics::explode_map_section(c, a.y - 0xc, a.x + 2, a.y - 0xe, a.x);
        for n in 0..4u16 {
            let (up, down) = if n & 1 == 0 { (5, 0) } else { (4, 2) };
            let x = a.x + i32::from(n) * 2;
            c.effect(DEBRIS_ROCK, x, a.y - 8, Motion::Debris(up), n);
            let y = a.y - 8 + i32::from(n) * 2;
            c.effect(DEBRIS_ROCK, x, y, Motion::Debris(down), n);
        }
    }
}

fn air_fan(a: &mut Actor, c: &mut Ctx, s: &mut AirFan) {
    let me = slot(c);
    if !s.fading {
        s.step += 1;
        if s.step == FAN_TOP {
            s.fading = true;
        }
    } else {
        s.step = s.step.saturating_sub(1);
        if s.step == 0 {
            s.fading = false;
        }
    }
    let step = usize::from(s.step);
    a.frame = FAN_FRAME[step];
    c.draw(AIR_STREAM, FAN_STREAM[step], a.x, a.y, SpriteLook::Normal);
    let (px, py) = player_at(c);
    if s.step >= FAN_LIFT_FROM
        && a.y < py + FAN_REACH
        && py < a.y
        && a.x <= px
        && px < a.x + FAN_WIDTH
        && c.player.state != PlayerState::Dying
    {
        c.player.state = PlayerState::AirFan;
        c.state.fan = me;
        if a.frame == 3 || py + FAN_REACH - 1 == a.y || py + FAN_REACH == a.y {
            c.sound(Sound::Whir);
        }
    }
    if c.player.state == PlayerState::AirFan
        && c.state.fan == me
        && (s.step < FAN_LIFT_FROM || px < a.x || a.x + FAN_WIDTH <= px || py + FAN_REACH < a.y)
    {
        // Out of the stream: falls from the top of a jump.
        c.player.state = PlayerState::Jumping;
        c.player.jump_step = 5;
        c.player.frame = 6;
    }
    if a.frame == 2 && c.on_screen(a.placed()) {
        c.sound(Sound::Whir);
    }
}

fn snake(a: &mut Actor, c: &mut Ctx, s: &mut Snake) {
    let me = slot(c);
    if c.state.grabber == me && c.player.state == PlayerState::Dying {
        // The player died in its jaws.
        c.remove(a);
        c.state.grabber = None;
        for (dx, dy, delay) in ROBOT_DEATH {
            c.effect(EXPLOSION_START, a.x + dx, a.y + dy, Motion::Still, delay);
        }
        return;
    }
    if s.caught {
        c.state.grabber = me;
        c.player.state = PlayerState::Grabbed;
        if s.swallow == 2 {
            c.player.frame = NO_FRAME;
        }
        if !s.inside {
            s.swallow += 1;
            if s.swallow == 7 {
                c.player.x = c.player.x.wrapping_add(2);
                s.inside = true;
            }
        }
        if s.inside {
            c.hurt_player();
            if c.input.contains(Buttons::FIRE) && c.player.state != PlayerState::Dying {
                // Spits him out and dies.
                a.health = 1;
                hits::react(a, c, 1);
                release_player(c);
                return;
            }
        }
        a.frame = (if s.right { 9 } else { 0 }) + s.swallow + u16::from(c.draw_page);
        if !s.inside {
            return;
        }
    }
    s.ticks = s.ticks.wrapping_add(1);
    if s.right {
        if !s.caught {
            if !c.draw_page {
                a.x += 1;
                a.frame = 9 + (a.x & 1) as u16;
            }
        } else if c.draw_page {
            c.player.x = c.player.x.wrapping_add(1);
            a.x += 1;
        }
        if c.move_blocked(a, Dir::Right) {
            s.right = false;
            a.x += 2;
        }
    } else {
        if !s.caught {
            if !c.draw_page {
                a.x -= 1;
                a.frame = (a.x & 1) as u16;
            }
        } else if c.draw_page {
            c.player.x = c.player.x.wrapping_sub(1);
            a.x -= 1;
        }
        if c.move_blocked(a, Dir::Left) {
            s.right = true;
            a.x -= 2;
        }
    }
}

fn ceiling_sucker(a: &mut Actor, c: &mut Ctx, s: &mut CeilingSucker) {
    const REACH: [u16; 12] = [0, 0, 1, 2, 3, 4, 5, 4, 3, 2, 1, 0];
    const CAUGHT: [u16; 32] = [
        0, 0, 0, 0, 0, 0, 10, 9, 8, 7, 6, 0, 6, 0, 6, 0, 6, 0, 6, 0, 6, 7, 8, 9, 10, 5, 4, 3, 2, 1,
        0, 1,
    ];
    let px = player_at(c).0;
    if s.timer == 0 && a.x <= px + 4 && px <= a.x + 4 {
        s.timer = 1;
    }
    if s.timer != 0 && s.timer < 100 {
        let table: &[u16] = if s.caught { &CAUGHT } else { &REACH };
        a.frame = table.get(usize::from(s.timer)).copied().unwrap_or(0);
        s.timer += 1;
        if s.caught && s.timer == 0x19 {
            // Lets go, and bites.
            release_player(c);
            c.player.frame = frame::STAND;
            c.player.x = a.x;
            c.hurt_player();
        }
        if (s.timer == 0xb && !s.caught) || s.timer == 0x1f {
            s.timer = 100;
            s.caught = false;
        }
    }
    if s.timer > 99 {
        s.timer += 1;
        if s.timer == 0x8c {
            s.timer = 0;
            s.caught = false;
        }
    }
}

/// The reaction to a hit for `damage`; false leaves it to the reactions by type
/// in [`super::hits`].
pub(super) fn hit(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    let Kind::Hazard(s) = &a.kind else {
        return false;
    };
    let mut s = *s;
    let handled = hit_by(a, c, &mut s, damage);
    a.kind = Kind::Hazard(s);
    handled
}

fn hit_by(a: &mut Actor, c: &mut Ctx, s: &mut State, damage: u8) -> bool {
    let (x, y) = (a.x, a.y);
    let lethal = a.health <= i16::from(damage);
    match s {
        // Shot, it goes off (the update does it).
        State::ForceFieldGenerator(g) => {
            if g.flash == 0 {
                g.flash = 1;
            }
            true
        }
        // Missiles tip over or ignite; which way depends on the shot.
        State::BrokenMissile(_) | State::LaunchableMissile(_) => {
            if !hits::hurt(a, c, damage) {
                return true;
            }
            let side = if hits::shot_from_left(c) { 1 } else { 2 };
            match s {
                State::BrokenMissile(m) => m.side = side,
                State::LaunchableMissile(m) => m.stage = side,
                _ => {}
            }
            c.particles(0x0f, Spread::Both, x + 5, y);
            true
        }
        // Not hurt while it clings to the player.
        State::Spider(_) => {
            if c.state.spiders.contains(slot(c)) || !hits::hurt(a, c, damage) {
                return true;
            }
            c.effect(EXPLOSION_START, x - 1, y + 1, Motion::Still, 0);
            c.add_score(100);
            c.remove(a);
            true
        }
        // Only special weapons destroy it, and it ducks back into the wall.
        State::LaserTurret(t) => {
            if t.rise != 0 {
                return true;
            }
            if c.player.weapon != Weapon::Regular || c.player.state == PlayerState::Ship {
                let path = match c.state.last_shot {
                    Some(ShotDirection::Left) => 3,
                    Some(ShotDirection::Right) => 1,
                    _ => 2,
                };
                c.effect(TURRET_DEBRIS, x, y, Motion::Debris(path), 0);
                c.add_score(499);
                c.remove(a);
                c.state.tally.laser_turrets_destroyed += 1;
            }
            c.effect(BLAST, x - 1, y + 2, Motion::Rise, 0);
            c.explosion_sound();
            t.rise = 0x28;
            c.add_score(1);
            true
        }
        // Killed while holding the player: lets him go.
        State::Snake(Snake { caught: true, .. })
        | State::CeilingSucker(CeilingSucker { caught: true, .. })
            if lethal =>
        {
            release_player(c);
            false
        }
        _ => false,
    }
}

/// The reaction to touching the player; false leaves it to [`super::contact`].
pub(super) fn touch(a: &mut Actor, c: &mut Ctx) -> bool {
    let Kind::Hazard(s) = &a.kind else {
        return false;
    };
    let mut s = *s;
    let handled = touch_by(a, c, &mut s);
    a.kind = Kind::Hazard(s);
    handled
}

fn touch_by(a: &mut Actor, c: &mut Ctx, s: &mut State) -> bool {
    let (px, py) = player_at(c);
    match s {
        // The force field that needs the cloaking device.
        State::ForceFieldGenerator(g) => {
            if c.player.cloak_timer != 0 {
                if g.destroy == 0 {
                    g.destroy = 1;
                }
                return true;
            }
            if g.flash == 0 {
                g.flash = 1;
            }
            // Pushed away sideways (the second test sees the first's move).
            if px + 2 <= a.x {
                c.player.x -= 1;
            }
            if c.player.x + 2 > a.x {
                c.player.x += 1;
            }
            c.hurt_player();
            c.state
                .messages
                .show_once(once::FORCE_FIELD, text::FORCE_FIELD);
            true
        }
        // The ship: the player climbs in by falling onto it.
        State::ParkedShip(ship) => {
            if c.player.state != PlayerState::Falling || ship.cooldown != 0 || a.fall != 0 {
                return true;
            }
            c.state.messages.show_once(once::SHIP, text::SHIP);
            c.player.enter_ship(a.x, a.y, ship_facing(a.id));
            c.sound(Sound::WeaponPickup);
            c.remove(a);
            true
        }
        // Caught by a ceiling sucker.
        State::CeilingSucker(sc) => {
            if c.player.state == PlayerState::Ship
                || a.frame != 5
                || sc.timer >= 10
                || px + 1 < a.x
                || a.x + 1 < px
            {
                return true;
            }
            c.state.grabber = slot(c);
            c.player.state = PlayerState::Grabbed;
            sc.caught = true;
            true
        }
        State::Spider(sp) => {
            c.hurt_player();
            // The original lets one register again on every frame it touches
            // the player, so it fills two places.
            if c.player.cloak_timer != 0 || sp.clinging {
                return true;
            }
            let places = c.state.spiders;
            let falling = a.fall != 0;
            if !(places.head.is_none() && falling)
                && ((places.face.is_some() && places.back.is_some()) || !sp.floor || a.frame >= 0xc)
            {
                return true;
            }
            let me = slot(c);
            if !falling {
                if places.face.is_some() {
                    c.state.spiders.back = me;
                } else {
                    c.state.spiders.face = me;
                }
            } else if places.head.is_none() {
                c.state.spiders.head = me;
            }
            a.health = 0;
            sp.clinging = true;
            a.flags.remove(ActorFlags::GRAVITY);
            a.fall = 0;
            true
        }
        // Hurts only while it comes down.
        State::CrusherPiston(cp) => {
            if cp.phase == Crusher::Extending {
                c.hurt_player();
            }
            true
        }
        // The eyeball thrower hurts above its knees.
        State::EyeballThrower(_) => {
            if a.y - 5 < py {
                c.hurt_player();
            }
            true
        }
        // Not while it is still in the wall.
        State::LaserTurret(t) => {
            if t.rise == 0 {
                c.hurt_player();
            }
            true
        }
        // Sets itself off when the player steps next to it.
        State::Snake(sn) => {
            if sn.caught || c.player.state != PlayerState::Standing || a.y != py {
                return true;
            }
            let (near, far) = if sn.right {
                (a.x + 2, a.x + 3)
            } else {
                (a.x - 2, a.x - 3)
            };
            if px == near || px == far {
                sn.caught = true;
            }
            true
        }
        _ => false,
    }
}
