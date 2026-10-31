//! The whole program driven by keys: logo, menus, a game, saving, quitting, the
//! high scores. Needs the game data (`DN2_DATA`, default `data/`); skipped
//! without. With `DN2_SHOTS` set to a folder, the screens are written there as
//! PPM pictures.

use std::collections::HashMap;
use std::path::PathBuf;

use dn2_assets::{Files, GameData, ReadFile};
use dn2_game::Buttons;
use dn2_ui::{App, Input, Key, Store, UiEvent};

struct Folder(HashMap<String, PathBuf>);

impl ReadFile for Folder {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.get(&name.to_ascii_uppercase())?).ok()
    }
}

#[derive(Default)]
struct Memory(HashMap<String, Vec<u8>>);

impl Store for Memory {
    fn load_store(&self, key: &str) -> Option<Vec<u8>> {
        self.0.get(key).cloned()
    }
    fn save_store(&mut self, key: &str, bytes: &[u8]) {
        self.0.insert(key.into(), bytes.to_vec());
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
    let files = Files::open(&folder).ok()?;
    GameData::load(&files).ok()
}

struct Driver {
    data: GameData,
    store: Memory,
    app: App,
    events: Vec<UiEvent>,
    shot: u32,
}

impl Driver {
    /// One update of 4 ticks with these keys pressed (and held).
    fn step(&mut self, keys: &[Key]) {
        let input = Input {
            pressed: keys,
            held: keys,
            buttons: Buttons::empty(),
            music_near_end: false,
        };
        let events = self.app.update(&self.data, &mut self.store, &input, 4);
        self.events.extend(events);
    }

    fn mode(&self) -> String {
        self.app.mode_name()
    }

    /// Steps until the mode starts with `prefix`, pressing `key` every 40
    /// steps; panics after `limit` steps.
    fn until(&mut self, prefix: &str, key: Option<Key>, limit: u32) {
        for n in 0..limit {
            if self.mode().starts_with(prefix) {
                return;
            }
            let keys: Vec<Key> = key.filter(|_| n % 40 == 39).into_iter().collect();
            self.step(&keys);
        }
        panic!("never reached {prefix}: still {}", self.mode());
    }

    /// Steps without keys until the screen is fully faded in and settles.
    fn settle(&mut self, steps: u32) {
        for _ in 0..steps {
            self.step(&[]);
        }
    }

    fn press(&mut self, key: Key) {
        self.step(&[key]);
        self.step(&[]);
    }

    fn shot(&mut self, name: &str) {
        let Some(dir) = std::env::var_os("DN2_SHOTS") else {
            return;
        };
        let screen = self.app.screen();
        let mut frame = dn2_render::FrameBuffer::new(0, 0);
        screen.present(&mut frame, 1, dn2_render::Filter::Nearest);
        let mut out = format!("P6 {} {} 255\n", frame.width, frame.height).into_bytes();
        for p in &frame.pixels {
            out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        self.shot += 1;
        let path = PathBuf::from(dir).join(format!("{:02}-{name}.ppm", self.shot));
        std::fs::write(path, out).unwrap();
    }
}

#[test]
fn menus_game_and_scores() {
    let Some(data) = data() else { return };
    let store = Memory::default();
    let app = App::new(&data, &store);
    let mut d = Driver {
        data,
        store,
        app,
        events: Vec::new(),
        shot: 0,
    };
    assert_eq!(d.mode(), "logo");
    d.shot("logo");
    d.press(Key::Enter);
    // The first run shows the advertisement, then the intro; keys skip
    // through to the attract mode, and a key in it to the menu.
    d.until("menu Main", Some(Key::Enter), 4000);
    d.settle(20);
    d.shot("main-menu");
    // New game, episode 1, medium.
    d.press(Key::Enter);
    d.until("menu Episode", None, 100);
    d.settle(20);
    d.shot("episode");
    d.press(Key::Enter);
    d.until("menu Skill", None, 100);
    d.settle(20);
    d.shot("skill");
    d.press(Key::Enter);
    d.until("game Loading", None, 100);
    d.settle(10);
    d.shot("loading");
    d.until("game Playing", None, 1000);
    d.settle(40);
    d.shot("playing");
    // F2: save in slot 1 as "abc".
    d.press(Key::F(2));
    d.settle(30);
    d.shot("save-menu");
    d.press(Key::Enter);
    for key in [Key::Letter('A'), Key::Letter('B'), Key::Letter('C')] {
        d.press(key);
    }
    d.shot("save-name");
    d.press(Key::Enter);
    d.until("game Playing", Some(Key::Enter), 400);
    assert!(d.store.0.contains_key("save1"), "saved");
    // Esc and yes: the game is over, the scores show.
    d.press(Key::Escape);
    d.until("game Overlay Quit", None, 10);
    d.settle(10);
    d.shot("quit");
    d.press(Key::Letter('Y'));
    d.until("menu ScoresTitle", None, 100);
    d.until("scene MainMenu", None, 400);
    d.settle(20);
    d.shot("scores");
    d.press(Key::Enter);
    d.until("menu Main", None, 100);
    assert!(
        d.events
            .contains(&UiEvent::Music(Some("DUKEIIA.IMF".into())))
    );
    // Restore the saved game.
    d.settle(20);
    d.press(Key::Letter('R'));
    d.until("menu Restore", None, 100);
    d.settle(20);
    d.shot("restore");
    d.press(Key::Enter);
    d.until("game Loading", Some(Key::Enter), 400);
    // Quit the program.
    d.until("game Playing", None, 1000);
    d.press(Key::Escape);
    d.settle(10);
    d.press(Key::Letter('Y'));
    d.until("menu ScoresTitle", None, 100);
    d.until("menu Main", Some(Key::Enter), 600);
    d.settle(20);
    d.press(Key::Letter('Q'));
    d.until("menu Quit", None, 100);
    d.settle(20);
    d.press(Key::Letter('Y'));
    assert_eq!(d.mode(), "over");
    assert!(d.events.contains(&UiEvent::Quit));
}

#[test]
fn attract_mode_plays_the_demo() {
    let Some(data) = data() else { return };
    let mut store = Memory::default();
    // Not the first run: no advertisement.
    store.save_store(
        dn2_ui::config::Config::KEY,
        &dn2_ui::config::Config::default().to_bytes(),
    );
    let app = App::new(&data, &store);
    let mut d = Driver {
        data,
        store,
        app,
        events: Vec::new(),
        shot: 100,
    };
    d.press(Key::Enter);
    d.until("movie 0", None, 100);
    d.settle(200);
    d.shot("intro");
    // A key ends the intro movie; the titles play without keys.
    d.press(Key::Enter);
    d.until("movie 1", None, 100);
    d.until("movie 4", None, 2000);
    d.settle(300);
    d.shot("title4");
    d.until("menu Story", None, 2000);
    d.settle(30);
    d.shot("story");
    // Pages of the story and the credits go on with a key.
    d.until("pause", Some(Key::Space), 40000);
    d.until("game Playing", None, 2000);
    d.settle(300);
    d.shot("demo");
    assert!(d.mode().starts_with("game"), "{}", d.mode());
    d.press(Key::Enter);
    d.until("menu Main", None, 10);
}

#[test]
fn widescreen_menus_and_game() {
    let Some(data) = data() else { return };
    let store = Memory::default();
    let mut app = App::new(&data, &store);
    app.set_columns(53);
    let mut d = Driver {
        data,
        store,
        app,
        events: Vec::new(),
        shot: 100,
    };
    assert_eq!(d.app.screen().image.width, 424);
    d.press(Key::Enter);
    d.until("menu Main", Some(Key::Enter), 4000);
    d.settle(20);
    d.shot("wide-main-menu");
    // The menu is in the middle: the borders are one colour.
    let screen = d.app.screen();
    let border = screen.image.get(0, 0);
    assert!((0..200).all(|y| (0..52).all(|x| screen.image.get(x, y) == border)));
    assert!((0..200).all(|y| (372..424).all(|x| screen.image.get(x, y) == border)));
    for _ in 0..3 {
        d.press(Key::Enter);
        d.settle(20);
    }
    // A key skips the loading bar: only its fade-out is left.
    assert_eq!(d.mode(), "game Loading");
    d.press(Key::Space);
    d.until("game Playing", None, 20);
    d.settle(40);
    d.shot("wide-playing");
    // The view reaches the right panel, 13 columns further on.
    let screen = d.app.screen();
    assert!((8..160).any(|y| screen.image.get(8 + 44 * 8, y) != 0));
    d.press(Key::Escape);
    d.until("game Overlay Quit", None, 10);
    d.settle(10);
    d.shot("wide-quit");
}

#[test]
fn the_end_of_the_fanfare_ends_the_intro() {
    let Some(data) = data() else { return };
    let mut store = Memory::default();
    store.save_store(
        dn2_ui::config::Config::KEY,
        &dn2_ui::config::Config::default().to_bytes(),
    );
    let app = App::new(&data, &store);
    let mut d = Driver {
        data,
        store,
        app,
        events: Vec::new(),
        shot: 200,
    };
    d.press(Key::Enter);
    d.until("movie 0", None, 100);
    d.settle(50);
    assert!(d.mode().starts_with("movie 0"), "{}", d.mode());
    let input = Input {
        pressed: &[],
        held: &[],
        buttons: Buttons::empty(),
        music_near_end: true,
    };
    d.app.update(&d.data, &mut d.store, &input, 4);
    assert!(!d.mode().starts_with("movie"), "{}", d.mode());
    d.until("movie 1", None, 100);
}
