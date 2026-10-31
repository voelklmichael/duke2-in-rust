//! The Duke Nukem II engine as frontends see it: implement [`Platform`],
//! create an [`Engine`], then feed it input and time and let it draw.
//!
//! The engine never does I/O, never reads the clock and never blocks: all
//! game data is read through the platform in [`Engine::new`], and time
//! arrives as ticks of [`TICK_HZ`].

#![no_std]

extern crate alloc;

mod browser;
mod engine;
mod viewer;

pub use dn2_assets::gfx::RgbaImage;
pub use dn2_assets::{ARCHIVE_NAME, AssetError, MOVIES, ReadFile};
pub use dn2_game::Buttons;
pub use dn2_render::{Filter, FrameBuffer, Screen};
pub use dn2_ui::{Key, Store};
pub use engine::{Engine, EngineEvent, FRAME_TICKS, Settings, Start, WIDESCREEN_COLUMNS};

/// The clock the engine runs on, as in the original (its timer interrupt
/// counted delays at 140 Hz). The game steps every 8 ticks at the default
/// speed, about 17.5 times a second.
pub use dn2_ui::TICK_HZ;

/// What a frontend provides.
///
/// [`ReadFile`] reads loose files from the game folder (`NUKEM2.CMP` and any
/// file that overrides the archive), by DOS name in any case, and the
/// pictures of a higher-detail pack, if there is one. The store
/// keeps the engine's own files: configuration, saved games, high scores.
pub trait Platform: ReadFile + Store {}

impl<T: ReadFile + Store> Platform for T {}
