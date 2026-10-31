mod common;

use dn2_assets::images::{Picture, TileSheet};

#[test]
fn all_pictures_parse() {
    let Some(folder) = common::folder() else {
        return;
    };
    let mut counts = (0, 0, 0);
    for (name, bytes) in common::read_all(&folder) {
        match bytes.len() {
            32000 => {
                let sheet = TileSheet::parse(&name, &bytes).unwrap();
                assert_eq!(sheet.image().width, 320);
                counts.0 += 1;
            }
            32048 => {
                let picture = Picture::parse_ega(&name, &bytes).unwrap();
                assert!(picture.image.pixels.iter().all(|&p| p < 16), "{name}");
                counts.1 += 1;
            }
            64768 => {
                Picture::parse_vga(&name, &bytes).unwrap();
                counts.2 += 1;
            }
            _ => {}
        }
    }
    assert!(
        counts.0 >= 2 && counts.1 >= 1 && counts.2 >= 1,
        "{counts:?}"
    );
}
