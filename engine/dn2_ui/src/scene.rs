//! The timed screens between the levels: the bonus screen, the loading screen,
//! the end of an episode.
//!
//! The original draws these in blocking functions full of `delay(n)`; here a
//! scene is a list of [`Step`]s that a [`Scene`] plays, a few timer ticks at a
//! time. The timings are the original's (ticks of the 140 Hz timer).
//!
//! * [`bonus_screen`]: the score and the bonus tally.
//! * [`loading_screen`]: the loading screen.
//! * [`episode_end`]: the end of an episode with its banner.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use dn2_game::Sound;
use dn2_game::state::Tallies;
use dn2_render::Screen;

use crate::Ctx;

/// Ticks of a whole fade out (16 steps of 2) and fade in (15 steps of 2).
pub const FADE_OUT_TICKS: u32 = 32;
pub const FADE_IN_TICKS: u32 = 30;

/// Sets the screen's fade for `elapsed` ticks of a fade in; true while the
/// fade runs.
pub(crate) fn fade_in(screen: &mut Screen, elapsed: u32) -> bool {
    let steps = u32::from(Screen::FADE_STEPS);
    screen.fade = (elapsed * steps / FADE_IN_TICKS).min(steps) as u8;
    elapsed < FADE_IN_TICKS
}

/// Sets the screen's fade for `elapsed` ticks of a fade out; true while the
/// fade runs.
pub(crate) fn fade_out(screen: &mut Screen, elapsed: u32) -> bool {
    let left = FADE_OUT_TICKS.saturating_sub(elapsed);
    screen.fade = (left * u32::from(Screen::FADE_STEPS) / FADE_OUT_TICKS) as u8;
    elapsed < FADE_OUT_TICKS
}

/// One thing a scene does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The screen fades to black, then is cleared.
    FadeOut,
    FadeIn,
    /// Draws a full-screen picture and takes its palette (the screen stays
    /// dark until [`Step::FadeIn`]).
    Image(String),
    /// Clears the screen.
    Clear,
    /// A string in the big font, `row`, `col`.
    Big {
        text: String,
        row: i32,
        col: i32,
    },
    /// A number in the big font ending at `right`.
    Number {
        value: u32,
        row: i32,
        right: i32,
    },
    /// Adds to the score and redraws it on the bonus screen.
    AddScore(u32),
    /// The number that counts down in the bonus screen's tally.
    TallyLeft {
        value: u32,
    },
    Sound(Sound),
    /// Starts a song (the file name) or stops the music.
    Music(Option<String>),
    /// The sprite of "THE END" at its final place.
    TheEnd,
    /// One column of the loading bar: 4 pixels at column `108 + x`.
    LoadBar(i32),
    Wait(u32),
    /// Waits for a key press, or until the ticks have passed.
    WaitKey(Option<u32>),
    /// Where a key press skips to: once a scene has passed one, a key runs
    /// the steps up to the next one (or the end) at once, without their
    /// waits and sounds.
    Skip,
}

impl Step {
    /// A string in the big font, `row`, `col`.
    pub fn big(text: &str, row: i32, col: i32) -> Self {
        Step::Big {
            text: String::from(text),
            row,
            col,
        }
    }

    /// Starts a song (an IMF file name).
    pub fn music(name: &str) -> Self {
        Step::Music(Some(String::from(name)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    None,
    Ticks(u32),
    FadeIn(u32),
    FadeOut(u32),
    Key(u32),
}

/// A scene being played.
pub struct Scene {
    steps: Vec<Step>,
    pc: usize,
    wait: Wait,
    score: u32,
    /// A [`Step::Skip`] has been passed: a key press skips.
    skippable: bool,
}

impl Scene {
    pub fn new(steps: Vec<Step>, score: u32) -> Self {
        Scene {
            steps,
            pc: 0,
            wait: Wait::None,
            score,
            skippable: false,
        }
    }

    /// The score with the bonuses added so far.
    pub fn score(&self) -> u32 {
        self.score
    }

    /// Goes on with the last step at once (the loading screen's fade-out).
    pub fn skip_to_last(&mut self) {
        if self.pc + 1 < self.steps.len() {
            self.pc = self.steps.len() - 1;
            self.wait = Wait::None;
        }
    }

    /// Runs the steps up to the next [`Step::Skip`] or the end at once,
    /// leaving out the waits and sounds.
    fn skip(&mut self, ctx: &mut Ctx) {
        self.wait = Wait::None;
        while let Some(step) = self.steps.get(self.pc) {
            let at = self.pc;
            self.pc += 1;
            match step {
                Step::Skip => break,
                Step::Wait(_) | Step::WaitKey(_) | Step::Sound(_) => {}
                _ => self.run_one_at(ctx, at, &mut false),
            }
        }
    }

    /// Plays `ctx.ticks` ticks. Returns whether the scene has ended.
    pub fn step(&mut self, ctx: &mut Ctx) -> bool {
        let mut budget = ctx.ticks;
        // A key press ends one wait only.
        let mut pressed = !ctx.pressed.is_empty();
        if pressed && self.skippable {
            pressed = false;
            self.skip(ctx);
        }
        loop {
            match self.wait {
                Wait::None => {
                    if self.pc >= self.steps.len() {
                        return true;
                    }
                    self.pc += 1;
                    self.run_one_at(ctx, self.pc - 1, &mut pressed);
                }
                Wait::Ticks(left) => {
                    if left > budget {
                        self.wait = Wait::Ticks(left - budget);
                        return false;
                    }
                    budget -= left;
                    self.wait = Wait::None;
                }
                Wait::FadeIn(elapsed) => {
                    let elapsed = elapsed + budget;
                    if fade_in(ctx.screen, elapsed) {
                        self.wait = Wait::FadeIn(elapsed);
                        return false;
                    }
                    budget = elapsed - FADE_IN_TICKS;
                    self.wait = Wait::None;
                }
                Wait::FadeOut(elapsed) => {
                    let elapsed = elapsed + budget;
                    if fade_out(ctx.screen, elapsed) {
                        self.wait = Wait::FadeOut(elapsed);
                        return false;
                    }
                    ctx.screen.clear(0);
                    budget = elapsed - FADE_OUT_TICKS;
                    self.wait = Wait::None;
                }
                Wait::Key(left) => {
                    if pressed {
                        pressed = false;
                        budget = 0;
                        self.wait = Wait::None;
                    } else if left != u32::MAX && left <= budget {
                        budget -= left;
                        self.wait = Wait::None;
                    } else {
                        if left != u32::MAX {
                            self.wait = Wait::Key(left - budget);
                        }
                        return false;
                    }
                }
            }
        }
    }

    fn run_one_at(&mut self, ctx: &mut Ctx, at: usize, pressed: &mut bool) {
        let step = self.steps[at].clone();
        let data = ctx.data;
        let gfx = ctx.gfx();
        match step {
            Step::FadeOut => self.wait = Wait::FadeOut(0),
            Step::FadeIn => {
                ctx.screen.fade = 0;
                self.wait = Wait::FadeIn(0);
            }
            Step::Image(name) => {
                if let Some(picture) = data.pictures.get(&name) {
                    let screen = &mut *ctx.screen;
                    screen.show_hd(&picture.image, data.hd.pictures.get(&name));
                    screen.palette = picture.palette.clone();
                }
            }
            Step::Clear => ctx.screen.clear(0),
            Step::Big { text, row, col } => gfx.big_string(ctx.screen, text.as_bytes(), row, col),
            Step::Number { value, row, right } => gfx.number_text(ctx.screen, value, row, right),
            Step::AddScore(points) => {
                self.score = self.score.wrapping_add(points);
                gfx.number_text(ctx.screen, self.score, 9, 0x22);
            }
            Step::TallyLeft { value } => gfx.number_text(ctx.screen, value, 0x12, 0x16),
            Step::Sound(sound) => ctx.sound(sound),
            Step::Music(name) => ctx.music(name.as_deref()),
            Step::TheEnd => {
                // Sprite 175 (30 by 7 tiles), 5 columns in and at pixel row 65,
                // where the original leaves it after rising.
                if let Some(f) = data.sprites.get(0xaf).and_then(|s| s.frames.first()) {
                    ctx.screen.draw(&f.image, 5 * 8, 65);
                }
            }
            Step::LoadBar(x) => {
                for (row, colour) in LOAD_BAR_COLOURS.into_iter().enumerate() {
                    for dx in 0..4 {
                        ctx.screen.put(108 + x + dx, 105 + row as i32, colour);
                    }
                }
            }
            Step::Wait(ticks) => self.wait = Wait::Ticks(ticks),
            Step::WaitKey(ticks) => {
                // A press from before the wait does not count.
                *pressed = false;
                self.wait = Wait::Key(ticks.unwrap_or(u32::MAX));
            }
            Step::Skip => self.skippable = true,
        }
    }
}

/// The text of the bonus tally.
const TALLY_STRINGS: [&str; 33] = [
    "S  ",
    "ONUS  ",
    " BONUS  ",
    "ET BONUS  ",
    "CRET BONUS  ",
    "SECRET BONUS  ",
    "            ,,",
    "          ,,,,",
    "        ,,,,,,",
    "      ,,,,,,,,",
    "    ,,,,,,,,,,",
    "  ,,,,,,,,,,,,",
    ",,,,,,,,,,,,,,",
    "  ,,,,,,,,,,,,",
    "   N,,,,,,,,,,",
    "   NO ,,,,,,,,",
    "   NO BO,,,,,,",
    "   NO BONU,,,,",
    "   NO BONUS!,,",
    "   NO BONUS!  ",
    " NO BONUS!  BE",
    "O BONUS! BETTE",
    "BONUS! BETTER ",
    "NUS! BETTER LU",
    "S! BETTER LUCK",
    " BETTER LUCK! ",
    "ETTER LUCK!  N",
    "TER LUCK!  NEX",
    "R LUCK!  NEXT ",
    "LUCK!  NEXT TI",
    "CK!  NEXT TIME",
    "!  NEXT TIME! ",
    "  NEXT TIME!  ",
];
const TALLY_ROW: i32 = 0x12;
const TALLY_COL: i32 = 6;

fn tally(steps: &mut Vec<Step>, range: core::ops::Range<usize>, delay: u32) {
    for i in range {
        steps.push(Step::big(TALLY_STRINGS[i], TALLY_ROW, TALLY_COL));
        steps.push(Step::Wait(delay));
    }
}

/// The bonus tally: `n` 0 is "NO BONUS! BETTER LUCK NEXT TIME!", 1 to 7 are the
/// secret bonuses worth 100000 points. The original's sound 1 is the big
/// explosion, sound 0xd the jump.
fn bonus_tally(steps: &mut Vec<Step>, n: u8) {
    // A key skips to the next tally.
    steps.push(Step::Skip);
    steps.push(Step::Wait(100));
    if n == 0 {
        tally(steps, 6..0x14, 5);
        steps.push(Step::Sound(Sound::BigExplosion));
        steps.push(Step::Wait(0x82));
        tally(steps, 0x14..0x1a, 10);
        steps.push(Step::Sound(Sound::BigExplosion));
        steps.push(Step::Wait(0x82));
        tally(steps, 0x1a..0x21, 10);
        steps.push(Step::Wait(0xf));
        steps.push(Step::Sound(Sound::BigExplosion));
    } else {
        tally(steps, 0..6, 5);
        steps.push(Step::Number {
            value: u32::from(n),
            row: TALLY_ROW,
            right: 0x22,
        });
        steps.push(Step::Sound(Sound::BigExplosion));
        steps.push(Step::Wait(0xbe));
        steps.push(Step::big("  100000 PTS  ", TALLY_ROW, TALLY_COL));
        steps.push(Step::Wait(100));
        for i in 0..100u32 {
            steps.push(Step::AddScore(1000));
            steps.push(Step::Wait(2));
            steps.push(Step::TallyLeft {
                value: 100_000 - 1000 * (i + 1),
            });
            steps.push(Step::Sound(Sound::Jump));
        }
        steps.push(Step::big("       0 PTS  ", TALLY_ROW, TALLY_COL));
        // A key during the countdown completes it.
        steps.push(Step::Skip);
        steps.push(Step::Sound(Sound::BigExplosion));
        steps.push(Step::Wait(0x32));
    }
}

/// The bonus numbers (1-7) earned in a level, in order: all cameras destroyed,
/// never hurt, all weapons, all bonus items, all laser turrets destroyed, no
/// fuse boxes left, no bonus globes left. "All" of none does not count.
pub fn secret_bonuses(tally: &Tallies, was_hurt: bool) -> Vec<u8> {
    let all = |found: u16, total: u16| found == total && found != 0;
    let conditions = [
        all(tally.cameras_destroyed, tally.cameras),
        !was_hurt,
        all(tally.weapons_collected, tally.weapons),
        all(tally.bonus_items_collected, tally.bonus_items),
        all(tally.laser_turrets_destroyed, tally.laser_turrets),
        tally.fuse_boxes_left == 0,
        tally.bonus_globes_left == 0,
    ];
    (1..=7u8)
        .zip(conditions)
        .filter_map(|(n, c)| c.then_some(n))
        .collect()
}

/// The bonus screen: the score, then the secret bonuses earned in the level or
/// a "better luck next time". `last_level` is the eighth level of an episode,
/// which has no bonus music.
pub fn bonus_screen(score: u32, bonuses: &[u8], last_level: bool) -> Vec<Step> {
    let mut steps = alloc::vec![Step::FadeOut, Step::Image(String::from("BONUSSCN.MNI"))];
    if !last_level {
        steps.push(Step::music("OPNGATEA.IMF"));
    }
    steps.push(Step::big("SCORE", 9, 6));
    steps.push(Step::Number {
        value: score,
        row: 9,
        right: 0x22,
    });
    steps.push(Step::FadeIn);
    // From here on a key skips (see `bonus_tally`); after the last tally it
    // ends the screen.
    steps.push(Step::Skip);
    steps.push(Step::Wait(0x3c));
    for &n in bonuses {
        bonus_tally(&mut steps, n);
    }
    if bonuses.is_empty() {
        bonus_tally(&mut steps, 0);
    }
    steps.push(Step::Skip);
    steps.push(Step::Wait(0x1a9));
    if !last_level {
        steps.push(Step::Music(None));
    }
    steps
}

/// The colours of the 4 rows of the loading bar.
const LOAD_BAR_COLOURS: [u8; 4] = [0x0c, 0x0b, 0x0a, 0x0b];
/// The loading bar's counter ends at this value; the timer interrupt adds 1
/// every tick once the loading is done (the step delay is 0 then).
const LOAD_BAR_END: u32 = 0x11c;

/// The loading screen with its music and the loading bar: the picture of the
/// episode (from 1) fades in, the bar fills (a column for every 4 counts, one
/// count a tick, as it does once the original has loaded everything), the
/// picture fades out.
pub fn loading_screen(episode: u8) -> Vec<Step> {
    let episode = if (1..=4).contains(&episode) {
        episode
    } else {
        1
    };
    let mut steps = alloc::vec![
        Step::FadeOut,
        Step::music("MENUSNG2.IMF"),
        Step::Image(format!("LOAD{episode}.MNI")),
        Step::FadeIn,
    ];
    // The counter starts at 1; column n / 4 is drawn when it reaches n.
    for count_at in 2..=LOAD_BAR_END {
        steps.push(Step::Wait(1));
        if count_at == 2 || count_at % 4 == 0 {
            steps.push(Step::LoadBar((count_at >> 2) as i32));
        }
    }
    steps.push(Step::FadeOut);
    steps
}

/// The end of an episode: NEVRENDA.IMF, the pictures of the episode (from 1),
/// "THE END" for the last one, then the bonus screen.
pub fn episode_end(episode: u8, score: u32, bonuses: &[u8]) -> Vec<Step> {
    let mut steps = alloc::vec![Step::music("NEVRENDA.IMF"), Step::FadeOut, Step::Wait(0x8c),];
    let pictures: &[&str] = match episode {
        1 => &["END1-3.MNI", "END1-1.MNI", "END1-2.MNI"],
        2 => &["END2-1.MNI"],
        3 => &["END3-1.MNI"],
        4 => &["END4-1.MNI", "END4-3.MNI"],
        _ => &[],
    };
    for (i, name) in pictures.iter().enumerate() {
        steps.push(Step::Image(String::from(*name)));
        steps.push(Step::FadeIn);
        steps.push(Step::WaitKey(None));
        if i + 1 < pictures.len() {
            steps.push(Step::FadeOut);
        }
    }
    // show_the_end_banner: the last episode only.
    if episode > 3 {
        steps.extend([
            Step::FadeOut,
            Step::Clear,
            Step::TheEnd,
            Step::FadeIn,
            Step::WaitKey(Some(0x5dc)),
        ]);
    }
    steps.push(Step::Clear);
    steps.extend(bonus_screen(score, bonuses, true));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_collected_is_no_bonus() {
        let tally = Tallies {
            fuse_boxes_left: 1,
            bonus_globes_left: 2,
            ..Tallies::default()
        };
        assert!(secret_bonuses(&tally, true).is_empty());
        // Never hurt is bonus 2; nothing left of fuse boxes and globes is 6 and 7.
        assert_eq!(secret_bonuses(&Tallies::default(), false), [2, 6, 7]);
    }

    #[test]
    fn all_items_found_is_a_bonus() {
        let mut tally = Tallies {
            fuse_boxes_left: 1,
            bonus_globes_left: 1,
            bonus_items: 5,
            bonus_items_collected: 5,
            ..Tallies::default()
        };
        assert_eq!(secret_bonuses(&tally, true), [4]);
        // Zero of zero is not "all".
        tally.bonus_items = 0;
        tally.bonus_items_collected = 0;
        assert!(secret_bonuses(&tally, true).is_empty());
    }

    #[test]
    fn secret_bonus_adds_100000_points() {
        let added: u32 = bonus_screen(1234, &[2], false)
            .iter()
            .map(|s| if let Step::AddScore(n) = s { *n } else { 0 })
            .sum();
        assert_eq!(added, 100_000);
    }

    #[test]
    fn step_lists() {
        let steps = loading_screen(2);
        assert_eq!(steps[2], Step::Image(String::from("LOAD2.MNI")));
        assert_eq!(steps.last(), Some(&Step::FadeOut));
        assert!(episode_end(4, 0, &[]).contains(&Step::TheEnd));
        assert!(!episode_end(1, 0, &[]).contains(&Step::TheEnd));
    }
}
