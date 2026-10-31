//! The player's slot, decorations that animate, bonus globes and security
//! cameras.

use super::{Actor, Ctx, Kind};
use crate::messages::text;
use crate::player::{Facing, PlayerState, SpiderPlaces, frame};
use crate::state::LevelExit;
use crate::{Buttons, Sound, SpriteLook};

/// Items of the cloak and rapid fire in the inventory, and the level type
/// the cloak comes back as.
const CLOAK_ITEM: u16 = 0xd3;
const RAPID_FIRE_ITEM: u16 = 0x34;
const CLOAK_TYPE: u16 = 0x72;
/// Frames left when the power-ups warn that they are ending.
const WARNING_FRAMES: u16 = 30;

/// The slot follows the player and shows it blinking while it cannot be hurt,
/// and as a tint while cloaked (white flashes in the last moments). The cloak
/// and rapid fire run out here: a message warns, the inventory icon blinks
/// away, and the cloak item reappears where it was found. Falling out of the
/// map ends the level. Walking through a door (after a key or card was used)
/// and riding a turbo lift show the player facing front; blown out of an
/// airlock, he dies when he is past the map's edge. The ship shows its
/// thrusters.
pub(super) fn player(a: &mut Actor, c: &mut Ctx) {
    let p = c.player.placed();
    (a.id, a.frame, a.x, a.y) = (p.id, p.frame, p.x, p.y);
    if matches!(
        c.player.state,
        PlayerState::AirlockLeft | PlayerState::AirlockRight
    ) && c.player.x > i32::from(c.map.width)
    {
        if c.state.exit.is_none() {
            c.state.exit = Some(LevelExit::Died);
            c.sound(Sound::Die);
        }
        return;
    }
    // The original's unsigned rows: the row a few tiles below the map's
    // bottom.
    let below = a.y - 4 - i32::from(c.map.height);
    if (1..10).contains(&below) {
        if c.state.exit.is_none() {
            c.state.exit = Some(LevelExit::Died);
            c.sound(Sound::Die);
        }
        return;
    }
    c.player.update_fading();
    if c.player.cloak_timer != 0 {
        a.look = SpriteLook::OnePlane;
        c.player.cloak_timer -= 1;
        let timer = c.player.cloak_timer;
        if timer == WARNING_FRAMES {
            c.state.messages.show(text::CLOAK_ENDING);
        }
        if timer < WARNING_FRAMES && c.draw_page {
            a.look = SpriteLook::WhiteFlash;
        }
        if timer == 0 {
            c.player.take_item(CLOAK_ITEM);
            let (x, y) = c.player.cloak_origin;
            c.spawn(CLOAK_TYPE, x, y);
        }
    }
    if c.player.rapid_fire_timer != 0 {
        c.player.rapid_fire_timer -= 1;
        let timer = c.player.rapid_fire_timer;
        if timer == WARNING_FRAMES {
            c.state.messages.show(text::RAPID_FIRE_ENDING);
        }
        if timer == 0 {
            c.player.take_item(RAPID_FIRE_ITEM);
        }
    }
    if c.player.hurt_timer != 0 {
        if c.player.hurt_timer & 1 != 0 {
            a.look = if c.player.hurt_timer > 10 {
                SpriteLook::Hidden
            } else {
                SpriteLook::WhiteFlash
            };
        }
        c.player.hurt_timer -= 1;
    }
    // Held or gone, the player is not drawn, and no spider clings on.
    if c.player.state == PlayerState::Grabbed || c.player.frame == frame::GONE {
        a.look = SpriteLook::Hidden;
        c.state.spiders = SpiderPlaces::default();
    }
    if c.player.state == PlayerState::Ship {
        if a.look != SpriteLook::Hidden {
            ship_thrusters(c);
        }
    } else if c.player.door_step != 0 {
        if c.player.state == PlayerState::Standing {
            a.frame = FRONT;
        }
        c.player.door_step += 1;
        if c.player.door_step == 9 {
            c.player.door_step = 0;
        }
    } else if c.player.state == PlayerState::Lift {
        a.frame = FRONT;
    } else if c.player.state == PlayerState::AirFan {
        a.frame = AIR_FAN;
    }
}

/// The ship's thruster flames.
const THRUSTER: u16 = 0x5c;

/// The thrusters of the ship: behind it when it flies forwards (frames 2
/// and 3 to the right, 4 and 5 to the left), under it when it rises (0 and
/// 1), flickering on the draw page.
fn ship_thrusters(c: &mut Ctx) {
    let (x, y) = (c.player.x, c.player.y);
    let input = c.player.input;
    let page = u16::from(c.draw_page);
    let left = c.player.facing == Facing::Left;
    if left && input.contains(Buttons::LEFT) {
        c.draw(THRUSTER, page + 4, x, y, SpriteLook::Normal);
    }
    if !left && input.contains(Buttons::RIGHT) {
        c.draw(THRUSTER, page + 2, x, y, SpriteLook::Normal);
    }
    if input.contains(Buttons::UP) {
        c.draw(THRUSTER, page, x + i32::from(left), y, SpriteLook::Normal);
    }
}

/// The player's frame facing the viewer.
const FRONT: u16 = 0x21;
/// The player's frame on an air fan.
const AIR_FAN: u16 = 6;

/// Frames of type 239 (the special crystal globe), one per step.
const CRYSTAL_FRAMES: [u16; 26] = [
    0, 1, 2, 3, 4, 5, 4, 5, 4, 5, 4, 5, 4, 3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Steps through the sprite's frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animate {
    /// Frames to loop through.
    frames: u16,
    /// Of a transporter: 50 or 51, so each finds the other.
    pub pair: u16,
    /// Position in the crystal globe's list.
    step: usize,
}

impl Animate {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            frames: vars[0],
            pair: vars[1],
            // The crystal globe keeps its place in its list in the same
            // variable, so it starts at the frame count.
            step: usize::from(vars[0]),
        }
    }
}

pub(super) fn animate(a: &mut Actor, c: &mut Ctx) {
    let Kind::Animate(s) = &mut a.kind else {
        return;
    };
    match a.id {
        // Flickers at random.
        0x105 => a.frame = u16::from(c.random.next_byte() & 4 == 0),
        0xef => {
            a.frame = CRYSTAL_FRAMES[s.step];
            s.step = (s.step + 1) % CRYSTAL_FRAMES.len();
        }
        id => {
            // Some step on every other frame only.
            let slow = matches!(id, 0x101 | 0x102 | 0xbc);
            if !slow || c.draw_page {
                a.frame += 1;
            }
            if a.frame == s.frames {
                a.frame = 0;
            }
        }
    }
}

/// The actor is the glass case; the globe inside is another sprite that spins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BonusGlobe {
    /// The globe's sprite (which also says what it is worth).
    pub globe: u16,
    spin: u16,
}

impl BonusGlobe {
    pub(super) fn new(vars: [u16; 5]) -> Self {
        Self {
            globe: vars[0],
            spin: 0,
        }
    }
}

pub(super) fn bonus_globe(a: &mut Actor, c: &mut Ctx) {
    let Kind::BonusGlobe(s) = &mut a.kind else {
        return;
    };
    s.spin = (s.spin + 1) % 4;
    let (globe, spin) = (s.globe, s.spin);
    c.draw(globe, spin, a.x, a.y, SpriteLook::Normal);
}

/// The camera that hangs from the ceiling: it looks from a row lower.
const CEILING_CAMERA: u16 = 0x4f;

/// The camera turns to the player: frame 0 looks down, then on clockwise in
/// steps of 45 degrees. It looks down while the player is cloaked.
pub(super) fn security_camera(a: &mut Actor, c: &mut Ctx) {
    if c.player.cloak_timer != 0 {
        a.frame = 0;
        return;
    }
    let y = a.y + i32::from(a.id == CEILING_CAMERA);
    let (px, py) = (c.player.x, c.player.y);
    let vertical = |above, below, level| {
        if py < y - 1 {
            above
        } else if y < py {
            below
        } else {
            level
        }
    };
    a.frame = if a.x + 1 < px {
        vertical(3, 1, 2)
    } else if px < a.x {
        vertical(5, 7, 6)
    } else if y < py {
        0
    } else {
        4
    };
}
