//! Asteroids, as voxels, from a seed and nothing else.
//!
//! A rock is the one thing in the game that is not a ship, and it exists so
//! the swarm has something to go round. It is built the way everything else
//! here is built, on a lattice, so the mesher, the damage grid and the
//! materials all take it without learning what it is.
//!
//! What makes it read as a rock rather than as a die is that the radius
//! varies with direction: three octaves of a value noise ON THE SPHERE,
//! sampled by the direction from the centre. A single threshold on distance
//! gives a ball and a ball is a planet; a threshold on distance against a
//! direction dependent radius gives lumps and shelves, which is what a rock
//! looks like from far enough away to be worth drawing.
//!
//! There is no `sin` anywhere, for the reason the chitin has none: an
//! expression built from `floor`, `fract` and multiplies gives the same
//! answer on every machine, and the suite pins the shapes it makes.

use crate::rng::Rng;
use crate::voxel::{mat, Surface, VoxelModel, SURF_ARMOUR, SURF_FRAME};

/// How far a rock's surface may swing from its mean radius, as a share of it.
/// Over about a half the lumps stop touching and the flood fill that keeps a
/// model in one piece has work to do; under about a fifth it is a ball.
const RELIEF: f32 = 0.42;

/// The ore a rock carries, as a share of its cells. Ore is `mat::ACCENT`,
/// which the livery draws in its own colour, so a seam reads at a glance
/// without anything here knowing what colour it will be drawn in.
const ORE_SHARE: f32 = 0.06;

/// Value noise on the integer lattice, hashed rather than tabled.
///
/// Three dimensional, because a rock's relief is a function of a DIRECTION
/// and a direction is three numbers. Hashing rather than a permutation table
/// keeps it a pure function of its inputs with no state to carry.
fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD8163_841u32)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 16;
    h as f32 / u32::MAX as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Value noise at a point, trilinear between the lattice corners.
fn noise3(p: [f32; 3]) -> f32 {
    let fl = [p[0].floor(), p[1].floor(), p[2].floor()];
    let i = [fl[0] as i32, fl[1] as i32, fl[2] as i32];
    let f = [smooth(p[0] - fl[0]), smooth(p[1] - fl[1]), smooth(p[2] - fl[2])];
    let mut c = [0.0f32; 8];
    for (n, slot) in c.iter_mut().enumerate() {
        let (dx, dy, dz) = ((n & 1) as i32, ((n >> 1) & 1) as i32, ((n >> 2) & 1) as i32);
        *slot = hash3(i[0] + dx, i[1] + dy, i[2] + dz);
    }
    let x0 = lerp(lerp(c[0], c[1], f[0]), lerp(c[2], c[3], f[0]), f[1]);
    let x1 = lerp(lerp(c[4], c[5], f[0]), lerp(c[6], c[7], f[0]), f[1]);
    lerp(x0, x1, f[2])
}

/// Three octaves, each half the amplitude and double the frequency, about
/// nought. Three rather than more because a rock is drawn at a few dozen
/// cells across and a fourth octave is finer than one cell: it would cost a
/// lookup per cell to change nothing anybody can see.
fn relief(d: [f32; 3], seed: f32) -> f32 {
    let mut amp = 1.0;
    let mut freq = 1.7;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for _ in 0..3 {
        let p = [d[0] * freq + seed, d[1] * freq + seed * 1.7, d[2] * freq + seed * 2.3];
        sum += (noise3(p) - 0.5) * amp;
        norm += 0.5 * amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm.max(1e-4)
}

/// Build one asteroid on a lattice `n` cells on a side.
///
/// `cell` is what one cell is worth in the world, so a rock is placed on the
/// same ladder a hull is: the app scales nothing, it asks for the lattice it
/// wants and puts the model where it wants it.
pub fn generate(n: usize, cell: f32, seed: u64) -> VoxelModel {
    let mut m = VoxelModel::new(n, n, n, cell);
    // Two surfaces, and that is the whole material story of a rock: the stone
    // and the ore in it. A rock with one surface is a rock with no seam, and
    // the seam is the only thing on it worth looking at twice.
    m.surfaces = (0..crate::voxel::SURF_COUNT)
        .map(|s| {
            if s == SURF_FRAME as usize {
                // The ore: tighter and glossier than the stone round it.
                Surface { finish: "greeble".into(), metal: 0.55, rough: 0.38 }
            } else {
                // The stone: battered, and barely metallic at all.
                Surface { finish: "battered".into(), metal: 0.04, rough: 0.94 }
            }
        })
        .collect();

    let mut rng = Rng::new(seed);
    let noise_seed = rng.range(0.0, 64.0);
    // The mean radius, in cells, with a margin so the relief cannot push a
    // lump off the end of the lattice: a cell outside is a cell the index
    // arithmetic would wrap onto the other face.
    let half = n as f32 * 0.5;
    let mean = half * (0.80 / (1.0 + RELIEF));
    // A rock is not a sphere with bumps, it is a LUMP: three different
    // radii, so the silhouette has a long axis and reads as tumbling rather
    // than as a ball whatever way it is turned.
    let stretch = [rng.range(0.72, 1.28), rng.range(0.72, 1.28), rng.range(0.72, 1.28)];
    let c = half - 0.5;

    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let d = [
                    (i as f32 - c) / stretch[0],
                    (j as f32 - c) / stretch[1],
                    (k as f32 - c) / stretch[2],
                ];
                let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if r < 1e-4 {
                    // The middle is always stone: a direction is undefined
                    // there, and dividing by a length of nought is the NaN
                    // that would put a hole through the centre of the rock.
                    let idx = m.index(i, j, k);
                    m.grid[idx] = mat::PLATE;
                    m.surf[idx] = SURF_ARMOUR;
                    m.colour[idx] = 0x6A6258;
                    continue;
                }
                let u = [d[0] / r, d[1] / r, d[2] / r];
                let want = mean * (1.0 + RELIEF * relief(u, noise_seed));
                if r > want {
                    continue;
                }
                let idx = m.index(i, j, k);
                // Ore where a second, coarser field is high, and only below
                // the skin: an ore seam is something a rock is made of, so a
                // vein that ran over the surface would read as paint.
                let deep = r < want - 1.2;
                let vein = noise3([u[0] * 3.1 + 11.0, u[1] * 3.1 + 11.0, u[2] * 3.1 + 11.0]);
                if deep && vein > 1.0 - ORE_SHARE * 4.0 {
                    m.grid[idx] = mat::ACCENT;
                    m.surf[idx] = SURF_FRAME;
                    m.colour[idx] = 0xC8A24A;
                } else {
                    m.grid[idx] = mat::PLATE;
                    m.surf[idx] = SURF_ARMOUR;
                    // A little variation in the stone, keyed on the cell
                    // rather than rolled, so the same rock comes back the
                    // same way and two rocks are not the same grey.
                    let t = hash3(i as i32, j as i32, k as i32);
                    let g = (0x58 as f32 + t * 26.0) as u32;
                    m.colour[idx] = (g << 16) | ((g - 6) << 8) | (g - 14);
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::mat;

    /// A rock has to be ONE rock. The relief can cut a lump off the main
    /// body, and a floating shelf beside an asteroid is the same defect
    /// redux-tribes calls an orphan: a cell touching nothing.
    #[test]
    fn a_rock_is_one_piece() {
        for seed in 0..8u64 {
            let m = generate(20, 0.2, seed);
            let solid: Vec<bool> = m.grid.iter().map(|&x| x != mat::EMPTY).collect();
            let start = solid.iter().position(|&s| s).expect("a rock has cells");
            // Flood fill on SIX neighbours: two cells meeting at an edge are
            // two cells touching along a line, which is not a weld.
            let mut seen = vec![false; solid.len()];
            let mut stack = vec![start];
            seen[start] = true;
            let mut count = 0;
            while let Some(n) = stack.pop() {
                count += 1;
                let i = n % m.nx;
                let j = (n / m.nx) % m.ny;
                let k = n / (m.nx * m.ny);
                let steps: [(i32, i32, i32); 6] =
                    [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
                for (dx, dy, dz) in steps {
                    let (a, b, c) = (i as i32 + dx, j as i32 + dy, k as i32 + dz);
                    if a < 0 || b < 0 || c < 0 || a >= m.nx as i32 || b >= m.ny as i32 || c >= m.nz as i32 {
                        continue;
                    }
                    let q = m.index(a as usize, b as usize, c as usize);
                    if solid[q] && !seen[q] {
                        seen[q] = true;
                        stack.push(q);
                    }
                }
            }
            assert_eq!(count, solid.iter().filter(|&&s| s).count(), "seed {seed} came out in pieces");
        }
    }

    /// A rock must not touch the wall of its own lattice. A cell on the face
    /// is a cell the relief was about to push outside, and outside is the
    /// other side of the model: the index arithmetic wraps and a lump
    /// reappears across the rock.
    #[test]
    fn a_rock_clears_its_own_lattice() {
        for seed in 0..8u64 {
            let m = generate(20, 0.2, seed);
            for k in 0..m.nz {
                for j in 0..m.ny {
                    for i in 0..m.nx {
                        let edge = i == 0 || j == 0 || k == 0 || i == m.nx - 1 || j == m.ny - 1 || k == m.nz - 1;
                        if edge {
                            assert_eq!(m.grid[m.index(i, j, k)], mat::EMPTY, "seed {seed} reaches its wall");
                        }
                    }
                }
            }
        }
    }

    /// Two seeds are two rocks. Without this the generator could ignore its
    /// seed entirely and every other test here would still pass.
    #[test]
    fn a_seed_is_a_different_rock() {
        let a = generate(20, 0.2, 1);
        let b = generate(20, 0.2, 2);
        let differ = a.grid.iter().zip(&b.grid).filter(|(x, y)| x != y).count();
        assert!(differ > 40, "only {differ} cells differ between two seeds");
    }

    /// And it carries ore, or the second surface is a material nothing is
    /// drawn in and the seam is a claim rather than a picture.
    #[test]
    fn a_rock_carries_ore() {
        let m = generate(24, 0.2, 5);
        let ore = m.grid.iter().filter(|&&x| x == mat::ACCENT).count();
        let stone = m.grid.iter().filter(|&&x| x == mat::PLATE).count();
        assert!(ore > 0, "no ore at all");
        assert!(ore < stone / 4, "{ore} ore against {stone} stone is a rock made of ore");
    }
}
