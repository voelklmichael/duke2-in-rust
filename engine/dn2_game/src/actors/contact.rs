//! Actors touching the player: enemies hurt, items are picked up. The original
//! switches on the actor's type once the two sprites overlap.

use super::items::{
    BIG_HEALTH, CLOAK, CLOAK_ITEM, HEALTH, HINT_GLOBE, RAPID_FIRE, RAPID_FIRE_ITEM, SODA, Stage,
};
use super::{Actor, Ctx, Kind};
use crate::effects::Motion;
use dn2_assets::level::LevelFlags;

use crate::messages::{once, text};
use crate::player::{PlayerState, Weapon};
use crate::state::LevelExit;
use crate::{Buttons, Sound};

/// Score numbers that float up where something was picked up.
pub(super) const SPARKLE_100: u16 = 0x7b;
pub(super) const SPARKLE_500: u16 = 0x7c;
pub(super) const SPARKLE_2000: u16 = 0x7d;
pub(super) const SPARKLE_5000: u16 = 0x7e;
pub(super) const FIREWORK: u16 = 0x7f;
/// Frames of cloak and rapid fire.
const POWER_UP_FRAMES: u16 = 700;
const MAX_HEALTH: u8 = 9;

/// Calls the contact reaction of `a` if it overlaps the player.
pub fn touch(a: &mut Actor, c: &mut Ctx) {
    if matches!(c.player.state, PlayerState::Dying | PlayerState::Grabbed) {
        return;
    }
    if c.touches_player(a.placed()) {
        react(a, c);
    }
}

/// A score number floating up from `x`, `y`.
pub(super) fn sparkle(c: &mut Ctx, id: u16, x: i32, y: i32) {
    c.effect(id, x, y, Motion::Float, 0);
}

pub(super) fn heal(c: &mut Ctx, health: u8) {
    c.player.health = (c.player.health + health).min(MAX_HEALTH);
}

/// Whether an item is far enough out of its box to be taken.
fn ready(a: &Actor) -> bool {
    match &a.kind {
        Kind::ItemBox(s) => matches!(s.stage, Stage::Hop(6..) | Stage::Out),
        _ => true,
    }
}

/// A bonus item: points and the bonus tally.
fn bonus(a: &mut Actor, c: &mut Ctx, points: u16) {
    c.sound(Sound::Pickup);
    c.add_score(points);
    c.state.tally.bonus_items_collected += 1;
    c.remove(a);
}

/// What every letter ends with.
fn letter_pickup(a: &mut Actor, c: &mut Ctx) {
    c.add_score(10000);
    sparkle(c, SPARKLE_100, a.x, a.y);
    c.add_score(100);
    c.sound(Sound::Pickup);
    c.remove(a);
}

/// One of N, U, K, E: counts if the ones before came in order.
fn letter(a: &mut Actor, c: &mut Ctx, order: u16, message: (u8, &'static str)) {
    let letters = c.state.letters;
    let in_order = if order == 0 {
        letters == 0
    } else {
        letters & 7 == order
    };
    if in_order {
        c.state.letters += 1;
        c.state.messages.show_once(message.0, message.1);
    }
    c.state.letters |= 0x100 << order;
    letter_pickup(a, c);
}

/// The transporter's two pads (type 50 and 51, both sprite 51), and the
/// sound of using one.
const TRANSPORTER: u16 = 51;

/// Bonus items: their points, and the score numbers that float up from them
/// (sprite, and the rows above the item).
fn bonus_item(id: u16) -> Option<(u16, &'static [(u16, i32)])> {
    Some(match id {
        172 => (100, &[(SPARKLE_100, 0)]),
        184 => (2500, &[(SPARKLE_2000, -1), (SPARKLE_500, 0)]),
        173 => (2000, &[(SPARKLE_2000, 0)]),
        183 => (
            1500,
            &[(SPARKLE_500, 0), (SPARKLE_500, -1), (SPARKLE_500, -2)],
        ),
        181 => (1000, &[(SPARKLE_500, 0), (SPARKLE_500, -1)]),
        274 => (5000, &[(SPARKLE_5000, -2)]),
        160 | 182 | 185 | 186 | 275 => (500, &[(SPARKLE_500, 0)]),
        _ => return None,
    })
}

/// Standing on a transporter pad, up sends the player to the other pad
/// (the level ends if there is none). A level with the backdrop switch
/// toggles the backdrop with it.
fn transporter(a: &Actor, c: &mut Ctx) {
    let Kind::Animate(pad) = &a.kind else {
        return;
    };
    let (px, py) = (c.player.x, c.player.y);
    let on_it = a.x <= px && px <= a.x + 3 && a.y == py;
    let standing = c.player.state == PlayerState::Standing;
    if on_it && standing {
        c.state
            .messages
            .show_once(once::TRANSPORTER, text::TRANSPORTER);
    }
    if !on_it || !standing || !c.player.input.contains(Buttons::UP) {
        return;
    }
    c.sound(Sound::Transporter);
    let toggles = c
        .map
        .level
        .flags
        .contains(LevelFlags::BACKDROP_SWITCH_TOGGLE);
    let target = if pad.pair == 50 {
        if toggles {
            c.state.backdrop_alternate = true;
        }
        51
    } else {
        if toggles {
            c.state.backdrop_alternate = false;
        }
        50
    };
    let other = c
        .actors
        .iter()
        .find(|e| e.id == TRANSPORTER && matches!(&e.kind, Kind::Animate(o) if o.pair == target))
        .map(|e| (e.x, e.y));
    match other {
        Some(at) => {
            c.state.debris_timer = 0;
            c.state.teleport = Some(at);
        }
        None => c.state.exit = Some(LevelExit::Finished),
    }
}

fn react(a: &mut Actor, c: &mut Ctx) {
    if super::family_touch(a, c) {
        return;
    }
    let (x, y) = (a.x, a.y);
    if let Some((points, sparkles)) = bonus_item(a.id) {
        bonus(a, c, points);
        for &(id, dy) in sparkles {
            sparkle(c, id, x, y + dy);
        }
        return;
    }
    match a.id {
        // Plain enemies and hazards hurt.
        0 | 31 | 32 | 38 | 39 | 49 | 54 | 58 | 63 | 64 | 67 | 70 | 97 | 118 | 134 | 151 | 159
        | 176 | 188 | 189 | 190 | 203 | 212 | 235 | 244 | 262 | 263 | 271 | 272 | 273 => {
            c.hurt_player();
        }
        // Hurts and goes up in smoke.
        280 | 55 | 56 | 57 | 256 | 259 => {
            c.hurt_player();
            c.effect(crate::effects::EXPLOSION_START, x, y, Motion::Still, 0);
            c.remove(a);
        }
        // Weapons.
        19 | 20 | 22 | 23 => {
            if !ready(a) {
                return;
            }
            let (id, message, ammo) = match a.id {
                19 => (once::ROCKET_LAUNCHER, text::ROCKET_LAUNCHER, 0x20),
                20 => (once::FLAME_THROWER, text::FLAME_THROWER, 0x40),
                22 => (once::REGULAR_WEAPON, text::REGULAR_WEAPON, 0x20),
                _ => (once::LASER, text::LASER, 0x20),
            };
            c.state.messages.show_once(id, message);
            c.player.ammo = ammo;
            c.state.tally.weapons_collected += 1;
            if let Kind::ItemBox(s) = &a.kind {
                c.player.weapon = match s.weapon {
                    1 => Weapon::Laser,
                    2 => Weapon::Rocket,
                    3 => Weapon::FlameThrower,
                    _ => Weapon::Regular,
                };
            }
            c.sound(Sound::WeaponPickup);
            c.remove(a);
            sparkle(c, SPARKLE_2000, x, y);
            c.add_score(2000);
        }
        SODA => {
            c.remove(a);
            let shot = matches!(&a.kind, Kind::ItemBox(s) if s.timer != 0);
            if shot {
                c.add_score(2000);
                c.sound(Sound::Pickup);
                sparkle(c, SPARKLE_2000, x, y);
            } else {
                c.state.messages.show_once(once::SODA, text::SODA);
                c.add_score(100);
                c.sound(Sound::Food);
                heal(c, 1);
                sparkle(c, SPARKLE_100, x, y);
            }
        }
        BIG_HEALTH => {
            c.sound(Sound::Food);
            c.remove(a);
            c.add_score(100);
            heal(c, 6);
            sparkle(c, SPARKLE_100, x, y);
        }
        HEALTH => {
            if !ready(a) {
                return;
            }
            c.state.messages.show_once(once::HEALTH, text::HEALTH);
            c.sound(Sound::Food);
            if c.player.health >= MAX_HEALTH {
                c.add_score(10000);
                sparkle(c, FIREWORK, x, y);
            } else {
                heal(c, 1);
                sparkle(c, SPARKLE_500, x, y);
                c.add_score(500);
            }
            c.remove(a);
        }
        // The letters of NUKEM.
        155 => letter(a, c, 0, (once::LETTER_N, text::LETTER_N)),
        156 => letter(a, c, 1, (once::LETTER_U, text::LETTER_U)),
        157 => letter(a, c, 2, (once::LETTER_K, text::LETTER_K)),
        158 => letter(a, c, 3, (once::LETTER_E, text::LETTER_E)),
        187 => {
            c.state.letters |= 0x1000;
            if c.state.letters & 7 != 4 {
                letter_pickup(a, c);
                return;
            }
            // All five in order: fireworks.
            c.sound(Sound::Fanfare);
            c.state
                .messages
                .show_once(once::ALL_LETTERS, text::ALL_LETTERS);
            for k in 0..10 {
                let dx = [-3, 0, 3, 0][k as usize & 3];
                sparkle(c, FIREWORK, x + dx, y - k);
            }
            c.add_score(50000);
            c.add_score(50000);
            c.remove(a);
        }
        // Things to carry: the cloak, the key and the access card.
        37 | CLOAK | 121 => {
            if !ready(a) {
                return;
            }
            c.sound(Sound::Pickup);
            if a.id == CLOAK {
                c.state.messages.show(text::CLOAK);
                c.player.give_item(CLOAK_ITEM);
                c.player.cloak_timer = POWER_UP_FRAMES;
                c.player.cloak_origin = (a.x, a.y);
            } else {
                if a.id == 121 {
                    c.state.messages.show_once(once::KEY, text::KEY);
                } else {
                    c.state
                        .messages
                        .show_once(once::ACCESS_CARD, text::ACCESS_CARD);
                }
                sparkle(c, SPARKLE_500, x, y);
                c.player.inventory.push(a.id);
            }
            c.add_score(500);
            c.remove(a);
        }
        RAPID_FIRE => {
            if !ready(a) {
                return;
            }
            c.sound(Sound::WeaponPickup);
            c.state
                .messages
                .show_once(once::RAPID_FIRE, text::RAPID_FIRE);
            c.player.give_item(RAPID_FIRE_ITEM);
            sparkle(c, SPARKLE_500, x, y);
            c.add_score(500);
            c.player.rapid_fire_timer = POWER_UP_FRAMES;
            c.remove(a);
        }
        TRANSPORTER => transporter(a, c),
        // The crystal globe for the hint pedestal.
        239 => {
            c.sound(Sound::Pickup);
            c.state.messages.show(text::HINT_GLOBE);
            c.player.inventory.push(HINT_GLOBE);
            sparkle(c, FIREWORK, x, y);
            c.add_score(10000);
            c.remove(a);
        }
        // The bonus globes: worth what their globe says.
        44 => {
            c.add_score(a.score);
            let id = SPARKLE_500
                + match a.score {
                    2000 => 1,
                    5000 => 2,
                    10000 => 3,
                    _ => 0,
                };
            sparkle(c, id, x, y);
            c.sound(Sound::Pickup);
            c.remove(a);
        }
        // The others react in their own modules (`family_touch`), or not at all.
        _ => {}
    }
}
