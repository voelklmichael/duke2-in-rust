//! The renderer: draws the world into an RGB frame of any size, in game
//! pixels (8x8 tiles) times an integer scale.

#![no_std]

extern crate alloc;

mod frame;
pub mod hud;
pub mod scale;
mod screen;
pub mod view;
pub mod xbr;

pub use frame::FrameBuffer;
pub use scale::Filter;
pub use screen::{HdBlock, HdPicture, ORIGINAL_WIDTH, Screen};
