//! Pictures: backdrops and the status sheet (1000 solid tiles each), EGA
//! full-screen pictures with their palette, VGA pictures.

use alloc::vec::Vec;

use crate::AssetError;
use crate::gfx::{self, Image, Palette, SOLID_TILE_BYTES, Tile};
use crate::hd::SHEET_COLUMNS;
use crate::reader::Reader;

/// 1000 solid tiles, 40 to a row: the backdrops (DROP*.MNI, one 320x200
/// picture) and STATUS.MNI (the status bar, fonts and menu boxes).
#[derive(Clone, Debug)]
pub struct TileSheet {
    pub tiles: Vec<Tile>,
}

impl TileSheet {
    pub const COLUMNS: u32 = SHEET_COLUMNS;
    const TILES: usize = 1000;

    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let data = Reader::new(file, bytes).bytes(Self::TILES * SOLID_TILE_BYTES)?;
        Ok(Self {
            tiles: gfx::decode_solid_tiles(data),
        })
    }

    /// The whole sheet as one 320x200 picture.
    pub fn image(&self) -> Image {
        Image::from_tiles(&self.tiles, Self::COLUMNS)
    }
}

/// A full-screen picture with its palette.
#[derive(Clone, Debug)]
pub struct Picture {
    pub image: Image,
    pub palette: Palette,
}

impl Picture {
    /// EGA: four planes (blue, green, red, intensity) of 200 rows of 40
    /// bytes, one after the other, then a 16-colour palette.
    pub fn parse_ega(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        const PLANE: usize = 8000;
        let mut reader = Reader::new(file, bytes);
        let planes = reader.bytes(4 * PLANE)?;
        let palette = Palette::parse_ega(file, reader.remaining())?;
        let mut image = Image::new(320, 200, 0);
        for (i, pixel) in image.pixels.iter_mut().enumerate() {
            let (byte, bit) = (i / 8, 0x80 >> (i % 8));
            *pixel = (0..4)
                .map(|p| ((planes[p * PLANE + byte] & bit != 0) as u8) << p)
                .sum();
        }
        Ok(Self { image, palette })
    }

    /// VGA mode 13h: a 256-colour palette, then 320x200 bytes.
    pub fn parse_vga(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut reader = Reader::new(file, bytes);
        let palette = Palette::parse_vga(file, reader.bytes(768)?)?;
        let pixels = reader.bytes(64000)?.to_vec();
        Ok(Self {
            image: Image {
                width: 320,
                height: 200,
                pixels,
            },
            palette,
        })
    }
}
