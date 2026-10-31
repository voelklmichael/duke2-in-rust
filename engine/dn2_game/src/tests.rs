//! Tests on tiny hand-built levels. The sprites only have the frame sizes
//! of the real ones (tiles wide, high), no pixels.

extern crate std;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use dn2_assets::gfx::Image;
use dn2_assets::level::{Level, LevelActor, LevelFlags};
use dn2_assets::sprites::{ActorSprite, SpriteFrame, Sprites};
use dn2_assets::tileset::{TileAttributes as A, Tileset};

use crate::map::{Body, Dir, Hit, Mover};
use crate::player::{Facing, PlayerState, frame};
use crate::{Buttons, GameEvent, Sound, World};

const W: usize = 48;
const H: usize = 30;
const SOLID: u16 = 1;
const FLOOR_ONLY: u16 = 2;
const PIPE: u16 = 3;
const LADDER: u16 = 4;
const BELT_LEFT: u16 = 5;
const BELT_RIGHT: u16 = 6;
/// A masked tile that is a ladder.
const MASKED_LADDER_TILE: u16 = 3;

/// The player's frame sizes, tiles wide and high.
const PLAYER_FRAMES: [(u32, u32); 39] = [
    (4, 5),
    (4, 5),
    (4, 5),
    (4, 5),
    (4, 5),
    (4, 4),
    (4, 5),
    (4, 5),
    (4, 5),
    (4, 4),
    (4, 4),
    (4, 4),
    (4, 4),
    (4, 4),
    (4, 4),
    (4, 4),
    (3, 6),
    (4, 4),
    (4, 5),
    (3, 6),
    (4, 6),
    (3, 6),
    (3, 6),
    (3, 6),
    (3, 6),
    (3, 6),
    (3, 6),
    (4, 6),
    (4, 4),
    (4, 5),
    (5, 4),
    (5, 3),
    (5, 2),
    (3, 5),
    (4, 4),
    (4, 5),
    (4, 5),
    (3, 5),
    (3, 5),
];

fn sprite(frames: &[(u32, u32, i16, i16)]) -> Option<ActorSprite> {
    Some(ActorSprite {
        draw_pass: 0,
        frames: frames
            .iter()
            .map(|&(w, h, x_offset, y_offset)| SpriteFrame {
                x_offset,
                y_offset,
                image: Image::new(w * 8, h * 8, 0),
            })
            .collect(),
    })
}

fn sprites() -> Sprites {
    let mut actors = vec![None; 300];
    let player: Vec<_> = PLAYER_FRAMES.iter().map(|&(w, h)| (w, h, 0, 0)).collect();
    actors[5] = sprite(&player);
    actors[6] = sprite(&player);
    actors[2] = sprite(&[(6, 5, 0, 0); 8]);
    actors[3] = sprite(&[(3, 3, 0, 0); 7]);
    actors[0x0b] = sprite(&[(1, 1, 0, 0); 4]);
    for (id, w, h) in [(7, 1, 4), (8, 1, 4), (9, 4, 1), (10, 4, 1)] {
        actors[id] = sprite(&[(w, h, 0, 0)]);
    }
    // Machines: the vertical door's pieces, the hatch, the exit.
    actors[0x80] = sprite(&[(1, 1, 0, 0); 9]);
    actors[0x84] = sprite(&[(5, 1, 0, 0); 3]);
    actors[139] = sprite(&[(1, 1, 0, 0)]);
    actors[21] = sprite(&[(5, 6, -1, 0)]);
    actors[24] = sprite(&[(5, 1, 0, 0)]);
    actors[25] = sprite(&[(1, 5, 0, 0)]);
    actors[26] = sprite(&[(2, 1, 0, 0)]);
    actors[27] = sprite(&[(1, 2, 0, 0)]);
    for id in [33, 34, 35, 36] {
        actors[id] = sprite(&[(3, 2, 0, 0)]);
    }
    for id in [204, 205, 206] {
        actors[id] = sprite(&[(5, 6, 0, 0)]);
    }
    actors[81] = sprite(&[(2, 3, 0, 0); 6]);
    actors[130] = sprite(&[(5, 2, 0, 0); 4]);
    actors[219] = sprite(&[(2, 2, 0, 0); 2]);
    actors[0x104] = sprite(&[(5, 10, 0, -2); 4]);
    actors[51] = sprite(&[(4, 2, 0, 0); 4]);
    // The ship facing right and left, its shot.
    actors[87] = sprite(&[(8, 5, 0, 0)]);
    actors[88] = sprite(&[(8, 5, 0, 0)]);
    actors[0x5b] = sprite(&[(3, 1, 0, -1); 4]);
    // Ids without sprites spawn in the first pass after all.
    let passes = vec![0; actors.len()];
    Sprites::new(actors, passes)
}

/// A level of `W` x `H` tiles being built.
struct Grid {
    cells: Vec<u16>,
    start: (u16, u16),
    height: u16,
}

impl Grid {
    /// With a solid floor in row 20; the player starts on it at `x`.
    fn new(x: u16) -> Self {
        let mut grid = Self {
            cells: vec![0; 32750],
            start: (x, 19),
            height: H as u16,
        };
        grid.rect(0, 20, W as u16 - 1, 21, SOLID);
        grid
    }

    /// Fills the tiles from `x0`, `y0` to `x1`, `y1` including.
    fn rect(&mut self, x0: u16, y0: u16, x1: u16, y1: u16, tile: u16) -> &mut Self {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells[usize::from(y) * W + usize::from(x)] = tile * 8;
            }
        }
        self
    }

    fn masked_ladder(&mut self, x: u16, y0: u16, y1: u16) -> &mut Self {
        for y in y0..=y1 {
            self.cells[usize::from(y) * W + usize::from(x)] = 8000 + 40 * MASKED_LADDER_TILE;
        }
        self
    }

    fn at(&mut self, x: u16, y: u16) -> &mut Self {
        self.start = (x, y);
        self
    }

    fn world(&self) -> World {
        let level = Level {
            tileset: String::new(),
            backdrop: String::new(),
            music: String::new(),
            flags: LevelFlags::empty(),
            alternate_backdrop: 0,
            actors: vec![LevelActor {
                kind: 6,
                x: self.start.0,
                y: self.start.1,
            }],
            width: W as u16,
            height: self.height,
            cells: self.cells.clone(),
            extra: vec![0; 32750],
        };
        let mut solid_attributes = vec![A::empty(); 1000];
        solid_attributes[usize::from(SOLID)] = A::SOLID;
        solid_attributes[usize::from(FLOOR_ONLY)] = A::SOLID_TOP;
        solid_attributes[usize::from(PIPE)] = A::HANGABLE;
        solid_attributes[usize::from(LADDER)] = A::CLIMBABLE;
        solid_attributes[usize::from(BELT_LEFT)] = A::SOLID_TOP | A::CONVEYOR_LEFT;
        solid_attributes[usize::from(BELT_RIGHT)] = A::SOLID_TOP | A::CONVEYOR_RIGHT;
        let mut masked_attributes = vec![A::empty(); 160];
        masked_attributes[usize::from(MASKED_LADDER_TILE)] = A::CLIMBABLE | A::IN_FRONT;
        let tileset = Tileset {
            solid: Vec::new(),
            masked: Vec::new(),
            solid_attributes,
            masked_attributes,
        };
        World::new(&level, &tileset, &sprites(), 2)
    }
}

/// Steps with `input`, for `n` frames; the sounds heard.
fn run(world: &mut World, input: Buttons, n: usize) -> Vec<Sound> {
    let sprites = sprites();
    let mut sounds = Vec::new();
    for _ in 0..n {
        for event in world.step(input, &sprites) {
            if let GameEvent::Sound(id) = event {
                sounds.push(id);
            }
        }
    }
    sounds
}

/// The player's y after each of `n` frames.
fn ys(world: &mut World, input: Buttons, n: usize) -> Vec<i32> {
    (0..n)
        .map(|_| {
            run(world, input, 1);
            world.player.y
        })
        .collect()
}

#[test]
fn attributes_follow_the_cell_kind() {
    let mut grid = Grid::new(5);
    grid.masked_ladder(3, 5, 6).rect(7, 5, 7, 5, PIPE);
    // A layered cell (bit 15) has no attributes.
    grid.cells[8 * W + 8] = 0x8000 | 1;
    let world = grid.world();
    let map = &world.map;
    assert_eq!(map.attributes_at(0, 20), A::SOLID);
    assert_eq!(map.attributes_at(3, 5), A::CLIMBABLE | A::IN_FRONT);
    assert_eq!(map.attributes_at(7, 5), A::HANGABLE);
    assert_eq!(map.attributes_at(8, 8), A::empty());
    assert_eq!(map.attributes_at(10, 3), A::empty());
    // Past the right edge is the next row's start, as in the original;
    // below the stored cells, nothing.
    assert_eq!(map.attributes_at(W as i32, 19), map.attributes_at(0, 20));
    assert_eq!(map.attributes_at(W as i32, 19), A::SOLID);
    assert_eq!(map.attributes_at(2, 200), A::empty());
}

#[test]
fn collision_outside_the_map() {
    let mut grid = Grid::new(5);
    grid.rect(0, 0, 0, 29, SOLID);
    let world = grid.world();
    let map = &world.map;
    let body = |x, y| Body {
        x,
        y,
        width: 3,
        height: 5,
    };
    let probe = |b, dir| map.probe(b, dir, Mover::Thing).hit;
    // Past the left and right edges: solid.
    assert_eq!(probe(body(-1, 10), Dir::Left), Hit::Solid);
    assert_eq!(probe(body(W as i32 - 2, 10), Dir::Right), Hit::Solid);
    // Above the top: free, whatever is there.
    assert_eq!(probe(body(5, 2), Dir::Up), Hit::Free);
    assert_eq!(probe(body(5, 2), Dir::Left), Hit::Free);
    // Below the last row: free.
    assert_eq!(probe(body(5, H as i32 + 5), Dir::Down), Hit::Free);
    // Floors stop downwards movement only.
    assert_eq!(probe(body(5, 20), Dir::Down), Hit::Solid);
    assert_eq!(probe(body(5, 19), Dir::Down), Hit::Free);
    assert_eq!(probe(body(5, 24), Dir::Up), Hit::Solid);
}

#[test]
fn new_player_has_the_start_values() {
    let world = Grid::new(5).world();
    let p = &world.player;
    assert_eq!((p.x, p.y, p.facing), (5, 19, Facing::Right));
    assert_eq!(p.health, 9);
    assert_eq!(p.ammo, 0x20);
    assert_eq!(p.state, PlayerState::Standing);
}

#[test]
fn walking_stops_at_a_wall() {
    let mut grid = Grid::new(5);
    grid.rect(14, 10, 14, 19, SOLID);
    let mut world = grid.world();
    let xs: Vec<i32> = (0..10)
        .map(|_| {
            run(&mut world, Buttons::RIGHT, 1);
            world.player.x
        })
        .collect();
    // One tile a frame, until the standing box's last column meets the wall.
    assert_eq!(&xs[..6], &[6, 7, 8, 9, 10, 11]);
    assert_eq!(xs[9], 11);
    assert_eq!(world.player.frame, frame::STAND);
}

#[test]
fn walking_animates_every_other_frame() {
    let mut world = Grid::new(20).world();
    let frames: Vec<u16> = (0..6)
        .map(|_| {
            run(&mut world, Buttons::LEFT, 1);
            world.player.frame
        })
        .collect();
    assert_eq!(world.player.facing, Facing::Left);
    // The frame goes on every other game frame.
    assert_eq!(frames, [1, 1, 2, 2, 3, 3]);
}

#[test]
fn a_one_tile_ledge_is_a_step() {
    let mut grid = Grid::new(5);
    grid.rect(9, 19, 20, 19, SOLID);
    let mut world = grid.world();
    run(&mut world, Buttons::RIGHT, 8);
    assert_eq!((world.player.x, world.player.y), (13, 18));
    assert_eq!(world.player.state, PlayerState::Standing);
}

#[test]
fn jump_rises_seven_tiles_and_comes_down() {
    let mut world = Grid::new(5).world();
    let heard = run(&mut world, Buttons::JUMP, 1);
    assert_eq!(heard, [Sound::Jump]);
    assert_eq!(world.player.state, PlayerState::Jumping);
    let rise = ys(&mut world, Buttons::JUMP, 12);
    // Steps of 2 2 1 1 1, then three frames at the top, then the fall: 1 1 1
    // and two tiles a frame from the fourth step.
    assert_eq!(rise, [17, 15, 14, 13, 12, 12, 12, 12, 13, 14, 15, 17]);
    assert_eq!(world.player.state, PlayerState::Falling);
}

#[test]
fn landing_after_a_long_fall_crouches() {
    let mut world = Grid::new(5).world();
    run(&mut world, Buttons::JUMP, 1);
    let mut states = Vec::new();
    let mut heard = Vec::new();
    for _ in 0..16 {
        heard.extend(run(&mut world, Buttons::empty(), 1));
        states.push((world.player.state, world.player.frame));
    }
    // The fall reached its fourth step: crouch for a frame, with the sound
    // when getting up.
    let crouch = states
        .iter()
        .position(|&s| s == (PlayerState::Landing, frame::CROUCH))
        .unwrap();
    assert_eq!(states[crouch + 1], (PlayerState::Standing, frame::STAND));
    assert_eq!(heard.iter().filter(|&&s| s == Sound::Land).count(), 1);
    assert_eq!(world.player.y, 19);
}

#[test]
fn a_short_fall_lands_on_the_feet() {
    let mut grid = Grid::new(5);
    grid.at(5, 17);
    let mut world = grid.world();
    let heard = run(&mut world, Buttons::empty(), 6);
    assert_eq!(
        (world.player.y, world.player.state),
        (19, PlayerState::Standing)
    );
    assert!(heard.is_empty());
}

#[test]
fn a_jump_needs_the_key_let_go_first() {
    let mut world = Grid::new(5).world();
    let heard = run(&mut world, Buttons::JUMP, 30);
    assert_eq!(heard.iter().filter(|&&s| s == Sound::Jump).count(), 1);
}

#[test]
fn letting_go_early_makes_a_short_hop() {
    let mut world = Grid::new(5).world();
    run(&mut world, Buttons::JUMP, 1);
    let ys = ys(&mut world, Buttons::empty(), 8);
    // The jump key was let go before the fourth step: 5 tiles, not 7, and
    // only three frames at the top.
    assert_eq!(ys, [17, 15, 14, 14, 14, 15, 16, 17]);
}

#[test]
fn a_ceiling_ends_the_rise() {
    let mut grid = Grid::new(5);
    grid.rect(0, 12, 20, 12, SOLID);
    let mut world = grid.world();
    run(&mut world, Buttons::JUMP, 1);
    // The head (5 rows) stops under the ceiling in row 12; the fall starts.
    assert_eq!(ys(&mut world, Buttons::JUMP, 5), [17, 18, 19, 19, 19]);
}

#[test]
fn walking_off_an_edge_falls() {
    let mut grid = Grid::new(5);
    grid.rect(10, 20, 20, 21, 0);
    let mut world = grid.world();
    let mut states = Vec::new();
    for _ in 0..12 {
        run(&mut world, Buttons::RIGHT, 1);
        states.push(world.player.state);
    }
    assert!(states.contains(&PlayerState::Falling));
    // The hole is 11 wide: the player is still falling past the floor row.
    assert!(world.player.y > 20);
}

#[test]
fn ladders_are_climbed_and_left_at_the_foot() {
    let mut grid = Grid::new(5);
    grid.masked_ladder(6, 8, 19);
    let mut world = grid.world();
    run(&mut world, Buttons::UP, 1);
    assert_eq!(world.player.state, PlayerState::Climbing);
    assert_eq!(world.player.y, 18);
    let ys = ys(&mut world, Buttons::UP, 4);
    assert_eq!(ys, [17, 16, 15, 14]);
    assert!(world.player.frame == frame::CLIMB_A || world.player.frame == frame::CLIMB_B);
    // Down again, past the foot of the ladder: off it and falling.
    run(&mut world, Buttons::DOWN, 12);
    assert_eq!(world.player.y, 19);
    assert_ne!(world.player.state, PlayerState::Climbing);
}

#[test]
fn a_ladder_beside_the_head_pulls_the_player_in_line() {
    let mut grid = Grid::new(5);
    grid.rect(5, 8, 5, 19, LADDER);
    let mut world = grid.world();
    // The ladder is under the player's left column: one to the left.
    run(&mut world, Buttons::UP, 1);
    assert_eq!(world.player.state, PlayerState::Climbing);
    assert_eq!(world.player.x, 4);
}

#[test]
fn jumping_off_a_ladder() {
    let mut grid = Grid::new(5);
    grid.rect(6, 8, 6, 19, LADDER);
    let mut world = grid.world();
    run(&mut world, Buttons::UP, 6);
    run(&mut world, Buttons::empty(), 1);
    run(&mut world, Buttons::JUMP | Buttons::RIGHT, 1);
    assert_eq!(world.player.state, PlayerState::Jumping);
}

#[test]
fn pipes_are_grabbed_from_a_jump_and_walked_along() {
    let mut grid = Grid::new(5);
    grid.rect(3, 13, 30, 13, PIPE);
    let mut world = grid.world();
    let heard = run(&mut world, Buttons::JUMP, 8);
    assert_eq!(world.player.state, PlayerState::Hanging);
    assert!(heard.contains(&Sound::Clank));
    let x = world.player.x;
    run(&mut world, Buttons::RIGHT, 6);
    assert_eq!(world.player.state, PlayerState::Hanging);
    assert_eq!(world.player.x, x + 6);
    // Down and jump: let go.
    run(&mut world, Buttons::empty(), 1);
    run(&mut world, Buttons::DOWN | Buttons::JUMP, 1);
    assert_eq!(world.player.state, PlayerState::Falling);
}

#[test]
fn a_pipe_ends_and_the_player_drops() {
    let mut grid = Grid::new(5);
    grid.rect(3, 13, 12, 13, PIPE);
    let mut world = grid.world();
    run(&mut world, Buttons::JUMP, 8);
    assert_eq!(world.player.state, PlayerState::Hanging);
    let mut states = Vec::new();
    for _ in 0..12 {
        run(&mut world, Buttons::RIGHT, 1);
        states.push(world.player.state);
    }
    assert!(states.contains(&PlayerState::Falling));
}

#[test]
fn hanging_player_pulls_up_with_jump() {
    let mut grid = Grid::new(5);
    grid.rect(3, 13, 30, 13, PIPE);
    let mut world = grid.world();
    run(&mut world, Buttons::JUMP, 8);
    assert_eq!(world.player.state, PlayerState::Hanging);
    run(&mut world, Buttons::empty(), 1);
    run(&mut world, Buttons::JUMP, 1);
    assert_eq!(world.player.state, PlayerState::Jumping);
}

#[test]
fn belts_move_the_player() {
    let mut grid = Grid::new(10);
    grid.rect(8, 20, 20, 20, BELT_RIGHT);
    let mut world = grid.world();
    let sprites = sprites();
    world.step(Buttons::empty(), &sprites);
    world.step(Buttons::empty(), &sprites);
    assert_eq!(world.player.x, 12);
    assert_eq!(world.player.state, PlayerState::Standing);

    let mut grid = Grid::new(10);
    grid.rect(8, 20, 20, 20, BELT_LEFT);
    let mut world = grid.world();
    run(&mut world, Buttons::empty(), 3);
    assert_eq!(world.player.x, 7);
}

#[test]
fn camera_follows_sideways_and_stays_in_the_map() {
    let mut world = Grid::new(10).world();
    assert_eq!(world.camera.0, 0);
    // Walking right: the camera follows when the player is 19 columns from
    // the view's left edge.
    run(&mut world, Buttons::RIGHT, 12);
    assert_eq!(world.player.x, 22);
    assert_eq!(world.camera.0, 22 - 18);
    // It stops at the map's right edge: 48 - 32.
    run(&mut world, Buttons::RIGHT, 20);
    assert_eq!(world.camera.0, 16);
    // Walking back: follows when the player is closer than 10 columns.
    run(&mut world, Buttons::LEFT, 20);
    assert_eq!(world.player.x - i32::from(world.camera.0), 10);
    run(&mut world, Buttons::LEFT, 30);
    assert_eq!((world.player.x, world.camera.0), (0, 0));
}

#[test]
fn camera_looks_up_and_down() {
    let mut grid = Grid::new(10);
    grid.height = 50;
    grid.rect(0, 31, 20, 32, SOLID).at(10, 30);
    let mut world = grid.world();
    let depth = |w: &World| w.player.y - i32::from(w.camera.1);
    // At rest the player may be anywhere from 6 to 18 rows below the top.
    run(&mut world, Buttons::empty(), 3);
    assert_eq!(depth(&world), 18);
    // Looking down runs the camera ahead until the player is 5 rows down.
    run(&mut world, Buttons::DOWN, 12);
    assert_eq!(depth(&world), 5);
    // Let go: it comes back to 6.
    run(&mut world, Buttons::empty(), 4);
    assert_eq!(depth(&world), 6);
    // Looking up scrolls the other way, two rows a frame, to 18.
    run(&mut world, Buttons::UP, 1);
    assert_eq!(depth(&world), 8);
    run(&mut world, Buttons::UP, 10);
    assert_eq!(depth(&world), 18);
}

#[test]
fn camera_follows_a_fall() {
    let mut grid = Grid::new(5);
    grid.rect(10, 20, 20, 21, 0);
    grid.at(12, 5).rect(10, 28, 20, 29, SOLID);
    let mut world = grid.world();
    for _ in 0..40 {
        run(&mut world, Buttons::empty(), 1);
        let (cx, cy) = world.camera;
        let cy = i32::from(cy);
        assert!(cy <= world.player.y && world.player.y - cy < 20, "{cy}");
        assert!(cx <= 16);
    }
    assert_eq!(world.player.state, PlayerState::Standing);
}

#[test]
fn camera_jumps_up_with_the_player_near_the_top_of_the_map() {
    let mut grid = Grid::new(5);
    grid.at(5, 4).rect(0, 5, 20, 6, SOLID);
    let mut world = grid.world();
    run(&mut world, Buttons::empty(), 3);
    run(&mut world, Buttons::JUMP, 1);
    run(&mut world, Buttons::JUMP, 12);
    assert_eq!(world.camera.1, 0);
}

#[test]
fn a_shot_flies_and_stops_at_a_wall() {
    let mut grid = Grid::new(5);
    grid.rect(30, 10, 30, 19, SOLID);
    let mut world = grid.world();
    let heard = run(&mut world, Buttons::FIRE, 1);
    assert_eq!(heard, [Sound::Shot]);
    assert_eq!(world.shots.len(), 1);
    assert_eq!(world.player.frame, frame::FIRE);
    // Muzzle flash.
    let flash = |d: &crate::Draw| matches!(d, crate::Draw::Sprite(s) if s.id == 0x24);
    assert!(world.draws.iter().any(flash));
    // One shot per press: holding the key adds no more.
    run(&mut world, Buttons::FIRE, 4);
    assert_eq!(world.shots.len(), 1);
    let shot = &world.shots[0];
    assert_eq!(shot.sprite, 26);
    // Starts 3 tiles right of and 2 above the player; drawn there, then
    // moved on each frame.
    assert_eq!((shot.x, shot.y), (5 + 3 + 2 * 5, 17));
    for _ in 0..20 {
        run(&mut world, Buttons::empty(), 1);
        if world.shots.is_empty() {
            break;
        }
    }
    assert!(world.shots.is_empty());
    // It was gone when it reached the wall (column 30): its left edge
    // flew into it, then a puff.
    assert!(world.effects.iter().any(|e| e.id == 3));
}

#[test]
fn shots_leave_with_the_view() {
    let mut world = Grid::new(5).world();
    run(&mut world, Buttons::FIRE, 1);
    run(&mut world, Buttons::empty(), 30);
    assert!(world.shots.is_empty());
}

#[test]
fn looking_up_fires_upwards() {
    let mut world = Grid::new(5).world();
    run(&mut world, Buttons::UP, 1);
    assert_eq!(world.player.frame, frame::LOOK_UP);
    run(&mut world, Buttons::UP | Buttons::FIRE, 1);
    let shot = &world.shots[0];
    assert_eq!(shot.direction, crate::shots::ShotDirection::Up);
    assert_eq!(shot.sprite, 27);
    assert_eq!(world.player.frame, frame::FIRE_UP);
}

#[test]
fn weapons_use_ammunition_and_fall_back() {
    use crate::player::Weapon;
    let mut world = Grid::new(5).world();
    world.player.weapon = Weapon::Laser;
    world.player.ammo = 2;
    let heard = run(&mut world, Buttons::FIRE, 1);
    assert_eq!(heard, [Sound::Laser]);
    assert_eq!(world.player.ammo, 1);
    run(&mut world, Buttons::empty(), 1);
    run(&mut world, Buttons::FIRE, 1);
    assert_eq!(world.player.weapon, Weapon::Regular);
    assert_eq!(world.player.ammo, 0x20);
    assert_eq!(world.shots[0].sprite, 24);
}

#[test]
fn lasers_fly_through_walls() {
    use crate::player::Weapon;
    let mut grid = Grid::new(5);
    grid.rect(15, 10, 15, 19, SOLID);
    let mut world = grid.world();
    world.player.weapon = Weapon::Laser;
    run(&mut world, Buttons::FIRE, 1);
    run(&mut world, Buttons::empty(), 4);
    assert_eq!(world.shots.len(), 1);
    assert!(world.shots[0].x > 15 + 5);
}

#[test]
fn rockets_explode_on_walls() {
    use crate::player::Weapon;
    let mut grid = Grid::new(5);
    grid.rect(20, 10, 20, 19, SOLID);
    let mut world = grid.world();
    world.player.weapon = Weapon::Rocket;
    run(&mut world, Buttons::FIRE, 1);
    let mut heard = Vec::new();
    for _ in 0..12 {
        heard.extend(run(&mut world, Buttons::empty(), 1));
    }
    assert!(world.shots.is_empty());
    assert!(heard.contains(&Sound::Explosion));
}

#[test]
fn dying_hops_lies_explodes_and_ends_the_level() {
    use crate::state::LevelExit;
    let mut world = Grid::new(5).world();
    world.player.health = 1;
    world.player.hurt_timer = 0;
    let mut events = Vec::new();
    world.player.damage(2, &mut events);
    assert_eq!(world.player.state, PlayerState::Dying);
    let mut frames = 0;
    while world.state.exit.is_none() {
        run(&mut world, Buttons::FIRE | Buttons::LEFT, 1);
        frames += 1;
        assert!(frames < 100);
        // The dead cannot move or shoot.
        assert_eq!(world.player.x, 5);
        assert!(world.shots.is_empty());
    }
    assert_eq!(world.state.exit, Some(LevelExit::Died));
    // The hop, the landing, then lying until the explosion has run.
    assert!((40..55).contains(&frames), "{frames}");
    assert_eq!(world.player.frame, frame::GONE);
    assert_eq!(world.player.y, 19);
}

#[test]
fn the_cloak_warns_and_wears_off() {
    let mut world = Grid::new(5).world();
    world.player.cloak_timer = 32;
    world.player.inventory.push(0xd3);
    run(&mut world, Buttons::empty(), 1);
    assert!(world.state.messages.current.is_none());
    run(&mut world, Buttons::empty(), 1);
    assert_eq!(
        world.state.messages.current.as_deref(),
        Some(crate::messages::text::CLOAK_ENDING)
    );
    run(&mut world, Buttons::empty(), 30);
    assert_eq!(world.player.cloak_timer, 0);
    // The icon blinks for a while, then goes.
    assert!(world.player.inventory.contains(&0xd3));
    run(&mut world, Buttons::empty(), 11);
    assert!(world.player.inventory.is_empty());
}

#[test]
fn the_flame_thrower_lifts_the_player_while_firing_down() {
    use crate::player::Weapon;
    let mut world = Grid::new(5).world();
    world.player.weapon = Weapon::FlameThrower;
    let ys = ys(&mut world, Buttons::DOWN | Buttons::FIRE, 4);
    assert_eq!(ys, [18, 17, 16, 15]);
    assert_eq!(world.player.state, PlayerState::FlameHover);
    // Letting go makes the player fall again.
    run(&mut world, Buttons::empty(), 1);
    assert_eq!(world.player.state, PlayerState::Falling);
}

/// A placed type in a test world, updated even away from the view.
fn place(world: &mut World, kind: u16, x: i32, y: i32) {
    world
        .actors
        .spawn(kind, x, y, &mut world.state, &mut world.random);
    let actor = world.actors.slots.iter_mut().flatten().last().unwrap();
    actor.flags |= crate::actors::ActorFlags::ALWAYS_UPDATE;
}

#[test]
fn an_air_fan_lifts_the_player_and_lets_go() {
    let mut world = Grid::new(10).world();
    place(&mut world, 130, 10, 25);
    let ys = ys(&mut world, Buttons::empty(), 120);
    assert!(ys.iter().any(|&y| y < 10), "lifted: {ys:?}");
    assert!(world.player.state != PlayerState::AirFan);
    assert_eq!(*ys.last().unwrap(), 19, "lands again");
}

#[test]
fn a_crusher_slams_down_and_comes_back() {
    let mut world = Grid::new(10).at(30, 19).world();
    place(&mut world, 219, 20, 5);
    let sounds = run(&mut world, Buttons::empty(), 8);
    assert!(!sounds.contains(&Sound::Thud));
    let sounds = run(&mut world, Buttons::empty(), 40);
    assert!(sounds.contains(&Sound::Thud), "slammed");
    run(&mut world, Buttons::empty(), 60);
    let y = world.actors.iter().find(|a| a.id == 219).unwrap().y;
    assert!(y < 12, "retracted: {y}");
}

#[test]
fn a_ceiling_sucker_holds_the_player_and_bites() {
    let mut world = Grid::new(10).world();
    place(&mut world, 81, 10, 17);
    let health = world.player.health;
    run(&mut world, Buttons::empty(), 12);
    assert_eq!(world.player.state, PlayerState::Grabbed);
    let y = world.player.y;
    run(&mut world, Buttons::LEFT | Buttons::JUMP, 6);
    assert_eq!(
        (world.player.state, world.player.y),
        (PlayerState::Grabbed, y)
    );
    run(&mut world, Buttons::empty(), 20);
    assert_eq!(world.player.state, PlayerState::Standing);
    assert_eq!(world.player.health, health - 1);
}

#[test]
fn a_spider_on_the_face_blocks_the_weapon() {
    let mut world = Grid::new(10).world();
    world.state.spiders.face = Some(0);
    run(&mut world, Buttons::FIRE, 1);
    assert!(world.shots.is_empty());
    world.state.spiders.face = None;
    run(&mut world, Buttons::FIRE, 1);
    assert_eq!(world.shots.len(), 1);
}

/// A level with a solid tile that blocks on all sides: its attributes are
/// what `fully_solid_cell` looks for, and `SOLID` is the first such tile.
fn spawn(world: &mut World, level_type: u16, x: i32, y: i32) {
    world
        .actors
        .spawn(level_type, x, y, &mut world.state, &mut world.random);
}

#[test]
fn a_falling_section_lands_on_the_floor() {
    use crate::mechanics::{MapSection, SectionKind};
    let mut grid = Grid::new(5);
    grid.rect(10, 5, 11, 6, SOLID);
    let mut world = grid.world();
    world.state.sections.push(MapSection {
        left: 10,
        top: 5,
        right: 11,
        bottom: 6,
        kind: SectionKind::FALL_THEN_STOP.0,
    });
    let sounds = run(&mut world, Buttons::empty(), 20);
    let section = world.state.sections[0];
    // Rests on the floor in row 20.
    assert_eq!((section.top, section.bottom), (18, 19));
    assert_eq!(world.map.cell(10, 19), SOLID * 8);
    assert_eq!(world.map.cell(10, 5), 0);
    assert!(sounds.contains(&Sound::SectionLand));
}

#[test]
fn a_shot_section_blows_up_into_debris() {
    use crate::mechanics::{MapSection, SectionKind};
    let mut grid = Grid::new(5);
    grid.rect(14, 15, 15, 16, SOLID);
    let mut world = grid.world();
    world.state.sections.push(MapSection {
        left: 14,
        top: 15,
        right: 15,
        bottom: 16,
        kind: SectionKind::SHOOTABLE.0,
    });
    run(&mut world, Buttons::FIRE, 1);
    run(&mut world, Buttons::empty(), 8);
    assert_eq!(world.map.cell(14, 15), 0);
    assert_eq!(world.state.debris.len(), 4);
    assert!(world.state.debris_timer > 0);
    assert!(
        world
            .draws
            .iter()
            .any(|d| matches!(d, crate::Draw::Tile { .. }))
    );
}

#[test]
fn the_exit_ends_the_level_once_the_radar_dishes_are_gone() {
    use crate::state::LevelExit;
    let mut world = Grid::new(5).world();
    spawn(&mut world, 139, 6, 19);
    world.state.radar_dishes = 1;
    run(&mut world, Buttons::empty(), 2);
    assert_eq!(world.state.exit, None);
    world.state.radar_dishes = 0;
    run(&mut world, Buttons::empty(), 1);
    assert_eq!(world.state.exit, Some(LevelExit::Finished));
}

#[test]
fn a_vertical_door_opens_for_the_player_and_closes_again() {
    let mut grid = Grid::new(30);
    grid.rect(0, 20, 47, 21, SOLID);
    let mut world = grid.world();
    spawn(&mut world, 128, 10, 19);
    // Far away: closed, its tiles replaced by the solid one.
    run(&mut world, Buttons::empty(), 2);
    assert_eq!(world.map.cell(10, 15), SOLID * 8);
    // Near: open after a few frames.
    world.player.x = 8;
    let sounds = run(&mut world, Buttons::empty(), 10);
    assert_eq!(world.map.cell(10, 15), 0);
    assert!(sounds.contains(&Sound::Door));
    world.player.x = 30;
    run(&mut world, Buttons::empty(), 12);
    assert_eq!(world.map.cell(10, 15), SOLID * 8);
}

#[test]
fn dying_at_a_beacon_puts_the_player_back_there() {
    use crate::state::Checkpoint;
    let mut world = Grid::new(5).world();
    world.state.checkpoint = Some(Checkpoint {
        x: 20,
        y: 19,
        health: 1,
        weapon: crate::player::Weapon::Laser,
        ammo: 7,
        backdrop_alternate: false,
    });
    world.player.health = 0;
    world.state.exit = Some(crate::state::LevelExit::Died);
    assert!(world.respawn());
    assert_eq!((world.player.x, world.player.y), (20, 19));
    assert_eq!(world.player.health, 2);
    assert_eq!(world.player.ammo, 7);
    assert_eq!(world.state.exit, None);
}

#[test]
fn the_ship_takes_the_player_flies_fires_and_is_left_behind() {
    use crate::shots::{SHIP_SHOT, ShotKind};
    let mut world = Grid::new(5).at(11, 12).world();
    place(&mut world, 87, 10, 19);
    run(&mut world, Buttons::empty(), 8);
    assert_eq!(world.player.state, PlayerState::Ship);
    assert_eq!((world.player.x, world.player.y), (10, 19));
    assert_eq!(world.player.sprite_id(), crate::player::SHIP_RIGHT);
    // One tile a frame, two from the fourth.
    let xs: Vec<i32> = (0..6)
        .map(|_| {
            run(&mut world, Buttons::RIGHT, 1);
            world.player.x
        })
        .collect();
    assert_eq!(xs, [11, 12, 13, 15, 17, 19]);
    // The cannon fires every other frame, from the nose, without ammunition.
    let ammo = world.player.ammo;
    run(&mut world, Buttons::FIRE, 2);
    assert_eq!(world.shots.len(), 1);
    assert_eq!(world.shots[0].kind, ShotKind::Blast(SHIP_SHOT));
    assert_eq!(world.player.ammo, ammo);
    // Up a tile a frame; jump gets out and leaves the ship there.
    run(&mut world, Buttons::UP, 2);
    assert_eq!(world.player.y, 17);
    run(&mut world, Buttons::JUMP, 1);
    assert_eq!(world.player.state, PlayerState::Jumping);
    assert_eq!(world.player.x, 20);
    let ship = world.actors.iter().find(|a| a.id == 87).unwrap();
    assert_eq!((ship.x, ship.y), (19, 17));
}

#[test]
fn a_transporter_leaves_the_move_to_the_caller() {
    let mut world = Grid::new(10).world();
    place(&mut world, 50, 9, 19);
    place(&mut world, 51, 30, 19);
    let sprites = sprites();
    let events = world.step(Buttons::UP, &sprites);
    assert!(events.contains(&GameEvent::Teleport));
    // Still where he was until the caller has faded the view out.
    assert_eq!((world.player.x, world.player.y), (10, 19));
    assert!(world.finish_teleport());
    assert_eq!((world.player.x, world.player.y), (31, 19));
    assert!(!world.finish_teleport());
}

#[test]
fn a_wider_view_shows_actors_beyond_the_original_one_standing_still() {
    // The camera is at column 9 of the 48: 32 columns to 41.
    let mut world = Grid::new(24).world();
    spawn(&mut world, 219, 5, 15);
    spawn(&mut world, 219, 20, 15);
    run(&mut world, Buttons::empty(), 2);
    assert_eq!(world.camera.0, 9);
    // 45 columns: 6 more on the left, the map ends 7 more on the right.
    assert_eq!(world.view_camera(45), (3, world.camera.1));
    assert_eq!(world.view_camera(32), world.camera);
    let sprites = sprites();
    let margin = world.margin_draws(&sprites, 45);
    assert_eq!(margin.len(), 1, "{margin:?}");
    assert!(matches!(margin[0], crate::Draw::Sprite(s) if s.x == 5));
    assert!(world.margin_draws(&sprites, 32).is_empty());
    // The one outside the original view does not move.
    let before = world.actors.iter().find(|a| a.x == 5).cloned();
    run(&mut world, Buttons::empty(), 40);
    assert_eq!(world.actors.iter().find(|a| a.x == 5).cloned(), before);
}
