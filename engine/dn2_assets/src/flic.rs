//! Movies: NUKEM2.F1-F5, Autodesk FLI. See docs/FORMATS.md "Movies".

use alloc::vec::Vec;

use crate::AssetError;
use crate::gfx::{Image, Palette, Rgb};
use crate::reader::Reader;

const HEADER_BYTES: usize = 128;
const FILE_MAGIC: u16 = 0xaf11;
const FRAME_MAGIC: u16 = 0xf1fa;
const FRAME_HEADER_BYTES: usize = 16;
const CHUNK_HEADER_BYTES: u32 = 6;

const COLOR_256: u16 = 4;
const COLOR_64: u16 = 11;
const LC: u16 = 12;
const BRUN: u16 = 15;
const COPY: u16 = 16;

/// A parsed movie: the header and the frame records, not yet decoded.
#[derive(Clone, Debug)]
pub struct Flic {
    pub width: u16,
    pub height: u16,
    /// All frame records in file order (one more than the header's count:
    /// the last returns to the second, the loop start).
    pub frames: Vec<FlicFrame>,
}

/// One frame record: its chunks in order. Chunks of unknown type (and the
/// ignored BLACK chunk) are dropped at parse time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlicFrame {
    pub chunks: Vec<Chunk>,
}

/// A run of pixels in a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    /// These pixels as they are.
    Copy(Vec<u8>),
    /// `count` pixels of one value.
    Fill { count: u8, value: u8 },
}

/// Colours `first..first + colors.len()` of a palette chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalettePacket {
    pub first: usize,
    pub colors: Vec<Rgb>,
}

/// A run of a delta line, after skipping `skip` unchanged pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeltaPacket {
    pub skip: u8,
    pub run: Run,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    /// Chunks 4 and 11: new palette entries (already widened to 8 bits).
    Palette(Vec<PalettePacket>),
    /// Chunk 12 (LC): the lines from `first_line` on that changed, as skip
    /// and run packets.
    Delta {
        first_line: u16,
        lines: Vec<Vec<DeltaPacket>>,
    },
    /// Chunk 15 (BRUN): every line of the picture as runs.
    Brun(Vec<Vec<Run>>),
    /// Chunk 16: the whole picture, uncompressed.
    Copy(Vec<u8>),
}

/// Decodes frames one after the other into a picture and its palette.
#[derive(Clone, Debug)]
pub struct FlicPlayer {
    pub image: Image,
    pub palette: Palette,
}

impl Flic {
    /// Index of the record the loop returns to. Playback is the UI's job:
    /// record 0 plays once, then the records from `LOOP_START` on repeat.
    /// The last record is a delta that turns the final picture back into
    /// the one after record 0, so applying it and then record 1 again
    /// loops seamlessly.
    pub const LOOP_START: usize = 1;

    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut r = Reader::new(file, bytes);
        r.u32()?;
        if r.u16()? != FILE_MAGIC {
            return Err(r.error("not a FLI file"));
        }
        let count = r.u16()? as usize + 1;
        let (width, height) = (r.u16()?, r.u16()?);
        r.seek(HEADER_BYTES)?;
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            frames.push(parse_frame(file, &mut r, width, height)?);
        }
        Ok(Self {
            width,
            height,
            frames,
        })
    }
}

fn parse_frame(
    file: &str,
    r: &mut Reader,
    width: u16,
    height: u16,
) -> Result<FlicFrame, AssetError> {
    let size = r.u32()? as usize;
    if r.u16()? != FRAME_MAGIC {
        return Err(r.error("bad frame magic"));
    }
    let count = r.u16()?;
    let body = size
        .checked_sub(8)
        .ok_or_else(|| r.error("frame too small"))?;
    let mut r = Reader::new(file, r.bytes(body)?);
    r.bytes(FRAME_HEADER_BYTES - 8)?;
    let mut chunks = Vec::new();
    for _ in 0..count {
        let size = r.u32()?;
        let kind = r.u16()?;
        let len = size
            .checked_sub(CHUNK_HEADER_BYTES)
            .ok_or_else(|| r.error("chunk too small"))?;
        let mut data = Reader::new(file, r.bytes(len as usize)?);
        let chunk = match kind {
            COLOR_256 => Chunk::Palette(parse_palette(&mut data, false)?),
            COLOR_64 => Chunk::Palette(parse_palette(&mut data, true)?),
            LC => parse_delta(&mut data, height)?,
            BRUN => parse_brun(&mut data, width, height)?,
            COPY => Chunk::Copy(data.bytes(width as usize * height as usize)?.to_vec()),
            _ => continue,
        };
        chunks.push(chunk);
    }
    Ok(FlicFrame { chunks })
}

/// Packets of (skip, count with 0 meaning 256, RGB triples).
fn parse_palette(r: &mut Reader, six_bit: bool) -> Result<Vec<PalettePacket>, AssetError> {
    // 8-bit values go through the 6-bit VGA DAC like everything else.
    let dac = |v: u8| if six_bit { v } else { v >> 2 };
    let wide = |v: u8| dac(v) << 2 | dac(v) >> 4;
    let mut first = 0;
    (0..r.u16()?)
        .map(|_| {
            first += r.u8()? as usize;
            let count = match r.u8()? {
                0 => 256,
                n => n as usize,
            };
            let colors = r
                .bytes(count * 3)?
                .chunks(3)
                .map(|c| Rgb {
                    r: wide(c[0]),
                    g: wide(c[1]),
                    b: wide(c[2]),
                })
                .collect();
            let packet = PalettePacket { first, colors };
            first += count;
            Ok(packet)
        })
        .collect()
}

/// LC: a first line and a line count, then per line a packet count and
/// packets of skip and signed count (positive copies, negative fills). A
/// fill packet's value byte follows its count, as in the FLI layout.
fn parse_delta(r: &mut Reader, height: u16) -> Result<Chunk, AssetError> {
    let first_line = r.u16()?;
    let count = r.u16()?;
    if first_line as usize + count as usize > height as usize {
        return Err(r.error("delta lines past the picture"));
    }
    let lines = (0..count)
        .map(|_| {
            (0..r.u8()?)
                .map(|_| {
                    let skip = r.u8()?;
                    let run = match r.u8()? as i8 {
                        n if n >= 0 => Run::Copy(r.bytes(n as usize)?.to_vec()),
                        n => Run::Fill {
                            count: n.unsigned_abs(),
                            value: r.u8()?,
                        },
                    };
                    Ok(DeltaPacket { skip, run })
                })
                .collect()
        })
        .collect::<Result<_, _>>()?;
    Ok(Chunk::Delta { first_line, lines })
}

/// BRUN: per line an ignored packet count, then signed counts until the line
/// is full (negative copies, positive fills).
fn parse_brun(r: &mut Reader, width: u16, height: u16) -> Result<Chunk, AssetError> {
    let lines = (0..height)
        .map(|_| {
            r.u8()?;
            let mut runs = Vec::new();
            let mut x = 0;
            while x < width as usize {
                let run = match r.u8()? as i8 {
                    n if n < 0 => Run::Copy(r.bytes(n.unsigned_abs() as usize)?.to_vec()),
                    n => Run::Fill {
                        count: n as u8,
                        value: r.u8()?,
                    },
                };
                x += run.len();
                runs.push(run);
            }
            Ok(runs)
        })
        .collect::<Result<_, _>>()?;
    Ok(Chunk::Brun(lines))
}

impl Run {
    fn len(&self) -> usize {
        match self {
            Run::Copy(data) => data.len(),
            Run::Fill { count, .. } => *count as usize,
        }
    }
}

impl FlicPlayer {
    /// A black picture and palette, as before the first frame.
    pub fn new(flic: &Flic) -> Self {
        Self {
            image: Image::new(flic.width as u32, flic.height as u32, 0),
            palette: Palette(alloc::vec![Rgb::default(); 256]),
        }
    }

    /// Applies one frame record to the picture and palette.
    pub fn apply(&mut self, frame: &FlicFrame) {
        for chunk in &frame.chunks {
            match chunk {
                Chunk::Palette(packets) => {
                    for p in packets {
                        for (i, &c) in p.colors.iter().enumerate() {
                            if let Some(slot) = self.palette.0.get_mut(p.first + i) {
                                *slot = c;
                            }
                        }
                    }
                }
                Chunk::Delta { first_line, lines } => {
                    for (y, packets) in (*first_line as u32..).zip(lines) {
                        let mut x = 0;
                        for p in packets {
                            x += p.skip as u32;
                            x += self.put_run(x, y, &p.run);
                        }
                    }
                }
                Chunk::Brun(lines) => {
                    for (y, runs) in (0..).zip(lines) {
                        let mut x = 0;
                        for run in runs {
                            x += self.put_run(x, y, run);
                        }
                    }
                }
                Chunk::Copy(pixels) => self.image.pixels.copy_from_slice(pixels),
            }
        }
    }

    /// Draws a run at (x, y), clipped to the line; returns its length.
    fn put_run(&mut self, x: u32, y: u32, run: &Run) -> u32 {
        let width = self.image.width;
        if x < width && y < self.image.height {
            let start = (y * width + x) as usize;
            let end = (start + run.len()).min(((y + 1) * width) as usize);
            let dst = &mut self.image.pixels[start..end];
            match run {
                Run::Copy(data) => dst.copy_from_slice(&data[..dst.len()]),
                Run::Fill { value, .. } => dst.fill(*value),
            }
        }
        run.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn chunk(kind: u16, body: &[u8]) -> Vec<u8> {
        let mut v = (body.len() as u32 + 6).to_le_bytes().to_vec();
        v.extend(kind.to_le_bytes());
        v.extend(body);
        v
    }

    fn frame(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = chunks.concat();
        let mut v = (body.len() as u32 + 16).to_le_bytes().to_vec();
        v.extend(FRAME_MAGIC.to_le_bytes());
        v.extend((chunks.len() as u16).to_le_bytes());
        v.extend([0; 8]);
        v.extend(body);
        v
    }

    fn file(w: u16, h: u16, frames: &[Vec<u8>]) -> Vec<u8> {
        let mut v = vec![0; 4];
        v.extend(FILE_MAGIC.to_le_bytes());
        v.extend((frames.len() as u16 - 1).to_le_bytes());
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.resize(HEADER_BYTES, 0);
        v.extend(frames.concat());
        v
    }

    fn play(w: u16, h: u16, frames: &[Vec<u8>]) -> FlicPlayer {
        let flic = Flic::parse("T", &file(w, h, frames)).unwrap();
        assert_eq!(flic.frames.len(), frames.len());
        let mut p = FlicPlayer::new(&flic);
        for f in &flic.frames {
            p.apply(f);
        }
        p
    }

    #[test]
    fn brun_then_lc() {
        // 4x2: line 0 = fill 4 x 7; line 1 = copy 1,2 then fill 2 x 9.
        let brun = chunk(BRUN, &[1, 4, 7, 2, 0xfe, 1, 2, 2, 9]);
        // Line 1: skip 1, copy [5]; skip 1, fill 2 x 8 (clipped at the edge).
        let lc = chunk(LC, &[1, 0, 1, 0, 2, 1, 1, 5, 1, 0xfe, 8]);
        let p = play(4, 2, &[frame(&[brun]), frame(&[lc]), frame(&[])]);
        assert_eq!(p.image.pixels, [7, 7, 7, 7, 1, 5, 9, 8]);
    }

    #[test]
    fn lc_clips_and_copy() {
        let copy = chunk(COPY, &[1, 2, 3, 4, 5, 6]);
        let lc = chunk(LC, &[0, 0, 1, 0, 1, 2, 3, 9, 9, 9]);
        let p = play(3, 2, &[frame(&[copy, chunk(13, &[])]), frame(&[lc])]);
        assert_eq!(p.image.pixels, [1, 2, 9, 4, 5, 6]);
    }

    #[test]
    fn palettes() {
        // 6-bit: skip 1, 2 colours.
        let six = chunk(COLOR_64, &[1, 0, 1, 2, 63, 0, 32, 1, 2, 3]);
        let p = play(1, 1, &[frame(&[six]), frame(&[])]);
        assert_eq!(p.palette.0[0], Rgb::default());
        assert_eq!(
            p.palette.0[1],
            Rgb {
                r: 255,
                g: 0,
                b: 130
            }
        );
        assert_eq!(p.palette.0[2], Rgb { r: 4, g: 8, b: 12 });
        // 8-bit: count 0 means 256 colours.
        let mut full = vec![1, 0, 0, 0];
        full.extend((0..768).map(|i| (i % 256) as u8));
        let p = play(1, 1, &[frame(&[chunk(COLOR_256, &full)]), frame(&[])]);
        assert_eq!(p.palette.0.len(), 256);
        assert_eq!(p.palette.0[1], Rgb { r: 0, g: 4, b: 4 });
    }

    #[test]
    fn malformed() {
        let ok = file(2, 1, &[frame(&[]), frame(&[])]);
        assert!(Flic::parse("T", &ok).is_ok());
        assert!(Flic::parse("T", &ok[..ok.len() - 1]).is_err());
        let mut bad = ok.clone();
        bad[4] = 0;
        assert!(Flic::parse("T", &bad).is_err());
        let short = file(2, 1, &[frame(&[chunk(COPY, &[1])]), frame(&[])]);
        assert!(Flic::parse("T", &short).is_err());
    }
}
