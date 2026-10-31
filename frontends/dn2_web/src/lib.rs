//! Duke Nukem II in a web page. The page (`www/main.js`) runs the loop: it
//! gets the game files, then each animation frame tells [`Game`] the ticks
//! that passed, draws what [`Game::render`] returns and keeps the sound
//! queue full with [`Game::mix`].

mod keys;
mod platform;

use std::collections::HashMap;

use dn2_engine::{
    ARCHIVE_NAME, Engine, EngineEvent, Filter, FrameBuffer, Key, MOVIES, Settings, Start, TICK_HZ,
    WIDESCREEN_COLUMNS,
};
use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;
use web_sys::ImageData;

use platform::WebPlatform;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
    log::set_logger(&ConsoleLog).ok();
    log::set_max_level(log::LevelFilter::Info);
}

/// The files the page fetches from the server when it has them.
#[wasm_bindgen]
pub fn data_files() -> Vec<String> {
    std::iter::once(ARCHIVE_NAME)
        .chain(MOVIES)
        .map(String::from)
        .collect()
}

/// The engine's clock rate: [`Game::update`] counts in these ticks.
#[wasm_bindgen]
pub fn tick_hz() -> u32 {
    TICK_HZ
}

#[wasm_bindgen]
pub struct Game {
    engine: Engine,
    frame: FrameBuffer,
    /// The frame as RGBA bytes, reused.
    rgba: Vec<u8>,
    /// Keys passed to the engine as down and not yet up.
    pressed: Vec<Key>,
}

#[wasm_bindgen]
impl Game {
    /// Loads the game. `files` maps file names to their bytes
    /// (`Uint8Array`); `pictures`, if given, maps the names of an HD pack's
    /// pictures (`CZONE1.MNI/solid`, without `.png`) to their PNG bytes;
    /// `options` are the launcher's, as a query string: `wide`,
    /// `filter=epx`, `scale=N`, `play=L1`, `level=L1`, `viewer`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        files: js_sys::Map,
        pictures: Option<js_sys::Map>,
        options: &str,
    ) -> Result<Game, JsError> {
        let data = byte_map(&files);
        let pictures = pictures.as_ref().map(byte_map).unwrap_or_default();
        let settings = parse_options(options, !pictures.is_empty());
        let platform = WebPlatform::new(data, pictures);
        let engine = Engine::new(Box::new(platform), settings)
            .map_err(|e| JsError::new(&format!("cannot load the game data: {e}")))?;
        Ok(Self {
            engine,
            frame: FrameBuffer::new(0, 0),
            rgba: Vec::new(),
            pressed: Vec::new(),
        })
    }

    /// Passes a key press on (`KeyboardEvent.code`); false if the game does
    /// not use the key, so the browser may.
    pub fn key_down(&mut self, code: &str) -> bool {
        let Some(key) = keys::translate(code) else {
            return false;
        };
        if !self.pressed.contains(&key) {
            self.pressed.push(key);
        }
        self.engine.key_down(key);
        true
    }

    pub fn key_up(&mut self, code: &str) -> bool {
        let Some(key) = keys::translate(code) else {
            return false;
        };
        self.pressed.retain(|&k| k != key);
        self.engine.key_up(key);
        true
    }

    /// Lets go of all keys, when the page loses the focus.
    pub fn release_all(&mut self) {
        for key in self.pressed.drain(..) {
            self.engine.key_up(key);
        }
    }

    /// Advances by `ticks` of [`TICK_HZ`]; false once the player quit.
    pub fn update(&mut self, ticks: u32) -> bool {
        // Sounds and music are already in the engine's mix.
        !self.engine.update(ticks).contains(&EngineEvent::Quit)
    }

    /// Fills `out` with mono samples at `rate` Hz.
    pub fn mix(&mut self, out: &mut [f32], rate: u32) {
        self.engine.mix_audio(out, rate);
    }

    /// Draws the current screen. Its size may change between frames (the
    /// viewer is narrower than the wide game).
    pub fn render(&mut self) -> Result<ImageData, JsValue> {
        self.engine.render(&mut self.frame);
        self.rgba.clear();
        self.rgba.extend(
            self.frame
                .pixels
                .iter()
                .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]),
        );
        ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.rgba),
            self.frame.width,
            self.frame.height,
        )
    }
}

/// A `Map` of names to `Uint8Array`s, by upper-cased name.
fn byte_map(map: &js_sys::Map) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    map.for_each(&mut |bytes, name| {
        if let (Some(name), Ok(bytes)) = (name.as_string(), bytes.dyn_into::<js_sys::Uint8Array>())
        {
            out.insert(name.to_ascii_uppercase(), bytes.to_vec());
        }
    });
    out
}

/// The settings the query asks for. Without `scale=N` the scale is 1, or
/// 3 with a smoothing filter or an HD pack (`hd`).
fn parse_options(query: &str, hd: bool) -> Settings {
    let mut settings = Settings {
        start: Start::Game,
        ..Settings::default()
    };
    let mut scale = None;
    let file = |name: &str| {
        let name = name.to_ascii_uppercase();
        if name.contains('.') {
            name
        } else {
            format!("{name}.MNI")
        }
    };
    for pair in query.trim_start_matches('?').split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "wide" => settings.columns = WIDESCREEN_COLUMNS,
            "filter" if value == "epx" => settings.filter = Filter::Epx,
            "scale" => scale = value.parse().ok(),
            "play" => settings.start = Start::Play { name: file(value) },
            "level" => {
                settings.start = Start::Level {
                    name: file(value),
                    camera: None,
                }
            }
            "viewer" => settings.start = Start::Viewer,
            _ => {}
        }
    }
    let default_scale = if settings.filter == Filter::Nearest && !hd {
        1
    } else {
        3
    };
    settings.scale = scale.unwrap_or(default_scale).max(1);
    settings
}

/// Sends log records to the browser console.
struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        let text = JsValue::from(format!("{}: {}", record.target(), record.args()));
        match record.level() {
            log::Level::Error => web_sys::console::error_1(&text),
            log::Level::Warn => web_sys::console::warn_1(&text),
            _ => web_sys::console::log_1(&text),
        }
    }

    fn flush(&self) {}
}
