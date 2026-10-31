//! The script runner against the real TEXT.MNI. Needs the game data
//! (`DN2_DATA`, default `data/`); skipped without.

use std::collections::HashMap;
use std::path::PathBuf;

use dn2_assets::script::Command;
use dn2_assets::{Files, GameData, ReadFile};
use dn2_game::Sound;
use dn2_render::Screen;
use dn2_ui::config::{Bindings, Options};
use dn2_ui::runner::{Runner, ScriptEnd, ScriptEnv};
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

struct Rig {
    data: GameData,
    screen: Screen,
    options: Options,
    bindings: Bindings,
    menus: [u8; 16],
    events: Vec<UiEvent>,
}

impl Rig {
    fn new() -> Option<Self> {
        let data = data()?;
        let screen = Screen::new(320, 200, data.palette.clone());
        Some(Rig {
            data,
            screen,
            options: Options::default(),
            bindings: Bindings::default(),
            menus: [0; 16],
            events: Vec::new(),
        })
    }

    fn runner(&self, file: &str, section: &str) -> Runner {
        Runner::new(self.data.scripts[file].section(section).expect(section))
    }

    /// The first section of a file with a command that `want` accepts and
    /// none that `avoid` accepts.
    fn find(&self, file: &str, want: fn(&Command) -> bool, avoid: fn(&Command) -> bool) -> String {
        self.data.scripts[file]
            .sections
            .iter()
            .find(|s| s.commands.iter().any(want) && !s.commands.iter().any(avoid))
            .map(|s| s.name.clone())
            .expect("a matching section")
    }

    /// One step of `ticks` with these presses.
    fn step(&mut self, runner: &mut Runner, ticks: u32, pressed: &[Key]) -> Option<ScriptEnd> {
        let slots = vec![String::new(); 8];
        let mut env = ScriptEnv {
            options: &mut self.options,
            bindings: &self.bindings,
            slot_names: &slots,
            episode: 1,
            level: 1,
            menus: &mut self.menus,
        };
        let mut ctx = Ctx {
            screen: &mut self.screen,
            data: &self.data,
            pressed,
            held: false,
            ticks,
            events: &mut self.events,
        };
        runner.step(&mut ctx, &mut env)
    }

    /// Steps with no keys until the fade-in is over.
    fn settle(&mut self, runner: &mut Runner) {
        for _ in 0..100 {
            assert_eq!(self.step(runner, 2, &[]), None);
        }
    }

    /// Runs a menu: settles, then presses.
    fn run(&mut self, file: &str, section: &str, presses: &[Key]) -> Option<ScriptEnd> {
        let mut runner = self.runner(file, section);
        self.settle(&mut runner);
        let mut end = None;
        for key in presses {
            end = self.step(&mut runner, 1, &[*key]);
        }
        end
    }
}

#[test]
fn menu_choices_hot_keys_and_cancel() {
    let Some(mut rig) = Rig::new() else { return };
    let down = [Key::Down, Key::Down, Key::Enter];
    assert_eq!(
        rig.run("TEXT.MNI", "Main_Menu", &down),
        Some(ScriptEnd::Chosen(3))
    );
    assert_eq!(rig.menus[1], 3);
    assert!(rig.events.contains(&UiEvent::Sound(Sound::MenuMove)));
    assert_eq!(
        rig.run("TEXT.MNI", "Main_Menu", &[Key::Letter('Q')]),
        Some(ScriptEnd::Chosen(8))
    );
    assert_eq!(
        rig.run("TEXT.MNI", "Main_Menu", &[Key::Escape]),
        Some(ScriptEnd::Cancel)
    );
    // Up wraps to the last page.
    rig.menus = [0; 16];
    assert_eq!(
        rig.run("TEXT.MNI", "Main_Menu", &[Key::Up, Key::Space]),
        Some(ScriptEnd::Chosen(8))
    );
}

#[test]
fn menu_remembers_its_selection() {
    let Some(mut rig) = Rig::new() else { return };
    rig.run("TEXT.MNI", "Main_Menu", &[Key::Down, Key::Down, Key::Enter]);
    // The next time it starts on the third entry.
    assert_eq!(
        rig.run("TEXT.MNI", "Main_Menu", &[Key::Enter]),
        Some(ScriptEnd::Chosen(3))
    );
}

#[test]
fn keys_are_ignored_while_the_menu_fades_in() {
    let Some(mut rig) = Rig::new() else { return };
    let mut runner = rig.runner("TEXT.MNI", "Main_Menu");
    assert_eq!(rig.step(&mut runner, 2, &[Key::Enter]), None);
    assert!(rig.screen.fade < Screen::FADE_STEPS);
    rig.settle(&mut runner);
    assert_eq!(rig.screen.fade, Screen::FADE_STEPS);
    assert_eq!(
        rig.step(&mut runner, 1, &[Key::Enter]),
        Some(ScriptEnd::Chosen(1))
    );
    // And it stays finished.
    assert_eq!(rig.step(&mut runner, 1, &[]), Some(ScriptEnd::Chosen(1)));
}

#[test]
fn wait_sections_continue_on_a_key() {
    let Some(mut rig) = Rig::new() else { return };
    let name = rig.find(
        "TEXT.MNI",
        |c| matches!(c, Command::Wait),
        |c| matches!(c, Command::Menu { .. } | Command::SetKeys { .. }),
    );
    let mut runner = rig.runner("TEXT.MNI", &name);
    let mut end = None;
    for _ in 0..400 {
        end = rig.step(&mut runner, 2, &[]);
        if end.is_some() {
            break;
        }
    }
    assert_eq!(end, None, "{name} waits for a key");
    assert_eq!(
        rig.step(&mut runner, 1, &[Key::Letter('X')]),
        Some(ScriptEnd::Done)
    );
}

#[test]
fn the_idle_menu_goes_to_the_demo() {
    let Some(mut rig) = Rig::new() else { return };
    let name = rig.find("TEXT.MNI", |c| matches!(c, Command::ExitToDemo), |_| false);
    let mut runner = rig.runner("TEXT.MNI", &name);
    rig.settle(&mut runner);
    // A press that does nothing restarts the count.
    assert_eq!(rig.step(&mut runner, 3000, &[Key::Tab]), None);
    let mut end = None;
    for _ in 0..1000 {
        end = rig.step(&mut runner, 10, &[]);
        if end.is_some() {
            break;
        }
    }
    assert_eq!(end, Some(ScriptEnd::ExitToDemo));
}

#[test]
fn toggles_switch_options() {
    let Some(mut rig) = Rig::new() else { return };
    let name = rig.find(
        "OPTIONS.MNI",
        |c| matches!(c, Command::Toggs { .. }),
        |_| false,
    );
    let before = rig.options;
    let mut runner = rig.runner("OPTIONS.MNI", &name);
    rig.settle(&mut runner);
    assert_eq!(rig.step(&mut runner, 1, &[Key::Enter]), None);
    assert_ne!(rig.options, before);
    assert!(rig.events.contains(&UiEvent::Sound(Sound::MenuToggle)));
    assert_eq!(
        rig.step(&mut runner, 1, &[Key::Escape]),
        Some(ScriptEnd::Cancel)
    );
}

#[test]
fn quit_select_and_every_section_runs() {
    let Some(mut rig) = Rig::new() else { return };
    if rig.data.scripts["TEXT.MNI"]
        .section("Quit_Select")
        .is_some()
    {
        let end = rig.run("TEXT.MNI", "Quit_Select", &[Key::Letter('N')]);
        assert_eq!(end, Some(ScriptEnd::Chosen(2)));
        let end = rig.run("TEXT.MNI", "Quit_Select", &[Key::Letter('Y')]);
        assert_eq!(end, Some(ScriptEnd::Chosen(1)));
    }
    // Every section runs through to something without panicking.
    let names: Vec<(String, String)> = rig
        .data
        .scripts
        .iter()
        .flat_map(|(f, s)| s.sections.iter().map(move |s| (f.clone(), s.name.clone())))
        .collect();
    for (file, name) in names {
        let mut runner = rig.runner(&file, &name);
        for _ in 0..3000 {
            if rig.step(&mut runner, 7, &[Key::Enter]).is_some() {
                break;
            }
        }
    }
}
