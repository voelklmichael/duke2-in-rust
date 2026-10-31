//! Making actors: from the level's list at the start, and during play.

use dn2_assets::level::Level;
use dn2_assets::sprites::Sprites;

use super::table::SPAWNS;
use super::{Actor, ActorFlags, Actors, Kind, MAX_ACTORS};
use crate::state::LevelState;
use crate::{Random, SpriteLook};

/// Placed types that mark the player's start, facing left and right.
pub const START_LEFT: u16 = 5;
pub const START_RIGHT: u16 = 6;
/// Markers: the next actor of the list exists only at medium and hard, or only
/// at hard.
const NOT_EASY: u16 = 0x52;
const HARD_ONLY: u16 = 0x53;

/// The original's update functions (`actor_update_*`), which the spawn data
/// names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Animate,
    Hoverbot,
    Player,
    ItemBox,
    FlameThrowerBot,
    BonusGlobe,
    Watchbot,
    RocketTurret,
    EnemyRocket,
    WatchbotContainerCarrier,
    WatchbotContainer,
    BomberShip,
    FallingBomb,
    BouncingBall,
    Reactor,
    SlimeContainer,
    SlimeBlob,
    Snake,
    SecurityCamera,
    CeilingSucker,
    ParkedShip,
    BrokenMissile,
    EyeballThrower,
    MapSectionMarker,
    HoverbotGenerator,
    MessengerDrone,
    SlimePipe,
    SlimeDrop,
    ForceField,
    AccessCardSlot,
    KeyHole,
    VerticalDoor,
    HorizontalDoor,
    RespawnBeacon,
    Skeleton,
    AirFan,
    LaserTurret,
    EnemyLaserShot,
    LevelExit,
    ForceFieldGenerator,
    LaunchableMissile,
    ExtendingHazardColumn,
    HoverDrone,
    Spider,
    Soldier,
    SpikedGreenCreature,
    FastWalker,
    Turkey,
    PatrolWalker,
    CeilingDropper,
    TurboLift,
    CrusherPiston,
    Water,
    WaterDrop,
    DripSpawner,
    FireJet,
    RadarCounterDisplay,
    HintGlobePedestal,
    WindblownSpiderGenerator,
    UnicycleBot,
    RandomCrawler,
    Airlock,
    AggressivePrisoner,
    ExplosionSpawner,
    BossEpisode1,
    BossPathFlyer,
    BossRocket,
    BossMineLayer,
    HomingDrone,
    TerrainFollowingFlyer,
    JumpingGunner,
}

/// A starting health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amount {
    Fixed(i16),
    /// `a` × difficulty (1 easy to 3 hard) + `b`.
    PerDifficulty(i16, i16),
    /// The boss health the type sets up.
    Boss,
}

/// The bonus-screen tallies: placed objects of a kind are counted at the
/// start, and the player has to get all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tally {
    Weapon,
    BonusItem,
    FuseBox,
    BonusGlobe,
    Camera,
    LaserTurret,
}

/// What a placed type starts as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub level_type: u16,
    pub update: Update,
    /// The sprite, which is also the actor's type from then on.
    pub sprite: u16,
    pub health: Amount,
    pub score: u16,
    pub flags: ActorFlags,
    /// The original's five actor variables as the spawn code sets them: what
    /// they mean depends on the update function.
    pub vars: [u16; 5],
    pub tally: Option<Tally>,
}

/// The spawn data for a placed type, if it makes an actor.
pub fn lookup(level_type: u16) -> Option<&'static Spawn> {
    SPAWNS
        .binary_search_by_key(&level_type, |s| s.level_type)
        .ok()
        .map(|i| &SPAWNS[i])
}

/// The actor for a placed type at `x`, `y`; `None` for types that make none.
pub fn new_actor(
    level_type: u16,
    x: i32,
    y: i32,
    state: &mut LevelState,
    random: &mut Random,
) -> Option<Actor> {
    if level_type == START_LEFT || level_type == START_RIGHT {
        return Some(Actor {
            id: level_type,
            frame: 0,
            x,
            y,
            flags: ActorFlags::ALWAYS_UPDATE,
            fall: 0,
            health: 0,
            score: 0,
            look: SpriteLook::Normal,
            removed: false,
            kind: Kind::Player,
        });
    }
    let s = lookup(level_type)?;
    let difficulty = i16::from(state.difficulty);
    let mut vars = s.vars;
    let mut score = s.score;
    match level_type {
        97 => vars[0] = u16::from(random.next_byte() & 3),
        176 => vars[4] = x as u16,
        _ => {}
    }
    if matches!(level_type, 101 | 176) {
        score = y as u16;
    }
    let health = match s.health {
        Amount::Fixed(h) => h,
        Amount::PerDifficulty(a, b) => a * difficulty + b,
        Amount::Boss if level_type == 265 => 75 * difficulty + 600,
        Amount::Boss => 20 * difficulty + 90,
    };
    if matches!(level_type, 101 | 200 | 265 | 279) {
        state.boss_health = if level_type == 265 {
            health / 4
        } else {
            health
        } as u16;
    }
    if let Some(tally) = s.tally {
        state.tally.placed(tally);
    }
    match level_type {
        221 | 233 | 234 => state.water = true,
        236 => state.radar_dishes += 1,
        _ => {}
    }
    Some(Actor {
        id: s.sprite,
        frame: 0,
        x,
        y,
        flags: s.flags,
        fall: 0,
        health,
        score,
        look: SpriteLook::Normal,
        removed: false,
        kind: Kind::new(s, vars),
    })
}

impl Actors {
    /// The level's actors, in five passes by their sprites' draw order, so that
    /// decides what is in front.
    pub fn spawn_level(
        &mut self,
        level: &Level,
        sprites: &Sprites,
        state: &mut LevelState,
        random: &mut Random,
    ) {
        for pass in -1..=3 {
            let mut i = 0;
            while i < level.actors.len() {
                let a = level.actors[i];
                if (a.kind == NOT_EASY && state.difficulty == 1)
                    || (a.kind == HARD_ONLY && state.difficulty != 3)
                {
                    i += 2;
                    continue;
                }
                i += 1;
                let in_pass = sprites.draw_pass(a.kind) == Some(pass);
                if in_pass && self.slots.len() < MAX_ACTORS {
                    let (x, y) = (i32::from(a.x), i32::from(a.y));
                    if let Some(actor) = new_actor(a.kind, x, y, state, random) {
                        self.slots.push(Some(actor));
                    }
                }
            }
        }
    }

    /// An actor made during play goes into the first free slot (not the one
    /// updating now), or a new one at the end; nothing when all are taken.
    pub fn spawn(
        &mut self,
        level_type: u16,
        x: i32,
        y: i32,
        state: &mut LevelState,
        random: &mut Random,
    ) {
        let taken = self.current.filter(|_| !self.current_removed);
        let free = (0..self.slots.len()).find(|&i| self.slots[i].is_none() && Some(i) != taken);
        let slot = match free {
            Some(i) => i,
            None if self.slots.len() < MAX_ACTORS => {
                self.slots.push(None);
                self.slots.len() - 1
            }
            None => return,
        };
        self.slots[slot] = new_actor(level_type, x, y, state, random);
    }
}
