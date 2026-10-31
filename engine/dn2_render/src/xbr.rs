//! Making the picture larger with its edges smoothed and blended, in the
//! manner of Hyllian's xBR: where the colours around a pixel's corner run
//! along a line, the corner is cut along that line and takes the colour on
//! its other side, blended by how much of each frame pixel the cut covers.
//!
//! The edge detection follows the published xBR rules (level 2: lines at
//! 45 degrees, and shallow and steep ones at 1:2); the cut is drawn
//! geometrically, so every scale from 2 works the same way.
//!
//! Unlike EPX the result has colours the palette lacks, so this works on
//! RGB: the picture's palette indices, and the colours they stand for.

use alloc::vec;
use alloc::vec::Vec;

use dn2_assets::gfx::Image;

use crate::scale::{pixel_at, put_block};

/// Samples a frame pixel's coverage is measured with, a side.
const SAMPLES: u32 = 4;
/// Coverage of a whole frame pixel: two for each sample, as one on the
/// line counts half.
const FULL: u32 = 2 * SAMPLES * SAMPLES;

/// How a corner is cut, from the pixel's bottom right corner: the parts of
/// the unit square beyond a line take the neighbour's colour.
#[derive(Clone, Copy)]
enum Cut {
    /// At 45 degrees, from (1, 1/2) to (1/2, 1): `x + y > 3/2`.
    Diagonal,
    /// Shallow, along the bottom from (0, 1) to (1, 1/2): `x + 2y > 2`.
    Shallow,
    /// Steep, along the right from (1, 0) to (1/2, 1): `2x + y > 2`.
    Steep,
    /// Both the shallow and the steep cut.
    Both,
}

/// For each cut, how much of each of the `scale` x `scale` frame pixels of
/// a game pixel it covers (0 to [`FULL`]), row by row, from the bottom
/// right corner as [`Cut`] describes.
struct Masks([Vec<u8>; 4]);

impl Masks {
    fn new(scale: u32) -> Self {
        let mask = |cut: Cut| {
            // Sample points at (2i + 1) / (2 SAMPLES scale): in whole numbers
            // of `unit` across the game pixel.
            let unit = 2 * SAMPLES * scale;
            // 2 beyond the line, 1 on it.
            let side = |beyond: u32, line: u32| u8::from(beyond >= line) + u8::from(beyond > line);
            let inside = |x: u32, y: u32| match cut {
                Cut::Diagonal => side(2 * (x + y), 3 * unit),
                Cut::Shallow => side(x + 2 * y, 2 * unit),
                Cut::Steep => side(2 * x + y, 2 * unit),
                Cut::Both => side(x + 2 * y, 2 * unit).max(side(2 * x + y, 2 * unit)),
            };
            let mut out = Vec::with_capacity((scale * scale) as usize);
            for row in 0..scale {
                for column in 0..scale {
                    let mut covered = 0;
                    for sy in 0..SAMPLES {
                        for sx in 0..SAMPLES {
                            let x = 2 * (column * SAMPLES + sx) + 1;
                            let y = 2 * (row * SAMPLES + sy) + 1;
                            covered += inside(x, y);
                        }
                    }
                    out.push(covered);
                }
            }
            out
        };
        Self([Cut::Diagonal, Cut::Shallow, Cut::Steep, Cut::Both].map(mask))
    }

    fn get(&self, cut: Cut) -> &[u8] {
        &self.0[cut as usize]
    }
}

/// A colour split into brightness and two colour differences, in whole
/// numbers, for comparing colours as the eye does.
#[derive(Clone, Copy, Default)]
struct Yuv {
    y: i32,
    u: i32,
    v: i32,
}

impl Yuv {
    fn new(rgb: u32) -> Self {
        let (r, g, b) = (
            (rgb >> 16 & 0xff) as i32,
            (rgb >> 8 & 0xff) as i32,
            (rgb & 0xff) as i32,
        );
        let y = (77 * r + 150 * g + 29 * b) >> 8;
        Self {
            y,
            u: b - y,
            v: r - y,
        }
    }

    /// How different two colours look; brightness weighs most.
    fn distance(self, other: Self) -> i32 {
        48 * (self.y - other.y).abs() + 3 * (self.u - other.u).abs() + 5 * (self.v - other.v).abs()
    }
}

/// `image` made `scale` (at least 2) times larger into `out`, a frame of
/// 0x00RRGGBB pixels `scale` times its size; `colors` are the palette's.
pub fn xbr(image: &Image, colors: &[u32], scale: u32, out: &mut [u32]) {
    let masks = Masks::new(scale);
    let yuv: Vec<Yuv> = colors.iter().map(|&c| Yuv::new(c)).collect();
    let (w, h) = (image.width as i32, image.height as i32);
    let n = scale as usize;
    let mut block = vec![0u32; n * n];
    for y in 0..h {
        for x in 0..w {
            let at = |dx: i32, dy: i32| pixel_at(image, x + dx, y + dy);
            let e = at(0, 0);
            block.fill(colors[usize::from(e)]);
            // Each corner, as the bottom right one of the picture mirrored.
            for (mx, my) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                let around = |dx: i32, dy: i32| at(mx * dx, my * dy);
                let Some((cut, color)) = corner(&around, &yuv) else {
                    continue;
                };
                let rgb = colors[usize::from(color)];
                let mask = masks.get(cut);
                for row in 0..n {
                    for column in 0..n {
                        // The mask is for the bottom right corner.
                        let mr = if my > 0 { row } else { n - 1 - row };
                        let mc = if mx > 0 { column } else { n - 1 - column };
                        let covered = u32::from(mask[mr * n + mc]);
                        if covered > 0 {
                            let pixel = &mut block[row * n + column];
                            *pixel = blend(*pixel, rgb, covered);
                        }
                    }
                }
            }
            put_block(out, image.width * scale, x as u32, y as u32, scale, &block);
        }
    }
}

/// How the bottom right corner of the pixel at `around(0, 0)` is cut and
/// the colour beyond the cut, if it is. `around(dx, dy)` is the palette
/// index of the pixel at that offset, `yuv` the palette's colours.
///
/// ```text
///        B1 C1         names of the offsets; the pixel is E
///     D  E  F  F4
///     G  H  I  I4
///        H5 I5
/// ```
fn corner(around: &impl Fn(i32, i32) -> u8, yuv: &[Yuv]) -> Option<(Cut, u8)> {
    let e = around(0, 0);
    let f = around(1, 0);
    let h = around(0, 1);
    // A corner inside a flat area or a straight edge stays.
    if e == f || e == h {
        return None;
    }
    let b = around(0, -1);
    let c = around(1, -1);
    let d = around(-1, 0);
    let g = around(-1, 1);
    let i = around(1, 1);
    let f4 = around(2, 0);
    let i4 = around(2, 1);
    let h5 = around(0, 2);
    let i5 = around(1, 2);
    // Single pixels and square corners stay, unless they sit on a line.
    if !((f != b && h != d) || (e == i && f != i4 && h != i5) || e == g || e == c) {
        return None;
    }
    let dist = |p: u8, q: u8| yuv[usize::from(p)].distance(yuv[usize::from(q)]);
    // How much the colours change across the corner's diagonal (F to H)
    // against along it (E to I): a line runs where they change least.
    let across = dist(e, c) + dist(e, g) + dist(i, f4) + dist(i, h5) + 4 * dist(h, f);
    let along = dist(h, d) + dist(h, i5) + dist(f, i4) + dist(f, b) + 4 * dist(e, i);
    if across >= along {
        return None;
    }
    // A shallow line runs from G to F, a steep one from C to H.
    let (shallow_ends, steep_ends) = (dist(f, g), dist(h, c));
    let shallow = 2 * shallow_ends <= steep_ends && e != g && d != g;
    let steep = shallow_ends >= 2 * steep_ends && e != c && b != c;
    let cut = match (shallow, steep) {
        (true, true) => Cut::Both,
        (true, false) => Cut::Shallow,
        (false, true) => Cut::Steep,
        (false, false) => Cut::Diagonal,
    };
    let color = if dist(e, f) <= dist(e, h) { f } else { h };
    Some((cut, color))
}

/// `base` moved towards `over` by `covered` / [`FULL`].
fn blend(base: u32, over: u32, covered: u32) -> u32 {
    let channel = |shift: u32| {
        let (b, o) = ((base >> shift & 0xff) as i32, (over >> shift & 0xff) as i32);
        ((b + (o - b) * covered as i32 / FULL as i32) as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: u32 = 0x000000;
    const WHITE: u32 = 0xffffff;

    fn image(width: u32, rows: &[&[u8]]) -> Image {
        Image {
            width,
            height: rows.len() as u32,
            pixels: rows.concat(),
        }
    }

    fn run(source: &Image, scale: u32) -> Vec<u32> {
        let mut out = vec![0; (source.width * source.height * scale * scale) as usize];
        xbr(source, &[BLACK, WHITE], scale, &mut out);
        out
    }

    #[test]
    fn masks_cover_as_xbr_does() {
        // At 2x the published xBR weights: a 45 degree cut takes half the
        // corner pixel; a shallow one 3/4 of it and 1/4 of its neighbour.
        let masks = Masks::new(2);
        assert_eq!(masks.get(Cut::Diagonal), &[0, 0, 0, 16]);
        assert_eq!(masks.get(Cut::Shallow), &[0, 0, 8, 24]);
        assert_eq!(masks.get(Cut::Steep), &[0, 8, 0, 24]);
    }

    #[test]
    fn flat_areas_and_straight_edges_stay() {
        let flat = image(3, &[&[1, 1, 1], &[1, 1, 1], &[1, 1, 1]]);
        assert!(run(&flat, 3).iter().all(|&p| p == WHITE));
        let edge = image(4, &[&[1, 1, 0, 0], &[1, 1, 0, 0], &[1, 1, 0, 0]]);
        let out = run(&edge, 2);
        for row in out.as_chunks::<8>().0 {
            assert_eq!(
                row,
                &[WHITE, WHITE, WHITE, WHITE, BLACK, BLACK, BLACK, BLACK]
            );
        }
    }

    #[test]
    fn a_single_pixel_stays_square() {
        let dot = image(3, &[&[0, 0, 0], &[0, 1, 0], &[0, 0, 0]]);
        let out = run(&dot, 2);
        let white = out.iter().filter(|&&p| p == WHITE).count();
        assert_eq!(white, 4);
        assert!(out.iter().all(|&p| p == WHITE || p == BLACK));
    }

    #[test]
    fn a_diagonal_is_smoothed_both_ways() {
        // White below a 45 degree staircase.
        let stairs = image(
            5,
            &[
                &[0, 0, 0, 0, 1],
                &[0, 0, 0, 1, 1],
                &[0, 0, 1, 1, 1],
                &[0, 1, 1, 1, 1],
                &[1, 1, 1, 1, 1],
            ],
        );
        let out = run(&stairs, 2);
        let get = |x: usize, y: usize| out[y * 10 + x];
        // The black pixel (1, 2), above the step: its bottom right corner
        // is half grey, the rest black.
        assert_eq!(get(3, 5), 0x7f7f7f);
        assert_eq!((get(2, 4), get(3, 4), get(2, 5)), (BLACK, BLACK, BLACK));
        // The white pixel (2, 2) on the step: its top left corner is grey.
        assert_eq!(get(4, 4), 0x808080);
        // The staircase is the same mirrored over the diagonal, and so is
        // the picture: shallow and steep lines are treated alike.
        for y in 0..10 {
            for x in 0..10 {
                assert_eq!(get(x, y), get(y, x), "at {x}, {y}");
            }
        }
    }
}
