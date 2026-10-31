//! Making the picture larger: each game pixel becomes `scale` x `scale`
//! frame pixels, as blocks or smoothed with EPX.
//!
//! The filters work on the whole picture of palette indices, not on each
//! tile or sprite: the map's tiles continue across their edges, which a
//! filter on single tiles would show as seams. EPX (also called Scale2x)
//! and its 3x form only copy neighbouring pixels, so the result has the
//! original's colours and nothing is blurred.

use alloc::vec;
use alloc::vec::Vec;

use dn2_assets::gfx::Image;

/// How a picture is made larger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// Every pixel a block.
    #[default]
    Nearest,
    /// Edges smoothed: EPX at scale 2, its 3x form at 3, EPX twice at 4;
    /// other scales fall back to blocks.
    Epx,
}

/// `image` made `scale` times larger.
pub fn upscale(image: &Image, scale: u32, filter: Filter) -> Image {
    match (filter, scale) {
        (_, 0 | 1) => image.clone(),
        (Filter::Epx, 2) => epx2(image),
        (Filter::Epx, 3) => epx3(image),
        (Filter::Epx, 4) => epx2(&epx2(image)),
        _ => nearest(image, scale),
    }
}

fn nearest(image: &Image, scale: u32) -> Image {
    let (width, height) = (image.width * scale, image.height * scale);
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in image.pixels.chunks_exact(image.width as usize) {
        let start = pixels.len();
        for &p in row {
            pixels.extend(core::iter::repeat_n(p, scale as usize));
        }
        for _ in 1..scale {
            pixels.extend_from_within(start..start + width as usize);
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// The 3x3 neighbourhood of a pixel, edges repeated: `[a b c d e f g h i]`
/// with `e` the pixel.
fn neighbours(image: &Image, x: u32, y: u32) -> [u8; 9] {
    let (w, h) = (image.width as i32, image.height as i32);
    let at = |dx: i32, dy: i32| {
        let px = (x as i32 + dx).clamp(0, w - 1);
        let py = (y as i32 + dy).clamp(0, h - 1);
        image.pixels[(py * w + px) as usize]
    };
    [
        at(-1, -1),
        at(0, -1),
        at(1, -1),
        at(-1, 0),
        at(0, 0),
        at(1, 0),
        at(-1, 1),
        at(0, 1),
        at(1, 1),
    ]
}

/// Writes `block` (n x n, row by row) for the pixel at `x`, `y`.
fn put(out: &mut Image, x: u32, y: u32, n: u32, block: &[u8]) {
    for (row, line) in block.chunks_exact(n as usize).enumerate() {
        let start = ((y * n + row as u32) * out.width + x * n) as usize;
        out.pixels[start..start + n as usize].copy_from_slice(line);
    }
}

/// EPX / Scale2x.
fn epx2(image: &Image) -> Image {
    let mut out = Image {
        width: image.width * 2,
        height: image.height * 2,
        pixels: vec![0; image.pixels.len() * 4],
    };
    for y in 0..image.height {
        for x in 0..image.width {
            let [_, b, _, d, e, f, _, h, _] = neighbours(image, x, y);
            let block = if b != h && d != f {
                [
                    if d == b { d } else { e },
                    if b == f { f } else { e },
                    if d == h { d } else { e },
                    if h == f { f } else { e },
                ]
            } else {
                [e; 4]
            };
            put(&mut out, x, y, 2, &block);
        }
    }
    out
}

/// Scale3x, the 3x form of EPX.
fn epx3(image: &Image) -> Image {
    let mut out = Image {
        width: image.width * 3,
        height: image.height * 3,
        pixels: vec![0; image.pixels.len() * 9],
    };
    for y in 0..image.height {
        for x in 0..image.width {
            let [a, b, c, d, e, f, g, h, i] = neighbours(image, x, y);
            let block = if b != h && d != f {
                [
                    if d == b { d } else { e },
                    if (d == b && e != c) || (b == f && e != a) {
                        b
                    } else {
                        e
                    },
                    if b == f { f } else { e },
                    if (d == b && e != g) || (d == h && e != a) {
                        d
                    } else {
                        e
                    },
                    e,
                    if (b == f && e != i) || (h == f && e != c) {
                        f
                    } else {
                        e
                    },
                    if d == h { d } else { e },
                    if (d == h && e != i) || (h == f && e != g) {
                        h
                    } else {
                        e
                    },
                    if h == f { f } else { e },
                ]
            } else {
                [e; 9]
            };
            put(&mut out, x, y, 3, &block);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, rows: &[&[u8]]) -> Image {
        Image {
            width,
            height: rows.len() as u32,
            pixels: rows.concat(),
        }
    }

    #[test]
    fn nearest_makes_blocks() {
        let out = upscale(&image(2, &[&[1, 2]]), 3, Filter::Nearest);
        assert_eq!((out.width, out.height), (6, 3));
        assert_eq!(&out.pixels[..6], &[1, 1, 1, 2, 2, 2]);
        assert_eq!(out.pixels[12..], out.pixels[..6]);
    }

    #[test]
    fn epx_rounds_a_diagonal() {
        // A staircase: the corner pixel (1, 1) takes the colour of its two
        // equal neighbours above and left in its top left quarter.
        let source = image(3, &[&[1, 1, 0], &[1, 0, 0], &[0, 0, 0]]);
        let out = upscale(&source, 2, Filter::Epx);
        assert_eq!(out.get(2, 2), 1);
        assert_eq!((out.get(3, 2), out.get(2, 3), out.get(3, 3)), (0, 0, 0));
        // Flat areas and straight edges stay as blocks.
        assert_eq!(upscale(&source, 2, Filter::Epx).get(0, 0), 1);
        let flat = image(2, &[&[4, 4], &[4, 4]]);
        assert!(
            upscale(&flat, 3, Filter::Epx)
                .pixels
                .iter()
                .all(|&p| p == 4)
        );
        let edge = image(2, &[&[1, 0], &[1, 0]]);
        assert_eq!(
            upscale(&edge, 2, Filter::Epx),
            upscale(&edge, 2, Filter::Nearest)
        );
    }

    #[test]
    fn epx3_rounds_a_diagonal() {
        let source = image(3, &[&[1, 1, 0], &[1, 0, 0], &[0, 0, 0]]);
        let out = upscale(&source, 3, Filter::Epx);
        assert_eq!((out.width, out.height), (9, 9));
        // Corner and the two edge pixels next to it.
        assert_eq!((out.get(3, 3), out.get(4, 3), out.get(3, 4)), (1, 0, 0));
        assert_eq!(out.get(4, 4), 0);
    }

    #[test]
    fn epx4_is_epx2_twice() {
        let source = image(3, &[&[1, 1, 0], &[1, 0, 0], &[0, 0, 0]]);
        let out = upscale(&source, 4, Filter::Epx);
        assert_eq!(out, epx2(&epx2(&source)));
        assert_eq!(upscale(&source, 5, Filter::Epx), nearest(&source, 5));
    }
}
