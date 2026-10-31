mod common;

use dn2_assets::ReadFile;
use dn2_assets::flic::{Chunk, Flic, FlicPlayer};

#[test]
fn movies_parse_and_play() {
    let Some(folder) = common::folder() else {
        return;
    };
    for n in 1..=5 {
        let name = format!("NUKEM2.F{n}");
        let bytes = folder.read_file(&name).expect(&name);
        let flic = Flic::parse(&name, &bytes).unwrap();
        let header_count = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        assert_eq!(flic.frames.len(), header_count + 1, "{name}");
        assert_eq!((flic.width, flic.height), (320, 200), "{name}");
        let mut kinds = [0; 4];
        let mut player = FlicPlayer::new(&flic);
        for (i, frame) in flic.frames.iter().enumerate() {
            for chunk in &frame.chunks {
                kinds[match chunk {
                    Chunk::Palette(_) => 0,
                    Chunk::Delta { .. } => 1,
                    Chunk::Brun(_) => 2,
                    Chunk::Copy(_) => 3,
                }] += 1;
            }
            player.apply(frame);
            if i == 0 {
                assert_eq!(player.palette.0.len(), 256, "{name}");
            }
        }
        assert_eq!(player.image.pixels.len(), 320 * 200);
        eprintln!(
            "{name}: {} records, chunks palette/delta/brun/copy {kinds:?}",
            flic.frames.len()
        );
    }
}
