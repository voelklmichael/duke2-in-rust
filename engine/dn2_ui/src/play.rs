//! A game from its first level to the end of the episode: the loading screen,
//! the fades, the level, the restart after dying, the bonus screen and the
//! menus over the game.

use alloc::string::String;

use dn2_assets::GameData;
use dn2_assets::script::Command;
use dn2_game::Buttons;
use dn2_game::messages::text;
use dn2_game::state::LevelExit;
use dn2_render::Screen;

use crate::app::Shared;
use crate::demo::{Demo, DemoFrame, demo_levels};
use crate::draw::{BOX_FILL, TEXT_CURSOR};
use crate::game::{Carry, Level, Session};
use crate::runner::{Runner, ScriptEnd};
use crate::saves::{SLOT_NAME_LEN, SavedGame};
use crate::scene::{self, FADE_IN_TICKS, FADE_OUT_TICKS, Scene};
use crate::{Ctx, Key, Store, UiEvent};

/// How a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayEnd {
    /// The episode was won or the player quit: the high scores follow.
    Over { episode: u8, score: u32 },
    /// The demo ended, or a key ended it.
    Demo { aborted: bool },
    /// The options menu over the game chose the key configuration; the
    /// game waits ([`Play::back_to_options`]).
    KeyConfig,
    /// The level could not be loaded.
    Failed,
}

/// What the game is doing.
enum Phase {
    /// The loading picture; then the level is loaded.
    Loading(Scene),
    /// The first frame of a level, after a restart or after a screen over
    /// the game, fading in: ticks so far.
    FadeIn(u32),
    Playing,
    /// The player died or fell out of the map: the fade out before the
    /// restart.
    RestartOut(u32),
    /// A transporter: the fade out before the player moves.
    TeleportOut(u32),
    /// The exit was reached.
    Bonus(Scene),
    /// The boss died.
    Ending(Scene),
    /// A script over the game.
    Overlay(Runner, Overlay),
    /// Typing the name of a saved game in slot 1-8.
    SaveName {
        slot: u8,
        name: String,
        blink: u32,
    },
}

/// Which script is over the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    /// Pause, music or sound on or off: a box over the game.
    Message,
    Help,
    Quit,
    Options,
    Speed,
    SaveSlot,
    /// "SAVING!!", then the slot is written.
    Saving(u8),
    RestoreSlot,
    RestoreMissing,
    /// "Restoring!", then the slot's level loads.
    Restoring(u8),
}

/// The script section `name` of `file`, as a runner.
pub(crate) fn script(data: &GameData, file: &str, name: &str) -> Option<Runner> {
    Some(Runner::new(data.scripts.get(file)?.section(name)?))
}

/// The hint of the `Hints` section of HELP.MNI for a level (both from 1),
/// upper case.
fn hint(data: &GameData, episode: u8, level: u8) -> Option<String> {
    let section = data.scripts.get("HELP.MNI")?.section("Hints")?;
    section.commands.iter().find_map(|c| match c {
        Command::HelpText {
            episode: e,
            level: l,
            text,
        } if *e == episode && *l == level => {
            Some(String::from_utf8_lossy(text).to_ascii_uppercase())
        }
        _ => None,
    })
}

pub struct Play {
    pub session: Session,
    level: Option<Level>,
    phase: Phase,
    /// The input of the attract mode's demo instead of the keys.
    demo: Option<Demo>,
    /// "WELCOME TO DUKE NUKEM II!" once the first level has loaded.
    welcome: bool,
    /// Ticks towards the next game frame.
    ticks: u32,
}

impl Play {
    /// A game of `session`, starting with its loading screen.
    pub fn new(session: Session, welcome: bool) -> Self {
        Self {
            phase: Phase::Loading(Scene::new(
                scene::loading_screen(session.episode),
                session.start.score,
            )),
            session,
            level: None,
            demo: None,
            welcome,
            ticks: 0,
        }
    }

    /// The demo of the attract mode: levels 1, 3, 5 and 7 of episode 1 on hard,
    /// played by NUKEM2.MNI.
    pub fn demo(data: &GameData) -> Self {
        let mut play = Self::new(Session::new(1, 3), false);
        play.demo = Some(Demo::new(&data.demo));
        play
    }

    /// One level without a loading screen (`--level` and the browser).
    pub fn level(data: &GameData, session: Session) -> Self {
        let mut play = Self::new(session, false);
        play.phase = Phase::Playing;
        play.level = Level::load(data, &play.session);
        play
    }

    fn load_level(&self, data: &GameData) -> Option<Level> {
        match &self.demo {
            Some(_) => {
                let index = usize::from(self.session.level).checked_sub(1)?;
                let file = demo_levels().get(index).copied()?;
                Level::load_file(
                    data,
                    file,
                    self.session.level * 2 - 1,
                    self.session.difficulty,
                    self.session.start,
                )
            }
            None => Level::load(data, &self.session),
        }
    }

    /// The end of loading a level: the level's music, and one frame of play
    /// shown while fading in.
    fn start_level(&mut self, ctx: &mut Ctx, held: Buttons) -> bool {
        let Some(mut level) = self.load_level(ctx.data) else {
            return false;
        };
        if core::mem::take(&mut self.welcome) {
            level.world.state.messages.show(text::WELCOME);
        }
        ctx.events.push(UiEvent::Music(Some(level.music(ctx.data))));
        self.level = Some(level);
        self.first_frame(ctx, held);
        true
    }

    /// A frame of play drawn from black, then the fade in.
    fn first_frame(&mut self, ctx: &mut Ctx, held: Buttons) {
        ctx.screen.fade = 0;
        if let Some(level) = &mut self.level {
            level.step(held, ctx.data, ctx.events);
        }
        self.draw(ctx.data, ctx.screen);
        self.phase = Phase::FadeIn(0);
    }

    /// Back from a screen over the game (help, options): the fades, the status
    /// bar and a frame.
    fn resume(&mut self, ctx: &mut Ctx) {
        ctx.screen.fade = 0;
        self.draw(ctx.data, ctx.screen);
        self.phase = Phase::FadeIn(0);
    }

    /// Back to the options menu after the key configuration.
    pub fn back_to_options(&mut self, data: &GameData) {
        if let Some(runner) = script(data, "OPTIONS.MNI", "My_Options") {
            self.phase = Phase::Overlay(runner, Overlay::Options);
        } else {
            self.phase = Phase::FadeIn(0);
        }
    }

    /// The phase's name, for logs and tests.
    pub fn phase_name(&self) -> String {
        String::from(match &self.phase {
            Phase::Loading(_) => "Loading",
            Phase::FadeIn(_) => "FadeIn",
            Phase::Playing => "Playing",
            Phase::RestartOut(_) => "RestartOut",
            Phase::TeleportOut(_) => "TeleportOut",
            Phase::Bonus(_) => "Bonus",
            Phase::Ending(_) => "Ending",
            Phase::Overlay(_, Overlay::Message) => "Overlay Message",
            Phase::Overlay(_, Overlay::Quit) => "Overlay Quit",
            Phase::Overlay(..) => "Overlay",
            Phase::SaveName { .. } => "SaveName",
        })
    }

    pub fn draw(&self, data: &GameData, screen: &mut Screen) {
        if let Some(level) = &self.level {
            level.draw(data, screen);
        }
    }

    /// Advances the game by `ctx.ticks`. `held` is what the keys and the
    /// gamepad hold of the game's buttons.
    pub fn step(
        &mut self,
        ctx: &mut Ctx,
        shared: &mut Shared,
        store: &mut dyn Store,
        held: Buttons,
    ) -> Option<PlayEnd> {
        let phase = core::mem::replace(&mut self.phase, Phase::Playing);
        match phase {
            Phase::Loading(mut scene) => {
                // A key skips the loading bar (the original waits for it).
                if !ctx.pressed.is_empty() {
                    scene.skip_to_last();
                }
                if !scene.step(ctx) {
                    self.phase = Phase::Loading(scene);
                } else if !self.start_level(ctx, held) {
                    return Some(PlayEnd::Failed);
                }
            }
            Phase::FadeIn(elapsed) => {
                let elapsed = elapsed + ctx.ticks;
                let steps = u32::from(Screen::FADE_STEPS);
                ctx.screen.fade = (elapsed * steps / FADE_IN_TICKS).min(steps) as u8;
                self.draw(ctx.data, ctx.screen);
                if elapsed < FADE_IN_TICKS {
                    self.phase = Phase::FadeIn(elapsed);
                } else {
                    self.ticks = 0;
                }
            }
            Phase::Playing => return self.playing(ctx, shared, held),
            Phase::RestartOut(elapsed) => {
                let elapsed = elapsed + ctx.ticks;
                if fade_out(ctx, elapsed) {
                    self.phase = Phase::RestartOut(elapsed);
                } else {
                    return self.restart(ctx, held);
                }
            }
            // Fade out, the move, then a frame and fade in.
            Phase::TeleportOut(elapsed) => {
                let elapsed = elapsed + ctx.ticks;
                if fade_out(ctx, elapsed) {
                    self.phase = Phase::TeleportOut(elapsed);
                } else {
                    if let Some(level) = &mut self.level {
                        level.world.finish_teleport();
                    }
                    self.first_frame(ctx, held);
                }
            }
            Phase::Bonus(mut scene) => {
                if !scene.step(ctx) {
                    self.phase = Phase::Bonus(scene);
                } else {
                    return self.next_level(scene.score());
                }
            }
            Phase::Ending(mut scene) => {
                if !scene.step(ctx) {
                    self.phase = Phase::Ending(scene);
                } else {
                    return Some(PlayEnd::Over {
                        episode: self.session.episode,
                        score: scene.score(),
                    });
                }
            }
            Phase::Overlay(mut runner, overlay) => {
                let (episode, level) = (self.session.episode, self.session.level);
                match runner.step(ctx, &mut shared.env(episode, level)) {
                    None => self.phase = Phase::Overlay(runner, overlay),
                    Some(end) => return self.overlay_done(ctx, shared, store, overlay, end),
                }
            }
            Phase::SaveName { slot, name, blink } => {
                return self.save_name(ctx, shared, store, slot, name, blink);
            }
        }
        None
    }

    /// Game frames for the time that passed, after the hot keys.
    fn playing(&mut self, ctx: &mut Ctx, shared: &mut Shared, held: Buttons) -> Option<PlayEnd> {
        if self.demo.is_some() {
            // A key ends the demo.
            if !ctx.pressed.is_empty() {
                return Some(PlayEnd::Demo { aborted: true });
            }
        } else if let Some((runner, overlay)) = self.hot_key(ctx, shared) {
            self.phase = Phase::Overlay(runner, overlay);
            return None;
        }
        self.ticks += ctx.ticks;
        let frame_ticks = shared.config.options.frame_ticks();
        while self.ticks >= frame_ticks && matches!(self.phase, Phase::Playing) {
            self.ticks -= frame_ticks;
            if let Some(end) = self.frame(ctx, held) {
                return Some(end);
            }
        }
        self.draw(ctx.data, ctx.screen);
        None
    }

    /// One iteration of the game loop: a frame and what it set.
    fn frame(&mut self, ctx: &mut Ctx, held: Buttons) -> Option<PlayEnd> {
        let mut input = held;
        let mut exit_level = false;
        if let Some(demo) = &mut self.demo {
            match demo.next() {
                DemoFrame::End => return Some(PlayEnd::Demo { aborted: false }),
                DemoFrame::Input {
                    buttons,
                    exit_level: exit,
                } => (input, exit_level) = (buttons, exit),
            }
        }
        let (episode, number) = (self.session.episode, self.session.level);
        let level = self.level.as_mut()?;
        level.step(input, ctx.data, ctx.events);
        let world = &mut level.world;
        if exit_level {
            world.state.exit.get_or_insert(LevelExit::Finished);
        }
        // The hint machine types the level's hint on the status line.
        if core::mem::take(&mut world.state.hint_requested)
            && let Some(text) = hint(ctx.data, episode, number)
        {
            world.state.messages.show_text(text);
        }
        let bonuses = || scene::secret_bonuses(&world.state.tally, world.player.was_hurt);
        match world.state.exit {
            None if world.state.teleport.is_some() => self.phase = Phase::TeleportOut(0),
            None => {}
            Some(LevelExit::Died) => self.phase = Phase::RestartOut(0),
            // The demo has no bonus screen.
            Some(LevelExit::Finished) if self.demo.is_some() => {
                let score = world.state.score;
                return self.next_level(score);
            }
            Some(LevelExit::Finished) => {
                let steps = scene::bonus_screen(world.state.score, &bonuses(), number == 8);
                self.phase = Phase::Bonus(Scene::new(steps, world.state.score));
            }
            Some(LevelExit::BossKilled) => {
                let steps = scene::episode_end(episode, world.state.score, &bonuses());
                self.phase = Phase::Ending(Scene::new(steps, world.state.score));
            }
        }
        None
    }

    /// After dying: back at the respawn beacon, or the level from its
    /// start as it was entered.
    fn restart(&mut self, ctx: &mut Ctx, held: Buttons) -> Option<PlayEnd> {
        let respawned = self.level.as_mut().is_some_and(|l| l.world.respawn());
        if respawned {
            self.first_frame(ctx, held);
        } else {
            self.level = Some(self.load_level(ctx.data)?);
            self.first_frame(ctx, held);
        }
        None
    }

    /// After the bonus screen: the next level with what the player has.
    fn next_level(&mut self, score: u32) -> Option<PlayEnd> {
        if let Some(level) = &self.level {
            self.session.start = Carry {
                score,
                ..Carry::of(&level.world)
            };
        }
        self.session.level += 1;
        let last = if self.demo.is_some() { 4 } else { 8 };
        if self.session.level > last {
            return Some(match self.demo {
                Some(_) => PlayEnd::Demo { aborted: false },
                None => PlayEnd::Over {
                    episode: self.session.episode,
                    score,
                },
            });
        }
        self.phase = Phase::Loading(Scene::new(
            scene::loading_screen(self.session.episode),
            score,
        ));
        None
    }

    /// The keys of the game: P pause, M music, S sound, H help, F1 options, F2
    /// save, F3 restore, Esc quit.
    fn hot_key(&mut self, ctx: &mut Ctx, shared: &mut Shared) -> Option<(Runner, Overlay)> {
        let options = &mut shared.config.options;
        let (file, section, overlay) = ctx.pressed.iter().find_map(|key| {
            Some(match key {
                Key::Letter('P') => ("TEXT.MNI", "Paused", Overlay::Message),
                Key::Letter('M') => {
                    options.music = !options.music;
                    let section = if options.music {
                        "Music_On"
                    } else {
                        "Music_Off"
                    };
                    ("TEXT.MNI", section, Overlay::Message)
                }
                Key::Letter('S') => {
                    shared.sound = !shared.sound;
                    let section = if shared.sound {
                        "Sound_On"
                    } else {
                        "Sound_Off"
                    };
                    ("TEXT.MNI", section, Overlay::Message)
                }
                Key::Letter('H') => ("TEXT.MNI", "&Instructions", Overlay::Help),
                Key::F(1) => ("OPTIONS.MNI", "My_Options", Overlay::Options),
                Key::F(2) => ("OPTIONS.MNI", "Save_Game", Overlay::SaveSlot),
                Key::F(3) => ("OPTIONS.MNI", "Restore_Game", Overlay::RestoreSlot),
                Key::Escape => ("TEXT.MNI", "2Quit_Select", Overlay::Quit),
                _ => return None,
            })
        })?;
        if section.starts_with("Music") {
            let song = self.level.as_ref().map(|l| l.music(ctx.data));
            ctx.events
                .push(UiEvent::Music(song.filter(|_| shared.config.options.music)));
        }
        Some((script(ctx.data, file, section)?, overlay))
    }

    fn overlay_done(
        &mut self,
        ctx: &mut Ctx,
        shared: &mut Shared,
        store: &mut dyn Store,
        overlay: Overlay,
        end: ScriptEnd,
    ) -> Option<PlayEnd> {
        let next = match (overlay, end) {
            (Overlay::Quit, ScriptEnd::Chosen(1)) => {
                let score = self.level.as_ref().map_or(0, |l| l.world.state.score);
                return Some(PlayEnd::Over {
                    episode: self.session.episode,
                    score,
                });
            }
            (Overlay::Message | Overlay::Quit, _) => {
                // The box is gone: the game shows again.
                self.draw(ctx.data, ctx.screen);
                self.ticks = 0;
                return None;
            }
            // The options menu goes round until Esc.
            (Overlay::Options, ScriptEnd::Chosen(5)) => return Some(PlayEnd::KeyConfig),
            (Overlay::Options, ScriptEnd::Chosen(7)) => {
                Some(("OPTIONS.MNI", "Game_Speed", Overlay::Speed))
            }
            (Overlay::Options, ScriptEnd::Cancel) => {
                shared.save_config(store);
                None
            }
            (Overlay::Options, _) => Some(("OPTIONS.MNI", "My_Options", Overlay::Options)),
            (Overlay::Speed, end) => {
                if let ScriptEnd::Chosen(speed) = end {
                    shared.config.options.speed = speed.clamp(1, 7);
                }
                Some(("OPTIONS.MNI", "My_Options", Overlay::Options))
            }
            (Overlay::SaveSlot, ScriptEnd::Chosen(slot)) if (1..=8).contains(&slot) => {
                self.start_save_name(ctx, shared, slot);
                return None;
            }
            (Overlay::Saving(slot), _) => {
                self.save(shared, store, slot);
                None
            }
            (Overlay::RestoreSlot, ScriptEnd::Chosen(slot)) if (1..=8).contains(&slot) => {
                if SavedGame::load(store, slot).is_some() {
                    Some(("TEXT.MNI", "&Load", Overlay::Restoring(slot)))
                } else {
                    Some(("OPTIONS.MNI", "No_Game_Restore", Overlay::RestoreMissing))
                }
            }
            (Overlay::RestoreMissing, _) => {
                Some(("OPTIONS.MNI", "Restore_Game", Overlay::RestoreSlot))
            }
            (Overlay::Restoring(slot), _) => {
                if let Some(saved) = SavedGame::load(store, slot) {
                    *self = Self::new(saved.session(), false);
                    return None;
                }
                None
            }
            (Overlay::Help | Overlay::SaveSlot | Overlay::RestoreSlot, _) => None,
        };
        match next
            .and_then(|(file, section, overlay)| Some((script(ctx.data, file, section)?, overlay)))
        {
            Some((runner, overlay)) => self.phase = Phase::Overlay(runner, overlay),
            None => self.resume(ctx),
        }
        None
    }

    /// The slot list stays and the name is typed in its line; an empty
    /// slot's "Empty" is wiped out first.
    fn start_save_name(&mut self, ctx: &mut Ctx, shared: &Shared, slot: u8) {
        let name = shared.slots.0[usize::from(slot - 1)].clone();
        let row = i32::from(slot - 1) * 2 + 6;
        let gfx = ctx.gfx();
        if name.is_empty() {
            for r in [row - 1, row] {
                for col in 14..32 {
                    gfx.small_char(ctx.screen, b' ', r, col);
                }
            }
        }
        gfx.colour_string(ctx.screen, 2, name.as_bytes(), row, 13);
        self.phase = Phase::SaveName {
            slot,
            name,
            blink: 0,
        };
    }

    /// Typing the name: letters, digits and spaces; Enter saves, Esc gives
    /// up.
    fn save_name(
        &mut self,
        ctx: &mut Ctx,
        shared: &mut Shared,
        store: &mut dyn Store,
        slot: u8,
        mut name: String,
        blink: u32,
    ) -> Option<PlayEnd> {
        let row = i32::from(slot - 1) * 2 + 6;
        let gfx = ctx.gfx();
        let blink = blink + ctx.ticks;
        for &key in ctx.pressed {
            match key {
                Key::Escape => {
                    self.resume(ctx);
                    return None;
                }
                Key::Enter => {
                    shared.slots.set(slot, &name);
                    match script(ctx.data, "TEXT.MNI", "&Save") {
                        Some(runner) => self.phase = Phase::Overlay(runner, Overlay::Saving(slot)),
                        None => {
                            self.save(shared, store, slot);
                            self.resume(ctx);
                        }
                    }
                    return None;
                }
                Key::Backspace => _ = name.pop(),
                key => {
                    if let Some(c) = typed(key, shared.shift)
                        && name.len() < SLOT_NAME_LEN
                    {
                        name.push(c);
                    }
                }
            }
            // The line is written again, wiping the cursor and the letter
            // that went.
            for col in 14..34 {
                gfx.small_char(ctx.screen, b' ', row - 1, col);
                gfx.small_char(ctx.screen, b' ', row, col);
            }
            gfx.colour_string(ctx.screen, 2, name.as_bytes(), row, 13);
        }
        text_cursor(ctx, row, 14 + name.len() as i32, blink);
        self.phase = Phase::SaveName { slot, name, blink };
        None
    }

    /// F2 saves the start of the level.
    fn save(&self, shared: &Shared, store: &mut dyn Store, slot: u8) {
        SavedGame::of(&self.session).save(store, slot);
        shared.slots.save(store);
    }
}

impl SavedGame {
    /// What a saved game keeps of a session: the start of its level.
    pub fn of(session: &Session) -> Self {
        Self {
            episode: session.episode,
            level: session.level,
            difficulty: session.difficulty,
            score: session.start.score,
            weapon: session.start.weapon,
            ammo: session.start.ammo,
            shown: session.start.shown,
        }
    }

    pub fn session(&self) -> Session {
        Session {
            episode: self.episode,
            level: self.level,
            difficulty: self.difficulty,
            start: Carry {
                score: self.score,
                weapon: self.weapon,
                ammo: self.ammo,
                shown: self.shown,
            },
        }
    }
}

/// The character a key types into a name, upper case with Shift.
pub(crate) fn typed(key: Key, shift: bool) -> Option<char> {
    match key {
        Key::Letter(c) if shift => Some(c),
        Key::Letter(c) => Some(c.to_ascii_lowercase()),
        Key::Digit(d) => Some(char::from(b'0' + d)),
        Key::Space => Some(' '),
        _ => None,
    }
}

/// The blinking text cursor: four pictures, the next one every 2 frames of
/// about 4 ticks.
pub(crate) fn text_cursor(ctx: &mut Ctx, row: i32, col: i32, ticks: u32) {
    let picture = (ticks / 8 % 4) as u16 * 8;
    ctx.gfx().solid(ctx.screen, row, col, TEXT_CURSOR + picture);
}

/// Wipes the text cursor out.
pub(crate) fn clear_cursor(ctx: &mut Ctx, row: i32, col: i32) {
    ctx.gfx().solid(ctx.screen, row, col, BOX_FILL);
}

/// Sets the fade for `elapsed` ticks of a fade out; false once it is over.
fn fade_out(ctx: &mut Ctx, elapsed: u32) -> bool {
    let left = FADE_OUT_TICKS.saturating_sub(elapsed);
    ctx.screen.fade = (left * u32::from(Screen::FADE_STEPS) / FADE_OUT_TICKS) as u8;
    elapsed < FADE_OUT_TICKS
}
