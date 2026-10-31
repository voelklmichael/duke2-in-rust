use alloc::vec;
use alloc::vec::Vec;

/// A frame of 0x00RRGGBB pixels, row by row.
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl FrameBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    /// Keeps the buffer at this size, clearing it if the size changes.
    pub fn resize(&mut self, width: u32, height: u32) {
        if (self.width, self.height) != (width, height) {
            *self = Self::new(width, height);
        }
    }
}
