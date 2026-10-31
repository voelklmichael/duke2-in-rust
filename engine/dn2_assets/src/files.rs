//! Finding a data file: a loose file in the game folder comes first, then
//! the archive, as in the original.

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::gfx::RgbaImage;
use crate::{Archive, AssetError};

/// Reads a loose file from the game folder by its DOS name (any case).
pub trait ReadFile {
    fn read_file(&self, name: &str) -> Option<Vec<u8>>;

    /// A picture of the higher-detail pack by its name (see [`crate::hd`]),
    /// decoded; without a pack, none.
    fn read_image(&self, _name: &str) -> Option<RgbaImage> {
        None
    }
}

pub struct Files<'a> {
    loose: &'a dyn ReadFile,
    archive: Archive,
}

impl<'a> Files<'a> {
    pub fn open(loose: &'a dyn ReadFile) -> Result<Self, AssetError> {
        let bytes = loose
            .read_file(crate::ARCHIVE_NAME)
            .ok_or_else(|| AssetError::Missing(crate::ARCHIVE_NAME.to_string()))?;
        Ok(Self {
            loose,
            archive: Archive::parse(bytes)?,
        })
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        self.loose
            .read_file(name)
            .or_else(|| self.archive.get(name).map(<[u8]>::to_vec))
            .ok_or_else(|| AssetError::Missing(name.to_string()))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.archive.get(name).is_some() || self.loose.read_file(name).is_some()
    }
}
