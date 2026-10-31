//! Tests against the original game data. They read the game folder from
//! `DN2_DATA` (default: `data/` in the repository) and are skipped, with a
//! note, when it is missing.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use dn2_assets::{Files, ReadFile};

pub struct Folder(HashMap<String, PathBuf>);

impl ReadFile for Folder {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.get(&name.to_ascii_uppercase())?).ok()
    }
}

/// The game folder, or `None` (after printing why) when it is missing.
pub fn folder() -> Option<Folder> {
    let dir = std::env::var_os("DN2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no game data in {} (set DN2_DATA)", dir.display());
        return None;
    };
    let files = entries
        .flatten()
        .filter_map(|e| Some((e.file_name().to_str()?.to_ascii_uppercase(), e.path())))
        .collect();
    Some(Folder(files))
}

/// Every archive file name with its contents (loose files first).
pub fn read_all(folder: &Folder) -> Vec<(String, Vec<u8>)> {
    let files = Files::open(folder).expect("NUKEM2.CMP");
    files
        .archive()
        .entries()
        .iter()
        .map(|e| (e.name.clone(), files.read(&e.name).unwrap()))
        .collect()
}
