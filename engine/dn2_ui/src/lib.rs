//! The screens around the game: the text-script interpreter (TEXT.MNI,
//! OPTIONS.MNI, HELP.MNI), menus, intro and attract mode, bonus screens.

#![no_std]

extern crate alloc;

mod app;
pub mod config;
pub mod demo;
pub mod draw;
pub mod game;
mod key;
pub mod movie;
mod play;
pub mod runner;
pub mod saves;
pub mod scene;

pub use app::{App, Input};
pub use key::Key;

use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_game::Sound;
use dn2_render::Screen;

use draw::Gfx;

/// What the screens ask of the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Sound(Sound),
    /// Start a song (an IMF file name), or stop the music.
    Music(Option<String>),
    /// The player chose to quit.
    Quit,
}

/// What a screen gets for one step: the screen to draw on (its palette and
/// fade included), the data, the input and the time.
pub struct Ctx<'a> {
    pub screen: &'a mut Screen,
    pub data: &'a GameData,
    /// Keys pressed since the last step, in order.
    pub pressed: &'a [Key],
    /// Whether any key is held now.
    pub held: bool,
    /// Ticks of [`TICK_HZ`] since the last step.
    pub ticks: u32,
    pub events: &'a mut Vec<UiEvent>,
}

impl<'a> Ctx<'a> {
    pub fn gfx(&self) -> Gfx<'a> {
        Gfx {
            status: &self.data.status,
            sprites: &self.data.sprites,
            font: &self.data.font,
            hd: &self.data.hd,
        }
    }

    pub fn sound(&mut self, sound: Sound) {
        self.events.push(UiEvent::Sound(sound));
    }
}

/// The timer the screens count in, as in the original (its timer
/// interrupt counted delays at 140 Hz).
pub const TICK_HZ: u32 = 140;

/// The engine's own files (configuration, saved games, high scores), kept
/// by the platform under a key.
pub trait Store {
    fn load_store(&self, key: &str) -> Option<Vec<u8>>;
    fn save_store(&mut self, key: &str, bytes: &[u8]);
}
