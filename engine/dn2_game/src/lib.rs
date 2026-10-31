//! The simulation: player, actors, level state and rules. Deterministic: it
//! never reads the clock and steps only when told to. Nothing here knows
//! about pixels or sound devices; the game reports what happened as events.

#![no_std]

extern crate alloc;

pub mod actors;
pub mod draw;
pub mod effects;
mod input;
pub mod map;
pub mod mechanics;
pub mod messages;
pub mod particles;
pub mod player;
mod random;
pub mod shots;
mod sound;
pub mod state;
mod world;

pub use draw::{Draw, Sprite, SpriteLook};
pub use input::Buttons;
pub use random::Random;
pub use sound::Sound;
pub use world::{GameEvent, World};

#[cfg(test)]
mod tests;
