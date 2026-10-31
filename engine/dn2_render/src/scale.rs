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
    /// Edges smoothed and blended, after xBR ([`crate::xbr`]): lines get
    /// soft edges and new colours, at any scale from 2. [`upscale`] works
    /// on palette indices and so makes blocks; [`Screen::present`] uses it.
    ///
    /// [`Screen::present`]: crate::Screen::present
    Xbr,
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

/// The palette index at `x`, `y`, with the edges repeated: a clamped read,
/// so a neighbour past the edge is the edge pixel.
pub(crate) fn pixel_at(image: &Image, x: i32, y: i32) -> u8 {
    let (w, h) = (image.width as i32, image.height as i32);
    image.pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
}

/// The 3x3 neighbourhood of a pixel, edges repeated: `[a b c d e f g h i]`
/// with `e` the pixel.
fn neighbours(image: &Image, x: u32, y: u32) -> [u8; 9] {
    let at = |dx: i32, dy: i32| pixel_at(image, x as i32 + dx, y as i32 + dy);
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

/// Writes `block` (n x n, row by row) for the pixel at `x`, `y` of `out`,
/// which is `width` pixels wide.
pub(crate) fn put_block<T: Copy>(out: &mut [T], width: u32, x: u32, y: u32, n: u32, block: &[T]) {
    for (row, line) in block.chunks_exact(n as usize).enumerate() {
        let start = ((y * n + row as u32) * width + x * n) as usize;
        out[start..start + n as usize].copy_from_slice(line);
    }
}

/// `a` if `cond`, else `e`: one of an EPX rule's choices.
fn pick(cond: bool, a: u8, e: u8) -> u8 {
    if cond { a } else { e }
}

/// EPX at `scale`: each pixel's block of `N` (`scale` squared) comes from
/// its 3x3 neighbourhood by `rule`.
fn epx<const N: usize>(image: &Image, scale: u32, rule: impl Fn([u8; 9]) -> [u8; N]) -> Image {
    let mut out = Image {
        width: image.width * scale,
        height: image.height * scale,
        pixels: vec![0; image.pixels.len() * N],
    };
    for y in 0..image.height {
        for x in 0..image.width {
            let block = rule(neighbours(image, x, y));
            put_block(&mut out.pixels, out.width, x, y, scale, &block);
        }
    }
    out
}

/// EPX / Scale2x.
fn epx2(image: &Image) -> Image {
    epx(image, 2, |[_, b, _, d, e, f, _, h, _]| {
        if b != h && d != f {
            [
                pick(d == b, d, e),
                pick(b == f, f, e),
                pick(d == h, d, e),
                pick(h == f, f, e),
            ]
        } else {
            [e; 4]
        }
    })
}

/// Scale3x, the 3x form of EPX.
fn epx3(image: &Image) -> Image {
    epx(image, 3, |[a, b, c, d, e, f, g, h, i]| {
        if b != h && d != f {
            [
                pick(d == b, d, e),
                pick((d == b && e != c) || (b == f && e != a), b, e),
                pick(b == f, f, e),
                pick((d == b && e != g) || (d == h && e != a), d, e),
                e,
                pick((b == f && e != i) || (h == f && e != c), f, e),
                pick(d == h, d, e),
                pick((d == h && e != i) || (h == f && e != g), h, e),
                pick(h == f, f, e),
            ]
        } else {
            [e; 9]
        }
    })
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
