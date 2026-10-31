//! The movies against the original data (`DN2_DATA`, skipped without it).

use std::path::PathBuf;

use dn2_assets::gfx::Palette;
use dn2_assets::{Files, GameData, ReadFile};
use dn2_game::Sound;
use dn2_render::Screen;
use dn2_ui::demo::{Demo, DemoFrame, demo_levels};
use dn2_ui::movie::{Movie, MovieKind};
use dn2_ui::{Ctx, UiEvent};

struct Folder(PathBuf);

impl ReadFile for Folder {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        for entry in std::fs::read_dir(&self.0).ok()?.flatten() {
            if entry.file_name().to_str()?.eq_ignore_ascii_case(name) {
                return std::fs::read(entry.path()).ok();
            }
        }
        None
    }
}

fn data() -> Option<GameData> {
    let dir = std::env::var_os("DN2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")));
    if !dir.is_dir() {
        eprintln!("skipped: no game data in {} (set DN2_DATA)", dir.display());
        return None;
    }
    let folder = Folder(dir);
    let files = Files::open(&folder).expect("archive");
    Some(GameData::load(&files).expect("game data"))
}

/// Plays a movie to its end in steps of four ticks: the ticks it took,
/// the sounds and whether any frame had a lit pixel.
fn play(data: &GameData, kind: MovieKind) -> (u32, Vec<Sound>, bool) {
    let mut movie = Movie::new(data, kind).expect("movie");
    let mut screen = Screen::new(320, 200, Palette(Vec::new()));
    let mut events = Vec::new();
    let (mut ticks, mut lit) = (0, false);
    loop {
        let mut ctx = Ctx {
            screen: &mut screen,
            data,
            pressed: &[],
            ticks: 4,
            events: &mut events,
        };
        let over = movie.step(&mut ctx);
        ticks += 4;
        lit |= screen.image.pixels.iter().any(|&p| {
            let c = screen.palette.get(p);
            c.r != 0 || c.g != 0 || c.b != 0
        });
        if over {
            break;
        }
        assert!(ticks < 140 * 600, "{kind:?} does not end");
    }
    let sounds = events
        .into_iter()
        .filter_map(|e| match e {
            UiEvent::Sound(s) => Some(s),
            _ => None,
        })
        .collect();
    (ticks, sounds, lit)
}

#[test]
fn every_movie_plays_to_its_end() {
    let Some(data) = data() else { return };
    let kinds = [
        MovieKind::Intro,
        MovieKind::Title(1),
        MovieKind::Title(2),
        MovieKind::Title(3),
        MovieKind::Title(4),
    ];
    for kind in kinds {
        let (ticks, sounds, lit) = play(&data, kind);
        eprintln!("{kind:?}: {ticks} ticks, {} sounds", sounds.len());
        assert!(lit, "{kind:?} draws nothing");
        assert!(ticks > 140, "{kind:?} is over at once");
        match kind {
            MovieKind::Title(2) => assert_eq!(sounds[0], Sound::Movie2a),
            MovieKind::Title(3) => assert!(sounds.contains(&Sound::Movie2b)),
            MovieKind::Title(4) => {
                assert_eq!(sounds[0], Sound::Movie2c);
                assert!(sounds.contains(&Sound::Movie30));
                assert_eq!(sounds.last(), Some(&Sound::BigExplosion));
            }
            _ => assert!(sounds.is_empty(), "{kind:?}: {sounds:?}"),
        }
    }
}

#[test]
fn a_missing_movie_is_none() {
    let Some(data) = data() else { return };
    assert!(Movie::new(&data, MovieKind::Title(0)).is_none());
    assert!(Movie::new(&data, MovieKind::Title(5)).is_none());
}

#[test]
fn the_demo_file_is_input_and_ends() {
    let Some(data) = data() else { return };
    let mut demo = Demo::new(&data.demo);
    let mut frames = 0;
    while let DemoFrame::Input { .. } = demo.next() {
        frames += 1;
    }
    assert!(frames > 100);
    assert_eq!(demo_levels()[0], "L1.MNI");
}
