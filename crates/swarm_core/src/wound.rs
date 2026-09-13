//! What a wound HANDS OUT: the three things a hit produces for whatever is
//! drawing it, and the two functions that make them.
//!
//! Split out of `damage` when that file went over this project's own nine
//! hundred lines, along the line its own module docs already drew. `damage`
//! answers what a hit DOES to the cells, `heat` answers what the hole then
//! looks like, and this is what the hole gives the renderer to throw: a
//! breach, a vent and a chunk. The `impl DamageGrid` for `vents` lives here
//! with them, because Rust lets one type keep its inherent methods in more
//! than one file of a crate, and where a method belongs is decided by what
//! it is ABOUT.

use crate::damage::DamageGrid;
use crate::rng::drift_of;
use crate::voxel::{mat, VoxelModel, NEIGHBOURS};

/// A cell that died this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breach {
    pub cell: u32,
    pub tick: u32,
    /// Which way out of the hull the face it was bitten from looks.
    pub outward: [f32; 3],
}

/// Where smoke leaves a hull: the centre of one face a hit opened, and the
/// way OUT of it, so a plume goes into space rather than through the ship.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vent {
    pub at: [f32; 3],
    pub outward: [f32; 3],
    /// The dead cell it opened onto, which is what carries the heat.
    pub cell: u32,
}

/// A piece coming off, for the renderer to throw. Its drift is hashed from the
/// cell rather than rolled, so two screens watching one wound throw the same
/// debris.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chunk {
    pub cell: u32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub colour: u32,
    pub born: u32,
}

impl DamageGrid {
    /// Every face a hit opened, as somewhere smoke can leave.
    ///
    /// One per face rather than one per hole: a crater in a flank vents along
    /// its whole rim, and a single plume from the middle of it would read as a
    /// chimney.
    pub fn vents(&self, m: &VoxelModel, limit: usize) -> Vec<Vent> {
        let mut out = Vec::new();
        for n in 0..m.len() {
            if !self.is_dead(n) {
                continue;
            }
            let (i, j, k) = m.at(n);
            for (a, b, c) in NEIGHBOURS {
                let (x, y, z) = (i as i32 + a, j as i32 + b, k as i32 + c);
                if !m.inside(x, y, z) {
                    continue;
                }
                let live = m.index(x as usize, y as usize, z as usize);
                if m.grid[live] == mat::EMPTY || self.is_dead(live) {
                    continue;
                }
                // The face between them, which is half a cell off the dead
                // one, and the way out is from the live cell toward it.
                let p = m.centre_of(n);
                let h = m.cell * 0.5;
                out.push(Vent {
                    at: [
                        p[0] + a as f32 * h,
                        p[1] + b as f32 * h,
                        p[2] + c as f32 * h,
                    ],
                    outward: [-(a as f32), -(b as f32), -(c as f32)],
                    cell: n as u32,
                });
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }
}

/// The piece a breach throws, in the model's frame.
pub fn chunk_for(m: &VoxelModel, b: &Breach) -> Chunk {
    let n = b.cell as usize;
    let drift = drift_of(b.cell, b.tick);
    let speed = m.cell * 6.0;
    Chunk {
        cell: b.cell,
        origin: m.centre_of(n),
        velocity: [
            b.outward[0] * speed + drift[0] * speed * 0.5,
            b.outward[1] * speed + drift[1] * speed * 0.5,
            b.outward[2] * speed + drift[2] * speed * 0.5,
        ],
        colour: m.colour[n],
        born: b.tick,
    }
}
