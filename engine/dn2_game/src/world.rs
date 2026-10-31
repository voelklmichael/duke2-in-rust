//! A level being played.

use alloc::vec::Vec;

use dn2_assets::level::Level;
use dn2_assets::sprites::Sprites;
use dn2_assets::tileset::Tileset;

use crate::actors::spawn::{START_LEFT, START_RIGHT};
use crate::actors::{self, Actors, Ctx};
use crate::draw::{self, Draw, Sprite, SpriteLook};
use crate::effects::{EffectCtx, Effects};
use crate::map::Map;
use crate::mechanics;
use crate::particles::Particles;
use crate::player::{self, Facing, Player, PlayerState, SpiderPlaces};
use crate::shots::{self, Out, Shot};
use crate::state::{LevelExit, LevelState};
use crate::{Buttons, Random, Sound};

/// What happened in a game frame, for the engine to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GameEvent {
    Sound(Sound),
    /// A transporter sends the player away: the original fades the view
    /// out here, then [`World::finish_teleport`], a frame, and the fade in.
    Teleport,
}

pub struct World {
    pub map: Map,
    pub player: Player,
    pub actors: Actors,
    pub shots: Vec<Shot>,
    pub effects: Effects,
    pub particles: Particles,
    pub state: LevelState,
    /// The view's top left map tile.
    pub camera: (u16, u16),
    /// Game frames since the level started.
    pub frame: u32,
    /// The original's draw page: set on a level's first frame and after a
    /// respawn, then flipping every frame. Actors use it as a half-rate clock.
    pub draw_page: bool,
    pub random: Random,
    /// What the last frame drew over the map, in order.
    pub draws: Vec<Draw>,
}

impl World {
    /// The level at its start, at `difficulty` (1 easy to 3 hard).
    pub fn new(level: &Level, tileset: &Tileset, sprites: &Sprites, difficulty: u8) -> Self {
        let map = Map::new(level, tileset);
        // As in the original, a later start marker replaces an earlier one.
        let start = level
            .actors
            .iter()
            .rfind(|a| a.kind == START_LEFT || a.kind == START_RIGHT);
        let player = match start {
            Some(a) => Player::new(
                i32::from(a.x),
                i32::from(a.y),
                if a.kind == START_LEFT {
                    Facing::Left
                } else {
                    Facing::Right
                },
            ),
            None => Player::new(0, 0, Facing::Right),
        };
        let mut world = Self {
            map,
            player,
            actors: Actors::default(),
            shots: Vec::new(),
            effects: Effects::default(),
            particles: Particles::default(),
            state: LevelState {
                difficulty,
                ..LevelState::default()
            },
            camera: (0, 0),
            frame: 0,
            draw_page: true,
            random: Random::default(),
            draws: Vec::new(),
        };
        world
            .actors
            .spawn_level(level, sprites, &mut world.state, &mut world.random);
        world.camera = player::center_camera(&world.player, &world.map);
        world
    }

    /// The player comes back at the respawn beacon after dying, with the map
    /// and the actors as they are: the health, weapon and ammunition the beacon
    /// saved (a health of 1 becomes 2), a moment of not being hurt; what is
    /// short-lived starts over. False if no beacon was touched.
    pub fn respawn(&mut self) -> bool {
        let Some(checkpoint) = self.state.checkpoint else {
            return false;
        };
        let old = core::mem::take(&mut self.player);
        let mut player = Player::new(checkpoint.x, checkpoint.y, Facing::Right);
        player.inventory = old.inventory;
        player.cloak_timer = old.cloak_timer;
        player.rapid_fire_timer = old.rapid_fire_timer;
        player.was_hurt = old.was_hurt;
        player.weapon = checkpoint.weapon;
        player.ammo = checkpoint.ammo;
        player.health = if checkpoint.health == 1 {
            2
        } else {
            checkpoint.health
        };
        self.player = player;
        self.camera = player::center_camera(&self.player, &self.map);
        self.state.exit = None;
        self.state.teleport = None;
        self.state.backdrop_alternate = checkpoint.backdrop_alternate;
        self.state.spiders = SpiderPlaces::default();
        self.state.grabber = None;
        self.state.fan = None;
        self.state.debris_timer = 0;
        self.state.flash = None;
        self.effects = Effects::default();
        self.shots.clear();
        self.particles = Particles::default();
        self.random = Random::default();
        self.draw_page = true;
        true
    }

    /// The actors' view of the world.
    fn ctx<'a>(
        &'a mut self,
        sprites: &'a Sprites,
        events: &'a mut Vec<GameEvent>,
        input: Buttons,
    ) -> Ctx<'a> {
        Ctx {
            map: &mut self.map,
            sprites,
            player: &mut self.player,
            camera: self.camera,
            random: &mut self.random,
            state: &mut self.state,
            actors: &mut self.actors,
            effects: &mut self.effects,
            particles: &mut self.particles,
            shots: &mut self.shots,
            draws: &mut self.draws,
            events,
            input,
            draw_page: self.draw_page,
        }
    }

    /// Moves the player to where a transporter sent him (one tile right
    /// of the other pad), with the camera on him. False if none did.
    pub fn finish_teleport(&mut self) -> bool {
        let Some((x, y)) = self.state.teleport.take() else {
            return false;
        };
        self.player.x = x + 1;
        self.player.y = y;
        self.camera = player::center_camera(&self.player, &self.map);
        true
    }

    /// One game frame. A transporter's move is left to the caller
    /// ([`GameEvent::Teleport`]); one not done yet is done first.
    pub fn step(&mut self, input: Buttons, sprites: &Sprites) -> Vec<GameEvent> {
        self.finish_teleport();
        let mut events = Vec::new();
        let mut requests = Vec::new();
        self.draws.clear();
        self.player.clinging = self.state.spiders.map(|s| s.is_some());
        self.player.update(
            input,
            &self.map,
            sprites,
            &mut self.random,
            &mut requests,
            &mut events,
        );
        self.player.death_effects(
            &mut self.effects,
            &mut self.particles,
            &self.map,
            sprites,
            &mut self.random,
        );
        self.camera = player::follow_camera(self.camera, &mut self.player, &self.map);
        self.player.item_slot_active = false;
        self.state.flash = None;
        for request in &requests {
            shots::spawn(
                &mut self.shots,
                &mut self.effects,
                &self.map,
                sprites,
                request,
            );
        }
        let mut c = self.ctx(sprites, &mut events, input);
        if let Some((kind, x, y)) = c.player.left_ship.take() {
            c.spawn(kind, x, y);
        }
        mechanics::update_backdrop_and_earthquake(&mut c);
        mechanics::update_map_sections(&mut c);
        actors::update(&mut c);
        self.particles.update(&mut self.draws);
        let mut out = Out {
            random: &mut self.random,
            effects: &mut self.effects,
            events: &mut events,
            draws: &mut self.draws,
        };
        let (map, camera) = (&self.map, self.camera);
        self.shots
            .retain_mut(|shot| shot.update(map, sprites, camera, &mut out));
        let mut e = EffectCtx {
            map: &mut self.map,
            sprites,
            camera: self.camera,
            random: &mut self.random,
            draws: &mut self.draws,
            events: &mut events,
            player: self.player.placed(),
            hurt: false,
        };
        self.effects.update(&mut e);
        if e.hurt {
            self.player.damage(self.state.difficulty, &mut events);
        }
        let mut c = self.ctx(sprites, &mut events, input);
        actors::update_overlays(&mut c);
        mechanics::update_tile_debris(&mut self.state, self.camera, &mut self.draws);
        if self.state.teleport.is_some() {
            events.push(GameEvent::Teleport);
        }
        if self.player.state == PlayerState::Dying
            && self.player.death_timer >= player::DEATH_FRAMES
        {
            self.state.exit.get_or_insert(LevelExit::Died);
        }
        self.frame += 1;
        self.draw_page = !self.draw_page;
        events
    }

    /// The top left map tile of a view `columns` wide (more than the
    /// original's 32): around the original view at [`Self::camera`], inside
    /// the map where it fits.
    pub fn view_camera(&self, columns: u16) -> (u16, u16) {
        let extra = i32::from(columns) - player::VIEW_WIDTH;
        if extra <= 0 {
            return self.camera;
        }
        let max = (i32::from(self.map.width) - i32::from(columns)).max(0);
        let x = (i32::from(self.camera.0) - extra / 2).clamp(0, max);
        (x as u16, self.camera.1)
    }

    /// What a view `columns` wide shows besides [`Self::draws`]: the actors
    /// outside the original's view, as they are. Only what is in the
    /// original's view moves, so a wider view does not change the game.
    pub fn margin_draws(&self, sprites: &Sprites, columns: u16) -> Vec<Draw> {
        if i32::from(columns) <= player::VIEW_WIDTH {
            return Vec::new();
        }
        let view = self.view_camera(columns);
        let size = (i32::from(columns), player::VIEW_HEIGHT);
        self.actors
            .iter()
            .filter(|a| {
                a.look != SpriteLook::Hidden
                    && !draw::on_screen(sprites, self.camera, a.placed())
                    && draw::in_view(sprites, view, size, a.placed())
            })
            .map(|a| {
                Draw::Sprite(Sprite {
                    id: a.id,
                    frame: a.frame,
                    x: a.x,
                    y: a.y,
                    look: a.look,
                })
            })
            .collect()
    }
}
