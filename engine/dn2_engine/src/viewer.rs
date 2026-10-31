//! Browses the loaded game data: pictures, backdrops, tilesets, the status
//! sheet, actor sprites and movies. PageUp/PageDown change the category,
//! Left/Right the item, Up/Down the page within an item (masked tiles, more
//! sprite frames).

use core::fmt;

use dn2_assets::GameData;
use dn2_assets::flic::{Flic, FlicPlayer};
use dn2_assets::gfx::Image;
use dn2_assets::images::{Picture, TileSheet};
use dn2_assets::sprites::ActorSprite;
use dn2_assets::tileset::Tileset;
use dn2_render::Screen;

use crate::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    Pictures,
    Backdrops,
    Tilesets,
    Status,
    Sprites,
    Movies,
}

impl Category {
    /// In the order Page Down goes through them.
    const ALL: [Self; 6] = [
        Self::Pictures,
        Self::Backdrops,
        Self::Tilesets,
        Self::Status,
        Self::Sprites,
        Self::Movies,
    ];

    fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        Self::ALL[(self as usize + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// The item shown, with its data.
enum Item<'a> {
    Picture(&'a str, &'a Picture),
    Backdrop(&'a str, &'a TileSheet),
    Tileset(&'a str, &'a Tileset),
    Status,
    Sprite(u16, &'a ActorSprite),
    Movie(&'a str, &'a Flic),
}

impl fmt::Display for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Picture(name, _)
            | Self::Backdrop(name, _)
            | Self::Tileset(name, _)
            | Self::Movie(name, _) => f.write_str(name),
            Self::Status => f.write_str("STATUS.MNI"),
            Self::Sprite(id, _) => write!(f, "{id}"),
        }
    }
}

/// Timer ticks per movie frame (the game sets the speed per movie; this is
/// close enough to look at them).
const MOVIE_TICKS: u32 = 10;

/// Sprite frames per page.
const PAGE_FRAMES: usize = 8;

pub(crate) struct Viewer {
    category: Category,
    item: usize,
    page: usize,
    movie: Option<Movie>,
}

struct Movie {
    player: FlicPlayer,
    next: usize,
    ticks: u32,
}

impl Viewer {
    pub fn new(data: &GameData) -> Self {
        let viewer = Self {
            category: Category::Pictures,
            item: 0,
            page: 0,
            movie: None,
        };
        viewer.announce(data);
        viewer
    }

    /// The number of items in the current category.
    fn item_count(&self, data: &GameData) -> usize {
        match self.category {
            Category::Pictures => data.pictures.len(),
            Category::Backdrops => data.backdrops.len(),
            Category::Tilesets => data.tilesets.len(),
            Category::Status => 1,
            Category::Sprites => sprites(data).count(),
            Category::Movies => data.movies.len(),
        }
    }

    fn item<'a>(&self, data: &'a GameData) -> Option<Item<'a>> {
        let i = self.item;
        match self.category {
            Category::Pictures => data
                .pictures
                .iter()
                .nth(i)
                .map(|(n, p)| Item::Picture(n, p)),
            Category::Backdrops => data
                .backdrops
                .iter()
                .nth(i)
                .map(|(n, b)| Item::Backdrop(n, b)),
            Category::Tilesets => data
                .tilesets
                .iter()
                .nth(i)
                .map(|(n, t)| Item::Tileset(n, t)),
            Category::Status => (i == 0).then_some(Item::Status),
            Category::Sprites => sprites(data).nth(i).map(|(id, s)| Item::Sprite(id, s)),
            Category::Movies => data.movies.iter().nth(i).map(|(n, m)| Item::Movie(n, m)),
        }
    }

    fn announce(&self, data: &GameData) {
        let page = self.page + 1;
        let (number, count) = (self.item + 1, self.item_count(data));
        match self.item(data) {
            Some(item) => log::info!("{:?} {number}/{count}: {item}, page {page}", self.category),
            None => log::info!("{:?} {number}/{count}: (none), page {page}", self.category),
        }
    }

    pub fn key(&mut self, key: Key, data: &GameData) {
        let count = self.item_count(data).max(1);
        match key {
            Key::PageDown => (self.category, self.item, self.page) = (self.category.next(), 0, 0),
            Key::PageUp => (self.category, self.item, self.page) = (self.category.prev(), 0, 0),
            Key::Right => (self.item, self.page) = ((self.item + 1) % count, 0),
            Key::Left => (self.item, self.page) = ((self.item + count - 1) % count, 0),
            Key::Down => self.page += 1,
            Key::Up => self.page = self.page.saturating_sub(1),
            _ => return,
        }
        self.movie = None;
        self.announce(data);
    }

    pub fn update(&mut self, ticks: u32, data: &GameData) {
        let Some(Item::Movie(_, flic)) = self.item(data) else {
            return;
        };
        let movie = self.movie.get_or_insert_with(|| Movie {
            player: FlicPlayer::new(flic),
            next: 0,
            ticks: 0,
        });
        movie.ticks += ticks;
        while movie.ticks >= MOVIE_TICKS {
            movie.ticks -= MOVIE_TICKS;
            movie.player.apply(&flic.frames[movie.next]);
            // The last record returns to the second one, where the loop starts.
            movie.next = if movie.next + 1 < flic.frames.len() {
                movie.next + 1
            } else {
                1
            };
        }
    }

    pub fn draw(&self, data: &GameData, screen: &mut Screen) {
        screen.palette = data.palette.clone();
        screen.clear(0);
        let Some(item) = self.item(data) else {
            return;
        };
        match item {
            Item::Picture(_, picture) => {
                screen.palette = picture.palette.clone();
                screen.draw(&picture.image, 0, 0);
            }
            Item::Backdrop(_, backdrop) => screen.draw(&backdrop.image(), 0, 0),
            Item::Status => screen.draw(&data.status.image(), 0, 0),
            Item::Tileset(_, tileset) => {
                let tiles = if self.page.is_multiple_of(2) {
                    &tileset.solid
                } else {
                    &tileset.masked
                };
                screen.draw(&Image::from_tiles(tiles, TileSheet::COLUMNS), 0, 0);
            }
            Item::Sprite(_, sprite) => draw_frames(
                screen,
                sprite
                    .frames
                    .iter()
                    .map(|f| &f.image)
                    .skip(self.page * PAGE_FRAMES),
            ),
            Item::Movie(..) => {
                if let Some(movie) = &self.movie {
                    screen.palette = movie.player.palette.clone();
                    screen.draw(&movie.player.image, 0, 0);
                }
            }
        }
    }
}

/// The actors that have a sprite, by id.
fn sprites(data: &GameData) -> impl Iterator<Item = (u16, &ActorSprite)> {
    (0..=u16::MAX)
        .take(data.sprites.actors.len())
        .filter_map(|id| Some((id, data.sprites.get(id)?)))
}

/// Lays frames out left to right, wrapping, with a gap, on a solid backing
/// so their outline shows.
fn draw_frames<'a>(screen: &mut Screen, frames: impl Iterator<Item = &'a Image>) {
    /// Pixels around and between the frames.
    const GAP: u32 = 4;
    /// The backing's palette colour.
    const BACKING: u8 = 8;
    let (mut x, mut y, mut row_height) = (GAP, GAP, 0);
    for image in frames {
        if x + image.width > screen.image.width && x > GAP {
            (x, y, row_height) = (GAP, y + row_height + GAP, 0);
        }
        if y >= screen.image.height {
            break;
        }
        let backing = Image::new(image.width, image.height, BACKING);
        let (dx, dy) = (x as i32, y as i32);
        screen.draw(&backing, dx, dy);
        screen.draw(image, dx, dy);
        x += image.width + GAP;
        row_height = row_height.max(image.height);
    }
}
