//! Pixels as the game stores them. The game runs in EGA mode 0Dh: 16
//! colours, every graphic built from 8x8 tiles whose pixels are split over
//! four bit planes (blue, green, red, intensity; the leftmost pixel is the
//! top bit). A pixel's colour index is `b | g << 1 | r << 2 | i << 3`.

use alloc::vec;
use alloc::vec::Vec;

use crate::AssetError;
use crate::reader::Reader;

/// Pixel value for "nothing drawn here".
pub const TRANSPARENT: u8 = 0xff;

/// Bytes of one solid tile: 8 rows of blue, green, red, intensity bytes.
pub const SOLID_TILE_BYTES: usize = 32;
/// Bytes of one masked tile: 8 rows of mask (1 = transparent), blue, green,
/// red, intensity bytes.
pub const MASKED_TILE_BYTES: usize = 40;

/// An 8x8 tile, row by row: colour indices or [`TRANSPARENT`].
pub type Tile = [u8; 64];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// As 0x00RRGGBB.
    pub fn to_u32(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }

    /// From a 6-bit VGA DAC value per channel.
    fn from_dac(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: widen(r.min(63)),
            g: widen(g.min(63)),
            b: widen(b.min(63)),
        }
    }
}

/// A 6-bit VGA DAC value as 8 bits: 63 becomes 255.
pub(crate) fn widen(v: u8) -> u8 {
    (v << 2) | (v >> 4)
}

/// Colours of 3 bytes each, every channel through `shown`.
fn colours(bytes: &[u8], shown: impl Fn(u8) -> u8) -> Vec<Rgb> {
    bytes
        .chunks(3)
        .map(|c| Rgb::from_dac(shown(c[0]), shown(c[1]), shown(c[2])))
        .collect()
}

/// 16 colours (EGA pictures and the game) or 256 (VGA pictures, movies).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette(pub Vec<Rgb>);

impl Palette {
    /// The game's 16-colour palettes (GAMEPAL.PAL, after EGA pictures): 16 x
    /// RGB with values 0-64. The game always sets them by fading in, which
    /// shows value x 15 / 16 as the 6-bit DAC value, so 64 shows as 60.
    pub fn parse_ega(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let bytes = Reader::new(file, bytes).bytes(48)?;
        Ok(Self(colours(bytes, |v| (v as u16 * 15 / 16) as u8)))
    }

    /// A VGA palette: 256 x RGB, 6-bit values.
    pub fn parse_vga(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let bytes = Reader::new(file, bytes).bytes(768)?;
        Ok(Self(colours(bytes, |v| v)))
    }

    pub fn get(&self, index: u8) -> Rgb {
        self.0.get(index as usize).copied().unwrap_or_default()
    }
}

/// A true-colour picture: 0xAARRGGBB pixels, row by row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

/// A picture of palette indices, row by row; [`TRANSPARENT`] where nothing
/// is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, fill: u8) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; (width * height) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Copies `tile` with its top left at tile column `col`, row `row`.
    pub fn put_tile(&mut self, col: u32, row: u32, tile: &Tile) {
        for (y, line) in tile.as_chunks::<8>().0.iter().enumerate() {
            let start = ((row * 8 + y as u32) * self.width + col * 8) as usize;
            self.pixels[start..start + 8].copy_from_slice(line);
        }
    }

    /// Lays `tiles` out `columns` to a row.
    pub fn from_tiles(tiles: &[Tile], columns: u32) -> Self {
        let rows = (tiles.len() as u32).div_ceil(columns);
        let mut image = Self::new(columns * 8, rows * 8, TRANSPARENT);
        for (i, tile) in tiles.iter().enumerate() {
            image.put_tile(i as u32 % columns, i as u32 / columns, tile);
        }
        image
    }
}

pub fn decode_solid_tile(data: &[u8; SOLID_TILE_BYTES]) -> Tile {
    let mut tile = [0; 64];
    for (row, p) in data.as_chunks::<4>().0.iter().enumerate() {
        for x in 0..8 {
            tile[row * 8 + x] = pixel(p[0], p[1], p[2], p[3], 0x80 >> x);
        }
    }
    tile
}

pub fn decode_masked_tile(data: &[u8; MASKED_TILE_BYTES]) -> Tile {
    let mut tile = [TRANSPARENT; 64];
    for (row, p) in data.as_chunks::<5>().0.iter().enumerate() {
        for x in 0..8 {
            let bit = 0x80 >> x;
            if p[0] & bit == 0 {
                tile[row * 8 + x] = pixel(p[1], p[2], p[3], p[4], bit);
            }
        }
    }
    tile
}

/// Decodes back-to-back solid tiles.
pub fn decode_solid_tiles(data: &[u8]) -> Vec<Tile> {
    data.as_chunks::<SOLID_TILE_BYTES>()
        .0
        .iter()
        .map(decode_solid_tile)
        .collect()
}

/// Decodes back-to-back masked tiles.
pub fn decode_masked_tiles(data: &[u8]) -> Vec<Tile> {
    data.as_chunks::<MASKED_TILE_BYTES>()
        .0
        .iter()
        .map(decode_masked_tile)
        .collect()
}

fn pixel(b: u8, g: u8, r: u8, i: u8, bit: u8) -> u8 {
    let on = |plane: u8| (plane & bit != 0) as u8;
    on(b) | on(g) << 1 | on(r) << 2 | on(i) << 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_tile_combines_planes() {
        let mut data = [0; SOLID_TILE_BYTES];
        data[0] = 0x80; // row 0 blue, leftmost
        data[3] = 0x80; // row 0 intensity, leftmost
        data[2] = 0x01; // row 0 red, rightmost
        let tile = decode_solid_tile(&data);
        assert_eq!((tile[0], tile[7], tile[8]), (9, 4, 0));
    }

    #[test]
    fn masked_tile_honours_mask() {
        let mut data = [0; MASKED_TILE_BYTES];
        data[0] = 0x7f; // row 0: only the leftmost pixel drawn
        data[1] = 0xff;
        let tile = decode_masked_tile(&data);
        assert_eq!((tile[0], tile[1]), (1, TRANSPARENT));
    }

    #[test]
    fn ega_palette_shows_64_as_60() {
        let mut bytes = [0; 48];
        bytes[0] = 64;
        let palette = Palette::parse_ega("test", &bytes).unwrap();
        assert_eq!(palette.get(0).r, Rgb::from_dac(60, 0, 0).r);
    }
}
