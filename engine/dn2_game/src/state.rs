//! Level-wide state the actors share: score, tallies, the level's end.

use alloc::vec::Vec;

use dn2_assets::level::LevelFlags;

use crate::actors::spawn::Tally;
use crate::mechanics::{MapSection, TileDebris};
use crate::messages::Messages;
use crate::player::{SpiderPlaces, Weapon};
use crate::shots::ShotDirection;

/// Why the level is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelExit {
    /// The boss died: the episode is over.
    BossKilled,
    /// The player died or fell out of the map.
    Died,
    /// The exit was reached.
    Finished,
}

/// Placed objects of each tally kind, and how many the player got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tallies {
    pub cameras: u16,
    pub cameras_destroyed: u16,
    pub weapons: u16,
    pub weapons_collected: u16,
    pub bonus_items: u16,
    pub bonus_items_collected: u16,
    pub laser_turrets: u16,
    pub laser_turrets_destroyed: u16,
    /// Fuse boxes and bonus globes not destroyed yet.
    pub fuse_boxes_left: u16,
    pub bonus_globes_left: u16,
}

impl Tallies {
    /// One more placed object of `tally`.
    pub fn placed(&mut self, tally: Tally) {
        match tally {
            Tally::Weapon => self.weapons += 1,
            Tally::BonusItem => self.bonus_items += 1,
            Tally::FuseBox => self.fuse_boxes_left += 1,
            Tally::BonusGlobe => self.bonus_globes_left += 1,
            Tally::Camera => self.cameras += 1,
            Tally::LaserTurret => self.laser_turrets += 1,
        }
    }
}

/// What a respawn beacon saved: where the player comes back to after dying and
/// what he has then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// The beacon's position.
    pub x: i32,
    pub y: i32,
    pub health: u8,
    pub weapon: Weapon,
    pub ammo: u8,
    /// Whether the alternate backdrop showed (the original saves it too).
    pub backdrop_alternate: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LevelState {
    /// 1 easy, 2 medium, 3 hard.
    pub difficulty: u8,
    pub score: u32,
    pub tally: Tallies,
    /// Radar dishes left; the exit does not work while there are some.
    pub radar_dishes: u8,
    pub boss_health: u16,
    /// The boss's health bar is shown.
    pub boss_bar: bool,
    /// The level has water (its actors set this).
    pub water: bool,
    /// The direction of the shot that last hit an actor.
    pub last_shot: Option<ShotDirection>,
    pub exit: Option<LevelExit>,
    pub messages: Messages,
    /// The letters N, U, K, E, M picked up: a bit each, and in the low
    /// bits how many came in order.
    pub letters: u16,
    /// The actors (by slot) of the spiders clinging to the player.
    pub spiders: SpiderPlaces<Option<usize>>,
    /// The slot of the snake or ceiling sucker that holds the player (the
    /// original keeps the type).
    pub grabber: Option<usize>,
    /// The slot of the air fan that lifts the player (`FAN_INDEX`).
    pub fan: Option<usize>,
    /// A key or card was used on its slot; map section markers wait for it.
    pub key_used: bool,
    /// The access card turned the force fields off (the first one that
    /// notices goes dark).
    pub force_field_off: bool,
    /// The map sections the markers recorded.
    pub sections: Vec<MapSection>,
    /// Flying pieces of an exploded wall, and the frames they have flown
    /// (0: none).
    pub debris: Vec<TileDebris>,
    pub debris_timer: u8,
    /// The earthquake: frames to the next shake or to its end, and where it
    /// is a shake (`quake_timer` below `quake_limit`, not 0).
    pub quake_timer: u8,
    pub quake_limit: u8,
    /// The next frame is filled with this colour instead of the level
    /// (big explosions); the renderer shows it, the world clears it each
    /// frame.
    pub flash: Option<u8>,
    /// Pixels the whole picture jolts to the left in this frame, 0 to 7: the
    /// earthquake, a section landing or sinking. The original sets the EGA's
    /// horizontal pixel panning and puts it back at the end of every frame.
    pub shake: u8,
    /// Where a respawn beacon has the player return to.
    pub checkpoint: Option<Checkpoint>,
    /// A transporter moves the player here (the other one's position).
    pub teleport: Option<(i32, i32)>,
    /// The backdrop switch was made: the alternate backdrop shows (in
    /// levels with a half-speed backdrop). Transporters toggle it in levels
    /// that allow it; the reactor blowing up sets it in others.
    pub backdrop_alternate: bool,
    /// Frames since the switch, for its message.
    pub switch_frames: u8,
    /// The hint pedestal was given its globe: the engine shows the hint.
    pub hint_requested: bool,
}

impl LevelState {
    /// Whether the earthquake shakes right now.
    pub fn earthquake_going(&self, flags: LevelFlags) -> bool {
        flags.contains(LevelFlags::EARTHQUAKE)
            && self.quake_timer < self.quake_limit
            && self.quake_timer != 0
    }
}
