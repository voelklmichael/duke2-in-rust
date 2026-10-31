//! The original game data, parsed into typed values. Everything is read once
//! at startup through [`Files`]; nothing here does I/O itself.

#![no_std]

extern crate alloc;

mod archive;
mod error;
mod files;
pub mod flic;
pub mod gfx;
pub mod hd;
pub mod images;
pub mod level;
mod reader;
pub mod script;
pub mod sound;
pub mod sprites;
pub mod tileset;
pub mod voc;

pub use archive::{Archive, Entry};
pub use error::AssetError;
pub use files::{Files, ReadFile};

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use flic::Flic;
use gfx::Palette;
use hd::HdArt;
use images::{Picture, TileSheet};
use level::Level;
use script::Script;
use sound::{ImfCommand, Sounds};
use sprites::{ColourFont, Sprites};
use tileset::Tileset;

/// The archive the game data lives in.
pub const ARCHIVE_NAME: &str = "NUKEM2.CMP";

/// The movies, loose files next to the archive.
pub const MOVIES: [&str; 5] = [
    "NUKEM2.F1",
    "NUKEM2.F2",
    "NUKEM2.F3",
    "NUKEM2.F4",
    "NUKEM2.F5",
];

/// The scripts the menus and text screens come from.
pub const SCRIPTS: [&str; 3] = ["TEXT.MNI", "OPTIONS.MNI", "HELP.MNI"];

/// All the game data, parsed at startup. Maps are keyed by upper-case file
/// name, e.g. `"CZONE1.MNI"`.
pub struct GameData {
    /// Episodes whose levels are all present (1-4; the shareware has 1).
    pub episodes: Vec<Episode>,
    pub levels: BTreeMap<String, Level>,
    pub tilesets: BTreeMap<String, Tileset>,
    pub backdrops: BTreeMap<String, TileSheet>,
    /// STATUS.MNI: the status bar, fonts and menu boxes.
    pub status: TileSheet,
    pub sprites: Sprites,
    /// The colour font of the menus (sprite 1Dh).
    pub font: ColourFont,
    /// The game palette, GAMEPAL.PAL.
    pub palette: Palette,
    /// All 16-colour palettes (*.PAL).
    pub palettes: BTreeMap<String, Palette>,
    /// Full-screen pictures, EGA and VGA.
    pub pictures: BTreeMap<String, Picture>,
    pub scripts: BTreeMap<String, Script>,
    /// The movies present; a game folder without them still plays.
    pub movies: BTreeMap<String, Flic>,
    /// The demo's input, NUKEM2.MNI: a byte per frame (see
    /// docs/FORMATS.md); empty without the file.
    pub demo: Vec<u8>,
    pub sounds: Sounds,
    /// The songs (*.IMF), by upper-case file name.
    pub songs: BTreeMap<String, Vec<ImfCommand>>,
    /// Higher-detail pictures, loaded separately ([`HdArt::load`]); empty
    /// at first.
    pub hd: HdArt,
}

/// One episode and its eight level files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Episode {
    pub number: u8,
    pub levels: [String; 8],
}

impl GameData {
    pub fn load(files: &Files) -> Result<Self, AssetError> {
        let episodes = (1..=4)
            .filter_map(|number| {
                let letter = char::from(b'L' + number - 1);
                let levels = core::array::from_fn(|i| alloc::format!("{letter}{}.MNI", i + 1));
                levels
                    .iter()
                    .all(|name| files.contains(name))
                    .then_some(Episode { number, levels })
            })
            .collect::<Vec<_>>();
        if episodes.is_empty() {
            return Err(AssetError::NoEpisodes);
        }

        let mut levels = BTreeMap::new();
        for name in episodes.iter().flat_map(|e| &e.levels) {
            levels.insert(name.clone(), Level::parse(name, &files.read(name)?)?);
        }
        // The other files are recognised by name or, for pictures, by size.
        let (mut tilesets, mut backdrops, mut palettes, mut pictures, mut songs) = (
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );
        for entry in files.archive().entries() {
            let name = entry.name.as_str();
            let bytes = || files.read(name);
            if name.starts_with("CZONE") {
                tilesets.insert(entry.name.clone(), Tileset::parse(name, &bytes()?)?);
            } else if name.starts_with("DROP") {
                backdrops.insert(entry.name.clone(), TileSheet::parse(name, &bytes()?)?);
            } else if name.ends_with(".PAL") {
                palettes.insert(entry.name.clone(), Palette::parse_ega(name, &bytes()?)?);
            } else if name.ends_with(".IMF") {
                songs.insert(entry.name.clone(), sound::parse_imf(&bytes()?));
            } else if entry.size == 32048 {
                pictures.insert(entry.name.clone(), Picture::parse_ega(name, &bytes()?)?);
            } else if entry.size == 64768 {
                pictures.insert(entry.name.clone(), Picture::parse_vga(name, &bytes()?)?);
            }
        }
        let mut scripts = BTreeMap::new();
        for name in SCRIPTS {
            scripts.insert(String::from(name), Script::parse(name, &files.read(name)?)?);
        }
        let mut movies = BTreeMap::new();
        for name in MOVIES {
            match files.read(name) {
                Ok(bytes) => _ = movies.insert(String::from(name), Flic::parse(name, &bytes)?),
                Err(_) => log::warn!("{name} not found: no movie"),
            }
        }
        let palette = palettes
            .get("GAMEPAL.PAL")
            .cloned()
            .ok_or_else(|| AssetError::Missing(String::from("GAMEPAL.PAL")))?;
        let (info, actors) = (files.read("ACTRINFO.MNI")?, files.read("ACTORS.MNI")?);
        Ok(Self {
            episodes,
            levels,
            tilesets,
            backdrops,
            status: TileSheet::parse("STATUS.MNI", &files.read("STATUS.MNI")?)?,
            sprites: Sprites::parse(&info, &actors)?,
            font: ColourFont::parse(&info, &actors)?,
            palette,
            palettes,
            pictures,
            scripts,
            movies,
            demo: files.read("NUKEM2.MNI").unwrap_or_default(),
            sounds: Sounds::load(files)?,
            songs,
            hd: HdArt::default(),
        })
    }
}
