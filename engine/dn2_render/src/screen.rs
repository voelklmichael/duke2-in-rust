use dn2_assets::gfx::{Image, Palette, Rgb, RgbaImage, TRANSPARENT};

use alloc::vec::Vec;
use core::ops::Range;

use crate::FrameBuffer;
use crate::scale::{Filter, upscale};
use crate::xbr::xbr;

/// A picture of palette indices with its palette: what the original had in
/// video memory and the DAC.
pub struct Screen {
    pub image: Image,
    pub palette: Palette,
    /// Brightness of the palette when presented: 0 (black) to
    /// [`Screen::FADE_STEPS`] (as it is). The original fades by setting the DAC
    /// step by step (fading in: 15 steps from black, step k showing colour * k
    /// / 16; fading out: 16 steps down to black), so a palette is never
    /// brighter than step 15 of 16, which `Palette::parse_ega` already gives;
    /// the steps here are 0..=15 of that.
    pub fade: u8,
    /// Where the left edge of the original's 320-pixel picture is, on a
    /// widescreen: [`Self::draw`] and [`Self::put`] add it.
    /// The game's view and status bar use the whole width and set it to 0
    /// while they draw.
    pub left: i32,
    /// Higher-detail pixels over the picture, if switched on
    /// ([`Self::set_hd_scale`]).
    hd: Option<Overlay>,
}

/// True-colour pixels at `scale` times the screen's size, over the
/// picture of palette indices: where a replacement picture was drawn.
/// Every pixel of the picture written without one clears its block, so
/// whatever is drawn last shows, as on the original.
struct Overlay {
    scale: u32,
    /// Row by row: [`OPAQUE`] plus 0xRRGGBB, or 0 where the picture shows.
    pixels: Vec<u32>,
}

/// The higher-detail picture of one screen pixel: the overlay's scale
/// squared pixels of `image` with the top left at `x`, `y`.
#[derive(Clone, Copy, Debug)]
pub struct HdBlock<'a> {
    pub image: &'a RgbaImage,
    pub x: u32,
    pub y: u32,
    /// Every opaque pixel this colour (0xRRGGBB) instead: the hit flash.
    pub color: Option<u32>,
}

impl<'a> HdBlock<'a> {
    /// The block with its top left at `x`, `y` of `image`, in its colours.
    pub(crate) fn at(image: &'a RgbaImage, x: u32, y: u32) -> Self {
        Self {
            image,
            x,
            y,
            color: None,
        }
    }
}

/// The higher-detail picture of something drawn: its pixel `px`, `py` is
/// the block at `x + px * scale`, `y + py * scale` of `image`.
#[derive(Clone, Copy, Debug)]
pub struct HdPicture<'a> {
    pub image: &'a RgbaImage,
    pub x: u32,
    pub y: u32,
}

impl<'a> HdPicture<'a> {
    /// The whole of `image`.
    pub fn new(image: &'a RgbaImage) -> Self {
        Self { image, x: 0, y: 0 }
    }

    pub(crate) fn block(self, scale: u32, px: u32, py: u32) -> HdBlock<'a> {
        HdBlock::at(self.image, self.x + px * scale, self.y + py * scale)
    }
}

/// Marks a pixel of the overlay as set.
const OPAQUE: u32 = 0xff00_0000;

/// Width of the original's screen.
pub const ORIGINAL_WIDTH: u32 = 320;

impl Screen {
    /// A black screen. Wider than 320 pixels it is a widescreen one: the
    /// original's 320x200 picture sits in the middle (see [`Self::left`]).
    pub fn new(width: u32, height: u32, palette: Palette) -> Self {
        Self {
            image: Image::new(width, height, 0),
            palette,
            fade: Self::FADE_STEPS,
            left: (width.saturating_sub(ORIGINAL_WIDTH) / 2) as i32,
            hd: None,
        }
    }

    /// Switches the higher-detail overlay on at `scale` (it shows only when
    /// presented at that scale), or off with `None` or a scale below 2.
    /// The overlay starts empty.
    pub fn set_hd_scale(&mut self, scale: Option<u32>) {
        self.hd = scale.filter(|&s| s >= 2).map(|scale| Overlay {
            scale,
            pixels: alloc::vec![0; (self.image.width * scale * self.image.height * scale) as usize],
        });
    }

    /// The overlay's scale, if it is on.
    pub fn hd_scale(&self) -> Option<u32> {
        self.hd.as_ref().map(|hd| hd.scale)
    }

    /// The brightness of an unfaded screen.
    pub const FADE_STEPS: u8 = 15;

    /// Sets the pixel at `x`, `y` (without [`Self::left`]) to `f(old)`, if it
    /// is on the screen. The pixel's block of the overlay is cleared: it
    /// shows the new pixel.
    pub fn plot_at(&mut self, x: i32, y: i32, f: impl FnOnce(u8) -> u8) {
        let image = &mut self.image;
        if on_screen(image, x, y) {
            let (x, y) = (x as u32, y as u32);
            let pixel = &mut image.pixels[(y * image.width + x) as usize];
            *pixel = f(*pixel);
            if let Some(hd) = &mut self.hd {
                let width = image.width * hd.scale;
                for row in y * hd.scale..(y + 1) * hd.scale {
                    let start = (row * width + x * hd.scale) as usize;
                    hd.pixels[start..start + hd.scale as usize].fill(0);
                }
            }
        }
    }

    /// Draws a pixel at `x`, `y` (without [`Self::left`]) with a
    /// higher-detail picture of it: `pixel` goes to the picture of palette
    /// indices (`None` keeps what is there, a transparent pixel of the
    /// original), `block` to the overlay. Pixels of the block with an alpha
    /// below one half are see-through: they show the new `pixel`, or with
    /// `None` what was there. Without the overlay, or with a block outside
    /// its picture, only `pixel` is drawn.
    pub fn plot_hd(&mut self, x: i32, y: i32, pixel: Option<u8>, block: HdBlock) {
        if let Some(pixel) = pixel {
            self.plot_at(x, y, |_| pixel);
        }
        let image = &self.image;
        let Some(hd) = &mut self.hd else {
            return;
        };
        let n = hd.scale;
        if !fits(&block, n) || !on_screen(image, x, y) {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        let width = image.width * n;
        for row in 0..n {
            let line = &block.image.pixels[row_span(block.image.width, block.x, block.y + row, n)];
            let out_row = &mut hd.pixels[row_span(width, x * n, y * n + row, n)];
            for (out, &p) in out_row.iter_mut().zip(line) {
                if p >> 24 >= 0x80 {
                    *out = OPAQUE | (block.color.unwrap_or(p) & 0xff_ffff);
                }
            }
        }
    }

    /// Draws a pixel of a one-colour glyph at `x`, `y` (without
    /// [`Self::left`]) with a higher-detail picture of its ink: `pixel`
    /// goes to the picture of palette indices, and the overlay's block is
    /// filled. Pixels of `ink` with an alpha of at least one half show their
    /// colour times colour `colour` of the palette (white ink shows that
    /// colour); the others show colour `under`, or what was there if that
    /// is the pixel that was there. Without the overlay, or with a block
    /// outside its picture, only `pixel` is drawn.
    pub fn plot_ink(&mut self, x: i32, y: i32, pixel: u8, ink: HdBlock, colour: u8, under: u8) {
        let image = &mut self.image;
        let Some(hd) = &mut self.hd else {
            return self.plot_at(x, y, |_| pixel);
        };
        let n = hd.scale;
        if !fits(&ink, n) || !on_screen(image, x, y) {
            return self.plot_at(x, y, |_| pixel);
        }
        let (x, y) = (x as u32, y as u32);
        let old = core::mem::replace(&mut image.pixels[(y * image.width + x) as usize], pixel);
        let tint = self.palette.get(colour);
        let tinted = |p: u32| {
            let channel =
                |shift: u32, c: u8| (((p >> shift) & 0xff) * u32::from(c) / 0xff) << shift;
            OPAQUE | channel(16, tint.r) | channel(8, tint.g) | channel(0, tint.b)
        };
        let background = OPAQUE | self.palette.get(under).to_u32();
        let width = image.width * n;
        for row in 0..n {
            let line = &ink.image.pixels[row_span(ink.image.width, ink.x, ink.y + row, n)];
            let out_row = &mut hd.pixels[row_span(width, x * n, y * n + row, n)];
            for (out, &p) in out_row.iter_mut().zip(line) {
                *out = if p >> 24 >= 0x80 {
                    tinted(p)
                } else if under == old && *out != 0 {
                    *out
                } else if under == pixel {
                    0
                } else {
                    background
                };
            }
        }
    }

    /// Sets the pixel at `x`, `y` (plus [`Self::left`]), if on the screen.
    pub fn put(&mut self, x: i32, y: i32, pixel: u8) {
        self.plot_at(x + self.left, y, |_| pixel);
    }

    /// The pixel at `x`, `y` (plus [`Self::left`]), if on the screen.
    pub fn pixel(&self, x: i32, y: i32) -> Option<u8> {
        let (x, image) = (x + self.left, &self.image);
        on_screen(image, x, y).then(|| image.get(x as u32, y as u32))
    }

    /// Copies the pixel at `sx`, `sy` of `from` (without its
    /// [`Self::left`]) to `x`, `y` (plus [`Self::left`]), with its block of
    /// the overlay if both have one at the same scale.
    pub fn copy_pixel(&mut self, from: &Screen, sx: u32, sy: u32, x: i32, y: i32) {
        let pixel = from.image.get(sx, sy);
        let x = x + self.left;
        self.plot_at(x, y, |_| pixel);
        let (Some(hd), Some(source)) = (&mut self.hd, &from.hd) else {
            return;
        };
        let image = &self.image;
        if hd.scale != source.scale || !on_screen(image, x, y) {
            return;
        }
        let n = hd.scale;
        let (width, source_width) = (image.width * n, from.image.width * n);
        for row in 0..n {
            let dest = row_span(width, x as u32 * n, y as u32 * n + row, n);
            let src = row_span(source_width, sx * n, sy * n + row, n);
            hd.pixels[dest].copy_from_slice(&source.pixels[src]);
        }
    }

    /// The EGA's horizontal pixel panning (the earthquake's shake): every row
    /// starts `pixels` later, so the picture moves left and the right edge
    /// shows the start of the next row (black below the last).
    pub fn pan(&mut self, pixels: u32) {
        let pixels = self.image.pixels.len().min(pixels as usize);
        if pixels == 0 {
            return;
        }
        self.image.pixels.copy_within(pixels.., 0);
        let len = self.image.pixels.len();
        self.image.pixels[len - pixels..].fill(0);
        let Some(hd) = &mut self.hd else {
            return;
        };
        // Each scaled pixel takes the one its screen pixel moved from; that
        // one comes later in the buffer, so going forward reads only pixels
        // not yet written.
        let (w, n) = (self.image.width as usize, hd.scale as usize);
        for row in 0..self.image.height as usize * n {
            let (y, sub) = (row / n, row % n);
            for col in 0..w * n {
                let source = y * w + col / n + pixels;
                let (sx, sy) = (source % w, source / w);
                hd.pixels[row * w * n + col] = if source < len {
                    hd.pixels[(sy * n + sub) * w * n + sx * n + col % n]
                } else {
                    0
                };
            }
        }
    }

    /// The palette as presented: [`Self::fade`] applied.
    pub fn faded_palette(&self) -> Palette {
        let level = self.fade.min(Self::FADE_STEPS);
        Palette(
            self.palette
                .0
                .iter()
                .map(|c| Rgb {
                    r: dim(c.r, level),
                    g: dim(c.g, level),
                    b: dim(c.b, level),
                })
                .collect(),
        )
    }

    pub fn clear(&mut self, index: u8) {
        self.image.pixels.fill(index);
        if let Some(hd) = &mut self.hd {
            hd.pixels.fill(0);
        }
    }

    /// Copies `image` with its top left at `x`, `y` (plus [`Self::left`]),
    /// leaving out transparent pixels and whatever falls outside the screen.
    pub fn draw(&mut self, image: &Image, x: i32, y: i32) {
        for sy in 0..image.height as i32 {
            let dy = y + sy;
            if !(0..self.image.height as i32).contains(&dy) {
                continue;
            }
            for sx in 0..image.width as i32 {
                let dx = x + self.left + sx;
                let pixel = image.get(sx as u32, sy as u32);
                if pixel != TRANSPARENT {
                    self.plot_at(dx, dy, |_| pixel);
                }
            }
        }
    }

    /// [`Self::draw`] with a higher-detail picture of `image`; without
    /// one, or without the overlay, as [`Self::draw`].
    pub fn draw_hd(&mut self, image: &Image, hd: Option<HdPicture>, x: i32, y: i32) {
        let (Some(hd), Some(scale)) = (hd, self.hd_scale()) else {
            return self.draw(image, x, y);
        };
        for sy in 0..image.height {
            for sx in 0..image.width {
                let pixel = image.get(sx, sy);
                let block = hd.block(scale, sx, sy);
                let (dx, dy) = (x + self.left + sx as i32, y + sy as i32);
                self.plot_hd(dx, dy, (pixel != TRANSPARENT).then_some(pixel), block);
            }
        }
    }

    /// Shows a full-screen picture: the screen is cleared and `image`
    /// copied as it is (all 256 colours, none transparent) with its top
    /// left at 0, 0 (plus [`Self::left`]).
    pub fn show(&mut self, image: &Image) {
        self.show_hd(image, None);
    }

    /// [`Self::show`] with a higher-detail picture of `image`, if `hd` is
    /// given and the overlay is on; otherwise as [`Self::show`].
    pub fn show_hd(&mut self, image: &Image, hd: Option<&RgbaImage>) {
        self.clear(0);
        let hd = hd
            .zip(self.hd_scale())
            .map(|(hd, scale)| (HdPicture::new(hd), scale));
        for y in 0..image.height.min(self.image.height) {
            for x in 0..image.width {
                let pixel = image.get(x, y);
                match hd {
                    Some((hd, scale)) => {
                        let block = hd.block(scale, x, y);
                        self.plot_hd(x as i32 + self.left, y as i32, Some(pixel), block);
                    }
                    None => self.put(x as i32, y as i32, pixel),
                }
            }
        }
    }

    /// Converts to RGB, each pixel `scale` x `scale` frame pixels made
    /// with `filter`; `frame` is resized to fit.
    pub fn present(&self, frame: &mut FrameBuffer, scale: u32, filter: Filter) {
        let scale = scale.max(1);
        let palette = self.faded_palette();
        let colors: Vec<u32> = (0..=255).map(|i| palette.get(i).to_u32()).collect();
        if filter == Filter::Xbr && scale > 1 {
            // It blends colours: from the indices straight to RGB.
            frame.resize(self.image.width * scale, self.image.height * scale);
            xbr(&self.image, &colors, scale, &mut frame.pixels);
        } else {
            let large;
            let image = if scale == 1 {
                &self.image
            } else {
                large = upscale(&self.image, scale, filter);
                &large
            };
            frame.resize(image.width, image.height);
            for (pixel, &index) in frame.pixels.iter_mut().zip(&image.pixels) {
                *pixel = colors[usize::from(index)];
            }
        }
        let Some(hd) = self.hd.as_ref().filter(|hd| hd.scale == scale) else {
            return;
        };
        let level = self.fade.min(Self::FADE_STEPS);
        for (pixel, &p) in frame.pixels.iter_mut().zip(&hd.pixels) {
            if p != 0 {
                let channel = |shift: u32| u32::from(dim((p >> shift) as u8, level)) << shift;
                *pixel = channel(16) | channel(8) | channel(0);
            }
        }
    }
}

/// `v` at brightness `level` (0 to [`Screen::FADE_STEPS`]).
fn dim(v: u8, level: u8) -> u8 {
    (u16::from(v) * u16::from(level) / u16::from(Screen::FADE_STEPS)) as u8
}

/// Whether `x`, `y` is a pixel of `image`.
fn on_screen(image: &Image, x: i32, y: i32) -> bool {
    (0..image.width as i32).contains(&x) && (0..image.height as i32).contains(&y)
}

/// Whether the `n` by `n` pixels of `block` are inside its picture.
fn fits(block: &HdBlock<'_>, n: u32) -> bool {
    block.x + n <= block.image.width && block.y + n <= block.image.height
}

/// The `len` pixels from `x`, `y` of a picture `width` pixels wide, as
/// indices into its pixels.
fn row_span(width: u32, x: u32, y: u32, len: u32) -> Range<usize> {
    let start = (y * width + x) as usize;
    start..start + len as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Screen {
        let color = Rgb {
            r: 240,
            g: 120,
            b: 60,
        };
        Screen::new(320, 200, Palette(alloc::vec![color; 16]))
    }

    #[test]
    fn panning_moves_rows_left_into_the_next() {
        let mut s = screen();
        for (i, p) in s.image.pixels.iter_mut().enumerate() {
            *p = (i % 320 / 8) as u8;
        }
        s.image.pixels[320] = 9;
        s.pan(3);
        assert_eq!(s.image.pixels[0], 0);
        assert_eq!(s.image.pixels[5], 1);
        assert_eq!(s.image.pixels[317], 9);
        assert_eq!(*s.image.pixels.last().unwrap(), 0);
    }

    #[test]
    fn fade_levels() {
        let mut s = screen();
        let mut frame = FrameBuffer::new(0, 0);
        s.fade = 0;
        s.present(&mut frame, 1, Filter::Nearest);
        assert_eq!(frame.pixels[0], 0);
        s.fade = Screen::FADE_STEPS;
        s.present(&mut frame, 2, Filter::Nearest);
        assert_eq!((frame.width, frame.pixels[0]), (640, 0xf0783c));
        s.fade = 5;
        assert_eq!(
            s.faded_palette().get(0),
            Rgb {
                r: 80,
                g: 40,
                b: 20
            }
        );
    }

    const RED: u32 = 0xffff_0000;
    const CLEAR: u32 = 0x40ff_ffff;

    fn plot_hd(s: &mut Screen, x: i32, y: i32, pixel: Option<u8>, block: [u32; 4]) {
        let image = RgbaImage {
            width: 2,
            height: 2,
            pixels: block.to_vec(),
        };
        let block = HdBlock {
            image: &image,
            x: 0,
            y: 0,
            color: None,
        };
        s.plot_hd(x, y, pixel, block);
    }

    fn presented(s: &Screen, scale: u32) -> FrameBuffer {
        let mut frame = FrameBuffer::new(0, 0);
        s.present(&mut frame, scale, Filter::Nearest);
        frame
    }

    #[test]
    fn hd_block_shows_at_its_scale_only() {
        let mut s = screen();
        s.set_hd_scale(Some(2));
        plot_hd(&mut s, 1, 0, Some(0), [RED, CLEAR, RED, RED]);
        let frame = presented(&s, 2);
        let at = |x: usize, y: usize| frame.pixels[y * 640 + x];
        assert_eq!(
            (at(2, 0), at(3, 0), at(2, 1), at(3, 1)),
            (0xff0000, 0xf0783c, 0xff0000, 0xff0000)
        );
        assert_eq!(at(0, 0), 0xf0783c);
        assert_eq!(presented(&s, 3).pixels[3], 0xf0783c);
        s.fade = 5;
        assert_eq!(presented(&s, 2).pixels[2], 0x550000);
    }

    #[test]
    fn plain_pixels_cover_the_overlay() {
        let mut s = screen();
        s.set_hd_scale(Some(2));
        plot_hd(&mut s, 0, 0, Some(0), [RED; 4]);
        plot_hd(&mut s, 0, 0, None, [CLEAR, 0xff00_ff00, CLEAR, CLEAR]);
        assert_eq!(presented(&s, 2).pixels[..2], [0xff0000, 0x00ff00]);
        s.put(0, 0, 0);
        assert_eq!(presented(&s, 2).pixels[..2], [0xf0783c; 2]);
        plot_hd(&mut s, 0, 0, Some(0), [RED; 4]);
        s.clear(0);
        assert_eq!(presented(&s, 2).pixels[0], 0xf0783c);
    }

    #[test]
    fn panning_moves_the_overlay() {
        let mut s = screen();
        s.set_hd_scale(Some(2));
        plot_hd(&mut s, 4, 0, Some(0), [RED, CLEAR, CLEAR, RED]);
        plot_hd(&mut s, 1, 1, Some(0), [RED; 4]);
        s.pan(3);
        let frame = presented(&s, 2);
        let at = |x: usize, y: usize| frame.pixels[y * 640 + x] == 0xff0000;
        assert!(at(2, 0) && !at(3, 0) && !at(2, 1) && at(3, 1));
        assert!(!at(8, 0));
        // Pixel (1, 1) came from the start of row 1 to the end of row 0.
        assert!(at(636, 0) && at(637, 1));
    }

    #[test]
    fn pictures_with_hd() {
        let mut s = screen();
        s.set_hd_scale(Some(2));
        // A 2x1 image, its picture the right half of a 6x2 one.
        let picture = RgbaImage {
            width: 6,
            height: 2,
            pixels: alloc::vec![
                0,
                0,
                RED,
                CLEAR,
                0xff00_ff00,
                0xff00_ff00,
                0,
                0,
                RED,
                RED,
                CLEAR,
                RED
            ],
        };
        let hd = HdPicture {
            image: &picture,
            x: 2,
            y: 0,
        };
        let image = Image {
            width: 2,
            height: 1,
            pixels: alloc::vec![1, TRANSPARENT],
        };
        s.left = 1;
        s.draw_hd(&image, Some(hd), 0, 0);
        let frame = presented(&s, 2);
        let at = |x: usize, y: usize| frame.pixels[y * 640 + x];
        assert_eq!(
            (at(2, 0), at(3, 0), at(3, 1)),
            (0xff0000, 0xf0783c, 0xff0000)
        );
        // The transparent pixel's picture shows too.
        assert_eq!((at(4, 0), at(5, 1)), (0x00ff00, 0xff0000));
        assert_eq!(s.image.pixels[2], 0);
        // Full-screen: cleared first, then shown with the picture.
        s.left = 0;
        let full = RgbaImage {
            width: 640,
            height: 400,
            pixels: alloc::vec![0xff00_00ff; 640 * 400],
        };
        s.show_hd(&Image::new(320, 200, 2), Some(&full));
        assert!(presented(&s, 2).pixels.iter().all(|&p| p == 0x0000ff));
        assert_eq!(s.image.pixels[0], 2);
    }

    #[test]
    fn copied_pixels_bring_their_block() {
        let mut from = screen();
        from.set_hd_scale(Some(2));
        plot_hd(&mut from, 0, 0, Some(1), [RED; 4]);
        let mut to = screen();
        to.set_hd_scale(Some(2));
        to.copy_pixel(&from, 0, 0, 5, 1);
        to.copy_pixel(&from, 1, 0, 6, 1);
        let frame = presented(&to, 2);
        assert_eq!(frame.pixels[2 * 640 + 10], 0xff0000);
        assert_eq!(frame.pixels[2 * 640 + 12], 0xf0783c);
        assert_eq!(to.image.get(5, 1), 1);
    }

    #[test]
    fn ink_is_tinted_over_what_it_leaves() {
        let mut s = screen();
        s.palette.0[2] = Rgb {
            r: 0,
            g: 200,
            b: 100,
        };
        s.set_hd_scale(Some(2));
        let ink = RgbaImage {
            width: 2,
            height: 2,
            pixels: alloc::vec![0xff80_8080, CLEAR, CLEAR, 0xffff_ffff],
        };
        let block = HdBlock {
            image: &ink,
            x: 0,
            y: 0,
            color: None,
        };
        // Over a picture that stays: the ink tinted, the rest the picture.
        plot_hd(&mut s, 0, 0, Some(0), [RED; 4]);
        s.plot_ink(0, 0, 2, block, 2, 0);
        let at = |f: &FrameBuffer, x: usize, y: usize| f.pixels[y * 640 + x];
        let frame = presented(&s, 2);
        assert_eq!(
            (at(&frame, 0, 0), at(&frame, 1, 0), at(&frame, 1, 1)),
            (0x006432, 0xff0000, 0x00c864)
        );
        assert_eq!(s.image.pixels[0], 2);
        // The background changed: the rest shows its colour.
        s.plot_ink(0, 0, 2, block, 2, 1);
        assert_eq!(at(&presented(&s, 2), 1, 0), 0xf0783c);
        // The background is the new pixel: the rest shows the plain pixel.
        s.plot_ink(1, 0, 2, block, 2, 2);
        assert_eq!(at(&presented(&s, 2), 3, 0), 0x00c864);
    }

    #[test]
    fn hd_needs_the_overlay_on() {
        let mut s = screen();
        plot_hd(&mut s, 0, 0, Some(3), [RED; 4]);
        assert_eq!(s.image.pixels[0], 3);
        s.set_hd_scale(Some(1));
        assert_eq!(s.hd_scale(), None);
    }
}
