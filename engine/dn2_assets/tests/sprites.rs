mod common;

use dn2_assets::sprites::Sprites;

#[test]
fn all_actor_sprites_parse() {
    let Some(folder) = common::folder() else {
        return;
    };
    let files = common::read_all(&folder);
    let find = |name: &str| &files.iter().find(|(n, _)| n == name).unwrap().1;
    let sprites = Sprites::parse(find("ACTRINFO.MNI"), find("ACTORS.MNI")).unwrap();
    assert_eq!(sprites.actors.len(), 302);
    let with_frames: Vec<_> = sprites.actors.iter().flatten().collect();
    assert_eq!(with_frames.len(), 285);
    for sprite in with_frames {
        assert!((-1..=3).contains(&sprite.draw_pass));
        assert!(!sprite.frames.is_empty());
        for frame in &sprite.frames {
            let (w, h) = (frame.image.width, frame.image.height);
            assert!(w % 8 == 0 && h % 8 == 0, "{w}x{h}");
            assert!((8..=320).contains(&w) && (8..=200).contains(&h), "{w}x{h}");
        }
    }
}
