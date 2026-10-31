//! Higher-detail graphics: true-colour pictures that replace the original
//! ones where the picture is shown larger. The game and its screen of
//! palette indices do not change.
//!
//! The platform supplies the pictures by name ([`ReadFile::read_image`]);
//! every one is optional. A picture may be any whole multiple of the
//! original's size and is resampled to the scale it is shown at. The names
//! (those of the old port's packs):
//!
//! - `<tileset>/solid`, e.g. `CZONE1.MNI/solid`: the 1000 solid tiles, 40
//!   a row (320x200).
//! - `<tileset>/masked`: the 160 masked tiles, 40 a row (320x32).
//! - `<backdrop>`, e.g. `DROP1.MNI`: 320x200.
//! - `ACTORS.MNI/<id>/<frame>`, e.g. `ACTORS.MNI/0/3`: a sprite frame at
//!   its size.
//! - `STATUS.MNI`: the status sheet, 1000 tiles 40 a row (320x200): the
//!   status bar's pieces, the menu fonts and the text boxes.
//! - `ACTORS.MNI/29/font`: the colour font of the menus (sprite 1Dh, whose
//!   frames are glyphs, not pictures), its 67 8x16 glyphs in one row
//!   (536x16). Draw the ink white: it is tinted with the colour the text
//!   is drawn in. Leave the rest see-through.
//! - A full-screen picture by its file name, e.g. `NUKEM2.MNI`: 320x200.
//!
//! Pixels with an alpha below one half are see-through.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gfx::{Image, RgbaImage};
use crate::{GameData, ReadFile};

/// The pictures of a pack, resampled to [`HdArt::scale`].
#[derive(Clone, Debug, Default)]
pub struct HdArt {
    /// Frame pixels per game pixel of every picture; 0 without a pack.
    pub scale: u32,
    /// By tileset name.
    pub solid: BTreeMap<String, RgbaImage>,
    pub masked: BTreeMap<String, RgbaImage>,
    /// By backdrop name.
    pub backdrops: BTreeMap<String, RgbaImage>,
    /// By actor id and frame.
    pub sprites: BTreeMap<(u16, u16), RgbaImage>,
    /// The status sheet as one column of its tiles ([`status_strip`]).
    pub status: Option<RgbaImage>,
    /// Full-screen pictures by name.
    pub pictures: BTreeMap<String, RgbaImage>,
    /// The colour font's glyphs in one row.
    pub font: Option<RgbaImage>,
}

impl HdArt {
    /// Loads every picture `files` has for `data`'s graphics at `scale`
    /// (at least 2, or nothing is loaded). Pictures of the wrong size are
    /// logged and left out.
    pub fn load(files: &dyn ReadFile, data: &GameData, scale: u32) -> Self {
        let mut art = Self::default();
        if scale < 2 {
            return art;
        }
        art.scale = scale;
        let load = |name: &str, width: u32, height: u32| {
            let image = files.read_image(name)?;
            fit(image, width, height, scale)
                .inspect_err(|e| log::warn!("{name}: {e}"))
                .ok()
        };
        for (name, tileset) in &data.tilesets {
            let rows = |tiles: usize| (tiles as u32).div_ceil(SHEET_COLUMNS) * 8;
            let width = SHEET_COLUMNS * 8;
            if let Some(image) = load(&format!("{name}/solid"), width, rows(tileset.solid.len())) {
                art.solid.insert(name.clone(), image);
            }
            if let Some(image) = load(&format!("{name}/masked"), width, rows(tileset.masked.len()))
            {
                art.masked.insert(name.clone(), image);
            }
        }
        for name in data.backdrops.keys() {
            if let Some(image) = load(name, 320, 200) {
                art.backdrops.insert(name.clone(), image);
            }
        }
        for (id, actor) in data.sprites.actors.iter().enumerate() {
            let Some(actor) = actor else {
                continue;
            };
            for (index, frame) in actor.frames.iter().enumerate() {
                let Image { width, height, .. } = frame.image;
                if let Some(image) = load(&format!("ACTORS.MNI/{id}/{index}"), width, height) {
                    art.sprites.insert((id as u16, index as u16), image);
                }
            }
        }
        let status_rows = (data.status.tiles.len() as u32).div_ceil(SHEET_COLUMNS) * 8;
        art.status = load(STATUS, SHEET_COLUMNS * 8, status_rows)
            .map(|sheet| status_strip(&sheet, data.status.tiles.len()));
        art.font = load(FONT, FONT_GLYPH_WIDTH * data.font.glyphs.len() as u32, 16);
        for (name, picture) in &data.pictures {
            let Image { width, height, .. } = picture.image;
            if let Some(image) = load(name, width, height) {
                art.pictures.insert(name.clone(), image);
            }
        }
        log::info!(
            "HD pictures: {} solid and {} masked tile sheets, {} backdrops, {} sprite frames, \
             {} status sheet, {} full-screen pictures, {} colour font",
            art.solid.len(),
            art.masked.len(),
            art.backdrops.len(),
            art.sprites.len(),
            u8::from(art.status.is_some()),
            art.pictures.len(),
            u8::from(art.font.is_some())
        );
        art
    }

    pub fn is_empty(&self) -> bool {
        self.solid.is_empty()
            && self.masked.is_empty()
            && self.backdrops.is_empty()
            && self.sprites.is_empty()
            && self.status.is_none()
            && self.pictures.is_empty()
            && self.font.is_none()
    }
}

/// The name of the status sheet's picture.
pub const STATUS: &str = "STATUS.MNI";

/// The name of the colour font's picture.
pub const FONT: &str = "ACTORS.MNI/29/font";

/// Width of a glyph of the colour font.
pub const FONT_GLYPH_WIDTH: u32 = 8;

/// The first `tiles` tiles of a sheet 40 a row, one under the other: the
/// game addresses the status sheet by tile row (8 a tile, see
/// `dn2_render::hud::tile_at`), and row `r` is then row `r` of the strip,
/// times the scale.
pub fn status_strip(sheet: &RgbaImage, tiles: usize) -> RgbaImage {
    let size = sheet.width / SHEET_COLUMNS;
    let mut pixels = Vec::with_capacity((size * size) as usize * tiles);
    for tile in 0..tiles as u32 {
        let (x, y) = (tile % SHEET_COLUMNS * size, tile / SHEET_COLUMNS * size);
        for row in y..y + size {
            let start = (row * sheet.width + x) as usize;
            pixels.extend_from_slice(&sheet.pixels[start..start + size as usize]);
        }
    }
    RgbaImage {
        width: size,
        height: size * tiles as u32,
        pixels,
    }
}

/// Tiles a row in the tile sheets.
pub const SHEET_COLUMNS: u32 = 40;

/// `image` at `scale` times `width` by `height`, if it is a whole multiple
/// of that size.
pub fn fit(image: RgbaImage, width: u32, height: u32, scale: u32) -> Result<RgbaImage, String> {
    let k = image.width / width.max(1);
    if k == 0 || image.width != width * k || image.height != height * k {
        return Err(format!(
            "{}x{} is not a whole multiple of {width}x{height}",
            image.width, image.height
        ));
    }
    Ok(if k == scale {
        image
    } else {
        resample(&image, width * scale, height * scale)
    })
}

/// Each new pixel is the average of the old pixels it covers (at least
/// one), colours weighted by alpha.
fn resample(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let span = |i: u32, old: u32, new: u32| {
        let start = i * old / new;
        let end = ((i + 1) * old).div_ceil(new).max(start + 1);
        start..end.min(old)
    };
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        let ys = span(y, image.height, height);
        for x in 0..width {
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for sy in ys.clone() {
                for sx in span(x, image.width, width) {
                    let p = image.pixels[(sy * image.width + sx) as usize];
                    let a = u64::from(p >> 24);
                    sum[0] += a;
                    for (c, shift) in [(1, 16), (2, 8), (3, 0)] {
                        sum[c] += u64::from((p >> shift) & 0xff) * a;
                    }
                    count += 1;
                }
            }
            let a = sum[0] / count;
            let channel = |c: usize| sum[c].checked_div(sum[0]).unwrap_or(0);
            pixels.push(((a << 24) | (channel(1) << 16) | (channel(2) << 8) | channel(3)) as u32);
        }
    }
    RgbaImage {
        width,
        height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn image(width: u32, height: u32, pixels: Vec<u32>) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn pictures_are_fitted_to_the_scale() {
        // 2x a 2x1 original: left half red, right half see-through.
        let red = 0xffff_0000;
        let two = image(4, 2, vec![red, red, 0, 0, red, red, 0, 0]);
        assert_eq!(fit(two.clone(), 2, 1, 2).unwrap(), two);
        assert_eq!(fit(two.clone(), 2, 1, 1).unwrap().pixels, vec![red, 0]);
        let three = fit(two.clone(), 2, 1, 3).unwrap();
        assert_eq!((three.width, three.height), (6, 3));
        assert_eq!((three.pixels[0], three.pixels[5]), (red, 0));
        assert!(fit(two, 3, 1, 2).is_err());
        assert!(fit(image(4, 3, vec![0; 12]), 2, 1, 2).is_err());
    }

    #[test]
    fn the_status_strip_follows_the_sheet() {
        // Tile t of a 1x sheet filled with t.
        let sheet = image(
            320,
            200,
            (0..320 * 200)
                .map(|i| i / 320 / 8 * 40 + i % 320 / 8)
                .collect(),
        );
        let strip = status_strip(&sheet, 1000);
        assert_eq!((strip.width, strip.height), (8, 8000));
        let at = |x: u32, y: u32| strip.pixels[(y * 8 + x) as usize];
        assert_eq!(at(3, 41 * 8 + 2), 41);
        assert_eq!(at(0, 999 * 8 + 7), 999);
    }
}
