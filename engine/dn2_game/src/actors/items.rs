//! Item boxes and the items in them.
//!
//! Boxes (types 14, 161 to 164) and the items that come out of them share one
//! update function. A shot box breaks open and lets its item hop out; the item
//! then takes the box's place with its own sprite. Items placed in the open are
//! the same actor, past the box stage.

use super::{Actor, ActorFlags, Ctx, Kind};
use crate::SpriteLook;
use crate::effects::Motion;
use crate::map::{Dir, Hit};
use crate::particles::Spread;

/// Tiles an item moves while it hops out of its box, by step.
const HOP: [i32; 9] = [-3, -2, -1, 0, 1, 2, 3, -1, 1];
/// The pieces of a box breaking open (sprite, debris path), and the extra
/// ones of the plain grey box.
const BREAK_OPEN: [(u16, u8); 4] = [(165, 2), (166, 3), (167, 1), (166, 5)];
const BREAK_GREY: [(u16, u8); 4] = [(18, 2), (17, 5), (15, 3), (16, 1)];
/// The grey box also leaves a puff of smoke.
const GREY_SMOKE: u16 = 84;

const GREY_BOX: u16 = 0x0e;
/// Contents: nothing, a fuse box (which is an effect) or the turkey (an
/// actor of its own).
const NOTHING: u16 = 0xffff;
const FUSE_BOX: u16 = 0x4b;
const TURKEY: u16 = 0xca;
/// Items that animate or act.
pub(super) const HEALTH: u16 = 0x1c;
pub(super) const FUSE_ITEM: u16 = 0x2a;
pub(super) const RAPID_FIRE: u16 = 0x35;
pub(super) const CLOAK: u16 = 0x72;
pub(super) const SODA: u16 = 0xa8;
pub(super) const BIG_HEALTH: u16 = 0xae;
const SPECIAL: u16 = 0xb9;

/// Where a box or item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Closed,
    /// Shot this frame.
    Shot,
    Opening,
    /// The item hops out; the step counts on to `HOP.len()`.
    Hop(u8),
    /// Out (or placed in the open).
    Out,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemBox {
    pub stage: Stage,
    /// The item's type (its sprite).
    pub contents: u16,
    /// The weapon a weapon pickup gives (0 regular to 3 flame thrower).
    pub weapon: u16,
    /// Frames counted by the fuse box; for the soda can and big health
    /// item, set once they were shot.
    pub timer: u16,
}

impl ItemBox {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            stage: match vars[0] {
                0 => Stage::Closed,
                1 => Stage::Shot,
                2 => Stage::Opening,
                v @ 3..12 => Stage::Hop(v as u8 - 3),
                _ => Stage::Out,
            },
            contents: vars[1],
            weapon: vars[2],
            timer: 0,
        }
    }
}

pub(super) fn item_box(a: &mut Actor, c: &mut Ctx) {
    let Kind::ItemBox(s) = &mut a.kind else {
        return;
    };
    if s.stage == Stage::Closed {
        return;
    }
    if s.stage == Stage::Shot {
        s.stage = Stage::Opening;
        if a.id == GREY_BOX {
            a.frame += 1;
            return;
        }
        for (id, path) in BREAK_OPEN {
            c.effect(id, a.x, a.y, Motion::Debris(path), 0);
        }
        if s.contents == NOTHING {
            c.remove(a);
            return;
        }
    }
    let Kind::ItemBox(s) = &mut a.kind else {
        return;
    };
    if s.stage == Stage::Opening {
        if a.id == GREY_BOX {
            for (id, path) in BREAK_GREY {
                c.effect(id, a.x, a.y, Motion::Debris(path), 0);
            }
            c.effect(GREY_SMOKE, a.x, a.y, Motion::Still, 0);
        }
        s.stage = Stage::Hop(0);
        a.flags.remove(ActorFlags::GRAVITY);
        match s.contents {
            FUSE_BOX => {
                c.remove(a);
                c.effect(FUSE_BOX, a.x, a.y, Motion::Still, 1);
                return;
            }
            TURKEY => {
                c.remove(a);
                c.spawn(TURKEY, a.x, a.y);
                return;
            }
            NOTHING => {
                c.remove(a);
                return;
            }
            item => {
                a.id = item;
                a.frame = 0;
                if matches!(item, FUSE_ITEM | SODA | BIG_HEALTH) {
                    a.health = 1;
                }
            }
        }
    }
    let Kind::ItemBox(s) = &mut a.kind else {
        return;
    };
    if let Stage::Hop(step) = s.stage {
        a.flags.insert(ActorFlags::ALWAYS_UPDATE);
        a.y += HOP[usize::from(step)];
        let step = step + 1;
        s.stage = if usize::from(step) == HOP.len() {
            Stage::Out
        } else {
            Stage::Hop(step)
        };
        let over_nothing =
            step == 6 && c.collide(a.id, a.frame, a.x, a.y + 1, Dir::Down) == Hit::Free;
        if s.stage == Stage::Out || over_nothing {
            a.flags.insert(ActorFlags::GRAVITY);
        }
    }
    match a.id {
        SPECIAL => a.frame = u16::from(c.draw_page),
        RAPID_FIRE | CLOAK => a.frame = (a.frame + 1) % 4,
        HEALTH => a.frame = (a.frame + 1) % 9,
        FUSE_ITEM => fuse_item(a, c),
        SODA => soda_can(a, c),
        BIG_HEALTH => big_health(a, c),
        _ => {}
    }
}

fn timer(a: &mut Actor) -> &mut u16 {
    match &mut a.kind {
        Kind::ItemBox(s) => &mut s.timer,
        _ => unreachable!("an item has item box state"),
    }
}

/// The box with a fuse: it flashes, then after 32 frames explodes and sets
/// the floor on fire both ways.
fn fuse_item(a: &mut Actor, c: &mut Ctx) {
    a.frame = (a.frame + 1) % 8;
    let t = timer(a);
    *t += 1;
    let t = *t;
    if t > 0x18 && c.draw_page {
        a.look = SpriteLook::WhiteFlash;
    }
    if t == 0x20 {
        c.state.tally.fuse_boxes_left = c.state.tally.fuse_boxes_left.wrapping_sub(1);
        c.explosion_sound();
        c.particles(0x0f, Spread::Both, a.x + 1, a.y);
        spread_fire(a, c);
        c.remove(a);
    }
}

/// Fire along the floor on both sides of a fuse box, until it finds no
/// floor.
pub(super) fn spread_fire(a: &Actor, c: &mut Ctx) {
    let (mut left, mut right) = (true, true);
    for d in (0..12).step_by(2) {
        let delay = d as u16;
        if left {
            left = c.effect(crate::effects::FIRE, a.x - 2 - d, a.y, Motion::Still, delay);
        }
        if right {
            right = c.effect(crate::effects::FIRE, a.x + 2 + d, a.y, Motion::Still, delay);
        }
    }
}

/// The soda can: shot, it flies up to the ceiling and bursts.
fn soda_can(a: &mut Actor, c: &mut Ctx) {
    a.frame = (a.frame + 1) % 6;
    let t = timer(a);
    if *t == 0 {
        return;
    }
    *t += 1;
    a.flags.remove(ActorFlags::GRAVITY);
    a.y -= 1;
    if c.collide(SODA, 0, a.x, a.y, Dir::Up) == Hit::Free {
        // The rocket flame under the can.
        let frame = 6 + u16::from(c.draw_page);
        c.draw(SODA, frame, a.x, a.y, SpriteLook::Normal);
        return;
    }
    c.effect(0xa9, a.x, a.y, Motion::Debris(4), 0);
    c.effect(0xaa, a.x, a.y, Motion::Debris(0), 0);
    c.explosion_sound();
    c.remove(a);
}

/// The big health item: shot, it bursts for 10000 points.
fn big_health(a: &mut Actor, c: &mut Ctx) {
    if *timer(a) == 0 {
        return;
    }
    c.explosion_sound();
    c.remove(a);
    for u in 0..6u8 {
        let (x, y) = (a.x + i32::from(u & 2), a.y + i32::from(u & 1));
        c.effect(0xa9, x, y, Motion::Debris(u), 0);
        c.effect(0xaa, x, y, Motion::Debris(u), 0);
    }
    c.add_score(10000);
    c.effect(0x7f, a.x, a.y, Motion::Float, 0);
}
