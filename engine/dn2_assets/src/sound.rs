//! Sound effects and music: AUDIOHED.MNI/AUDIOT.MNI, the digitized effects
//! and the IMF songs. See docs/FORMATS.md, "Music" and "Sound effects".

use alloc::vec::Vec;

use crate::reader::Reader;
use crate::voc::{self, Sample};
use crate::{AssetError, Files};

/// Effect ids the game plays: 0-21h have AdLib versions, 0-1Dh may have a
/// digitized one (SB_(id + 1).MNI), 2Ah-30h are the movies' samples.
pub const EFFECTS: usize = 0x31;

/// Effect ids with an AdLib version (the original plays them up to 21h).
const ADLIB_EFFECTS: u8 = 0x22;

/// Entry in AUDIOHED.MNI of the AdLib version of effect `id` (id + 22h).
const ADLIB_ENTRY: usize = 0x22;

/// Effect ids that get a digitized version.
const DIGITIZED: u8 = 30;

/// The movies' samples INTRO3-9.MNI, effects 2Ah to 30h.
const INTRO_FIRST: u8 = 0x2a;

/// An AdLib sound effect from AUDIOT.MNI, played on channel 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdlibEffect {
    /// Registers 20h, 23h, 40h, 43h, 60h, 63h, 80h, 83h, E0h, E3h in the
    /// file's order: modulator and carrier alternate.
    pub instrument: [u8; 10],
    /// The octave for register B0h.
    pub block: u8,
    /// One frequency byte (register A0h) per 140 Hz tick; 0 is key off.
    pub notes: Vec<u8>,
}

/// One IMF record: an AdLib register, its value, and the 280 Hz ticks to
/// wait before the next record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImfCommand {
    pub reg: u8,
    pub value: u8,
    pub delay: u16,
}

/// All sound effects, by the original's effect id.
#[derive(Clone, Debug, Default)]
pub struct Sounds {
    pub adlib: Vec<Option<AdlibEffect>>,
    pub digitized: Vec<Option<Sample>>,
}

impl Sounds {
    /// Reads AUDIOHED.MNI, AUDIOT.MNI and the VOC files present.
    pub fn load(files: &Files) -> Result<Self, AssetError> {
        let head = files.read("AUDIOHED.MNI")?;
        let audiot = files.read("AUDIOT.MNI")?;
        let offsets: Vec<usize> = head
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c) as usize)
            .collect();
        let adlib = (0..ADLIB_EFFECTS)
            .map(|id| {
                let start = *offsets.get(usize::from(id) + ADLIB_ENTRY)?;
                adlib_effect(audiot.get(start..)?)
            })
            .collect();
        let mut digitized = alloc::vec![None; EFFECTS];
        let names = (0..DIGITIZED)
            .map(|id| (id, alloc::format!("SB_{}.MNI", id + 1)))
            .chain(
                (INTRO_FIRST..EFFECTS as u8)
                    .map(|id| (id, alloc::format!("INTRO{}.MNI", id - INTRO_FIRST + 3))),
            );
        for (id, name) in names {
            if let Ok(bytes) = files.read(&name) {
                digitized[usize::from(id)] = Some(voc::parse(&name, &bytes)?);
            }
        }
        Ok(Self { adlib, digitized })
    }
}

/// An AUDIOT.MNI entry: u32 length, u16 priority, 16 instrument bytes, the
/// block, then `length` frequency bytes. `None` for an empty or cut entry.
fn adlib_effect(entry: &[u8]) -> Option<AdlibEffect> {
    let mut r = Reader::new("AUDIOT.MNI", entry);
    let length = r.u32().ok()? as usize;
    let _priority = r.u16().ok()?;
    let bytes = r.bytes(16).ok()?;
    let block = r.u8().ok()?;
    let notes = r.bytes(length).ok()?;
    (length > 0).then(|| AdlibEffect {
        instrument: core::array::from_fn(|i| bytes[i]),
        block,
        notes: notes.to_vec(),
    })
}

/// Splits IMF data (no header, 4-byte records) into records; a trailing
/// partial record is dropped.
pub fn parse_imf(data: &[u8]) -> Vec<ImfCommand> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|c| ImfCommand {
            reg: c[0],
            value: c[1],
            delay: u16::from_le_bytes([c[2], c[3]]),
        })
        .collect()
}
