//! Walks through the levels without playing them: the arrow keys move the
//! camera, PageUp/PageDown change the level. The placed actors are shown with
//! the sprite of the same id, frame 0, which is right for many of them and a
//! stand-in until the actors exist.

use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_render::Screen;
use dn2_render::hud::{Hud, draw_hud};
use dn2_render::view::{
    DrawMode, HdGraphics, LevelGraphics, SpriteDraw, VIEW_COLUMNS, VIEW_ROWS, ViewDraw, ViewState,
    draw_level,
};

use crate::{Buttons, Key};

/// Actor types that mark where the player starts.
const PLAYER_START: [u16; 2] = [5, 6];

pub(crate) struct Browser {
    levels: Vec<String>,
    level: usize,
    state: ViewState,
}

impl Browser {
    /// Starts at `level` (a file name like "L1.MNI"), or the first level.
    pub fn new(data: &GameData, level: Option<&str>) -> Self {
        let levels: Vec<String> = data
            .episodes
            .iter()
            .flat_map(|e| e.levels.iter().cloned())
            .collect();
        let level = level
            .and_then(|name| levels.iter().position(|l| l.eq_ignore_ascii_case(name)))
            .unwrap_or(0);
        let mut browser = Self {
            levels,
            level,
            state: ViewState::default(),
        };
        browser.enter(data);
        browser
    }

    pub fn level_name(&self) -> &str {
        &self.levels[self.level]
    }

    /// Puts the camera on the player's start, as far as the map allows.
    fn enter(&mut self, data: &GameData) {
        let level = &data.levels[self.level_name()];
        let start = level.actors.iter().find(|a| PLAYER_START.contains(&a.kind));
        let (x, y) = start.map_or((0, 0), |a| (a.x, a.y));
        self.state = ViewState {
            camera_x: x.saturating_sub(VIEW_COLUMNS / 2),
            camera_y: y.saturating_sub(VIEW_ROWS / 2),
            frame: 0,
        };
        self.clamp(data);
        log::info!(
            "{}: {}x{} tiles, {} actors, {} / {}, flags {:?}",
            self.level_name(),
            level.width,
            level.height,
            level.actors.len(),
            level.tileset,
            level.backdrop,
            level.flags
        );
    }

    /// Moves the camera to a map tile.
    pub fn set_camera(&mut self, data: &GameData, x: u16, y: u16) {
        (self.state.camera_x, self.state.camera_y) = (x, y);
        self.clamp(data);
    }

    fn clamp(&mut self, data: &GameData) {
        let level = &data.levels[self.level_name()];
        self.state.camera_x = self
            .state
            .camera_x
            .min(level.width.saturating_sub(VIEW_COLUMNS));
        self.state.camera_y = self
            .state
            .camera_y
            .min(level.height.saturating_sub(VIEW_ROWS));
    }

    pub fn key(&mut self, key: Key, data: &GameData) {
        let count = self.levels.len();
        match key {
            Key::PageDown => self.level = (self.level + 1) % count,
            Key::PageUp => self.level = (self.level + count - 1) % count,
            _ => return,
        }
        self.enter(data);
    }

    /// One game frame: the camera follows the held arrows.
    pub fn step(&mut self, held: Buttons, data: &GameData) {
        let s = &mut self.state;
        if held.contains(Buttons::LEFT) {
            s.camera_x = s.camera_x.saturating_sub(1);
        }
        if held.contains(Buttons::RIGHT) {
            s.camera_x += 1;
        }
        if held.contains(Buttons::UP) {
            s.camera_y = s.camera_y.saturating_sub(1);
        }
        if held.contains(Buttons::DOWN) {
            s.camera_y += 1;
        }
        s.frame += 1;
        self.clamp(data);
    }

    pub fn draw(&self, data: &GameData, screen: &mut Screen) {
        let level = &data.levels[self.level_name()];
        let (Some(tileset), Some(backdrop)) = (
            data.tilesets.get(&level.tileset),
            data.backdrops.get(&level.backdrop),
        ) else {
            log::error!("{}: tileset or backdrop missing", self.level_name());
            return;
        };
        screen.palette = data.palette.clone();
        screen.clear(0);
        let sprites: Vec<ViewDraw> = level
            .actors
            .iter()
            .map(|a| {
                ViewDraw::Sprite(SpriteDraw {
                    id: a.kind,
                    frame: 0,
                    x: a.x.into(),
                    y: a.y.into(),
                    mode: DrawMode::Normal,
                })
            })
            .collect();
        let graphics = LevelGraphics {
            level,
            tileset,
            backdrop,
            sprites: &data.sprites,
            hd: HdGraphics::new(&data.hd, &level.tileset, &level.backdrop),
        };
        draw_level(screen, &graphics, &self.state, &sprites);
        draw_hud(
            screen,
            &data.status,
            &data.sprites,
            &data.hd,
            &Hud::default(),
        );
    }
}
