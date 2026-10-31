//! The bosses of the four episodes.
//!
//! Each shows the health bar on the first frame of its fight (the original
//! also swaps the boss song for the level's; later: music). The hit code
//! keeps `boss_health` for the bar. A boss that runs out of health goes
//! through `death_sequence` and ends the level.

use super::shooters::{
    ROCKET_DOWN_BOSS, ROCKET_LEFT, ROCKET_RIGHT, ROCKET_UP_BOSS, TRAIL_BOMB, blocked,
    touches_player_at,
};
use super::{Actor, ActorFlags, Ctx, Kind, Update};
use crate::effects::{BLAST, EXPLOSION_START, Motion};
use crate::map::Dir;
use crate::particles::Spread;
use crate::state::LevelExit;
use crate::{Sound, SpriteLook};

const BOSS_EPISODE_1: u16 = 200;
const BOSS_PATH_FLYER: u16 = 101;
const BOSS_ROCKET: u16 = 265;
const BOSS_MINE_LAYER: u16 = 279;
const HOMING_DRONE: u16 = 280;

/// Points for killing a boss.
const KILL_SCORE: u16 = 50000;

/// The death counter values at which a boss explodes.
const EXPLOSIONS: [u16; 15] = [
    1, 3, 7, 0xe, 0x10, 0x15, 0x19, 0x1b, 0x1e, 0x20, 0x24, 0x28, 0x2b, 0x30, 0x32,
];
/// The counter value where the boss blows up for good and rises off, and
/// the one where the level ends.
const DEATH_BANG: u16 = 0x32;
const DEATH_END: u16 = 0x3c;

/// How the first boss bobs while it hovers.
const HOVER: [i32; 10] = [-1, -1, 0, 0, 1, 1, 1, 0, 0, -1];

/// The long path of the path flyer: dx, dy and frame (0 stands for the
/// flickering one) per step.
const LONG_PATH: [i8; 118] = [
    0, 1, 2, 0, 1, 2, 1, 2, 3, 1, 2, 3, 2, 1, 3, 2, 1, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0,
    3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2,
    0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3, 2, 0, 3,
    2, 0, 3, 2, -1, 3, 2, -1, 3, 1, -2, 3, 1, -2, 3, 0, -1, 3, 0, -1, 3, 0,
];
/// The short path.
const SHORT_PATH: [i8; 28] = [
    0, -2, 0, 0, -2, 0, 1, -2, 0, 2, -1, 0, 3, 0, 0, 2, 1, 0, 1, 2, 0, 0, 2, 0, 0, 2, 0, 0,
];
/// The path position where the long path ends and the short one's lap
/// (both in path entries, three to a step).
const LONG_END: usize = 0x75;
const SHORT_LAP: usize = 0x1b;
/// Frames the path flyer waits before its first flight.
const START_DELAY: usize = 0x1e;

/// What the first boss is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode1 {
    /// The first frame of the fight.
    Start,
    /// Dropping to the floor, twice before it dashes.
    Drop,
    /// Rising back to its height.
    Rise,
    /// Dashing left, then right (leaving a trail of bombs), then down and
    /// back left along the floor.
    DashLeft,
    DashRight,
    Fall,
    Return,
    /// Rising to its height, to hover.
    Climb,
    /// Bobbing from side to side until the player is under it.
    Hover,
}

/// What the path flyer is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flight {
    /// Waiting for the fight.
    Wait,
    /// Out along the long path.
    Out,
    /// Back along it.
    Back,
    /// Falling to the floor.
    Fall,
    /// Along the short path, eight laps there ...
    ShortOut,
    /// ... and eight back.
    ShortBack,
    /// Rising to its resting height.
    Rise,
}

/// The state of each update function of the family. `death` is 0 while the
/// boss lives; the hit code sets it when the boss runs out of health, and
/// `death_sequence` counts it on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Episode1 {
        mode: Mode1,
        /// Hovering: flying right.
        right: bool,
        /// Drops made (while dropping) or hover frames.
        count: u16,
        /// Its height when the fight started.
        home_y: i32,
        death: u16,
    },
    PathFlyer {
        flight: Flight,
        /// Position in the path.
        at: usize,
        /// Frames left to stand still. While this is 2 or more the boss
        /// does not hurt the player (the original's contact test reads it
        /// as a death flag too).
        pause: u16,
        laps: u16,
        /// 1 is the frame the boss was killed, which only starts it.
        death: u16,
    },
    Rocket {
        started: bool,
        death: u16,
    },
    MineLayer {
        started: bool,
        /// Frames, for the animation.
        anim: u16,
        /// Steps taken since it last stood still.
        steps: u16,
        /// Frames of walking since the last drone.
        timer: u16,
        /// Frames to wait after releasing a drone.
        cooldown: u16,
        death: u16,
    },
}

impl State {
    fn death(&mut self) -> &mut u16 {
        match self {
            Self::Episode1 { death, .. }
            | Self::PathFlyer { death, .. }
            | Self::Rocket { death, .. }
            | Self::MineLayer { death, .. } => death,
        }
    }
}

/// The starting state for `update` (from the spawn data's `vars`), if it
/// belongs to this family.
pub(super) fn new(update: Update, vars: [u16; 5]) -> Option<State> {
    let _ = vars;
    Some(match update {
        Update::BossEpisode1 => State::Episode1 {
            mode: Mode1::Start,
            right: false,
            count: 0,
            home_y: 0,
            death: 0,
        },
        Update::BossPathFlyer => State::PathFlyer {
            flight: Flight::Wait,
            at: 0,
            pause: 0,
            laps: 0,
            death: 0,
        },
        Update::BossRocket => State::Rocket {
            started: false,
            death: 0,
        },
        Update::BossMineLayer => State::MineLayer {
            started: false,
            anim: 0,
            steps: 0,
            timer: 0,
            cooldown: 0,
            death: 0,
        },
        _ => return None,
    })
}

/// The start of a boss fight: the health bar shows.
/// later: the level's song plays in place of the boss song.
fn start_fight(c: &mut Ctx) {
    c.state.boss_bar = true;
}

/// A byte of the level's random numbers masked to `mask`, as an offset.
fn rand(c: &mut Ctx, mask: u8) -> i32 {
    i32::from(c.random.next_byte() & mask)
}

/// One frame of a boss's death, `n` counting up from 2. 50000 points at 2;
/// explosions at the frames of `EXPLOSIONS`; the boss blinks and flashes the
/// screen until a big bang at 50, after which it flies off the top; the level
/// ends at 60. Returns the new `n`.
fn death_sequence(a: &mut Actor, c: &mut Ctx, n: u16) -> u16 {
    if n == 2 {
        // later: the music stops.
        c.add_score(KILL_SCORE);
        a.flags.remove(ActorFlags::GRAVITY);
    }
    if n == DEATH_BANG {
        c.state.flash = Some(1);
        c.sound(Sound::BigExplosion);
    }
    if n > DEATH_BANG && a.y > 3 {
        a.y -= 2;
    }
    if n == DEATH_END {
        c.state.exit = Some(LevelExit::BossKilled);
        return n;
    }
    if EXPLOSIONS.contains(&n) {
        c.explosion_sound();
        let (x, y) = (a.x + rand(c, 3), a.y - rand(c, 7));
        let spread = if c.random.next_byte() & 1 == 0 {
            Spread::Left
        } else {
            Spread::Both
        };
        let color = c.random.next_byte() & 0x0f;
        c.particles(color, spread, x, y);
        let (x, y) = (a.x + rand(c, 3), a.y - rand(c, 7));
        c.effect(EXPLOSION_START, x, y, Motion::Still, 0);
        let (x, y) = (a.x + rand(c, 3), a.y - rand(c, 7));
        c.effect(BLAST, x, y, Motion::Debris(5), 0);
    }
    if n < DEATH_BANG {
        if c.draw_page {
            a.look = SpriteLook::Hidden;
        }
        if c.random.next_byte() & 4 != 0 && c.draw_page {
            c.state.flash = Some(1);
            c.sound(Sound::BigExplosion);
        } else {
            c.explosion_sound();
        }
    }
    n + 1
}

/// The actor's update function.
pub(super) fn update(a: &mut Actor, c: &mut Ctx) {
    let Kind::Boss(mut s) = a.kind.clone() else {
        return;
    };
    match &mut s {
        State::Episode1 {
            mode,
            right,
            count,
            home_y,
            death,
        } => episode1(a, c, mode, right, count, home_y, death),
        State::PathFlyer {
            flight,
            at,
            pause,
            laps,
            death,
        } => path_flyer(a, c, flight, at, pause, laps, death),
        State::Rocket { started, death } => rocket(a, c, started, death),
        State::MineLayer {
            started,
            anim,
            steps,
            timer,
            cooldown,
            death,
        } => mine_layer(a, c, started, anim, steps, timer, cooldown, death),
    }
    a.kind = Kind::Boss(s);
}

/// Drops to the floor (landing with a thud), rises again and dashes left and
/// right, leaving bombs on the way; after a second drop it hovers, bobbing to
/// and fro, until the player is under it for the next drop. Its own frames are
/// 2 (alive) and 3 (dead, blinking); frame 0 and 1 of the actor itself flicker.
fn episode1(
    a: &mut Actor,
    c: &mut Ctx,
    mode: &mut Mode1,
    right: &mut bool,
    count: &mut u16,
    home_y: &mut i32,
    death: &mut u16,
) {
    const ID: u16 = BOSS_EPISODE_1;
    a.frame = u16::from(c.draw_page);
    if *death == 0 {
        match *mode {
            Mode1::Start => {
                *mode = Mode1::DashLeft;
                *home_y = a.y;
                *count = 0;
                start_fight(c);
            }
            Mode1::Drop => {
                if *count == 2 {
                    *count = 0;
                    *mode = Mode1::DashLeft;
                    a.flags.remove(ActorFlags::GRAVITY);
                } else {
                    if !a.flags.contains(ActorFlags::GRAVITY) {
                        a.flags.insert(ActorFlags::GRAVITY);
                        a.fall = 0;
                        *count += 1;
                    }
                    if blocked(c, ID, a.x, a.y + 1, Dir::Down) {
                        a.flags.remove(ActorFlags::GRAVITY);
                        *mode = Mode1::Rise;
                        c.sound(Sound::Thud);
                    }
                }
            }
            Mode1::Rise | Mode1::Climb => {
                if a.y <= *home_y {
                    if *mode == Mode1::Rise {
                        *mode = Mode1::DashLeft;
                    } else {
                        *mode = Mode1::Hover;
                        a.y -= 1;
                    }
                } else {
                    a.y -= 1;
                }
            }
            Mode1::DashLeft => {
                a.x -= 2;
                if blocked(c, ID, a.x - 2, a.y, Dir::Left) {
                    *mode = Mode1::DashRight;
                }
            }
            Mode1::DashRight => {
                if c.draw_page {
                    c.spawn(TRAIL_BOMB, a.x + 3, a.y + 1);
                }
                a.x += 2;
                if blocked(c, ID, a.x + 2, a.y, Dir::Right) {
                    *mode = Mode1::Fall;
                }
            }
            Mode1::Fall => {
                *mode = Mode1::Return;
                a.flags.insert(ActorFlags::GRAVITY);
                a.fall = 0;
            }
            Mode1::Return => {
                if blocked(c, ID, a.x, a.y + 1, Dir::Down) {
                    a.flags.remove(ActorFlags::GRAVITY);
                }
                if !a.flags.contains(ActorFlags::GRAVITY) {
                    a.x -= 2;
                    if blocked(c, ID, a.x - 2, a.y, Dir::Left) {
                        *mode = Mode1::Climb;
                    }
                }
            }
            Mode1::Hover => {
                if !*right {
                    if blocked(c, ID, a.x - 1, a.y, Dir::Left) {
                        *right = true;
                    } else {
                        a.x -= 1;
                    }
                } else if blocked(c, ID, a.x + 1, a.y, Dir::Right) {
                    *right = false;
                } else {
                    a.x += 1;
                }
                let step = *count;
                *count += 1;
                a.y += HOVER[usize::from(step % 10)];
                let px = c.player.x;
                if *count > 0x32 && a.x - 1 <= px && px <= a.x + 9 {
                    *count = 0;
                    *mode = Mode1::Drop;
                }
            }
        }
    } else {
        *death = death_sequence(a, c, *death);
    }
    let mut draw = true;
    let frame = if *death < 3 {
        2
    } else {
        // Dead: it blinks.
        if c.draw_page {
            draw = false;
        }
        3
    };
    if draw {
        c.draw(ID, frame, a.x, a.y, SpriteLook::Normal);
    }
}

/// Waits, then flies out along the long path, back, falls to the floor, flies
/// eight laps of the short path out and back and rises to its resting height,
/// which is the actor's `score` (set to its row at the start; being killed also
/// scores that).
fn path_flyer(
    a: &mut Actor,
    c: &mut Ctx,
    flight: &mut Flight,
    at: &mut usize,
    pause: &mut u16,
    laps: &mut u16,
    death: &mut u16,
) {
    let page = u16::from(c.draw_page);
    // A path frame of 0 stands for the flickering pair.
    let frame = |f: i8| if f == 0 { page } else { f as u16 };
    if *death != 0 {
        if *death == 1 {
            *death = 2;
        } else {
            *death = death_sequence(a, c, *death);
        }
        return;
    }
    if *pause != 0 {
        *pause -= 1;
        a.frame = page;
        return;
    }
    let step = |path: &[i8], at: usize| {
        (
            i32::from(path[at]),
            i32::from(path[at + 1]),
            frame(path[at + 2]),
        )
    };
    match *flight {
        Flight::Wait => {
            a.frame = page;
            let waited = *at;
            *at += 1;
            if waited != START_DELAY {
                return;
            }
            *flight = Flight::Out;
            start_fight(c);
        }
        Flight::Out => {
            let (dx, dy, f) = step(&LONG_PATH, *at);
            (a.x, a.y, a.frame) = (a.x + dx, a.y + dy, f);
            *at += 3;
            if *at != LONG_END {
                return;
            }
            *pause = 0x19;
            *flight = Flight::Back;
            return;
        }
        Flight::Back => {
            *at -= 3;
            let (dx, dy, f) = step(&LONG_PATH, *at);
            (a.x, a.y, a.frame) = (a.x - dx, a.y - dy, f);
            if *at != 0 {
                return;
            }
            *flight = Flight::Fall;
            a.fall = 0;
            *pause = 0x19;
            return;
        }
        Flight::Fall => {
            a.flags.insert(ActorFlags::GRAVITY);
            if !blocked(c, BOSS_PATH_FLYER, a.x, a.y + 1, Dir::Down) {
                return;
            }
            *flight = Flight::ShortOut;
            *at = 0;
            *pause = 0x1e;
            *laps = 0;
            a.flags.remove(ActorFlags::GRAVITY);
            return;
        }
        Flight::ShortOut => {
            let (dx, dy, f) = step(&SHORT_PATH, *at);
            (a.x, a.y, a.frame) = (a.x + dx, a.y + dy, f);
            *at += 3;
            if *at == SHORT_LAP {
                *laps += 1;
                *at = 0;
            }
            if *laps != 8 {
                return;
            }
            *laps = 0;
            *at = SHORT_LAP;
            *flight = Flight::ShortBack;
            return;
        }
        Flight::ShortBack => {
            *at -= 3;
            let (dx, dy, f) = step(&SHORT_PATH, *at);
            (a.x, a.y, a.frame) = (a.x - dx, a.y - dy, f);
            if *at == 0 {
                *laps += 1;
                *at = SHORT_LAP;
            }
            if *laps != 8 {
                return;
            }
            *flight = Flight::Rise;
            return;
        }
        Flight::Rise => {
            a.y -= 1;
            if a.y != i32::from(a.score) {
                return;
            }
            *pause = 100;
            *flight = Flight::Out;
        }
    }
    *at = 0;
}

/// A step in x towards `tx - 4` on half of the frames (at random), in y towards
/// `ty + 4` on odd ones.
fn move_toward_jittery(a: &mut Actor, c: &mut Ctx, tx: i32, ty: i32) {
    if c.random.next_byte() & 1 != 0 {
        a.x += (tx - a.x - 4).signum();
    }
    if c.draw_page {
        a.y += (ty - a.y + 4).signum();
    }
}

/// Flies at the player and, on odd frames and half the time at random, fires a
/// rocket at him if he is within one of the four sides.
fn rocket(a: &mut Actor, c: &mut Ctx, started: &mut bool, death: &mut u16) {
    if *death > 1 {
        *death = death_sequence(a, c, *death);
        return;
    }
    if !*started {
        *started = true;
        start_fight(c);
    }
    let (px, py) = (c.player.x, c.player.y);
    move_toward_jittery(a, c, px + 3, py - 1);
    c.draw(
        BOSS_ROCKET,
        u16::from(c.draw_page) + 1,
        a.x,
        a.y,
        SpriteLook::Normal,
    );
    if !c.on_screen(a.placed()) || !c.draw_page {
        return;
    }
    if c.random.next_byte() & 1 == 0 {
        return;
    }
    let (x, y) = (a.x, a.y);
    let (id, rx, ry) = if touches_player_at(a, c, -9, 0) {
        (ROCKET_LEFT, x - 4, y - 4)
    } else if touches_player_at(a, c, 9, 0) {
        (ROCKET_RIGHT, x + 8, y - 4)
    } else if touches_player_at(a, c, 0, -9) {
        (ROCKET_UP_BOSS, x + 4, y - 8)
    } else if touches_player_at(a, c, 0, 9) {
        (ROCKET_DOWN_BOSS, x + 4, y + 3)
    } else {
        return;
    };
    c.spawn(id, rx, ry);
}

/// Follows the player, and after walking a while releases a homing drone, then
/// waits 12 frames.
#[allow(clippy::too_many_arguments)]
fn mine_layer(
    a: &mut Actor,
    c: &mut Ctx,
    started: &mut bool,
    anim: &mut u16,
    steps: &mut u16,
    timer: &mut u16,
    cooldown: &mut u16,
    death: &mut u16,
) {
    if !*started {
        *started = true;
        start_fight(c);
    }
    if *death > 1 {
        *death = death_sequence(a, c, *death);
        a.look = SpriteLook::Hidden;
        return;
    }
    *anim = anim.wrapping_add(1);
    let (px, py) = (c.player.x, c.player.y);
    if !(*cooldown != 0 || *steps > 0xd || !c.draw_page) {
        *steps = 0;
        if px < a.x + 4 {
            a.x -= 1;
            *steps += 1;
        } else if a.x + 4 < px {
            a.x += 1;
            *steps += 1;
        }
        if py < a.y + 4 {
            a.y -= 1;
            *steps += 1;
        } else if py > a.y + 4 {
            a.y += 1;
            *steps += 1;
        }
    }
    c.draw(
        BOSS_MINE_LAYER,
        (*anim & 3) + 1,
        a.x,
        a.y,
        SpriteLook::Normal,
    );
    if *cooldown == 0 {
        if *steps != 0 {
            *timer += 1;
            if *timer > 0xc {
                *steps = 0;
                c.spawn(HOMING_DRONE, a.x + 4, a.y + 2);
                *timer = 0;
                *cooldown = 0xc;
            }
        }
    } else {
        *cooldown -= 1;
    }
}

/// The reaction to a hit for `damage`: the boss loses health, which the bar
/// shows. Out of health it starts dying (its update runs `death_sequence`; the
/// path flyer starts a frame later).
pub(super) fn hit(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    let killed = super::hits::hurt(a, c, damage);
    if killed && let Kind::Boss(s) = &mut a.kind {
        *s.death() = if a.id == BOSS_PATH_FLYER { 1 } else { 2 };
    }
    // The rocket boss has four times the health, for the same bar.
    let health = if a.id == BOSS_ROCKET {
        a.health / 4
    } else {
        a.health
    };
    c.state.boss_health = health.max(0) as u16;
    c.state.boss_bar = true;
    true
}

/// The reaction to touching the player: a living boss hurts him.
pub(super) fn touch(a: &mut Actor, c: &mut Ctx) -> bool {
    let Kind::Boss(s) = &a.kind else {
        return false;
    };
    let hurts = match s {
        State::Episode1 { death, .. }
        | State::Rocket { death, .. }
        | State::MineLayer { death, .. } => *death < 2,
        // The original tests the pause here, which doubles as its death
        // counter.
        State::PathFlyer { pause, death, .. } => *death < 2 && *pause < 2,
    };
    if hurts {
        c.hurt_player();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_end_where_the_flyer_expects() {
        assert!(LONG_END <= LONG_PATH.len());
        assert!(SHORT_LAP <= SHORT_PATH.len());
        assert_eq!(LONG_END % 3, 0);
    }

    #[test]
    fn spawn_data_gives_each_boss_its_state() {
        for (update, level_type) in [
            (Update::BossEpisode1, 200),
            (Update::BossPathFlyer, 101),
            (Update::BossRocket, 265),
            (Update::BossMineLayer, 279),
        ] {
            assert!(new(update, [0; 5]).is_some());
            assert_eq!(
                crate::actors::spawn::lookup(level_type).unwrap().update,
                update
            );
        }
    }
}
