//! The program as the original runs it: the logo, the intro and the attract
//! mode, the main menu and the screens behind it, a game, the high scores.
//!
//! The original runs these as blocking functions that wait for keys and delay;
//! here a [`Mode`] says what is on the screen and each update runs it for the
//! ticks that passed.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_game::Buttons;
use dn2_render::Screen;

use crate::config::{Bindings, Config};
use crate::game::{Session, level_of};
use crate::movie::{Movie, MovieKind};
use crate::play::{Play, PlayEnd, clear_cursor, script, text_cursor, typed};
use crate::runner::{Runner, ScriptEnd, ScriptEnv};
use crate::saves::{HighScores, NAME_LEN, SavedGame, SlotNames, is_slot};
use crate::scene::{Scene, Step};
use crate::{Ctx, Key, Store, UiEvent};

/// Ticks the intro waits after its movie (`delay(0x1e)`).
const INTRO_SETTLE_TICKS: u32 = 30;
/// Ticks each of the attract mode's two 700 tick waits for a key.
const ATTRACT_PAUSE_TICKS: u32 = 700;
/// The menu's music.
const MENU_SONG: &str = "DUKEIIA.IMF";

/// What the screens share: the settings, the menus' last selections, the
/// saved games' names.
pub(crate) struct Shared {
    pub config: Config,
    /// The last selection of each menu.
    pub menus: [u8; 16],
    pub slots: SlotNames,
    /// The sound effects are on (the S key in a game).
    pub sound: bool,
    /// A Shift key is held, for typing names.
    pub shift: bool,
    /// [`Input::music_near_end`].
    pub music_near_end: bool,
    /// The program plays one level for the browser or `--play`: when the
    /// game is over, the program is too.
    pub single_level: bool,
}

impl Shared {
    pub fn env(&mut self) -> ScriptEnv<'_> {
        ScriptEnv {
            options: &mut self.config.options,
            bindings: &self.config.bindings,
            slot_names: &self.slots.0,
            menus: &mut self.menus,
        }
    }

    pub fn save_config(&self, store: &mut dyn Store) {
        store.save_store(Config::KEY, &self.config.to_bytes());
    }
}

/// The sections of the main menu's choices 1 to 7: the file, the section and
/// the menu they start.
const MAIN_MENU: [(&str, &str, Menu); 7] = [
    ("TEXT.MNI", "Episode_Select", Menu::Episode),
    ("OPTIONS.MNI", "Restore_Game", Menu::Restore),
    ("OPTIONS.MNI", "My_Options", Menu::Options),
    ("TEXT.MNI", "V4ORDER", Menu::Plain),
    ("TEXT.MNI", "Both_S_I", Menu::Both),
    ("TEXT.MNI", "Episode_Select", Menu::ScoresEpisode),
    ("TEXT.MNI", "&Credits", Menu::Credits),
];

/// What a menu script was started for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Menu {
    Main,
    /// Choosing the episode of a new game.
    Episode,
    /// Choosing the skill level of a new game of an episode.
    Skill(u8),
    /// Choosing whose high scores to see.
    ScoresEpisode,
    /// The title of an episode's high scores; the table follows.
    ScoresTitle(u8),
    /// The credits, then a key.
    Credits,
    /// Instructions or story?
    Both,
    Options,
    Speed,
    Restore,
    RestoreMissing,
    /// "Restoring!", then the slot's game starts.
    Restoring(u8),
    Quit,
    /// A screen that returns to the main menu.
    Plain,
    /// The advertisement before the intro, on the first run.
    Hype,
    /// The story in the attract mode.
    Story,
    /// The credits in the attract mode.
    AttractCredits,
}

/// What the screen shows.
enum Mode {
    /// LCR.MNI until a key.
    Logo,
    Script(Runner, Menu),
    /// A scene, then what follows it.
    Scene(Scene, AfterScene),
    /// The intro (`Title(0)` stands for it) or title `n` of the attract
    /// mode.
    Movie(Movie, u8),
    /// One of the attract mode's waits for a key: ticks left, the second.
    Pause(u32, bool),
    Play(Box<Play>),
    /// "New high score" before the name is typed.
    HighScore(Runner, Box<NameEntry>),
    NameEntry(Box<NameEntry>),
    KeyConfig(Box<KeyConfig>),
    /// The player chose to quit.
    Over,
}

impl Mode {
    /// A scene that then goes on to `after`.
    fn scene(steps: Vec<Step>, after: AfterScene) -> Self {
        Mode::Scene(Scene::new(steps, 0), after)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterScene {
    MainMenu,
    /// The fanfare has started: the intro movie.
    Fanfare,
    /// After the intro movie: the attract mode.
    Settle,
}

/// Typing a name for the high score table.
struct NameEntry {
    episode: u8,
    score: u32,
    position: usize,
    scores: HighScores,
    name: String,
    ticks: u32,
}

/// Where the key configuration returns to.
enum KeyBack {
    Menu,
    Game(Box<Play>),
}

/// Changing the keys (the key configuration of the options menu).
struct KeyConfig {
    runner: Runner,
    /// Waiting for the new key of key 1-6, and whether "THAT KEY IS ALREADY IN
    /// USE!" shows.
    asking: Option<(u8, bool)>,
    back: KeyBack,
    ticks: u32,
}

/// What a frontend gives an update.
pub struct Input<'a> {
    /// Keys pressed since the last update, in order.
    pub pressed: &'a [Key],
    /// Keys held now.
    pub held: &'a [Key],
    /// Buttons held on a gamepad.
    pub buttons: Buttons,
    /// Fewer than 41 bytes of the song are left: the movies stop.
    pub music_near_end: bool,
}

pub struct App {
    screen: Screen,
    mode: Mode,
    shared: Shared,
    /// The story shows in the first attract mode only.
    story_seen: bool,
}

impl App {
    /// The program from the logo. `store` has the settings and saved
    /// games.
    pub fn new(data: &GameData, store: &dyn Store) -> Self {
        let config = Config::load(store);
        let mut menus = [0; 16];
        // The skill level starts at the last one chosen, else "medium";
        // the game speed at its setting.
        menus[0] = match config.difficulty {
            0 => 2,
            d => d,
        };
        menus[8] = config.options.speed;
        let mut app = Self {
            screen: Screen::new(320, 200, data.palette.clone()),
            mode: Mode::Logo,
            shared: Shared {
                config,
                menus,
                slots: SlotNames::load(store),
                sound: true,
                shift: false,
                music_near_end: false,
                single_level: false,
            },
            story_seen: false,
        };
        app.screen.clear(0);
        if let Some(logo) = data.pictures.get("LCR.MNI") {
            app.screen.draw(&logo.image, 0, 0);
            app.screen.palette = logo.palette.clone();
        }
        app
    }

    /// The program playing one level (by file name) on medium, as a game
    /// of its episode from that level.
    pub fn level(data: &GameData, store: &dyn Store, file: &str) -> Option<Self> {
        let (episode, level) = level_of(data, file)?;
        let mut app = Self::new(data, store);
        let mut session = Session::new(episode, 2);
        session.level = level;
        app.screen.palette = data.palette.clone();
        app.shared.single_level = true;
        app.mode = Mode::Play(Box::new(Play::level(data, session)));
        Some(app)
    }

    /// The sound, music and speed options.
    pub fn options(&self) -> &crate::config::Options {
        &self.shared.config.options
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Switches the screen's higher-detail overlay on at `scale`, or off
    /// (see [`Screen::set_hd_scale`]).
    pub fn set_hd_scale(&mut self, scale: Option<u32>) {
        self.screen.set_hd_scale(scale);
    }

    /// Makes the screen `columns` tiles wide (40 is the original's): the
    /// game's view grows by the extra columns, everything else is shown in
    /// the middle. What is on the screen stays.
    pub fn set_columns(&mut self, columns: u16) {
        let old = &self.screen;
        let mut screen = Screen::new(u32::from(columns.max(40)) * 8, 200, old.palette.clone());
        screen.fade = old.fade;
        screen.set_hd_scale(old.hd_scale());
        let (left, old_left) = (screen.left, old.left);
        screen.left = 0;
        for y in 0..old.image.height {
            for x in 0..old.image.width {
                screen.put(x as i32 - old_left + left, y as i32, old.image.get(x, y));
            }
        }
        screen.left = left;
        self.screen = screen;
    }

    /// What is on the screen, for logs and tests: e.g. "logo", "movie",
    /// "menu Main", "game Playing".
    pub fn mode_name(&self) -> String {
        match &self.mode {
            Mode::Logo => "logo".into(),
            Mode::Script(_, menu) => format!("menu {menu:?}"),
            Mode::Scene(_, after) => format!("scene {after:?}"),
            Mode::Movie(_, title) => format!("movie {title}"),
            Mode::Pause(..) => "pause".into(),
            Mode::Play(play) => format!("game {}", play.phase_name()),
            Mode::HighScore(..) => "high score".into(),
            Mode::NameEntry(_) => "name entry".into(),
            Mode::KeyConfig(_) => "key config".into(),
            Mode::Over => "over".into(),
        }
    }

    /// Advances by `ticks` of [`crate::TICK_HZ`].
    pub fn update(
        &mut self,
        data: &GameData,
        store: &mut dyn Store,
        input: &Input,
        ticks: u32,
    ) -> Vec<UiEvent> {
        let mut events = Vec::new();
        self.shared.shift = input
            .held
            .iter()
            .any(|k| matches!(k, Key::LeftShift | Key::RightShift));
        self.shared.music_near_end = input.music_near_end;
        let held = self.held(input);
        let mode = core::mem::replace(&mut self.mode, Mode::Over);
        let mut ctx = Ctx {
            screen: &mut self.screen,
            data,
            pressed: input.pressed,
            ticks,
            events: &mut events,
        };
        let mut flow = Flow {
            shared: &mut self.shared,
            story_seen: &mut self.story_seen,
            store,
        };
        self.mode = flow.step(&mut ctx, mode, held);
        if matches!(self.mode, Mode::Over) {
            events.push(UiEvent::Quit);
        }
        if !self.shared.sound {
            events.retain(|e| !matches!(e, UiEvent::Sound(_)));
        }
        if !self.shared.config.options.music {
            events.retain(|e| !matches!(e, UiEvent::Music(Some(_))));
        }
        events
    }

    /// The game's buttons the keys and the gamepad hold.
    fn held(&self, input: &Input) -> Buttons {
        let b: &Bindings = &self.shared.config.bindings;
        let mut held = input.buttons;
        for (key, button) in [
            (b.up, Buttons::UP),
            (b.down, Buttons::DOWN),
            (b.left, Buttons::LEFT),
            (b.right, Buttons::RIGHT),
            (b.jump, Buttons::JUMP),
            (b.fire, Buttons::FIRE),
        ] {
            if input.held.contains(&key) {
                held |= button;
            }
        }
        held
    }
}

/// The state an update works with besides the screen.
struct Flow<'a> {
    shared: &'a mut Shared,
    story_seen: &'a mut bool,
    store: &'a mut dyn Store,
}

impl Flow<'_> {
    fn step(&mut self, ctx: &mut Ctx, mode: Mode, held: Buttons) -> Mode {
        match mode {
            Mode::Logo => {
                if ctx.pressed.is_empty() {
                    return Mode::Logo;
                }
                // Without a configuration (the first run) the
                // advertisement comes first.
                ctx.screen.clear(0);
                ctx.screen.palette = ctx.data.palette.clone();
                if self.store.load_store(Config::KEY).is_none()
                    && let Some(runner) = script(ctx.data, "TEXT.MNI", "HYPE")
                {
                    return Mode::Script(runner, Menu::Hype);
                }
                self.fanfare()
            }
            // A key skips the advertisement, which the original shows to
            // its end: three pictures, ten seconds of delays.
            Mode::Script(_, Menu::Hype) if !ctx.pressed.is_empty() => self.fanfare(),
            Mode::Script(mut runner, menu) => match runner.step(ctx, &mut self.shared.env()) {
                None => Mode::Script(runner, menu),
                Some(end) => self.chosen(ctx, menu, end),
            },
            Mode::Scene(mut scene, after) => {
                if !scene.step(ctx) {
                    return Mode::Scene(scene, after);
                }
                match after {
                    AfterScene::MainMenu => self.main_menu(ctx, false),
                    AfterScene::Fanfare => match Movie::new(ctx.data, MovieKind::Intro) {
                        Some(movie) => {
                            ctx.screen.fade = Screen::FADE_STEPS;
                            Mode::Movie(movie, 0)
                        }
                        None => self.attract(ctx),
                    },
                    AfterScene::Settle => self.attract(ctx),
                }
            }
            Mode::Movie(mut movie, title) => {
                // The movie stops for a key or the end of the song.
                let key = !ctx.pressed.is_empty() || self.shared.music_near_end;
                let over = movie.step(ctx);
                match title {
                    // A key ends the intro.
                    0 if key || over => Mode::scene(
                        alloc::vec![Step::Wait(INTRO_SETTLE_TICKS)],
                        AfterScene::Settle,
                    ),
                    // A key ends the titles and the attract mode with them.
                    _ if key => self.main_menu(ctx, true),
                    _ if over => self.title(ctx, title + 1),
                    _ => Mode::Movie(movie, title),
                }
            }
            Mode::Pause(left, second) => {
                if !ctx.pressed.is_empty() {
                    return self.main_menu(ctx, true);
                }
                match left.checked_sub(ctx.ticks) {
                    Some(left) if left > 0 => Mode::Pause(left, second),
                    _ if second => self.demo(ctx),
                    _ => Mode::Pause(ATTRACT_PAUSE_TICKS, true),
                }
            }
            Mode::Play(mut play) => {
                match play.step(ctx, self.shared, self.store, held) {
                    None => Mode::Play(play),
                    Some(PlayEnd::Over { .. }) if self.shared.single_level => Mode::Over,
                    Some(PlayEnd::Over { episode, score }) => self.game_over(ctx, episode, score),
                    // The end of the demo shows the intro again.
                    Some(PlayEnd::Demo { aborted: false }) => self.fanfare(),
                    Some(PlayEnd::Demo { aborted: true }) => self.main_menu(ctx, true),
                    Some(PlayEnd::KeyConfig) => self.key_config(ctx, KeyBack::Game(play)),
                    Some(PlayEnd::Failed) => self.main_menu(ctx, true),
                }
            }
            Mode::HighScore(mut runner, entry) => match runner.step(ctx, &mut self.shared.env()) {
                None => Mode::HighScore(runner, entry),
                Some(_) => Mode::NameEntry(entry),
            },
            Mode::NameEntry(entry) => self.name_entry(ctx, entry),
            Mode::KeyConfig(config) => self.step_key_config(ctx, config),
            Mode::Over => Mode::Over,
        }
    }

    /// A menu script, or the main menu if the data lacks it.
    fn script(&mut self, ctx: &mut Ctx, file: &str, section: &str, menu: Menu) -> Mode {
        match script(ctx.data, file, section) {
            Some(runner) => Mode::Script(runner, menu),
            None if menu == Menu::Main => Mode::Over,
            None => self.main_menu(ctx, false),
        }
    }

    /// The menu, with its music when coming from a game or the attract mode.
    fn main_menu(&mut self, ctx: &mut Ctx, music: bool) -> Mode {
        if music {
            ctx.music(Some(MENU_SONG));
        }
        ctx.screen.palette = ctx.data.palette.clone();
        self.script(ctx, "TEXT.MNI", "Main_Menu", Menu::Main)
    }

    /// What follows a script that ended with `end`.
    fn chosen(&mut self, ctx: &mut Ctx, menu: Menu, end: ScriptEnd) -> Mode {
        use ScriptEnd::{Cancel, Chosen, ExitToDemo};
        match (menu, end) {
            (Menu::Main, Chosen(choice @ 1..=7)) => {
                let (file, section, menu) = MAIN_MENU[usize::from(choice) - 1];
                self.script(ctx, file, section, menu)
            }
            (Menu::Main, ExitToDemo) => self.attract(ctx),
            (Menu::Main, _) => self.script(ctx, "TEXT.MNI", "Quit_Select", Menu::Quit),
            (Menu::Quit, Chosen(1)) => {
                self.shared.save_config(self.store);
                Mode::Over
            }
            (Menu::Episode, Chosen(episode)) => {
                if ctx.data.episodes.iter().any(|e| e.number == episode) {
                    self.script(ctx, "TEXT.MNI", "Skill_Select", Menu::Skill(episode))
                } else {
                    // The shareware has episode 1 only: how to order.
                    self.script(ctx, "TEXT.MNI", "V4ORDER", Menu::Plain)
                }
            }
            (Menu::Skill(_), Cancel) => {
                self.script(ctx, "TEXT.MNI", "Episode_Select", Menu::Episode)
            }
            (Menu::Skill(episode), Chosen(difficulty)) => {
                let difficulty = difficulty.clamp(1, 3);
                self.shared.config.difficulty = difficulty;
                self.shared.save_config(self.store);
                ctx.music(None);
                Mode::Play(Box::new(Play::new(Session::new(episode, difficulty), true)))
            }
            (Menu::ScoresEpisode, Chosen(episode)) => self.scores(ctx, episode.clamp(1, 4)),
            (Menu::ScoresTitle(episode), _) => {
                self.draw_scores(ctx, episode);
                let steps = alloc::vec![Step::FadeIn, Step::WaitKey(None)];
                Mode::scene(steps, AfterScene::MainMenu)
            }
            (Menu::Credits, _) => {
                Mode::scene(alloc::vec![Step::WaitKey(None)], AfterScene::MainMenu)
            }
            (Menu::Both, Chosen(1)) => self.script(ctx, "TEXT.MNI", "&Instructions", Menu::Plain),
            (Menu::Both, Chosen(2)) => self.script(ctx, "TEXT.MNI", "&Story", Menu::Plain),
            (Menu::Options, Chosen(5)) => self.key_config(ctx, KeyBack::Menu),
            (Menu::Options, Chosen(7)) => {
                self.script(ctx, "OPTIONS.MNI", "Game_Speed", Menu::Speed)
            }
            (Menu::Options, Cancel) => {
                self.shared.save_config(self.store);
                self.main_menu(ctx, false)
            }
            (Menu::Options, _) => self.script(ctx, "OPTIONS.MNI", "My_Options", Menu::Options),
            (Menu::Speed, end) => {
                if let Chosen(speed) = end {
                    self.shared.config.options.speed = speed.clamp(1, 7);
                }
                self.script(ctx, "OPTIONS.MNI", "My_Options", Menu::Options)
            }
            (Menu::Restore, Chosen(slot)) if is_slot(slot) => {
                if SavedGame::load(self.store, slot).is_some() {
                    self.script(ctx, "TEXT.MNI", "&Load", Menu::Restoring(slot))
                } else {
                    self.script(ctx, "OPTIONS.MNI", "No_Game_Restore", Menu::RestoreMissing)
                }
            }
            (Menu::RestoreMissing, _) => {
                self.script(ctx, "OPTIONS.MNI", "Restore_Game", Menu::Restore)
            }
            (Menu::Restoring(slot), _) => match SavedGame::load(self.store, slot) {
                Some(saved) => {
                    ctx.music(None);
                    Mode::Play(Box::new(Play::new(saved.session(), false)))
                }
                None => self.main_menu(ctx, false),
            },
            (Menu::Hype, _) => self.fanfare(),
            // Esc in the story returns to the menu.
            (Menu::Story, Cancel) => self.main_menu(ctx, true),
            (Menu::Story, _) => self.attract_credits(ctx),
            (Menu::AttractCredits, _) => Mode::Pause(ATTRACT_PAUSE_TICKS, false),
            _ => self.main_menu(ctx, false),
        }
    }

    /// The screen fades out and the fanfare starts; the movie follows.
    fn fanfare(&mut self) -> Mode {
        let steps = alloc::vec![Step::FadeOut, Step::music("FANFAREA.IMF")];
        Mode::scene(steps, AfterScene::Fanfare)
    }

    /// The music and the titles, the story (the first time), the credits, two
    /// waits for a key, the demo. Also what the main menu starts when it sits
    /// idle.
    fn attract(&mut self, ctx: &mut Ctx) -> Mode {
        ctx.music(Some("RANGEA.IMF"));
        self.title(ctx, 1)
    }

    /// Title movie `n` (1-4); after the last, the story or the credits.
    fn title(&mut self, ctx: &mut Ctx, n: u8) -> Mode {
        if let Some(movie) = Movie::new(ctx.data, MovieKind::Title(n)) {
            ctx.screen.fade = Screen::FADE_STEPS;
            return Mode::Movie(movie, n);
        }
        if n < 4 {
            return self.title(ctx, n + 1);
        }
        ctx.screen.clear(0);
        ctx.screen.palette = ctx.data.palette.clone();
        if !core::mem::replace(self.story_seen, true)
            && let Some(runner) = script(ctx.data, "TEXT.MNI", "&Story")
        {
            return Mode::Script(runner, Menu::Story);
        }
        self.attract_credits(ctx)
    }

    fn attract_credits(&mut self, ctx: &mut Ctx) -> Mode {
        match script(ctx.data, "TEXT.MNI", "&Credits") {
            Some(runner) => Mode::Script(runner, Menu::AttractCredits),
            None => Mode::Pause(ATTRACT_PAUSE_TICKS, false),
        }
    }

    fn demo(&mut self, ctx: &mut Ctx) -> Mode {
        if ctx.data.demo.is_empty() {
            return self.main_menu(ctx, true);
        }
        ctx.music(None);
        Mode::Play(Box::new(Play::demo(ctx.data)))
    }

    /// The episode's title, then the table.
    fn scores(&mut self, ctx: &mut Ctx, episode: u8) -> Mode {
        let section = format!("Volume{episode}");
        match script(ctx.data, "TEXT.MNI", &section) {
            Some(runner) => Mode::Script(runner, Menu::ScoresTitle(episode)),
            None => self.main_menu(ctx, true),
        }
    }

    /// Scores and names, the first one set apart by an empty line.
    fn draw_scores(&self, ctx: &mut Ctx, episode: u8) {
        let table = HighScores::load(self.store, episode);
        let gfx = ctx.gfx();
        for (i, (name, score)) in table.entries.iter().enumerate() {
            let row = (i + usize::from(i >= 1) + 6) as i32;
            gfx.text(ctx.screen, format!("{score}").as_bytes(), row, 10);
            gfx.text(ctx.screen, name.as_bytes(), row, 0x14);
        }
    }

    /// The game ended (won, or quit): a name for the high score table if
    /// the score is good enough, then the table, then the menu.
    fn game_over(&mut self, ctx: &mut Ctx, episode: u8, score: u32) -> Mode {
        ctx.music(Some(MENU_SONG));
        let scores = HighScores::load(self.store, episode);
        let Some(position) = scores.position(score) else {
            return self.scores(ctx, episode);
        };
        let entry = Box::new(NameEntry {
            episode,
            score,
            position,
            scores,
            name: String::new(),
            ticks: 0,
        });
        match script(ctx.data, "TEXT.MNI", "New_Highscore") {
            // The script runs first; then the name is typed.
            Some(runner) => Mode::HighScore(runner, entry),
            None => Mode::NameEntry(entry),
        }
    }

    /// Typing the name at row 14, from column 13.
    fn name_entry(&mut self, ctx: &mut Ctx, mut entry: Box<NameEntry>) -> Mode {
        const ROW: i32 = 14;
        let gfx = ctx.gfx();
        entry.ticks += ctx.ticks;
        for &key in ctx.pressed {
            let col = 13 + entry.name.len() as i32;
            match key {
                Key::Enter | Key::Escape => {
                    if key == Key::Escape {
                        entry.name.clear();
                    }
                    let NameEntry {
                        episode,
                        score,
                        position,
                        mut scores,
                        name,
                        ..
                    } = *entry;
                    scores.insert(position, &name, score);
                    scores.save(self.store, episode);
                    return self.scores(ctx, episode);
                }
                Key::Backspace if !entry.name.is_empty() => {
                    entry.name.pop();
                    clear_cursor(ctx, ROW, col);
                }
                key => {
                    if let Some(c) = typed(key, self.shared.shift)
                        && entry.name.len() < NAME_LEN
                    {
                        clear_cursor(ctx, ROW, col);
                        gfx.small_char(ctx.screen, c as u8, ROW, col);
                        entry.name.push(c);
                    }
                }
            }
        }
        let col = 13 + entry.name.len() as i32;
        text_cursor(ctx, ROW, col, entry.ticks);
        Mode::NameEntry(entry)
    }

    /// The screen of the six keys (OPTIONS.MNI `Key_Config`).
    fn key_config(&mut self, ctx: &mut Ctx, back: KeyBack) -> Mode {
        match script(ctx.data, "OPTIONS.MNI", "Key_Config") {
            Some(runner) => Mode::KeyConfig(Box::new(KeyConfig {
                runner,
                asking: None,
                back,
                ticks: 0,
            })),
            None => self.leave_key_config(ctx, back),
        }
    }

    fn leave_key_config(&mut self, ctx: &mut Ctx, back: KeyBack) -> Mode {
        self.shared.save_config(self.store);
        match back {
            KeyBack::Menu => self.script(ctx, "OPTIONS.MNI", "My_Options", Menu::Options),
            KeyBack::Game(mut play) => {
                play.back_to_options(ctx.data);
                Mode::Play(play)
            }
        }
    }

    fn step_key_config(&mut self, ctx: &mut Ctx, mut config: Box<KeyConfig>) -> Mode {
        let gfx = ctx.gfx();
        let Some((index, in_use)) = config.asking else {
            return match config.runner.step(ctx, &mut self.shared.env()) {
                None => Mode::KeyConfig(config),
                Some(ScriptEnd::Chosen(index)) if (1..=Bindings::COUNT).contains(&index) => {
                    gfx.text(ctx.screen, b"Press a key to use..", 0x13, 10);
                    gfx.text(ctx.screen, b"       ", i32::from(index) * 2 + 5, 0x1a);
                    config.asking = Some((index, false));
                    Mode::KeyConfig(config)
                }
                Some(_) => self.leave_key_config(ctx, config.back),
            };
        };
        let (row, col) = match in_use {
            false => (i32::from(index) * 2 + 5, 0x1a),
            true => (0x14, 0x1e),
        };
        config.ticks += ctx.ticks;
        let Some(&key) = ctx.pressed.first() else {
            text_cursor(ctx, row, col, config.ticks);
            return Mode::KeyConfig(config);
        };
        clear_cursor(ctx, row, col);
        if in_use {
            // Any key: ask again.
            gfx.text(ctx.screen, b"                           ", 0x13, 8);
            gfx.text(ctx.screen, b"                      ", 0x14, 8);
            gfx.text(ctx.screen, b"Press a key to use..", 0x13, 10);
            config.asking = Some((index, false));
            return Mode::KeyConfig(config);
        }
        let bindings = &mut self.shared.config.bindings;
        let taken = (1..=Bindings::COUNT).any(|i| i != index && bindings.get(i) == Some(key));
        if taken || key == Key::Escape {
            gfx.text(ctx.screen, b"THAT KEY IS ALREADY IN USE!", 0x13, 8);
            gfx.text(ctx.screen, b"   Select another key.", 0x14, 8);
            config.asking = Some((index, true));
            return Mode::KeyConfig(config);
        }
        if let Some(slot) = bindings.slot(index) {
            *slot = key;
        }
        self.key_config(ctx, config.back)
    }
}
