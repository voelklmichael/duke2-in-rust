//! The menu and text scripts: TEXT.MNI, OPTIONS.MNI and HELP.MNI hold sections
//! of text with `//COMMAND` lines that draw menus, pictures and message boxes.
//!
//! The original interprets a section in one blocking call, waiting for keys and
//! delaying inside it. Here a [`Runner`] is advanced a few timer ticks at a
//! time by [`Runner::step`], which returns how the section ended.
//!
//! A menu has a "page" for each entry between `//PAGESSTART` and `//PAGESEND`
//! (separated by `//APAGE`): each page redraws the items around the cursor with
//! the selected one lit and ends in `//WAIT`. Up and down move to the previous
//! and next page (wrapping), Enter or Space choose the page. A section with
//! only a `//WAIT` continues on any key. The first `//WAIT` of a menu fades the
//! screen in.

use alloc::string::String;
use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_assets::script::{Command, Section};
use dn2_game::Sound;
use dn2_render::Screen;

use crate::config::{Bindings, Options};
use crate::draw::{BLANK_2X2, CHECK_OFF, CHECK_ON, CURSOR, Gfx};
use crate::{Ctx, Key};

/// What the script needs of the program around it.
pub struct ScriptEnv<'a> {
    pub options: &'a mut Options,
    pub bindings: &'a Bindings,
    /// The 8 saved game names ("" for an empty slot), for `//GETNAMES`.
    pub slot_names: &'a [String],
    /// Episode and level, from 1, for `//HELPTEXT`.
    pub episode: u8,
    pub level: u8,
    /// The last selection of each menu number, from 1.
    pub menus: &'a mut [u8; 16],
}

/// How a section ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptEnd {
    /// The section ran to its end.
    Done,
    /// A menu entry (page number from 1) or a `//SETKEYS` key (its position
    /// from 1).
    Chosen(u8),
    /// Esc in a menu.
    Cancel,
    /// `//EXITTODEMO` and no key for the idle time.
    ExitToDemo,
}

/// Idle time after which `//EXITTODEMO` ends the section: the original counts
/// 600 polls of the menu keys (the first is the start), each of which waits 7
/// ticks while animating the cursor.
const EXIT_TO_DEMO_TICKS: u32 = 599 * 7;
/// Ticks a fade takes per brightness step.
const FADE_STEP_TICKS: u32 = 2;
/// Ticks the spinning menu cursor shows each picture.
const CURSOR_TICKS: u32 = 7;
/// The column of the menu cursor.
const CURSOR_COL: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    None,
    /// Delay ticks left.
    Ticks(u32),
    /// Ticks elapsed in a fade.
    FadeIn(u32),
    FadeOut(u32),
    /// Until the next step.
    Yield,
    Key,
}

/// An executing section.
pub struct Runner {
    commands: Vec<Command>,
    pc: usize,
    wait: Wait,
    /// The menu number, once `//MENU` ran.
    menu: Option<u8>,
    /// The first wait of a menu has faded the screen in.
    faded_in: bool,
    /// The check boxes of `//TOGGS`: the column and each option's row and
    /// letter.
    toggles: Option<(u8, Vec<(u8, u8)>)>,
    /// The first command of each page.
    pages: Vec<usize>,
    /// The selected page from 1; 0 outside of menus.
    selected: u8,
    hot_keys: Vec<u8>,
    /// Row of the spinning cursor (from 1, 0 for none), its picture and the
    /// ticks to the next one.
    cursor_row: i32,
    cursor_phase: u16,
    cursor_ticks: u32,
    /// The boxes still to draw while a `//CENTERWINDOW` opens, last first:
    /// width, height, row, column.
    box_frames: Option<Vec<[i32; 4]>>,
    /// The row where text goes after `//CENTERWINDOW`.
    text_row: i32,
    /// Columns that boxes and centred text are moved left by (`//SHIFTWIN`).
    shift: i32,
    hint: Option<Vec<u8>>,
    done: Option<ScriptEnd>,
    /// Ticks without a key, with `//EXITTODEMO`.
    idle: Option<u32>,
}

/// The boxes an animated text box draws before the final one, last first: a
/// flat box grows sideways from the middle, then up and down.
fn box_opening(width: i32, height: i32, row: i32) -> Vec<[i32; 4]> {
    let left = 20 - (width >> 1);
    let mut middle = row + (height >> 1);
    let mut frames = Vec::new();
    let mut w = 1;
    let mut col = 0x13;
    while left < col {
        w += 2;
        frames.push([w, 2, middle, col]);
        col -= 1;
    }
    let mut h = 0;
    while row + i32::from(height & 1 == 0) < middle {
        h += 2;
        frames.push([width, h, middle, left]);
        middle -= 1;
    }
    frames.reverse();
    frames
}

fn gfx(data: &GameData) -> Gfx<'_> {
    Gfx {
        status: &data.status,
        sprites: &data.sprites,
        font: &data.font,
        hd: &data.hd,
    }
}

/// The key of a scan code: the `//SETKEYS` bytes of the scripts are scan codes
/// (S R G O I H C Q is 1F 13 22 18 17 23 2E 10).
fn key_of_scan_code(code: u8) -> Option<Key> {
    const ROWS: [(u8, &str); 3] = [(0x10, "QWERTYUIOP"), (0x1e, "ASDFGHJKL"), (0x2c, "ZXCVBNM")];
    for (first, letters) in ROWS {
        if let Some(i) = code.checked_sub(first)
            && let Some(&c) = letters.as_bytes().get(usize::from(i))
        {
            return Some(Key::Letter(char::from(c)));
        }
    }
    Some(match code {
        0x01 => Key::Escape,
        0x02..=0x0a => Key::Digit(code - 1),
        0x0b => Key::Digit(0),
        0x0e => Key::Backspace,
        0x0f => Key::Tab,
        0x1c => Key::Enter,
        0x39 => Key::Space,
        _ => return None,
    })
}

impl Runner {
    /// Starts a section.
    pub fn new(section: &Section) -> Self {
        let mut pages = Vec::new();
        for (i, command) in section.commands.iter().enumerate() {
            if matches!(command, Command::PagesStart | Command::APage) {
                pages.push(i + 1);
            }
        }
        Runner {
            commands: section.commands.clone(),
            pc: 0,
            wait: Wait::None,
            menu: None,
            faded_in: false,
            toggles: None,
            pages,
            selected: 0,
            hot_keys: Vec::new(),
            cursor_row: 0,
            cursor_phase: 0,
            cursor_ticks: 0,
            box_frames: None,
            text_row: 0,
            shift: 0,
            hint: None,
            done: None,
            idle: None,
        }
    }

    /// The hint text `//HELPTEXT` chose for the episode and level, upper
    /// case.
    pub fn hint(&self) -> Option<&[u8]> {
        self.hint.as_deref()
    }

    fn finish(&mut self, env: &mut ScriptEnv, end: ScriptEnd) -> Option<ScriptEnd> {
        if matches!(end, ScriptEnd::Chosen(_) | ScriptEnd::Cancel)
            && let Some(slot) = self.menu.and_then(|n| env.menus.get_mut(usize::from(n)))
        {
            *slot = self.selected;
        }
        self.done = Some(end);
        self.done
    }

    /// Advances by `ctx.ticks`; `Some(end)` once finished (and on every
    /// later call).
    pub fn step(&mut self, ctx: &mut Ctx, env: &mut ScriptEnv) -> Option<ScriptEnd> {
        if self.done.is_some() {
            return self.done;
        }
        let mut ticks = ctx.ticks;
        let full = u32::from(Screen::FADE_STEPS);
        // The presses not acted on yet; the ones that come while a fade or
        // a delay runs are lost, as keys are in the original.
        let mut next_press = 0;
        // Time left over when a wait ends is not carried on to the next one.
        loop {
            match self.wait {
                Wait::None => {
                    if let Some(end) = self.run(ctx, env) {
                        return self.finish(env, end);
                    }
                    if self.wait == Wait::Yield {
                        self.wait = Wait::None;
                        return None;
                    }
                }
                Wait::Yield => self.wait = Wait::None,
                Wait::Ticks(left) => {
                    self.wait = if left > ticks {
                        Wait::Ticks(left - ticks)
                    } else {
                        Wait::None
                    };
                    return None;
                }
                Wait::FadeIn(elapsed) => {
                    let elapsed = elapsed + ticks;
                    let level = (elapsed / FADE_STEP_TICKS).min(full);
                    ctx.screen.fade = level as u8;
                    self.wait = if level < full {
                        Wait::FadeIn(elapsed)
                    } else {
                        Wait::None
                    };
                    return None;
                }
                Wait::FadeOut(elapsed) => {
                    let elapsed = elapsed + ticks;
                    let level = full.saturating_sub(elapsed / FADE_STEP_TICKS);
                    ctx.screen.fade = level as u8;
                    if level > 0 {
                        self.wait = Wait::FadeOut(elapsed);
                    } else {
                        // The screen is cleared and the palette reset.
                        ctx.screen.clear(0);
                        ctx.screen.palette = ctx.data.palette.clone();
                        self.wait = Wait::None;
                    }
                    return None;
                }
                Wait::Key => {
                    self.animate_cursor(ctx, ticks);
                    if let Some(idle) = &mut self.idle {
                        *idle += ticks;
                        if *idle >= EXIT_TO_DEMO_TICKS {
                            return self.finish(env, ScriptEnd::ExitToDemo);
                        }
                    }
                    ticks = 0;
                    let mut go = false;
                    while let Some(&key) = ctx.pressed.get(next_press) {
                        next_press += 1;
                        if let Some(idle) = &mut self.idle {
                            *idle = 0;
                        }
                        match self.press(ctx, env, key) {
                            Press::Nothing => {}
                            Press::End(end) => return self.finish(env, end),
                            Press::Go => {
                                go = true;
                                break;
                            }
                        }
                    }
                    if !go {
                        return None;
                    }
                    self.wait = Wait::None;
                }
            }
        }
    }

    /// Executes commands until one that waits (returns `None`) or the end
    /// of the section.
    fn run(&mut self, ctx: &mut Ctx, env: &mut ScriptEnv) -> Option<ScriptEnd> {
        let data = ctx.data;
        let g = gfx(data);
        while let Some(command) = self.commands.get(self.pc).cloned() {
            self.pc += 1;
            match command {
                Command::FadeIn => {
                    ctx.screen.fade = 0;
                    self.wait = Wait::FadeIn(0);
                    return None;
                }
                Command::FadeOut => {
                    self.wait = Wait::FadeOut(0);
                    return None;
                }
                Command::LoadRaw { file } => {
                    if let Some(picture) = data.pictures.get(&file.to_ascii_uppercase()) {
                        let name = file.to_ascii_uppercase();
                        ctx.screen
                            .show_hd(&picture.image, ctx.data.hd.pictures.get(&name));
                        ctx.screen.palette = picture.palette.clone();
                    }
                }
                Command::GetPal { file } => {
                    if let Some(palette) = data.palettes.get(&file.to_ascii_uppercase()) {
                        ctx.screen.palette = palette.clone();
                    }
                    ctx.screen.clear(0);
                }
                Command::SetKeys { keys } => self.hot_keys = keys,
                Command::XyText { col, row, text } => {
                    g.text(
                        ctx.screen,
                        &text,
                        i32::from(row),
                        i32::from(col) - self.shift,
                    );
                }
                Command::Menu { number } => {
                    self.menu = Some(number);
                    let pages = self.pages.len().max(1) as u8;
                    let last = env.menus.get(usize::from(number)).copied().unwrap_or(1);
                    self.selected = last.clamp(1, pages);
                    // Start at the remembered page, skipping the others.
                    if self.selected > 1
                        && let Some(&page) = self.pages.get(usize::from(self.selected) - 1)
                    {
                        self.pc = page;
                    }
                }
                Command::Toggs { col, entries } => {
                    self.toggles = Some((col, entries));
                    self.draw_toggles(ctx, env);
                }
                Command::GetNames { slot } => {
                    g.slot_list(ctx.screen, env.slot_names, usize::from(slot));
                }
                Command::Keys => {
                    // The keys in the order of the screen: rows 7, 9, ... at
                    // column 0x1a.
                    for index in 1..=Bindings::COUNT {
                        let name = env.bindings.get(index).map_or("", Key::name);
                        let row = 7 + 2 * i32::from(index - 1);
                        g.text(ctx.screen, name.as_bytes(), row, 0x1a);
                    }
                }
                Command::Z { row } => self.cursor_row = i32::from(row),
                Command::PagesStart => self.selected = 1,
                Command::ExitToDemo => self.idle = Some(0),
                Command::Wait => {
                    // The first wait of a menu fades the screen in.
                    if self.menu.is_some() && !self.faded_in {
                        self.faded_in = true;
                        self.pc -= 1;
                        ctx.screen.fade = 0;
                        self.wait = Wait::FadeIn(0);
                    } else {
                        self.wait = Wait::Key;
                        // Each wait for a key starts the count again.
                        if let Some(idle) = &mut self.idle {
                            *idle = 0;
                        }
                    }
                    return None;
                }
                Command::Delay { ticks } => {
                    self.wait = Wait::Ticks(ticks);
                    return None;
                }
                Command::ShiftWin => self.shift = 3,
                Command::CenterWindow { row, height, width } => {
                    let (row, height, width) =
                        (i32::from(row), i32::from(height), i32::from(width));
                    let frames = self
                        .box_frames
                        .get_or_insert_with(|| box_opening(width, height, row));
                    if !frames.is_empty() {
                        // A step lasts several ticks: the boxes of all of
                        // them are drawn, one over the other, before the
                        // step is shown.
                        for _ in 0..ctx.ticks.max(1) {
                            if let Some([w, h, r, c]) = frames.pop() {
                                g.text_box(ctx.screen, w, h, r, c - self.shift);
                            }
                        }
                        self.pc -= 1;
                        self.wait = Wait::Yield;
                        return None;
                    }
                    self.box_frames = None;
                    let col = 20 - (width >> 1) - self.shift;
                    g.text_box(ctx.screen, width, height, row, col);
                    self.text_row = row + 1;
                }
                Command::SkLine => self.text_row += 1,
                Command::CwText { text } => {
                    let col = 20 - (text.len() as i32 >> 1) - self.shift;
                    g.text(ctx.screen, &text, self.text_row, col);
                    self.text_row += 1;
                }
                Command::HelpText {
                    episode,
                    level,
                    text,
                } if episode == env.episode && level == env.level => {
                    self.hint = Some(text.to_ascii_uppercase());
                }
                _ => {}
            }
        }
        Some(ScriptEnd::Done)
    }

    /// A box that is lit when the option is on, 2 by 2 tiles left of the entry.
    fn draw_toggles(&self, ctx: &mut Ctx, env: &ScriptEnv) {
        let Some((col, entries)) = &self.toggles else {
            return;
        };
        let g = gfx(ctx.data);
        for &(row, letter) in entries {
            let offset = if env.options.get(letter) {
                CHECK_ON
            } else {
                CHECK_OFF
            };
            g.solid_2x2(ctx.screen, i32::from(row) - 1, i32::from(*col) - 1, offset);
        }
    }

    /// Spins the menu cursor.
    fn animate_cursor(&mut self, ctx: &mut Ctx, ticks: u32) {
        if self.cursor_row == 0 {
            return;
        }
        self.cursor_ticks += ticks;
        if self.cursor_ticks >= CURSOR_TICKS {
            self.cursor_ticks = 0;
            self.cursor_phase = (self.cursor_phase + 0x10) % 0x80;
        }
        gfx(ctx.data).solid_2x2(
            ctx.screen,
            self.cursor_row - 1,
            CURSOR_COL,
            CURSOR + self.cursor_phase,
        );
    }

    /// A key pressed in a `//WAIT`.
    fn press(&mut self, ctx: &mut Ctx, env: &mut ScriptEnv, key: Key) -> Press {
        // Erase the cursor.
        if self.cursor_row != 0 {
            gfx(ctx.data).solid_2x2(ctx.screen, self.cursor_row - 1, CURSOR_COL, BLANK_2X2);
        }
        if let Some(i) = self
            .hot_keys
            .iter()
            .position(|&b| key_of_scan_code(b) == Some(key))
        {
            return Press::End(ScriptEnd::Chosen(i as u8 + 1));
        }
        if self.selected == 0 {
            // Any other key continues the script.
            return Press::Go;
        }
        let pages = self.pages.len().max(1) as u8;
        match key {
            Key::Escape => return Press::End(ScriptEnd::Cancel),
            Key::Enter | Key::Space => {
                // With check boxes, Enter switches the option (when the
                // page is one of them) instead of choosing it.
                let letter = self
                    .toggles
                    .as_ref()
                    .and_then(|(_, entries)| entries.get(usize::from(self.selected) - 1))
                    .map(|e| e.1);
                let Some(letter) = letter else {
                    return Press::End(ScriptEnd::Chosen(self.selected));
                };
                env.options.toggle(letter);
                self.draw_toggles(ctx, env);
                ctx.sound(Sound::MenuToggle);
                return Press::Nothing;
            }
            Key::Up | Key::PageUp | Key::Left => {
                ctx.sound(Sound::MenuMove);
                self.selected = if self.selected <= 1 {
                    pages
                } else {
                    self.selected - 1
                };
            }
            Key::Down | Key::PageDown | Key::Right => {
                ctx.sound(Sound::MenuMove);
                self.selected = if self.selected >= pages {
                    1
                } else {
                    self.selected + 1
                };
            }
            _ => return Press::Nothing,
        }
        match self.pages.get(usize::from(self.selected) - 1) {
            Some(&page) => {
                self.pc = page;
                Press::Go
            }
            None => Press::Nothing,
        }
    }
}

enum Press {
    Nothing,
    End(ScriptEnd),
    Go,
}
