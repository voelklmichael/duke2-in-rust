//! A game in progress: the session that lives from level to level (starting a
//! game, loading a level) and the level being played, drawn with the status
//! bar.

use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_assets::level::LevelFlags;
use dn2_game::messages::{LAST_MESSAGE_LEVEL, text};
use dn2_game::player::{START_AMMO, Weapon};
use dn2_game::{Buttons, Draw, GameEvent, SpriteLook, World};
use dn2_render::Screen;
use dn2_render::hud::{self, Hud, draw_hud};
use dn2_render::view::{
    DrawMode, HdGraphics, LevelGraphics, SpriteDraw, ViewDraw, ViewState, draw_level, view_columns,
};

use crate::UiEvent;

/// What the player takes into the next level, and back to the start of a level
/// after dying (the 'T' save).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carry {
    pub score: u32,
    pub weapon: Weapon,
    pub ammo: u8,
    /// The one-shot messages already shown.
    pub shown: u32,
}

impl Default for Carry {
    fn default() -> Self {
        Self {
            score: 0,
            weapon: Weapon::Regular,
            ammo: START_AMMO,
            shown: 0,
        }
    }
}

impl Carry {
    pub fn of(world: &World) -> Self {
        Self {
            score: world.state.score,
            weapon: world.player.weapon,
            ammo: world.player.ammo,
            shown: world.state.messages.shown,
        }
    }

    fn apply(self, world: &mut World) {
        world.state.score = self.score;
        world.player.weapon = self.weapon;
        world.player.ammo = self.ammo;
        world.state.messages.shown = self.shown;
    }
}

/// Which level of which episode, with what the player had at its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    /// 1 to 4.
    pub episode: u8,
    /// 1 to 8.
    pub level: u8,
    /// 1 easy, 2 medium, 3 hard.
    pub difficulty: u8,
    pub start: Carry,
}

impl Session {
    pub fn new(episode: u8, difficulty: u8) -> Self {
        Self {
            episode,
            level: 1,
            difficulty,
            start: Carry::default(),
        }
    }
}

/// The level file of a level of an episode, if the data has the episode.
pub fn level_file(data: &GameData, episode: u8, level: u8) -> Option<&str> {
    let episode = data.episodes.iter().find(|e| e.number == episode)?;
    Some(
        episode
            .levels
            .get(usize::from(level).checked_sub(1)?)?
            .as_str(),
    )
}

/// The episode and level (from 1) of a level file, e.g. "M3.MNI".
pub fn level_of(data: &GameData, file: &str) -> Option<(u8, u8)> {
    data.episodes.iter().find_map(|e| {
        let at = e.levels.iter().position(|l| l.eq_ignore_ascii_case(file))?;
        Some((e.number, at as u8 + 1))
    })
}

/// A level being played.
pub struct Level {
    /// The level file.
    pub name: String,
    /// From 1.
    pub number: u8,
    pub world: World,
}

impl Level {
    /// The session's level at its start, with what the player carries.
    /// `None` if the data lacks the level or its tileset.
    pub fn load(data: &GameData, session: &Session) -> Option<Self> {
        let name = level_file(data, session.episode, session.level)?;
        Self::load_file(data, name, session.level, session.difficulty, session.start)
    }

    /// A level by file name, numbered `number` for the status bar and the
    /// messages.
    pub fn load_file(
        data: &GameData,
        name: &str,
        number: u8,
        difficulty: u8,
        carry: Carry,
    ) -> Option<Self> {
        let (name, level) = data
            .levels
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))?;
        let tileset = data.tilesets.get(&level.tileset)?;
        log::info!("playing {name}");
        let mut world = World::new(level, tileset, &data.sprites, difficulty);
        carry.apply(&mut world);
        // Messages only show on an episode's first levels.
        world.state.messages.disabled = number > LAST_MESSAGE_LEVEL;
        if world.state.radar_dishes != 0 {
            world.state.messages.show(text::RADAR);
        }
        Some(Self {
            name: name.clone(),
            number,
            world,
        })
    }

    /// The song of the level: CALM.IMF on the last one of an episode, else the
    /// level's own.
    pub fn music(&self, data: &GameData) -> String {
        if self.number == 8 {
            return String::from("CALM.IMF");
        }
        data.levels
            .get(&self.name)
            .map(|l| l.music.clone())
            .unwrap_or_default()
    }

    /// One game frame.
    pub fn step(&mut self, held: Buttons, data: &GameData, events: &mut Vec<UiEvent>) {
        for event in self.world.step(held, &data.sprites) {
            if let GameEvent::Sound(sound) = event {
                events.push(UiEvent::Sound(sound));
            }
        }
    }

    pub fn draw(&self, data: &GameData, screen: &mut Screen) {
        let level = &data.levels[&self.name];
        let world = &self.world;
        // After the backdrop switch, a level with a half-speed backdrop
        // shows the alternate one (DROPn.MNI).
        let alternate =
            world.state.backdrop_alternate && level.flags.contains(LevelFlags::PARALLAX_HORIZONTAL);
        let alternate_name = alloc::format!("DROP{}.MNI", level.alternate_backdrop);
        let backdrop_name = if alternate && data.backdrops.contains_key(&alternate_name) {
            &alternate_name
        } else {
            &level.backdrop
        };
        let backdrop = data.backdrops.get(backdrop_name);
        let (Some(tileset), Some(backdrop)) = (data.tilesets.get(&level.tileset), backdrop) else {
            return;
        };
        screen.palette = data.palette.clone();
        screen.clear(0);
        // A widescreen's view is wider than the original's 32 columns: it
        // shows the actors beyond them too, behind the rest.
        let columns = view_columns(screen);
        let camera = world.view_camera(columns);
        let state = ViewState {
            camera_x: camera.0,
            camera_y: camera.1,
            frame: world.frame,
        };
        let margin = world.margin_draws(&data.sprites, columns);
        let draws: Vec<ViewDraw> = margin
            .iter()
            .chain(&world.draws)
            .map(|d| match *d {
                Draw::Sprite(s) => ViewDraw::Sprite(SpriteDraw {
                    id: s.id,
                    frame: s.frame,
                    x: s.x,
                    y: s.y,
                    mode: match s.look {
                        SpriteLook::Hidden => DrawMode::Hidden,
                        SpriteLook::Normal => DrawMode::Normal,
                        SpriteLook::WhiteFlash => DrawMode::WhiteFlash,
                        SpriteLook::OverForeground => DrawMode::OverForeground,
                        SpriteLook::OnePlane => DrawMode::OnePlane,
                    },
                }),
                Draw::Pixel { x, y, color } => ViewDraw::Pixel { x, y, color },
                Draw::Tile { cell, x, y } => ViewDraw::Tile { cell, x, y },
                Draw::Water { wave, x, y } => ViewDraw::Water { wave, x, y },
            })
            .collect();
        let graphics = LevelGraphics {
            level: &world.map.level,
            tileset,
            backdrop,
            sprites: &data.sprites,
            hd: HdGraphics::new(&data.hd, &level.tileset, backdrop_name),
        };
        if let Some(color) = world.state.flash {
            // A big explosion: the view is one colour for a frame.
            dn2_render::view::fill_view(screen, color);
        } else {
            draw_level(screen, &graphics, &state, &draws);
        }
        draw_hud(screen, &data.status, &data.sprites, &data.hd, &self.hud());
        // The earthquake's shake moves the whole picture, status bar too.
        screen.pan(u32::from(world.state.shake));
    }

    fn hud(&self) -> Hud {
        let player = &self.world.player;
        // The page of the frame just played.
        let draw_page = !self.world.draw_page;
        let mut items = [None; 6];
        let mut items_hidden = [false; 6];
        for (i, &item) in player.inventory.iter().take(6).enumerate() {
            items[i] = Some(item);
            items_hidden[i] = player.item_hidden(item, draw_page);
        }
        Hud {
            score: self.world.state.score,
            level: Some(self.number),
            weapon: match player.weapon {
                Weapon::Regular => hud::Weapon::Regular,
                Weapon::Laser => hud::Weapon::Laser,
                Weapon::Rocket => hud::Weapon::Rocket,
                Weapon::FlameThrower => hud::Weapon::FlameThrower,
            },
            health: player.health,
            ammo: player.ammo,
            items,
            items_hidden,
            letters: core::array::from_fn(|i| self.world.state.letters & (0x100 << i) != 0),
            message: self.world.state.messages.line().to_vec(),
            // The health is a signed word.
            boss_health: self
                .world
                .state
                .boss_bar
                .then(|| (self.world.state.boss_health as i16).clamp(0, 255) as u8),
        }
    }
}
