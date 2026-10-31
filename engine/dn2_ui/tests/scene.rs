//! Runs every scene to its end with the real data (`DN2_DATA`, default `data/`;
//! skipped without).

use std::collections::HashMap;
use std::path::PathBuf;

use dn2_assets::gfx::Palette;
use dn2_assets::{Files, GameData, ReadFile};
use dn2_render::Screen;
use dn2_ui::scene::{Scene, Step, bonus_screen, episode_end, loading_screen};
use dn2_ui::{Ctx, Key, UiEvent};

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

/// Plays a scene to its end, pressing a key every `every` steps; returns the
/// final score and the events.
fn run_keys(
    data: &GameData,
    steps: Vec<Step>,
    score: u32,
    every: u32,
) -> Option<(u32, Vec<UiEvent>, u32)> {
    let mut screen = Screen::new(320, 200, Palette(Vec::new()));
    let mut events = Vec::new();
    let mut scene = Scene::new(steps, score);
    for i in 0..100_000u32 {
        let pressed: &[Key] = if i % every == every - 1 {
            &[Key::Space]
        } else {
            &[]
        };
        let mut ctx = Ctx {
            screen: &mut screen,
            data,
            pressed,
            held: !pressed.is_empty(),
            ticks: 5,
            events: &mut events,
        };
        if scene.step(&mut ctx) {
            return Some((scene.score(), events, i + 1));
        }
    }
    None
}

/// Plays a scene to its end, pressing a key now and then; returns the final
/// score and the events.
fn run(data: &GameData, steps: Vec<Step>, score: u32) -> (u32, Vec<UiEvent>) {
    let (score, events, _) = run_keys(data, steps, score, 100).expect("scene does not end");
    (score, events)
}

#[test]
fn bonus_screen_adds_100000_a_bonus() {
    let Some(data) = data() else { return };
    for bonuses in [&[][..], &[3], &[1, 2, 7]] {
        let (score, events) = run(&data, bonus_screen(500, bonuses, false), 500);
        assert_eq!(score, 500 + 100_000 * bonuses.len() as u32);
        assert!(events.contains(&UiEvent::Music(None)));
    }
}

#[test]
fn keys_skip_the_bonus_screen() {
    let Some(data) = data() else { return };
    let bonuses = [1, 2, 7];
    let (_, _, slow) = run_keys(&data, bonus_screen(500, &bonuses, false), 500, u32::MAX).unwrap();
    let (score, events, fast) =
        run_keys(&data, bonus_screen(500, &bonuses, false), 500, 1).unwrap();
    assert_eq!(score, 500 + 300_000);
    assert!(events.contains(&UiEvent::Music(None)));
    // The fades, then two keys a bonus (the countdown, the rest) and one for
    // the wait at the end.
    assert!(fast < 30, "{fast} steps");
    assert!(slow > 500, "{slow} steps");
}

#[test]
fn loading_and_episode_end_finish() {
    let Some(data) = data() else { return };
    for episode in 1..=4 {
        run(&data, loading_screen(episode), 0);
        let (score, _) = run(&data, episode_end(episode, 10, &[4]), 10);
        assert_eq!(score, 100_010);
    }
}
