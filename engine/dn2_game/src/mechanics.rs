//! Level-wide mechanics the actors set going: map sections that fall,
//! sink or blow up, the tile debris of an exploded wall, the earthquake
//! and the switch to the alternate backdrop.

use alloc::vec::Vec;

use dn2_assets::level::LevelFlags;
use dn2_assets::tileset::TileAttributes;

use crate::Sound;
use crate::actors::Ctx;
use crate::draw::Draw;
use crate::effects::Motion;
use crate::map::Map;
use crate::messages::{once, text};
use crate::state::LevelState;

/// The dust of a sinking section.
const DUST: u16 = 3;
/// What the screen flashes in for a section blown up by a shot.
const FLASH_COLOR: u8 = 1;

/// Kinds up to this one (but 0) need no update: they are done.
const NO_UPDATE: u16 = 0x62;

/// What a map section does (the marker's variable plus 0x62).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionKind(pub u16);

impl SectionKind {
    /// Sinks into the floor, a row at a time (the marker's variable 0).
    pub const SINK: Self = Self(0);
    /// Has done its work.
    pub const DONE: Self = Self(1);
    /// Blows up when shot.
    pub const SHOOTABLE: Self = Self(0x63);
    pub const FALL_AND_EXPLODE: Self = Self(0x65);
    pub const FALL_THEN_STOP: Self = Self(0x66);
    pub const FALL_THEN_STOP_2: Self = Self(0x67);
    pub const FALL_WHEN_UNSUPPORTED: Self = Self(0x68);
    pub const FALL_AND_EXPLODE_2: Self = Self(0x69);
    pub const FALL_THEN_STOP_3: Self = Self(0x6a);

    /// Falls, then lands on the floor with a jolt.
    fn falls_then_stops(self) -> bool {
        matches!(self, Self::FALL_THEN_STOP | Self::FALL_THEN_STOP_2)
    }

    /// Falls, then blows up.
    fn falls_and_explodes(self) -> bool {
        matches!(self, Self::FALL_AND_EXPLODE | Self::FALL_AND_EXPLODE_2)
    }
}

/// A rectangle of the map the level moves around, in tiles, edges
/// included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSection {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    /// 0, or what kind of section it is plus 0x62.
    pub kind: u16,
}

/// A piece of an exploded wall, flying. Positions are in tiles on the
/// screen, not in the map: the pieces stay with the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileDebris {
    dx: i32,
    /// Step in the table of vertical speeds.
    step: usize,
    /// The map cell it was.
    cell: u16,
    x: i32,
    y: i32,
}

/// How far the picture jolts sideways ([`LevelState::shake`]) when a
/// falling section lands, and while one sinks.
const LANDING_SHAKE: u8 = 7;
const SINKING_SHAKE: u8 = 2;

/// One frame of every section.
pub fn update_map_sections(c: &mut Ctx) {
    for i in 0..c.state.sections.len() {
        let mut s = c.state.sections[i];
        if s.kind != 0 && s.kind <= NO_UPDATE {
            continue;
        }
        update_section(c, &mut s);
        c.state.sections[i] = s;
    }
}

fn update_section(c: &mut Ctx, s: &mut MapSection) {
    let kind = SectionKind(s.kind);
    if kind == SectionKind::SHOOTABLE {
        if player_shot_hits_rect(c, s.bottom + 1, s.right + 2, s.top - 2, s.left - 1) {
            s.kind = SectionKind::DONE.0;
            c.state.flash = Some(FLASH_COLOR);
            explode_map_section(c, s.bottom, s.right, s.top, s.left);
        }
        return;
    }
    let stops = kind.falls_then_stops() || kind == SectionKind::FALL_WHEN_UNSUPPORTED;
    if stops && supported(c, s) {
        return;
    }
    if kind == SectionKind::FALL_WHEN_UNSUPPORTED {
        s.kind = SectionKind::FALL_AND_EXPLODE_2.0;
    }
    // Falls up to two rows a frame, until something is below.
    let mut fell_twice = false;
    while !supported(c, s) {
        shift_down(c.map, *s);
        s.top += 1;
        s.bottom += 1;
        if SectionKind(s.kind).falls_then_stops() && supported(c, s) {
            land(c);
            return;
        }
        if fell_twice {
            return;
        }
        fell_twice = true;
    }
    if s.kind == SectionKind::FALL_THEN_STOP_3.0 {
        s.kind = SectionKind::FALL_THEN_STOP.0;
    }
    let kind = SectionKind(s.kind);
    if kind.falls_then_stops() {
        land(c);
    } else if kind.falls_and_explodes() {
        s.kind = SectionKind::DONE.0;
        explode_map_section(c, s.bottom, s.right, s.top, s.left);
    } else {
        sink(c, s);
    }
}

/// A falling section lands: a sound, and the picture jolts.
fn land(c: &mut Ctx) {
    c.sound(Sound::SectionLand);
    c.state.shake = LANDING_SHAKE;
}

/// Sinks into the floor, a row at a time, until nothing is left.
fn sink(c: &mut Ctx, s: &mut MapSection) {
    if s.kind == SectionKind::SINK.0 {
        c.state.shake = SINKING_SHAKE;
        let width = (s.right - s.left).max(1);
        let dust = i32::from(c.random.next_byte()) % width;
        c.effect(DUST, s.left + dust, s.bottom + 1, Motion::Rise, 0);
        c.sound(Sound::Thud);
    }
    shift_down(
        c.map,
        MapSection {
            bottom: s.bottom - 1,
            ..*s
        },
    );
    let old_top = s.top;
    s.top += 1;
    if old_top == s.bottom {
        for x in s.left..=s.right {
            c.map.set_cell(x, s.bottom, 0);
        }
        c.sound(Sound::SectionLand);
        s.kind = SectionKind::DONE.0;
    }
}

/// Whether the cell below the section's bottom left or bottom right
/// corner has a solid top.
fn supported(c: &Ctx, s: &MapSection) -> bool {
    [s.left, s.right]
        .iter()
        .any(|&x| c.map.has(x, s.bottom + 1, TileAttributes::SOLID_TOP))
}

/// Moves the cells of a rectangle down one row, clearing the cells they leave.
fn shift_down(map: &mut Map, s: MapSection) {
    let mut cells = Vec::new();
    for y in s.top..=s.bottom {
        for x in s.left..=s.right {
            cells.push(map.cell(x, y));
            map.set_cell(x, y, 0);
        }
    }
    let mut cells = cells.into_iter();
    for y in s.top + 1..=s.bottom + 1 {
        for x in s.left..=s.right {
            if let Some(cell) = cells.next() {
                map.set_cell(x, y, cell);
            }
        }
    }
}

/// Whether any player shot overlaps the rectangle; the first one that does is
/// used up (gone at once: no actor can take it any more).
fn player_shot_hits_rect(c: &mut Ctx, bottom: i32, right: i32, top: i32, left: i32) -> bool {
    for i in 0..c.shots.len() {
        let (id, x, y) = (c.shots[i].sprite, c.shots[i].x, c.shots[i].y);
        if sprite_in_rect(c, (bottom, right, top, left), (x, y), id) {
            c.shots.remove(i);
            return true;
        }
    }
    false
}

/// Whether the first frame of a sprite at a position overlaps the rectangle.
fn sprite_in_rect(
    c: &Ctx,
    (bottom, right, top, left): (i32, i32, i32, i32),
    (x, y): (i32, i32),
    sprite: u16,
) -> bool {
    let Some(b) = Map::sprite_body(c.sprites, sprite, 0, x, y) else {
        return false;
    };
    let across = (left <= b.x && b.x < right) || (b.x <= left && left < b.x + b.width);
    let down = (b.y - b.height < bottom && bottom <= b.y) || (top < b.y && b.y <= bottom);
    across && down
}

/// Turns the tiles of a rectangle into flying debris, with a sound, and starts
/// the debris timer.
pub fn explode_map_section(c: &mut Ctx, bottom: i32, right: i32, top: i32, left: i32) {
    c.sound(Sound::BigExplosion);
    c.state.debris_timer = 1;
    c.state.debris.clear();
    let (camera_x, camera_y) = (i32::from(c.camera.0), i32::from(c.camera.1));
    for y in top..=bottom {
        for x in left..=right {
            let cell = c.map.cell(x, y);
            if cell != 0 {
                let dx = 3 - i32::from(c.random.next_byte() % 6);
                let step = usize::from(c.random.next_byte() % 5);
                c.state.debris.push(TileDebris {
                    dx,
                    step,
                    cell,
                    x: x - camera_x,
                    y: y - camera_y,
                });
                c.map.set_cell(x, y, 0);
            }
        }
    }
}

/// Vertical speeds of flying wall pieces by age (the first 11 words of the
/// original's table). The original reads 3 more words past its end: a piece's
/// index times 10, then a huge value that carries it far off the view, then 0.
/// Those are kept, as the pieces fall away at the end.
const DEBRIS_FALL: [i32; 11] = [-3, -3, -2, -2, -1, 0, 0, 1, 2, 2, 3];
/// Frames the debris flies.
const DEBRIS_FRAMES: u8 = 0x50;

/// Moves and draws the pieces of an exploded wall for 80 frames.
pub fn update_tile_debris(state: &mut LevelState, camera: (u16, u16), draws: &mut Vec<Draw>) {
    if state.debris_timer == 0 {
        return;
    }
    for (i, d) in state.debris.iter_mut().enumerate() {
        d.x += d.dx;
        d.y += match d.step {
            n @ 0..=10 => DEBRIS_FALL[n],
            11 => i as i32 * 10,
            12 => 0x27f8,
            _ => 0,
        };
        if d.step < 13 {
            d.step += 1;
        }
        draws.push(Draw::Tile {
            cell: d.cell,
            x: d.x + i32::from(camera.0),
            y: d.y + i32::from(camera.1),
        });
    }
    state.debris_timer += 1;
    if state.debris_timer == DEBRIS_FRAMES {
        state.debris_timer = 0;
    }
}

/// The backdrop and the earthquake, the parts that are not drawing: the
/// earthquake (a random wait, then a stretch of random length during which the
/// picture shakes 1 to 3 pixels sideways or a rumble sounds, and sections may
/// start), and the message when the backdrop switch has been made. The
/// backdrop's own scrolling is the renderer's.
pub fn update_backdrop_and_earthquake(c: &mut Ctx) {
    let flags = c.map.level.flags;
    c.state.shake = 0;
    if flags.contains(LevelFlags::EARTHQUAKE) {
        if c.state.earthquake_going(flags) {
            let byte = c.random.next_byte();
            c.state
                .messages
                .show_once(once::EARTHQUAKE, text::EARTHQUAKE);
            if byte & 3 == 0 {
                c.sound(Sound::Earthquake);
            } else {
                c.state.shake = byte & 3;
            }
        }
        if c.state.quake_timer == 0 {
            c.state.quake_timer = c.random.next_byte();
            c.state.quake_limit = c.random.next_byte() % 0x32;
        }
        c.state.quake_timer = c.state.quake_timer.wrapping_sub(1);
    }
    // After the switch, the backdrop of a level with a half-speed
    // backdrop changes (the renderer's part), and some say a few words.
    if flags.contains(LevelFlags::PARALLAX_HORIZONTAL)
        && flags.contains(LevelFlags::BACKDROP_SWITCH_ONCE)
        && c.state.backdrop_alternate
    {
        if c.state.switch_frames < 0xe {
            c.state.switch_frames += 1;
            if c.draw_page {
                c.sound(Sound::BigExplosion);
            }
        }
        if c.state.switch_frames == 0xd {
            c.state.messages.show(text::DESTROYED_EVERYTHING);
        }
    }
}
