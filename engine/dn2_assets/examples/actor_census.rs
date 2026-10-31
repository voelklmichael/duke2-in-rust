//! Lists the placed actor types of each episode with their counts.
//! `cargo run -p dn2_assets --example actor_census -- <game dir>`

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use dn2_assets::{Files, GameData, ReadFile};

struct Folder(HashMap<String, PathBuf>);

impl ReadFile for Folder {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.get(&name.to_ascii_uppercase())?).ok()
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("game dir");
    let files = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| Some((e.file_name().to_str()?.to_ascii_uppercase(), e.path())))
        .collect();
    let folder = Folder(files);
    let files = Files::open(&folder).unwrap();
    let data = GameData::load(&files).unwrap();
    for episode in &data.episodes {
        let mut counts: BTreeMap<u16, (usize, Vec<&str>)> = BTreeMap::new();
        for name in &episode.levels {
            let Some(level) = data.levels.get(name) else {
                continue;
            };
            for a in &level.actors {
                let e = counts.entry(a.kind).or_default();
                e.0 += 1;
                if !e.1.contains(&name.as_str()) {
                    e.1.push(name);
                }
            }
        }
        println!("episode {}", episode.number);
        for (kind, (n, levels)) in counts {
            println!("{kind}\t{n}\t{}", levels.join(","));
        }
    }
}
