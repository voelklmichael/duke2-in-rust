//! Things that shoot or drop things: turrets, rockets, bomber ship,
//! bouncing ball, reactor, slime and watchbot containers.

use dn2_assets::level::LevelFlags;

use super::{Actor, ActorFlags, Ctx, Kind, Update};
use crate::draw::Placed;
use crate::effects::{BLAST, EXPLOSION_START, Motion};
use crate::map::{Dir, Hit};
use crate::particles::Spread;
use crate::player::Facing;
use crate::shots::{self, REACTOR_BLASTS, ShotDirection, ShotKind, ShotRequest};
use crate::{Sound, SpriteLook};

/// The rockets a turret fires, by direction; the boss uses the same, and a
/// fourth one that flies down.
pub(super) const ROCKET_LEFT: u16 = 55;
const ROCKET_UP: u16 = 56;
pub(super) const ROCKET_RIGHT: u16 = 57;
pub(super) const ROCKET_UP_BOSS: u16 = 256;
pub(super) const ROCKET_DOWN_BOSS: u16 = 259;
const CARRIER: u16 = 58;
const CONTAINER: u16 = 59;
const WATCHBOT: u16 = 49;
const BOMBER: u16 = 62;
const BOMB: u16 = 63;
/// The bomb of the first boss's trail: it does not set the ground on fire.
pub(super) const TRAIL_BOMB: u16 = 76;
const BOUNCING_BALL: u16 = 64;
const REACTOR: u16 = 66;
const SLIME_BLOB: u16 = 67;
const SLIME_CONTAINER: u16 = 68;

/// Effects.
const FLAME: u16 = 0x2b;
const CARRIER_BLAST_AREA: u16 = CARRIER;
const CONTAINER_HALF_LEFT: u16 = 0x3c;
const CONTAINER_HALF_RIGHT: u16 = 0x3d;
const REACTOR_GLOW: u16 = 0x4a;
const REACTOR_FLASH: u16 = 0x42;
const REACTOR_SPARK: u16 = 0x54;

/// Frames a rocket turret waits between shots.
const TURRET_RELOAD: u16 = 0x19;
/// The frame of the carrier's life when its container is released, and
/// when it blows up.
const CARRIER_RELEASE: u16 = 7;
const CARRIER_BLOWS_UP: u16 = 0x23;
/// Tiles a container rises; the frame it splits open.
const CONTAINER_RISE: u16 = 10;
const CONTAINER_OPENS: u16 = 0x19;
/// The bomber's frames: the bomb drops, it flies off.
const BOMBER_DROPS: u16 = 10;
const BOMBER_LEAVES: u16 = 0x1e;

/// Which way a bouncing ball rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Roll {
    /// On the spot, until shot.
    Still,
    Left,
    Right,
}

/// The state of each update function of the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    RocketTurret {
        /// Frames since it turned; 0 is the moment to turn to the player.
        timer: u16,
    },
    EnemyRocket {
        /// Frames flown.
        age: u16,
    },
    Carrier {
        /// 0 while it drives to the player, then the frames since, up to
        /// `CARRIER_BLOWS_UP`.
        timer: u16,
        /// The container has been let go (it is drawn on top until then).
        released: bool,
    },
    Container {
        /// Frames since it was released.
        age: u16,
    },
    Bomber {
        /// 0 while it flies to the player, then the frames since.
        timer: u16,
    },
    /// The fall is the actor's.
    Bomb,
    BouncingBall {
        roll: Roll,
        /// Steps through a jump: 0 to 4 rising, 5 to 7 hanging, 8 falling.
        jump: u16,
    },
    Reactor,
    SlimeContainer {
        /// 0 while whole; once shot, the frames of its breaking (1 to 3).
        breaking: u16,
    },
}

/// The starting state for `update` (from the spawn data's `vars`), if it
/// belongs to this family.
pub(super) fn new(update: Update, vars: [u16; 5]) -> Option<State> {
    let _ = vars;
    Some(match update {
        Update::RocketTurret => State::RocketTurret { timer: 0 },
        Update::EnemyRocket => State::EnemyRocket { age: 0 },
        Update::WatchbotContainerCarrier => State::Carrier {
            timer: 0,
            released: false,
        },
        Update::WatchbotContainer => State::Container { age: 0 },
        Update::BomberShip => State::Bomber { timer: 0 },
        Update::FallingBomb => State::Bomb,
        Update::BouncingBall => State::BouncingBall {
            roll: Roll::Still,
            jump: 0,
        },
        Update::Reactor => State::Reactor,
        Update::SlimeContainer => State::SlimeContainer { breaking: 0 },
        _ => return None,
    })
}

/// Whether the actor's sprite frame is clear on `dir` at `x`, `y`; the
/// sprite is the one of `id`, not necessarily the actor's.
pub(super) fn blocked(c: &Ctx, id: u16, x: i32, y: i32, dir: Dir) -> bool {
    c.collide(id, 0, x, y, dir) != Hit::Free
}

fn sound_if_visible(a: &Actor, c: &mut Ctx, sound: Sound) {
    if c.on_screen(a.placed()) {
        c.sound(sound);
    }
}

/// Whether the middle of the actor's sprite is at most `dist` tiles from the
/// player horizontally.
fn player_near_x(a: &Actor, c: &Ctx, dist: i32) -> bool {
    let width = c
        .sprites
        .frame_box(a.id, a.frame)
        .map_or(0, |f| i32::from(f.width));
    let player = c.player.x
        + if c.player.facing == Facing::Left {
            2
        } else {
            1
        };
    (a.x + (width >> 1) - player).abs() <= dist
}

/// The actor's update function.
pub(super) fn update(a: &mut Actor, c: &mut Ctx) {
    let Kind::Shooter(mut s) = a.kind.clone() else {
        return;
    };
    run(&mut s, a, c);
    a.kind = Kind::Shooter(s);
}

fn run(s: &mut State, a: &mut Actor, c: &mut Ctx) {
    match s {
        State::RocketTurret { timer } => rocket_turret(a, c, timer),
        State::EnemyRocket { age } => enemy_rocket(a, c, age),
        State::Carrier { timer, released } => carrier(a, c, timer, released),
        State::Container { age } => container(a, c, age),
        State::Bomber { timer } => bomber(a, c, timer),
        State::Bomb => bomb(a, c),
        State::BouncingBall { roll, jump } => bouncing_ball(a, c, roll, jump),
        State::Reactor => {
            // Cycles its 4 frames.
            a.frame = (a.frame + 1) % 4;
        }
        State::SlimeContainer { breaking } => slime_container(a, c, breaking),
    }
}

/// Turns to the player (frame 0 left, 1 up, 2 right) and, 25 frames later,
/// fires a rocket that way.
fn rocket_turret(a: &mut Actor, c: &mut Ctx, timer: &mut u16) {
    if *timer == 0 {
        let (px, py) = (c.player.x, c.player.y);
        if px < a.x - 3 {
            a.frame = 0;
        } else if a.x + 3 < px {
            a.frame = 2;
        } else if py < a.y {
            a.frame = 1;
        }
        *timer = 1;
    } else if *timer < TURRET_RELOAD {
        *timer += 1;
        return;
    }
    if *timer != TURRET_RELOAD {
        return;
    }
    match a.frame {
        0 => c.spawn(ROCKET_LEFT, a.x - 2, a.y - 1),
        1 => c.spawn(ROCKET_UP, a.x + 1, a.y - 2),
        2 => c.spawn(ROCKET_RIGHT, a.x + 2, a.y - 1),
        _ => {}
    }
    *timer = 0;
}

/// The rockets (left, up, right, and the boss's up and down) fly a tile a
/// frame, two after the fourth, until they hit something or leave the view;
/// then they explode. The one flying down checks for walls above it, as in the
/// original.
fn enemy_rocket(a: &mut Actor, c: &mut Ctx, age: &mut u16) {
    if !c.on_screen(a.placed()) {
        c.remove(a);
        return;
    }
    *age += 1;
    if *age == 1 {
        c.sound(Sound::Roar);
    }
    let step = if *age > 4 { 2 } else { 1 };
    let (dir, dx, dy) = match a.id {
        ROCKET_LEFT => (Dir::Left, -step, 0),
        ROCKET_RIGHT => (Dir::Right, step, 0),
        ROCKET_UP | ROCKET_UP_BOSS => (Dir::Up, 0, -step),
        ROCKET_DOWN_BOSS => (Dir::Up, 0, step),
        _ => (Dir::Up, 0, 0),
    };
    if dx != 0 || dy != 0 {
        a.x += dx;
        a.y += dy;
        c.draw(
            a.id,
            u16::from(c.draw_page) + 1,
            a.x,
            a.y,
            SpriteLook::Normal,
        );
        if c.move_blocked(a, dir) {
            c.remove(a);
        }
    }
    if a.removed {
        c.effect(EXPLOSION_START, a.x, a.y, Motion::Still, 0);
    }
    if !c.on_screen(a.placed()) {
        c.remove(a);
    }
}

/// Drives towards the player; then it holds still, lets go of its container at
/// frame 7 and blows up at frame 35.
fn carrier(a: &mut Actor, c: &mut Ctx, timer: &mut u16, released: &mut bool) {
    if *timer == 0 {
        a.frame = 0;
        let mut done = player_near_x(a, c, 5);
        if !done {
            let px = c.player.x;
            done = if px < a.x {
                a.x -= 1;
                c.move_blocked(a, Dir::Left)
            } else if px <= a.x + 3 {
                false
            } else {
                a.x += 1;
                c.move_blocked(a, Dir::Right)
            };
        }
        if done {
            *timer = 1;
        }
    }
    if *timer != 0 {
        if *timer < CARRIER_BLOWS_UP {
            *timer += 1;
        }
        if *timer == CARRIER_RELEASE {
            a.frame = 1;
            *released = true;
            c.spawn(CONTAINER, a.x, a.y - 2);
        } else if *timer < 0x15 || *timer > 0x22 {
            if *timer == CARRIER_BLOWS_UP {
                c.remove(a);
                c.explosion_sound();
                c.cloud(CARRIER_BLAST_AREA, a.x, a.y, BLAST);
            }
        } else {
            a.frame = 0;
        }
    }
    if !*released {
        c.draw(CONTAINER, 0, a.x, a.y - 2, SpriteLook::Normal);
    }
}

/// Rises up to 10 tiles, and 25 frames after it was let go splits into two
/// halves and releases a watchbot.
fn container(a: &mut Actor, c: &mut Ctx, age: &mut u16) {
    a.frame += 1;
    if a.frame == 6 {
        a.frame = 1;
    }
    if *age < CONTAINER_RISE && c.collide(CONTAINER, 0, a.x, a.y - 1, Dir::Up) == Hit::Free {
        a.y -= 1;
    }
    *age += 1;
    if *age == CONTAINER_OPENS {
        a.look = SpriteLook::WhiteFlash;
        c.remove(a);
        c.effect(CONTAINER_HALF_LEFT, a.x, a.y, Motion::Debris(4), 0);
        c.effect(CONTAINER_HALF_RIGHT, a.x, a.y, Motion::Debris(0), 0);
        c.sound(Sound::Clank);
        c.spawn(WATCHBOT, a.x + 1, a.y + 3);
    } else {
        c.draw(CONTAINER, 0, a.x, a.y, a.look);
    }
}

/// Flies left until it is over the player (or blocked), then drops a bomb at
/// frame 10 and flies off up and left at frame 30.
fn bomber(a: &mut Actor, c: &mut Ctx, timer: &mut u16) {
    if *timer == 0 {
        a.x -= 1;
        let px = c.player.x;
        if c.move_blocked(a, Dir::Left) || (a.x <= px && px <= a.x + 6) {
            *timer = 1;
        }
    } else {
        if *timer < BOMBER_LEAVES {
            *timer += 1;
        }
        if *timer == BOMBER_DROPS {
            c.spawn(BOMB, a.x + 2, a.y + 1);
        }
        if *timer == BOMBER_LEAVES {
            a.y -= 1;
            a.x -= 2;
            if !c.on_screen(a.placed()) {
                c.remove(a);
                return;
            }
        }
    }
    // The bomb hangs under it until it drops; the ship's own pictures are
    // frames 1 and 2.
    if *timer < BOMBER_DROPS {
        c.draw(BOMB, 0, a.x + 2, a.y, SpriteLook::Normal);
    }
    c.draw(
        BOMBER,
        u16::from(c.draw_page) + 1,
        a.x,
        a.y,
        SpriteLook::Normal,
    );
}

/// Falls with the actor loop's gravity and explodes on landing; the bomb also
/// sets fire along the ground to both sides.
fn bomb(a: &mut Actor, c: &mut Ctx) {
    if a.fall == 1 || a.fall == 3 {
        a.frame += 1;
    } else if a.fall == 0 {
        c.remove(a);
        c.sound(Sound::BigExplosion);
        c.effect(FLAME, a.x, a.y, Motion::Still, 0);
        c.state.flash = Some(1);
        if a.id != TRAIL_BOMB {
            for n in [4, 8, 12, 16] {
                let delay = (n / 2) as u16;
                c.effect(FLAME, a.x - n, a.y + 2, Motion::Still, delay);
                c.effect(FLAME, a.x + n, a.y + 2, Motion::Still, delay);
            }
        }
    }
}

/// Bounces through a jump (rising 5 steps, hanging, falling by gravity). Once
/// shot from a side it also rolls that way, turning at walls.
fn bouncing_ball(a: &mut Actor, c: &mut Ctx, roll: &mut Roll, jump: &mut u16) {
    match *roll {
        Roll::Right => {
            a.x += 1;
            if c.move_blocked(a, Dir::Right) {
                *roll = Roll::Left;
            }
        }
        Roll::Left => {
            a.x -= 1;
            if c.move_blocked(a, Dir::Left) {
                *roll = Roll::Right;
            }
        }
        Roll::Still => {}
    }
    if *jump < 5 {
        a.y -= 1;
        if c.move_blocked(a, Dir::Up) {
            *jump = 5;
            sound_if_visible(a, c, Sound::Jump);
        }
        if *jump < 2 {
            a.y -= 1;
            if c.move_blocked(a, Dir::Up) {
                *jump = 5;
                sound_if_visible(a, c, Sound::Jump);
            }
        }
        *jump += 1;
        if *jump > 4 {
            a.flags.insert(ActorFlags::GRAVITY);
            a.fall = 0;
            return;
        }
    }
    if *jump < 5 || *jump > 7 {
        if a.fall == 0 && *jump == 8 {
            *jump = 0;
            a.flags.remove(ActorFlags::GRAVITY);
            sound_if_visible(a, c, Sound::Jump);
        }
    } else {
        *jump += 1;
    }
}

/// A glass tube of slime. Once shot it breaks over 7 frames and lets a slime
/// blob out.
fn slime_container(a: &mut Actor, c: &mut Ctx, breaking: &mut u16) {
    c.draw(SLIME_CONTAINER, 8, a.x, a.y, SpriteLook::Normal);
    if a.frame == 7 {
        return;
    }
    if *breaking == 0 || a.frame > 6 {
        c.draw(SLIME_CONTAINER, 2, a.x, a.y, SpriteLook::Normal);
        a.frame = u16::from(c.random.next_byte() & 1);
    } else {
        *breaking += 1;
        if *breaking == 4 {
            *breaking = 1;
            a.frame += 1;
        }
        if a.frame == 7 {
            c.spawn(SLIME_BLOB, a.x + 2, a.y);
        }
    }
}

/// The reaction to a hit for `damage`; false leaves it to the reactions by type
/// in [`super::hits`].
pub(super) fn hit(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    let (x, y) = (a.x, a.y);
    match a.id {
        // The bouncing ball rolls the way the shot flew; it explodes when
        // it is out of health.
        BOUNCING_BALL => {
            if let Kind::Shooter(State::BouncingBall { roll, .. }) = &mut a.kind {
                match c.state.last_shot {
                    Some(ShotDirection::Left) => *roll = Roll::Left,
                    Some(ShotDirection::Right) => *roll = Roll::Right,
                    _ => {}
                }
            }
            if !super::hits::hurt(a, c, damage) {
                return true;
            }
            c.particles(0x0f, Spread::Both, x + 1, y);
            c.effect(EXPLOSION_START, x - 1, y + 1, Motion::Still, 0);
            c.remove(a);
            true
        }
        // The reactor blows up in a long chain of explosions.
        REACTOR => {
            if !super::hits::hurt(a, c, damage) {
                return true;
            }
            c.state.flash = Some(2);
            for (blast, direction) in REACTOR_BLASTS
                .into_iter()
                .zip([ShotDirection::Left, ShotDirection::Right])
            {
                c.sound(Sound::Shot);
                let request = ShotRequest {
                    kind: ShotKind::Blast(blast),
                    x,
                    y,
                    direction,
                };
                shots::spawn(c.shots, c.effects, c.map, c.sprites, &request);
            }
            c.cloud(REACTOR, x, y, REACTOR_GLOW);
            c.effect(REACTOR_FLASH, x, y, Motion::Still, 0);
            for u in 0..12i32 {
                let motion = Motion::Debris(if u & 2 == 0 { 0 } else { 4 });
                let row = y - 9 + u;
                c.effect(BLAST, x + 1, row, motion, (u * 3) as u16);
                c.effect(REACTOR_SPARK, x - 1, row, Motion::Still, (u * 2) as u16);
            }
            c.explosion_sound();
            // The level's backdrop changes for good.
            if c.map.level.flags.contains(LevelFlags::BACKDROP_SWITCH_ONCE) {
                c.state.backdrop_alternate = true;
            }
            c.remove(a);
            true
        }
        // The slime container breaks open (see `slime_container`).
        SLIME_CONTAINER => {
            if !super::hits::hurt(a, c, damage) {
                return true;
            }
            if let Kind::Shooter(State::SlimeContainer { breaking }) = &mut a.kind {
                *breaking = 1;
            }
            a.frame = 2;
            c.sound(Sound::Break);
            a.look = SpriteLook::WhiteFlash;
            c.particles(0x0f, Spread::Both, x + 1, y);
            true
        }
        _ => false,
    }
}

/// The reaction to touching the player; false leaves it to [`super::contact`].
pub(super) fn touch(a: &mut Actor, c: &mut Ctx) -> bool {
    if a.id != REACTOR {
        return false;
    }
    // Kills even a player who is almost dead and cannot be hurt.
    c.player.health = 1;
    c.player.hurt_timer = 0;
    c.player.cloak_timer = 0;
    c.hurt_player();
    true
}

/// Whether the sprite `id` at the actor's place, moved by `dx`, `dy`,
/// touches the player: the boss's aiming (see `bosses`).
pub(super) fn touches_player_at(a: &Actor, c: &Ctx, dx: i32, dy: i32) -> bool {
    c.touches_player(Placed {
        x: a.x + dx,
        y: a.y + dy,
        ..a.placed()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ball_starts_still() {
        assert_eq!(
            new(Update::BouncingBall, [0; 5]),
            Some(State::BouncingBall {
                roll: Roll::Still,
                jump: 0
            })
        );
        assert_eq!(new(Update::Hoverbot, [0; 5]), None);
    }

    #[test]
    fn ids_match_the_table() {
        // The turret fires the rocket types the table has.
        for id in [ROCKET_LEFT, ROCKET_UP, ROCKET_RIGHT] {
            let s = crate::actors::spawn::lookup(id).unwrap();
            assert_eq!(s.update, Update::EnemyRocket);
        }
        let s = crate::actors::spawn::lookup(54).unwrap();
        assert_eq!(s.update, Update::RocketTurret);
    }
}
