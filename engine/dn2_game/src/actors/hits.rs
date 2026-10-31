//! Actors being shot.
//!
//! Each frame, an actor in view with health looks for burning fire or a
//! player's shot over it; then it reacts by the actor's type: most lose health
//! and, when it runs out, explode for their score.

use super::contact::{FIREWORK, SPARKLE_100, SPARKLE_500, SPARKLE_2000, sparkle};
use super::items::{BIG_HEALTH, FUSE_ITEM, SODA, spread_fire};
use super::{Actor, Ctx, FIRE, Kind};
use crate::draw::{Placed, overlap};
use crate::effects::{BLAST, EXPLOSION, EXPLOSION_START, Motion};
use crate::particles::Spread;
use crate::player::Weapon;
use crate::shots::{ShotDirection, ShotKind};
use crate::{Sound, SpriteLook};

/// Effects some deaths leave around the actor: dx, dy, debris path, delay.
const EYEBALL_DEATH: [(i32, i32, u8, u16); 5] = [
    (0, -6, 2, 0),
    (0, -5, 4, 1),
    (0, -4, 0, 0),
    (0, -3, 3, 1),
    (0, -1, 2, 0),
];
const WALKER_DEATH: [(i32, i32, u8, u16); 6] = [
    (1, 2, 2, 0),
    (0, 0, 1, 1),
    (-1, 1, 3, 2),
    (1, -1, 5, 3),
    (-1, 2, 1, 4),
    (1, 2, 3, 5),
];
/// Explosions (`Motion::Still`) around a destroyed robot or turret.
pub(super) const ROBOT_DEATH: [(i32, i32, u16); 3] = [(0, 0, 0), (-1, -2, 2), (1, -3, 4)];

/// The explosions around a destroyed robot or turret at `x`, `y`.
pub(super) fn robot_death(c: &mut Ctx, x: i32, y: i32) {
    for (dx, dy, delay) in ROBOT_DEATH {
        c.effect(EXPLOSION_START, x + dx, y + dy, Motion::Still, delay);
    }
}

/// The damage actor `a` takes this frame: 1 from fire, else from the first
/// player's shot over it. Shots that stop at it are marked as hit (lasers
/// fly on); a rocket explodes on it.
pub fn damage(a: &Actor, c: &mut Ctx) -> u8 {
    let p = a.placed();
    if c.effects.fire_touches(c.sprites, c.map.width, p) {
        return 1;
    }
    for k in 0..c.shots.len() {
        let s = &c.shots[k];
        let shot = Placed {
            id: s.sprite,
            frame: s.frame,
            x: s.x,
            y: s.y,
        };
        if !overlap(c.sprites, c.map.width, p, shot) {
            continue;
        }
        c.state.last_shot = Some(s.direction);
        let (x, y) = (shot.x, shot.y);
        let s = &mut c.shots[k];
        return match s.kind {
            ShotKind::Weapon(Weapon::Rocket) => {
                s.hit = true;
                c.effect(EXPLOSION, x - 3, y + 3, Motion::Still, 0);
                8
            }
            ShotKind::Weapon(Weapon::FlameThrower) => {
                s.hit = true;
                2
            }
            ShotKind::Weapon(Weapon::Laser) => 2,
            ShotKind::Weapon(Weapon::Regular) => {
                s.hit = true;
                1
            }
            // Flies on, with an explosion.
            ShotKind::Blast(_) => {
                c.effect(EXPLOSION, x - 3, y + 3, Motion::Still, 0);
                5
            }
        };
    }
    0
}

/// Takes the damage off and flashes the actor. True if that killed it (and its
/// score was added); else a hit sound.
pub fn hurt(a: &mut Actor, c: &mut Ctx, damage: u8) -> bool {
    a.health -= i16::from(damage);
    a.look = SpriteLook::WhiteFlash;
    if a.health <= 0 {
        c.add_score(a.score);
        true
    } else {
        c.sound(Sound::Hit);
        false
    }
}

/// Effects at offsets from the actor, each on a debris path after a delay.
fn debris(a: &Actor, c: &mut Ctx, id: u16, list: &[(i32, i32, u8, u16)]) {
    for &(dx, dy, path, delay) in list {
        c.effect(id, a.x + dx, a.y + dy, Motion::Debris(path), delay);
    }
}

/// What a hit for `damage` does to `a`, by its type.
pub fn react(a: &mut Actor, c: &mut Ctx, damage: u8) {
    if super::family_hit(a, c, damage) {
        return;
    }
    let (id, x, y) = (a.id, a.x, a.y);
    match id {
        // Ceiling dropper.
        203 => {
            if !hurt(a, c, damage) {
                return;
            }
            c.add_score(100);
            c.particles(5, Spread::Both, x, y);
            c.effect(EXPLOSION_START, x, y, Motion::Still, 0);
        }
        // Eyeball thrower.
        98 | 99 => {
            if !hurt(a, c, damage) {
                return;
            }
            c.sound(Sound::Splat);
            debris(a, c, 100, &EYEBALL_DEATH);
            c.particles(0x0d, Spread::Both, x, y);
        }
        // Hoverbot and homing drone.
        0 | 280 => {
            if !hurt(a, c, damage) {
                return;
            }
            c.cloud(id, x, y, BLAST);
            if id == 0 {
                c.effect(0x0c, x, y - 2, Motion::Debris(2), 0);
                c.effect(0x0d, x, y, Motion::Debris(5), 0);
            }
            c.particles(3, Spread::Both, x + 1, y);
            c.explosion_sound();
        }
        // Walkers, balls and slime: explode, some with flying bits.
        31 | 32 | 49 | 54 | 67 | 70 | 81 | 176 | 189 | 190 => {
            if !hurt(a, c, damage) {
                return;
            }
            c.cloud(id, x, y, BLAST);
            c.random_particles(Spread::Both, x + 1, y);
            if !matches!(id, 31 | 32 | 189 | 190 | 81 | 70 | 67 | 176) {
                c.explosion_sound();
            } else {
                debris(a, c, 0x5e, &WALKER_DEATH);
                if id == 189 || id == 190 {
                    a.x -= 2;
                    a.y -= 2;
                    debris(a, c, 0x5e, &WALKER_DEATH);
                    a.x = x;
                    a.y = y - 4;
                    debris(a, c, 0x5e, &WALKER_DEATH);
                }
                c.sound(Sound::Splat);
            }
        }
        // Skeleton and gunner.
        134 | 299 => {
            if !hurt(a, c, damage) {
                return;
            }
            c.explosion_sound();
            let effect = if id == 134 { BLAST } else { EXPLOSION_START };
            c.cloud(id, x, y, effect);
            c.random_particles(Spread::Both, x + 1, y);
            c.add_score(100);
        }
        // Small flyers: no health, just destroyed.
        271..=273 => {
            c.random_particles(Spread::Both, x, y);
            c.explosion_sound();
            c.add_score(100);
        }
        // Security cameras.
        79 | 80 => {
            c.random_particles(Spread::Both, x, y);
            sparkle(c, SPARKLE_100, x, y);
            c.state.tally.cameras_destroyed += 1;
            c.explosion_sound();
            c.add_score(100);
        }
        // Robots, turrets, radar dishes and the like.
        38 | 39 | 55 | 56 | 57 | 58 | 62 | 78 | 97 | 107 | 115 | 151 | 208 | 236 | 239 | 244
        | 252 | 296 => {
            if !hurt(a, c, damage) {
                return;
            }
            robot_death(c, x, y);
            c.random_particles(Spread::Both, x, y);
            c.random_particles(Spread::Left, x - 1, y - 1);
            c.random_particles(Spread::Right, x + 1, y - 2);
            match id {
                236 => {
                    c.state.radar_dishes = c.state.radar_dishes.saturating_sub(1);
                    sparkle(c, SPARKLE_2000, x, y);
                    c.add_score(1500);
                }
                208 | 252 => {
                    sparkle(c, FIREWORK, x, y);
                    c.add_score(10000);
                }
                296 => {
                    sparkle(c, SPARKLE_500, x, y);
                    c.add_score(500);
                }
                _ => {}
            }
        }
        // Boxes: break open; the box lets its item out next frame.
        14 | 161..=164 => {
            if !hurt(a, c, damage) {
                return;
            }
            let s = state!(a, ItemBox);
            if s.stage != super::items::Stage::Closed {
                return;
            }
            s.stage = super::items::Stage::Shot;
            c.explosion_sound();
            for color in [4, 0x0f, 0x0b] {
                c.particles(color, Spread::Both, x + 1, y);
            }
            return;
        }
        // Soda can and big health item: shot once, they go off.
        SODA | BIG_HEALTH => {
            if let Kind::ItemBox(s) = &mut a.kind
                && s.timer == 0
            {
                s.timer = 1;
            }
            return;
        }
        // Bomb.
        63 => {
            c.effect(FIRE, x, y, Motion::Still, 0);
            for d in (4..0x14).step_by(4) {
                let delay = (d / 2) as u16;
                c.effect(FIRE, x - d, y, Motion::Debris(5), delay);
                c.effect(FIRE, x + d, y, Motion::Debris(5), delay);
            }
            c.explosion_sound();
        }
        // The box with a fuse: shot, it goes off at once.
        FUSE_ITEM => {
            c.explosion_sound();
            c.state.tally.fuse_boxes_left = c.state.tally.fuse_boxes_left.wrapping_sub(1);
            c.particles(0x0f, Spread::Both, x + 1, y);
            spread_fire(a, c);
        }
        // The glass case of a bonus globe.
        44 => {
            c.effect(0x48, x, y, Motion::Debris(3), 0);
            c.effect(0x49, x + 2, y, Motion::Debris(1), 0);
            if let Kind::BonusGlobe(s) = &a.kind {
                let globe = s.globe;
                c.effect(globe, x, y, Motion::Debris(2), 0);
            }
            a.look = SpriteLook::WhiteFlash;
            c.add_score(100);
            sparkle(c, SPARKLE_100, x, y);
            c.sound(Sound::Break);
            c.particles(0x0f, Spread::Both, x + 1, y);
            c.state.tally.bonus_globes_left = c.state.tally.bonus_globes_left.wrapping_sub(1);
        }
        _ => return,
    }
    // Reactions that do not return above remove the actor.
    c.remove(a);
}

/// Whether the last hit came from the left (for things that tip over).
pub fn shot_from_left(c: &Ctx) -> bool {
    c.state.last_shot == Some(ShotDirection::Left)
}
