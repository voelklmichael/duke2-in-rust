//! Plays the real levels for a while with a few scripted inputs: nothing may
//! panic, and the doors, lifts and sections the levels have must at least run.
//! Needs the game data (`DN2_DATA`, default `data/`).

use std::collections::HashMap;
use std::path::PathBuf;

use dn2_assets::{Files, GameData, ReadFile};
use dn2_game::{Buttons, World};

struct Folder(HashMap<String, PathBuf>);

impl ReadFile for Folder {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.get(&name.to_ascii_uppercase())?).ok()
    }
}

fn data() -> Option<GameData> {
    let dir = std::env::var_os("DN2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no game data in {} (set DN2_DATA)", dir.display());
        return None;
    };
    let folder = Folder(
        entries
            .flatten()
            .filter_map(|e| Some((e.file_name().to_str()?.to_ascii_uppercase(), e.path())))
            .collect(),
    );
    GameData::load(&Files::open(&folder).ok()?).ok()
}

#[test]
fn every_level_plays_without_panics() {
    let Some(data) = data() else { return };
    let scripts = [
        Buttons::RIGHT | Buttons::FIRE,
        Buttons::LEFT | Buttons::JUMP,
        Buttons::UP,
        Buttons::RIGHT | Buttons::JUMP | Buttons::FIRE,
    ];
    for episode in &data.episodes {
        for name in &episode.levels {
            let level = &data.levels[name];
            let tileset = &data.tilesets[&level.tileset];
            for script in scripts {
                let mut world = World::new(level, tileset, &data.sprites, 2);
                for frame in 0..400 {
                    // Let go now and then.
                    let input = if frame % 37 < 5 {
                        Buttons::empty()
                    } else {
                        script
                    };
                    world.step(input, &data.sprites);
                    if world.state.exit.is_some() {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn every_placed_actor_has_its_behaviour() {
    let Some(data) = data() else { return };
    for episode in &data.episodes {
        for name in &episode.levels {
            let level = &data.levels[name];
            let tileset = &data.tilesets[&level.tileset];
            let world = World::new(level, tileset, &data.sprites, 2);
            for a in world.actors.iter() {
                assert!(
                    !matches!(a.kind, dn2_game::actors::Kind::Unported { .. }),
                    "{name}: actor {} at {},{}: {:?}",
                    a.id,
                    a.x,
                    a.y,
                    a.kind
                );
            }
        }
    }
}
