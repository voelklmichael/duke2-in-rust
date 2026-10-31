//! Digitized sound effects: Creative Voice (VOC) files for the Sound Blaster.
//!
//! The player skips the file header and walks the blocks: 1 sound data, 2
//! continued data, 3 silence, 6 and 7 repeat loop, 0 end. The game's files
//! (SB_n.MNI, INTRO3-9.MNI) only use blocks 1 and 3, so repeat blocks are
//! ignored here. Sound data is 8-bit unsigned PCM or one of the Creative ADPCM
//! packings that the DSP decodes itself (4, 2.6 and 2 bits per sample, chosen
//! by the pack byte). Here a whole file is decoded to 8-bit samples.

use alloc::vec::Vec;

use crate::AssetError;

/// A decoded sound: 8-bit unsigned mono samples at `rate` Hz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub rate: u32,
    pub data: Vec<u8>,
}

/// Parses and decodes a VOC file; `name` is for errors.
pub fn parse(name: &str, file: &[u8]) -> Result<Sample, AssetError> {
    const MAGIC: &[u8] = b"Creative Voice File\x1a";
    if file.len() < 26 || &file[..20] != MAGIC {
        return Err(AssetError::malformed(name, "bad signature"));
    }
    let mut pos = usize::from(u16::from_le_bytes([file[20], file[21]]));
    let mut rate = 0;
    let mut data = Vec::new();
    while pos < file.len() && file[pos] != 0 {
        let kind = file[pos];
        let size = file
            .get(pos + 1..pos + 4)
            .map(|s| usize::from(s[0]) | usize::from(s[1]) << 8 | usize::from(s[2]) << 16)
            .ok_or_else(|| AssetError::malformed(name, "truncated block"))?;
        let body = file
            .get(pos + 4..pos + 4 + size)
            .ok_or_else(|| AssetError::malformed(name, "truncated block"))?;
        pos += 4 + size;
        match kind {
            1 if body.len() >= 2 => {
                if rate == 0 {
                    rate = time_constant_rate(body[0]);
                }
                decode(body[1], &body[2..], &mut data)
                    .ok_or_else(|| AssetError::malformed(name, "unknown packing"))?;
            }
            2 => data.extend_from_slice(body),
            3 if body.len() >= 3 && rate != 0 => {
                // Silence: length - 1 as u16, then a time constant.
                let length = usize::from(u16::from_le_bytes([body[0], body[1]])) + 1;
                let silence_rate = time_constant_rate(body[2]) as usize;
                data.resize(data.len() + length * rate as usize / silence_rate, 128);
            }
            _ => {}
        }
    }
    if rate == 0 {
        return Err(AssetError::malformed(name, "no sound data"));
    }
    Ok(Sample { rate, data })
}

/// Sample rate for a DSP time constant.
fn time_constant_rate(tc: u8) -> u32 {
    1_000_000 / (256 - u32::from(tc))
}

/// Decodes the data of a sound block with pack type `pack` (0 = 8-bit PCM,
/// 1 = 4-bit ADPCM, 2 = 2.6-bit, 3 = 2-bit) and appends the 8-bit samples.
/// The ADPCM packings start with a reference sample byte.
fn decode(pack: u8, body: &[u8], out: &mut Vec<u8>) -> Option<()> {
    match pack {
        0 => out.extend_from_slice(body),
        1..=3 => {
            let Some((&first, rest)) = body.split_first() else {
                return Some(());
            };
            let mut state = Adpcm {
                reference: first,
                scale: 0,
            };
            out.push(first);
            for &byte in rest {
                match pack {
                    1 => {
                        out.push(state.step(byte >> 4, &SCALE4, &ADJUST4));
                        out.push(state.step(byte & 15, &SCALE4, &ADJUST4));
                    }
                    2 => {
                        out.push(state.step((byte >> 5) & 7, &SCALE3, &ADJUST3));
                        out.push(state.step((byte >> 2) & 7, &SCALE3, &ADJUST3));
                        out.push(state.step((byte & 3) << 1, &SCALE3, &ADJUST3));
                    }
                    _ => {
                        out.push(state.step(byte >> 6, &SCALE2, &ADJUST2));
                        out.push(state.step((byte >> 4) & 3, &SCALE2, &ADJUST2));
                        out.push(state.step((byte >> 2) & 3, &SCALE2, &ADJUST2));
                        out.push(state.step(byte & 3, &SCALE2, &ADJUST2));
                    }
                }
            }
        }
        _ => return None,
    }
    Some(())
}

/// The step tables of the 4-, 3- and 2-bit packings: the change to the
/// reference per code, and to the scale.
const SCALE4: [i8; 64] = [
    0, 1, 2, 3, 4, 5, 6, 7, 0, -1, -2, -3, -4, -5, -6, -7, 1, 3, 5, 7, 9, 11, 13, 15, -1, -3, -5,
    -7, -9, -11, -13, -15, 2, 6, 10, 14, 18, 22, 26, 30, -2, -6, -10, -14, -18, -22, -26, -30, 4,
    12, 20, 28, 36, 44, 52, 60, -4, -12, -20, -28, -36, -44, -52, -60,
];
const ADJUST4: [i8; 64] = [
    0, 0, 0, 0, 0, 16, 16, 16, 0, 0, 0, 0, 0, 16, 16, 16, -16, 0, 0, 0, 0, 16, 16, 16, -16, 0, 0,
    0, 0, 16, 16, 16, -16, 0, 0, 0, 0, 16, 16, 16, -16, 0, 0, 0, 0, 16, 16, 16, -16, 0, 0, 0, 0, 0,
    0, 0, -16, 0, 0, 0, 0, 0, 0, 0,
];
const SCALE3: [i8; 40] = [
    0, 1, 2, 3, 0, -1, -2, -3, 1, 3, 5, 7, -1, -3, -5, -7, 2, 6, 10, 14, -2, -6, -10, -14, 4, 12,
    20, 28, -4, -12, -20, -28, 5, 15, 25, 35, -5, -15, -25, -35,
];
const ADJUST3: [i8; 40] = [
    0, 0, 0, 8, 0, 0, 0, 8, -8, 0, 0, 8, -8, 0, 0, 8, -8, 0, 0, 8, -8, 0, 0, 8, -8, 0, 0, 8, -8, 0,
    0, 8, -8, 0, 0, 0, -8, 0, 0, 0,
];
const SCALE2: [i8; 24] = [
    0, 1, 0, -1, 1, 3, -1, -3, 2, 6, -2, -6, 4, 12, -4, -12, 8, 24, -8, -24, 6, 48, -16, -48,
];
const ADJUST2: [i8; 24] = [
    0, 4, 0, 4, -4, 4, -4, 4, -4, 4, -4, 4, -4, 4, -4, 4, -4, 4, -4, 4, -4, 0, -4, 0,
];

/// The Sound Blaster DSP's ADPCM decoder state: the current sample and a
/// step scale (a multiple of the code range, so it selects a table row).
struct Adpcm {
    reference: u8,
    scale: i32,
}

impl Adpcm {
    fn step(&mut self, code: u8, scale_map: &[i8], adjust_map: &[i8]) -> u8 {
        let index = (i32::from(code) + self.scale).clamp(0, scale_map.len() as i32 - 1) as usize;
        self.reference =
            (i32::from(self.reference) + i32::from(scale_map[index])).clamp(0, 255) as u8;
        self.scale = (self.scale + i32::from(adjust_map[index])).clamp(0, 255);
        self.reference
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_pcm_and_silence_blocks() {
        let mut file = b"Creative Voice File\x1a".to_vec();
        file.extend_from_slice(&[0x1a, 0, 0x0a, 1, 0x29, 0x11]);
        // Block 1: time constant 131 (8000 Hz), pack 0, 3 samples; block 3:
        // two samples of silence.
        file.extend_from_slice(&[1, 5, 0, 0, 131, 0, 10, 20, 30]);
        file.extend_from_slice(&[3, 3, 0, 0, 1, 0, 131]);
        file.push(0);
        let s = parse("T.VOC", &file).unwrap();
        assert_eq!(s.rate, 8000);
        assert_eq!(s.data, [10, 20, 30, 128, 128]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse("T.VOC", b"hello").is_err());
    }
}
