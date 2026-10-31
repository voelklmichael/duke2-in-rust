//! The FLI movies of the start of the game: the intro (NUKEM2.F5) and the title
//! animations of the attract mode (F2, F1, F3, F4), played the way the original
//! does, with its frame timing and sound cues.

use alloc::vec::Vec;

use dn2_assets::GameData;
use dn2_assets::flic::{Flic, FlicPlayer};
use dn2_game::Sound;

use crate::Ctx;

/// Which movie and how it is timed: the intro, or title n.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovieKind {
    /// NUKEM2.F5, animation 8, until it ends (255 loops).
    Intro,
    /// Title 1 to 4 in the order of the attract mode: F2 (animation 0, 6
    /// loops), F1 (4, 10 loops), F3 (5, 2 loops), F4 (6, 1 loop).
    Title(u8),
}

impl MovieKind {
    /// The file, the animation number and the loops.
    fn params(self) -> Option<(&'static str, u8, u32)> {
        Some(match self {
            MovieKind::Intro => ("NUKEM2.F5", 8, 255),
            MovieKind::Title(1) => ("NUKEM2.F2", 0, 6),
            MovieKind::Title(2) => ("NUKEM2.F1", 4, 10),
            MovieKind::Title(3) => ("NUKEM2.F3", 5, 2),
            MovieKind::Title(4) => ("NUKEM2.F4", 6, 1),
            MovieKind::Title(_) => return None,
        })
    }
}

/// What the original does for a frame: the speed the frame timer follows from
/// now on (`None` keeps it; negative means a frame lasts `-speed` seconds) and
/// a sound to start. Animations 0, 4, 5, 6 and 8 are the ones the game plays.
fn frame_cue(anim: u8, frame: usize) -> (Option<i32>, Option<Sound>) {
    match anim {
        0 => (Some(4), None),
        2 | 4 if frame == 0 => (Some(20), Some(Sound::Movie2a)),
        3 | 5 if matches!(frame, 0 | 3 | 6) => (Some(12), Some(Sound::Movie2b)),
        6 => match frame {
            0 => (Some(6), Some(Sound::Movie2c)),
            7 => (Some(6), Some(Sound::Movie2d)),
            17 | 24 => (Some(6), None),
            23 => (Some(-2), None),
            31 => (Some(-2), Some(Sound::Movie2e)),
            32 | 39 => (Some(-1), None),
            33 => (Some(5), Some(Sound::Movie2f)),
            37 => (None, Some(Sound::Movie30)),
            40 | 50 => (Some(0x11), None),
            49 => (Some(-1), Some(Sound::BigExplosion)),
            55 => (Some(-4), Some(Sound::BigExplosion)),
            _ => (None, None),
        },
        8 => (Some(8), None),
        _ => (None, None),
    }
}

/// How long a frame lasts in timer units of 280 Hz: 280 / speed, or
/// -speed * 280 for a negative speed.
fn frame_timer(speed: i32) -> u32 {
    if speed < 0 {
        speed.unsigned_abs() * 280
    } else {
        280 / speed.max(1) as u32
    }
}

/// A movie being played: the first frame, then `loops` times the frames after
/// it. A loop plays the records from the second up to the ring frame (which is
/// the next loop's first); the last loop stops one frame earlier. A frame lasts
/// exactly its timer and the time left over is not kept (the original zeroes
/// its counter); the time it spends decoding is not modelled.
pub struct Movie {
    file: &'static str,
    player: FlicPlayer,
    anim: u8,
    speed: i32,
    /// Timer units (280 Hz) left before the next frame.
    timer: u32,
    loops_left: u32,
    /// Frames read in the current loop, if one is running.
    in_loop: Option<usize>,
    sounds: Vec<Sound>,
    /// The picture changed since it was drawn.
    dirty: bool,
}

impl Movie {
    /// Starts a movie: decodes the first frame. `None` without the file.
    pub fn new(data: &GameData, kind: MovieKind) -> Option<Self> {
        let (file, anim, loops) = kind.params()?;
        let flic = data.movies.get(file)?;
        let mut player = FlicPlayer::new(flic);
        player.apply(flic.frames.first()?);
        let mut movie = Movie {
            file,
            player,
            anim,
            speed: 6,
            timer: 0,
            loops_left: loops,
            in_loop: None,
            sounds: Vec::new(),
            dirty: true,
        };
        movie.cue(0);
        Some(movie)
    }

    /// Advances by `ctx.ticks` (of 140 Hz), drawing the current frame and
    /// starting the sounds; true when the movie is over.
    pub fn step(&mut self, ctx: &mut Ctx) -> bool {
        // The timer counts 280 Hz, two units to a tick.
        let mut budget = ctx.ticks * 2;
        let flic = ctx.data.movies.get(self.file);
        let over = loop {
            if self.timer > budget {
                self.timer -= budget;
                break false;
            }
            self.timer = 0;
            if flic.is_some_and(|f| self.advance(f)) {
                // The next frame's wait starts from zero.
                budget = 0;
            } else {
                break true;
            }
        };
        for sound in self.sounds.drain(..) {
            ctx.sound(sound);
        }
        if self.dirty {
            self.dirty = false;
            let screen = &mut *ctx.screen;
            screen.show(&self.player.image);
            screen.palette = self.player.palette.clone();
        }
        over
    }

    /// Applies the cue for a frame of the cycle.
    fn cue(&mut self, frame: usize) {
        let (speed, sound) = frame_cue(self.anim, frame);
        if let Some(speed) = speed {
            self.speed = speed;
        }
        self.sounds.extend(sound);
        self.timer = frame_timer(self.speed);
    }

    /// Shows the next frame; false when the movie is over.
    fn advance(&mut self, flic: &Flic) -> bool {
        // The header's frame count; the file has one record more.
        let frames = flic.frames.len().saturating_sub(1);
        loop {
            if let Some(read) = self.in_loop {
                let bound = if self.loops_left == 0 {
                    frames.saturating_sub(1)
                } else {
                    frames
                };
                if read < bound {
                    let Some(record) = flic.frames.get(Flic::LOOP_START + read) else {
                        return false;
                    };
                    self.player.apply(record);
                    self.dirty = true;
                    self.in_loop = Some(read + 1);
                    self.cue(if read + 1 == frames { 0 } else { read + 1 });
                    return true;
                }
                self.in_loop = None;
            }
            if self.loops_left == 0 {
                return false;
            }
            self.loops_left -= 1;
            self.in_loop = Some(0);
        }
    }
}
