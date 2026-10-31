//! Particles: the coloured sparks of kills and broken things.
//!
//! The original has 5 groups of 64 one-pixel particles. A group flies for 28
//! frames from one tile; the particles spread sideways at random speeds and
//! fall along a curve. They are drawn after the actors, before the player's
//! shots.

use crate::Random;
use crate::draw::Draw;

const GROUPS: usize = 5;
const PARTICLES: usize = 64;
/// A group is over when its frame counter reaches this.
const LIFETIME: u16 = 0x1d;

/// Pixels a particle falls by its age (negative: rises).
const FALL: [i8; 41] = [
    -8, -8, -8, -8, -4, -4, -4, -2, -1, 0, 0, 1, 2, 4, 4, 4, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Particle {
    /// Pixels per frame sideways.
    vx: i16,
    age: u16,
    /// Pixels below where it started.
    dy: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Group {
    /// Frames flown plus one.
    frame: u16,
    x: i32,
    y: i32,
    color: u8,
    particles: [Particle; PARTICLES],
}

/// Which way a group flies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    Both,
    Left,
    Right,
}

#[derive(Clone, Debug, Default)]
pub struct Particles {
    groups: [Option<Group>; GROUPS],
}

impl Particles {
    /// Starts a group of `color` at tile `x`, `y`, if one is free.
    pub fn spawn(&mut self, random: &mut Random, color: u8, spread: Spread, x: i32, y: i32) {
        let Some(slot) = self.groups.iter_mut().find(|g| g.is_none()) else {
            return;
        };
        let mut particles = [Particle::default(); PARTICLES];
        for p in &mut particles {
            let speed = i16::from(random.next_byte() % 20);
            p.vx = match spread {
                Spread::Both => 10 - speed,
                Spread::Left => -(speed + 1),
                Spread::Right => speed + 1,
            };
            p.age = u16::from(random.next_byte() & 0xf);
        }
        *slot = Some(Group {
            frame: 1,
            x,
            y,
            color,
            particles,
        });
    }

    /// Moves every group on and draws it, in map pixels.
    pub fn update(&mut self, draws: &mut alloc::vec::Vec<Draw>) {
        for slot in &mut self.groups {
            let Some(g) = slot else {
                continue;
            };
            for p in &mut g.particles {
                let x = g.x * 8 + i32::from(p.vx) * i32::from(g.frame);
                // Past the table the original reads two zero bytes, then 8s.
                let fall = FALL.get(usize::from(p.age)).copied().unwrap_or(
                    if usize::from(p.age) < FALL.len() + 2 {
                        0
                    } else {
                        8
                    },
                );
                p.dy += i16::from(fall);
                p.age += 1;
                let y = g.y * 8 + i32::from(p.dy);
                draws.push(Draw::Pixel {
                    x,
                    y,
                    color: g.color,
                });
            }
            g.frame += 1;
            if g.frame == LIFETIME {
                *slot = None;
            }
        }
    }
}
