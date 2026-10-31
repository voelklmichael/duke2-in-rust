mod common;

use dn2_assets::level::{Cell, Level};

#[test]
fn all_levels_parse() {
    let Some(folder) = common::folder() else {
        return;
    };
    let files = common::read_all(&folder);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    let mut count = 0;
    for (name, bytes) in &files {
        let is_level = name.len() == 6
            && name.ends_with(".MNI")
            && "LMNO".contains(&name[..1])
            && name[1..2].chars().all(|c| ('1'..='8').contains(&c));
        if !is_level {
            continue;
        }
        count += 1;
        let level = Level::parse(name, bytes).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            [32, 64, 128, 256, 512, 1024].contains(&level.width),
            "{name}"
        );
        assert_eq!(level.height, 0x8000 / level.width - 2);
        for file in [&level.tileset, &level.backdrop, &level.music] {
            assert!(
                names.iter().any(|n| n.eq_ignore_ascii_case(file)),
                "{name}: missing {file}"
            );
        }

        let (mut solids, mut masks) = (0, 0);
        for y in 0..level.height + 1 {
            for x in 0..level.width {
                match level.cell(x, y) {
                    Cell::Empty => {}
                    Cell::Solid(s) => {
                        assert!(s < 1000, "{name}: solid {s}");
                        solids += 1;
                    }
                    Cell::Masked(m) => {
                        assert!(m < 160, "{name}: masked {m}");
                        masks += 1;
                    }
                    Cell::SolidAndMasked { solid, masked } => {
                        assert!(solid < 1000 && masked < 160, "{name}: {solid} {masked}");
                        solids += 1;
                        masks += 1;
                    }
                }
            }
        }
        assert!(solids > 0, "{name}");
        eprintln!(
            "{name} {}x{} actors {} masked {masks}",
            level.width,
            level.height,
            level.actors.len()
        );

        assert!(!level.actors.is_empty());
        let inside = level
            .actors
            .iter()
            .filter(|a| a.x < level.width && a.y < level.height)
            .count();
        assert!(inside * 10 >= level.actors.len() * 9, "{name}");
    }
    eprintln!("{count} levels");
    assert!(count >= 8, "found only {count} levels");
}
