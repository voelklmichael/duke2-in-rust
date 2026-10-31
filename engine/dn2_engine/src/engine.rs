use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::hd::HdArt;
use dn2_assets::{Files, GameData};
use dn2_audio::{Audio, EffectOptions};
use dn2_render::{Filter, Screen};

use crate::browser::Browser;
use crate::viewer::Viewer;
use crate::{AssetError, Buttons, FrameBuffer, Key, Platform};
use dn2_ui::{App, Input, UiEvent};

/// What the frontend chooses before starting the engine.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Frame pixels per game pixel, at least 1.
    pub scale: u32,
    /// How the picture is made larger.
    pub filter: Filter,
    /// The game screen's width in tiles: 40 is the original's 320 pixels,
    /// [`WIDESCREEN_COLUMNS`] fills a 16:9 display. The view gets the extra
    /// columns; menus and pictures are shown in the middle.
    pub columns: u16,
    pub start: Start,
}

/// Screen columns for 16:9: 424 by 200 pixels, shown with the original's
/// tall pixels (320x200 fills 4:3), is 1.77:1.
pub const WIDESCREEN_COLUMNS: u16 = 53;

impl Default for Settings {
    fn default() -> Self {
        Self {
            scale: 1,
            filter: Filter::Nearest,
            columns: 40,
            start: Start::Viewer,
        }
    }
}

/// What the engine shows first. Tab switches between the viewer and the
/// browser; Enter in the browser plays the level shown, and quitting it
/// goes back.
#[derive(Clone, Debug, Default)]
pub enum Start {
    /// The game as the original starts: the logo, the intro, the menus.
    #[default]
    Game,
    /// The asset viewer.
    Viewer,
    /// The level browser at a level (e.g. "L1.MNI"), optionally with the
    /// camera's top left at a map tile.
    Level {
        name: String,
        camera: Option<(u16, u16)>,
    },
    /// Playing a level.
    Play { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Viewer,
    Browser,
    /// The game; `from_browser` says whether it came from the browser.
    Game,
}

/// Timer ticks per game frame at the default speed.
pub const FRAME_TICKS: u32 = 8;

/// What happened, for the frontend to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngineEvent {
    /// The player chose to leave the game.
    Quit,
    /// A sound effect or music; [`Engine::mix_audio`] already plays it.
    Ui(UiEvent),
}

/// Game pixels of the original screen.
const SCREEN: (u32, u32) = (320, 200);

pub struct Engine {
    platform: Box<dyn Platform>,
    settings: Settings,
    data: GameData,
    /// Keys held now.
    keys: Vec<Key>,
    /// Keys pressed since the last update.
    pressed: Vec<Key>,
    buttons: Buttons,
    screen: Screen,
    mode: Mode,
    viewer: Viewer,
    browser: Browser,
    /// The program around the game, while it runs.
    app: Option<App>,
    /// The game was started from the browser and goes back to it.
    from_browser: bool,
    /// Ticks since the last browser frame.
    ticks: u32,
    /// The mixer, made at the rate of the first [`Engine::mix_audio`].
    audio: Option<Audio>,
    /// The song that plays, to restart it when the output rate changes.
    song: Option<String>,
}

impl Engine {
    /// Reads and parses all game data.
    pub fn new(platform: Box<dyn Platform>, settings: Settings) -> Result<Self, AssetError> {
        let files = Files::open(&*platform)?;
        log::info!(
            "{}: {} files",
            dn2_assets::ARCHIVE_NAME,
            files.archive().entries().len()
        );
        let mut data = GameData::load(&files)?;
        drop(files);
        data.hd = HdArt::load(&*platform, &data, settings.scale);
        for episode in &data.episodes {
            log::info!(
                "episode {}: {} to {}",
                episode.number,
                episode.levels[0],
                episode.levels[7]
            );
        }
        log::info!(
            "loaded {} levels, {} tilesets, {} backdrops, {} pictures, {} sprites, {} movies",
            data.levels.len(),
            data.tilesets.len(),
            data.backdrops.len(),
            data.pictures.len(),
            data.sprites.actors.iter().flatten().count(),
            data.movies.len()
        );
        let mut screen = Screen::new(SCREEN.0, SCREEN.1, data.palette.clone());
        screen.set_hd_scale(hd_scale(&data));
        let settings = Settings {
            scale: settings.scale.max(1),
            ..settings
        };
        let viewer = Viewer::new(&data);
        let (mode, browser, mut app, from_browser) = match &settings.start {
            Start::Game => (
                Mode::Game,
                Browser::new(&data, None),
                Some(App::new(&data, &*platform)),
                false,
            ),
            Start::Viewer => (Mode::Viewer, Browser::new(&data, None), None, false),
            Start::Level { name, camera } => {
                let mut browser = Browser::new(&data, Some(name));
                if let Some((x, y)) = *camera {
                    browser.set_camera(&data, x, y);
                }
                (Mode::Browser, browser, None, false)
            }
            Start::Play { name } => {
                let browser = Browser::new(&data, Some(name));
                match App::level(&data, &*platform, name) {
                    Some(app) => (Mode::Game, browser, Some(app), true),
                    None => (Mode::Browser, browser, None, false),
                }
            }
        };
        if let Some(app) = &mut app {
            prepare_app(app, &data, settings.columns);
        }
        Ok(Self {
            platform,
            settings,
            data,
            keys: Vec::new(),
            pressed: Vec::new(),
            buttons: Buttons::empty(),
            screen,
            mode,
            viewer,
            browser,
            app,
            from_browser,
            ticks: 0,
            audio: None,
            song: None,
        })
    }

    /// The screen on show now: the game's, or the engine's own (the asset
    /// viewer and the level browser draw on it).
    fn shown(&self) -> &Screen {
        match (&self.app, self.mode) {
            (Some(app), Mode::Game) => app.screen(),
            _ => &self.screen,
        }
    }

    /// The size [`Engine::render`] draws at now: the asset viewer and the
    /// level browser are the original's size, the game may be wider.
    pub fn frame_size(&self) -> (u32, u32) {
        let screen = self.shown();
        (
            screen.image.width * self.settings.scale,
            screen.image.height * self.settings.scale,
        )
    }

    pub fn key_down(&mut self, key: Key) {
        if !self.keys.contains(&key) {
            self.keys.push(key);
            self.pressed.push(key);
        }
    }

    pub fn key_up(&mut self, key: Key) {
        self.keys.retain(|&k| k != key);
    }

    /// Buttons held on a gamepad, in addition to those the keys give.
    pub fn set_buttons(&mut self, buttons: Buttons) {
        self.buttons = buttons;
    }

    /// Advances the engine by `ticks` of [`crate::TICK_HZ`].
    pub fn update(&mut self, ticks: u32) -> Vec<EngineEvent> {
        let pressed = core::mem::take(&mut self.pressed);
        match self.mode {
            Mode::Game => self.update_game(&pressed, ticks),
            Mode::Viewer | Mode::Browser => self.update_tools(&pressed, ticks),
        }
    }

    fn update_game(&mut self, pressed: &[Key], ticks: u32) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let Some(app) = &mut self.app else {
            self.mode = Mode::Browser;
            return events;
        };
        let input = Input {
            pressed,
            held: &self.keys,
            buttons: self.buttons,
            music_near_end: self.audio.as_ref().is_some_and(Audio::music_near_end),
        };
        let ui_events = app.update(&self.data, &mut *self.platform, &input, ticks);
        if let Some(audio) = &mut self.audio {
            audio.options = effect_options(app);
        }
        for event in ui_events {
            match event {
                UiEvent::Quit if self.from_browser => self.leave_game(),
                UiEvent::Quit => events.push(EngineEvent::Quit),
                UiEvent::Sound(sound) => {
                    if let Some(audio) = &mut self.audio {
                        audio.play(&self.data.sounds, sound);
                    }
                    events.push(EngineEvent::Ui(event));
                }
                UiEvent::Music(ref song) => {
                    self.set_song(song.clone());
                    events.push(EngineEvent::Ui(event));
                }
            }
        }
        events
    }

    /// Back from a game started in the browser.
    fn leave_game(&mut self) {
        (self.mode, self.app) = (Mode::Browser, None);
        self.set_song(None);
        if let Some(audio) = &mut self.audio {
            audio.stop_sounds();
        }
    }

    /// The asset viewer and the level browser.
    fn update_tools(&mut self, pressed: &[Key], ticks: u32) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        for &key in pressed {
            match (key, self.mode) {
                (Key::Escape, _) => events.push(EngineEvent::Quit),
                (Key::Tab, Mode::Viewer) => self.mode = Mode::Browser,
                (Key::Tab, Mode::Browser) => self.mode = Mode::Viewer,
                (Key::Enter, Mode::Browser) => {
                    let name = self.browser.level_name();
                    self.app = App::level(&self.data, &*self.platform, name);
                    if let Some(app) = &mut self.app {
                        prepare_app(app, &self.data, self.settings.columns);
                        (self.mode, self.from_browser) = (Mode::Game, true);
                    }
                }
                (key, Mode::Viewer) => self.viewer.key(key, &self.data),
                (key, Mode::Browser) => self.browser.key(key, &self.data),
                (_, Mode::Game) => {}
            }
        }
        match self.mode {
            Mode::Viewer => self.viewer.update(ticks, &self.data),
            Mode::Browser => {
                self.ticks += ticks;
                while self.ticks >= FRAME_TICKS {
                    self.ticks -= FRAME_TICKS;
                    let held = self.held();
                    self.browser.step(held, &self.data);
                }
            }
            Mode::Game => {}
        }
        events
    }

    /// Starts a song by file name, or stops the music.
    fn set_song(&mut self, name: Option<String>) {
        let name = name.map(|n| n.to_ascii_uppercase());
        if let Some(audio) = &mut self.audio {
            match name.as_ref().and_then(|n| self.data.songs.get(n)) {
                Some(song) => audio.start_music(song),
                None => audio.stop_music(),
            }
        }
        if let Some(n) = name.as_ref().filter(|n| !self.data.songs.contains_key(*n)) {
            log::warn!("song {n} not found");
        }
        self.song = name;
    }

    /// Fills `out` with mono samples at `rate` Hz, -1.0 to 1.0: the music
    /// and the effects of the events [`Engine::update`] returned. Call it
    /// with as many samples as the time that passed, so the music keeps
    /// its speed.
    pub fn mix_audio(&mut self, out: &mut [f32], rate: u32) {
        if self.audio.as_ref().is_none_or(|a| a.rate() != rate) {
            let mut audio = Audio::new(rate);
            if let Some(app) = &self.app {
                audio.options = effect_options(app);
            }
            self.audio = Some(audio);
            self.set_song(self.song.clone());
        }
        if let Some(audio) = &mut self.audio {
            audio.render(&self.data.sounds, out);
        }
    }

    fn held(&self) -> Buttons {
        let mut held = self.buttons;
        for key in &self.keys {
            held |= match key {
                Key::Up => Buttons::UP,
                Key::Down => Buttons::DOWN,
                Key::Left => Buttons::LEFT,
                Key::Right => Buttons::RIGHT,
                Key::LeftCtrl => Buttons::JUMP,
                Key::LeftAlt => Buttons::FIRE,
                _ => Buttons::empty(),
            };
        }
        held
    }

    /// Draws the current screen; `frame` is resized to [`Engine::frame_size`].
    pub fn render(&mut self, frame: &mut FrameBuffer) {
        match self.mode {
            Mode::Viewer => self.viewer.draw(&self.data, &mut self.screen),
            Mode::Browser => self.browser.draw(&self.data, &mut self.screen),
            Mode::Game => {}
        }
        self.shown()
            .present(frame, self.settings.scale, self.settings.filter);
    }
}

/// The scale of the higher-detail overlay: that of the pictures, if there
/// are any.
fn hd_scale(data: &GameData) -> Option<u32> {
    (!data.hd.is_empty()).then_some(data.hd.scale)
}

/// Fits a new game to the engine's screen.
fn prepare_app(app: &mut App, data: &GameData, columns: u16) {
    app.set_hd_scale(hd_scale(data));
    app.set_columns(columns);
}

/// The options menu's S and L.
fn effect_options(app: &App) -> EffectOptions {
    let options = app.options();
    EffectOptions {
        digitized: options.sound_blaster,
        adlib: options.adlib,
    }
}
