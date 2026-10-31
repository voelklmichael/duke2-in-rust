//! Sound effects and music, mixed to mono samples at any output rate.
//!
//! This is the original's sound system with a Sound Blaster and its AdLib chip,
//! the best it supports:
//! - Digitized effects (SB_n.MNI, INTRO3-9.MNI) play through the DAC.
//! - AdLib effects from AUDIOT.MNI play on channel 0 of the chip
//!   (serviced at 140 Hz).
//! - Music is IMF data on the same chip (serviced at 280 Hz).
//!
//! The original chooses between them with a priority per effect, so a quiet
//! effect does not cut off a loud one. The PC speaker versions are not used.
//! The chip is [`oplon`].
//!
//! The 280 Hz timer is counted in output samples, so music keeps its speed
//! whatever the game's frame rate.

#![no_std]

extern crate alloc;

mod imf;

use dn2_assets::sound::{AdlibEffect, ImfCommand, Sounds};
use dn2_game::Sound;
use oplon::Opl2;

use imf::Song;

/// The timer interrupt's rate.
const TIMER_HZ: u32 = 280;

/// Priority of effects 0-21h, compared when one starts.
const PRIORITY: [u8; 34] = [
    1, 4, 3, 5, 2, 0, 2, 2, 1, 2, 3, 1, 2, 0, 2, 0, 1, 0, 3, 1, 3, 0, 1, 0, 2, 0, 1, 2, 1, 3, 4, 4,
    4, 4,
];

/// Digitized effects from this id on (the movies') start at once and leave the
/// priority alone: the original returns after starting the sample for ids above
/// 28h.
const UNCHECKED_FROM: u8 = 0x29;

/// Loudness of the chip and of the digitized effects, to full scale 1.0.
const CHIP_GAIN: f32 = 1.0 / 32768.0;
const SAMPLE_GAIN: f32 = 0.7 / 128.0;

/// Which kinds of effects play: the options menu's S and L.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectOptions {
    pub digitized: bool,
    pub adlib: bool,
}

/// An AdLib effect that plays.
struct AdlibPlaying {
    id: u8,
    position: usize,
    /// Register B0h with key on: block << 2 | 20h.
    key_on: u8,
}

/// A digitized effect that plays.
struct SamplePlaying {
    id: u8,
    /// Position in samples, 16.16 fixed point.
    position: u64,
}

/// The mixer. The sound data is passed to each call, so the mixer owns
/// none of it.
pub struct Audio {
    opl: Opl2,
    rate: u32,
    /// Counts [`TIMER_HZ`] per output sample; the timer ticks each time it
    /// passes the output rate.
    timer_phase: u32,
    /// The effect service runs on every second timer tick.
    service_phase: bool,
    music: Option<Song>,
    adlib: Option<AdlibPlaying>,
    sample: Option<SamplePlaying>,
    /// Priorities of the sample and of the AdLib effect started last.
    sample_priority: u8,
    adlib_priority: u8,
    pub options: EffectOptions,
}

impl Audio {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        let mut audio = Self {
            opl: Opl2::new(rate),
            rate,
            timer_phase: 0,
            service_phase: false,
            music: None,
            adlib: None,
            sample: None,
            sample_priority: 0,
            adlib_priority: 0,
            options: EffectOptions {
                digitized: true,
                adlib: true,
            },
        };
        // Waveform select on, as the original's AdLib detection leaves it.
        audio.opl.write_reg(0x01, 0x20);
        audio
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Starts a song from the beginning, replacing the current one (first
    /// silencing the chip).
    pub fn start_music(&mut self, song: &[ImfCommand]) {
        self.silence();
        self.music = Some(Song::new(song.to_vec()));
    }

    /// Stops the music.
    pub fn stop_music(&mut self) {
        self.music = None;
        self.silence();
    }

    /// Whether fewer than 41 bytes of the song are left; the movies stop when
    /// they are.
    pub fn music_near_end(&self) -> bool {
        self.music.as_ref().is_some_and(Song::near_end)
    }

    /// Stops the effects that play.
    pub fn stop_sounds(&mut self) {
        self.sample = None;
        self.sample_priority = 0;
        if self.adlib.take().is_some() {
            self.opl.write_reg(0xb0, 0);
        }
    }

    /// Key off on all channels, no percussion.
    fn silence(&mut self) {
        self.opl.write_reg(0xbd, 0);
        for channel in 0..9 {
            self.opl.write_reg(0xb0 + channel, 0);
        }
        self.adlib = None;
    }

    /// Starts an effect.
    pub fn play(&mut self, sounds: &Sounds, sound: Sound) {
        let id = sound.id();
        let priority = PRIORITY.get(usize::from(id)).copied().unwrap_or(0);
        if !self.options.digitized {
            self.play_adlib_version(sounds, id, priority);
            return;
        }
        if self.sample.is_none() {
            self.sample_priority = 0;
        }
        if self.adlib.is_none() {
            self.adlib_priority = 0;
        }
        if digitized(sounds, id).is_some() {
            let start = SamplePlaying { id, position: 0 };
            if id >= UNCHECKED_FROM {
                self.sample = Some(start);
                return;
            }
            if self.sample_priority <= priority {
                self.sample = Some(start);
                self.sample_priority = priority;
            }
            // The original does not return here: the AdLib version starts
            // as well.
            self.play_adlib_version(sounds, id, priority);
        } else if usize::from(id) < PRIORITY.len() && priority >= self.adlib_priority {
            self.adlib_priority = priority;
            self.start_adlib(sounds, id);
        }
    }

    /// The AdLib version of an effect, when the AdLib option is on: checked
    /// against the sample's priority, which an idle chip first resets.
    fn play_adlib_version(&mut self, sounds: &Sounds, id: u8, priority: u8) {
        if !self.options.adlib {
            return;
        }
        if self.adlib.is_none() {
            self.sample_priority = 0;
        }
        if priority < self.sample_priority {
            return;
        }
        self.sample_priority = priority;
        self.start_adlib(sounds, id);
    }

    /// Stops the old effect and loads the instrument into channel 0.
    fn start_adlib(&mut self, sounds: &Sounds, id: u8) {
        let Some(effect) = adlib(sounds, id) else {
            return;
        };
        let i = &effect.instrument;
        self.opl.write_reg(0xb0, 0);
        for (reg, value) in [
            (0x20, i[0]),
            (0x40, i[2]),
            (0x60, i[4]),
            (0x80, i[6]),
            (0xe0, i[8]),
            (0x23, i[1]),
            (0x43, i[3]),
            (0x63, i[5]),
            (0x83, i[7]),
            (0xe3, i[9]),
            (0xc0, 0),
        ] {
            self.opl.write_reg(reg, value);
        }
        self.adlib = Some(AdlibPlaying {
            id,
            position: 0,
            key_on: ((effect.block & 7) << 2) | 0x20,
        });
    }

    /// The 140 Hz effect service: the next frequency byte, or key off for
    /// 0; the effect ends after its last byte.
    fn service_adlib(&mut self, sounds: &Sounds) {
        let Some(playing) = &mut self.adlib else {
            return;
        };
        let Some(effect) = adlib(sounds, playing.id) else {
            self.adlib = None;
            return;
        };
        let note = effect.notes[playing.position];
        playing.position += 1;
        if note == 0 {
            self.opl.write_reg(0xb0, 0);
        } else {
            self.opl.write_reg(0xa0, note);
            self.opl.write_reg(0xb0, playing.key_on);
        }
        if playing.position >= effect.notes.len() {
            self.opl.write_reg(0xb0, 0);
            self.adlib = None;
        }
    }

    /// One tick of the 280 Hz timer interrupt.
    fn timer_tick(&mut self, sounds: &Sounds) {
        if let Some(music) = &mut self.music {
            let opl = &mut self.opl;
            music.tick(|reg, value| opl.write_reg(reg, value));
        }
        self.service_phase = !self.service_phase;
        if self.service_phase {
            self.service_adlib(sounds);
        }
    }

    /// The next sample of the digitized effect, -1.0 to 1.0 times the gain.
    fn sample_value(&mut self, sounds: &Sounds) -> f32 {
        let Some(playing) = &mut self.sample else {
            return 0.0;
        };
        let Some(sample) = digitized(sounds, playing.id) else {
            self.sample = None;
            return 0.0;
        };
        let index = (playing.position >> 16) as usize;
        let Some(&a) = sample.data.get(index) else {
            self.sample = None;
            return 0.0;
        };
        let a = f32::from(a) - 128.0;
        let b = sample
            .data
            .get(index + 1)
            .map_or(a, |&b| f32::from(b) - 128.0);
        let fraction = (playing.position & 0xffff) as f32 / 65536.0;
        playing.position += (u64::from(sample.rate) << 16) / u64::from(self.rate);
        (a + (b - a) * fraction) * SAMPLE_GAIN
    }

    /// Fills `out` with mono samples at the output rate, -1.0 to 1.0.
    pub fn render(&mut self, sounds: &Sounds, out: &mut [f32]) {
        for value in out {
            self.timer_phase += TIMER_HZ;
            while self.timer_phase >= self.rate {
                self.timer_phase -= self.rate;
                self.timer_tick(sounds);
            }
            let (chip, _) = self.opl.render_frame();
            let mixed = chip as f32 * CHIP_GAIN + self.sample_value(sounds);
            *value = mixed.clamp(-1.0, 1.0);
        }
    }
}

fn adlib(sounds: &Sounds, id: u8) -> Option<&AdlibEffect> {
    sounds.adlib.get(usize::from(id))?.as_ref()
}

fn digitized(sounds: &Sounds, id: u8) -> Option<&dn2_assets::voc::Sample> {
    sounds.digitized.get(usize::from(id))?.as_ref()
}
