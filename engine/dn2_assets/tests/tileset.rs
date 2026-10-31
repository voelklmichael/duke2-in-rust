//! Tilesets against the original game data.

mod common;

use dn2_assets::tileset::{TileAttributes, Tileset};

#[test]
fn every_tileset_parses() {
    let Some(folder) = common::folder() else {
        return;
    };
    let mut count = 0;
    for (name, bytes) in common::read_all(&folder) {
        if !(name.starts_with("CZONE") && name.ends_with(".MNI")) {
            continue;
        }
        let t = Tileset::parse(&name, &bytes).unwrap();
        assert_eq!((t.solid.len(), t.masked.len()), (1000, 160), "{name}");
        assert_eq!(t.solid_attributes.len(), 1000);
        assert_eq!(t.masked_attributes.len(), 160);
        assert!(t.solid.iter().flatten().all(|&p| p < 16));
        assert!(
            t.solid_attributes
                .iter()
                .any(|a| a.contains(TileAttributes::SOLID_TOP))
        );
        count += 1;
    }
    assert!(count > 0);
}
