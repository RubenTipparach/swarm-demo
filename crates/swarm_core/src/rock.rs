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
use crate::voxel::{mat, Surface, VoxelModel, NEIGHBOURS, SURF_ARMOUR, SURF_DRIVE, SURF_FRAME};

/// How far a rock's surface may swing from its mean radius, as a share of it.
/// Over about a half the lumps stop touching and the flood fill that keeps a
/// model in one piece has work to do; under about a fifth it is a ball.
const RELIEF: f32 = 0.42;

/// The ore a rock carries, as a share of its cells. Ore is `mat::ACCENT`,
/// which the livery draws in its own colour, so a seam reads at a glance
/// without anything here knowing what colour it will be drawn in.
const ORE_SHARE: f32 = 0.06;

/// What share of a seam breaks the SURFACE, against the share that is buried.
///
/// Both seams used to be buried absolutely: ore is laid only below `want -
/// 1.2` because "a vein that ran over the surface would read as paint", and
/// `seed_crystal` takes a cell only if it has stone on all six faces for the
/// same reason. Each is right about a seam SMEARED over a rock, and together
/// they make a rock that tells a player nothing at all: measured over
/// fourteen seeds, 2.5% of the seam had a face open to space and four of the
/// fourteen rocks had none. There was no crystal anywhere a player could see,
/// which is what "no crystal to mine" is, and no way to tell a rich rock from
/// a poor one or to know which face to cut.
///
/// An OUTCROP is the other half of the same rule. A seam that reaches the
/// skin in a few places is what a mineral asteroid actually looks like, and
/// it is the one thing on a rock worth looking at twice, which is what this
/// project's own notes have claimed about ore all along. It is a FIFTH of the
/// buried rate, so the bulk of a seam is still inside the rock and cutting is
/// still how it is got: an outcrop says a rock is worth cutting and pays
/// almost nothing by itself.
const OUTCROP: f32 = 0.2;

/// The warm yellow an ore seam is drawn in, and the pale blue of a crystal.
/// A crystal is `mat::GLOW`, which is what everything that is a LIGHT in this
/// game is made of, so it is lit rather than painted and a shaft that reaches
/// one is a shaft with something shining at the bottom of it.
const ORE_COLOUR: u32 = 0xC8A24A;
const CRYSTAL_COLOUR: u32 = 0x86E8FF;

/// Which surface a crystal draws in.
///
/// A rock authors its own surface table, so the index is only a slot and any
/// would do; what matters is that it is ONE slot named in one place, because
/// the app has to override it with an emissive material and a second copy of
/// the number is a crystal that stops glowing the day either moves.
pub const CRYSTAL_SURF: u8 = SURF_DRIVE;

/// What a rock is made of besides stone.
///
/// Both seams are in every rock that is worth anything, and the flavour is
/// which way it LEANS. That is the whole reason crystal is seeded off the
/// ore rather than given rocks of its own: a miner sent for metal comes back
/// with fuel as well, so one rock is one decision instead of two, and a
/// system cannot stand a fleet up by handing it the wrong kind of asteroid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Flavour {
    /// Metal, with crystal in the seams: a miner cuts it mostly for
    /// materials.
    #[default]
    Ore,
    /// The same rock run through with crystal: mostly volatiles, which is
    /// what the tanker refines into jump fuel.
    Crystal,
    /// Stone all the way through, and the only thing in the field that is
    /// worth nothing at all.
    Barren,
}

impl Flavour {
    /// How much of the stone TOUCHING a seam is crystal.
    ///
    /// Crystal grows against the ore and never on its own, which is the
    /// owner's rule and is also what makes the two shares one number: a rock
    /// with no ore in it has no crystal either, and nothing has to say so.
    pub fn crystal_share(self) -> f32 {
        match self {
            Flavour::Ore => 0.34,
            Flavour::Crystal => 0.86,
            Flavour::Barren => 0.0,
        }
    }
}

/// Value noise on the integer lattice, hashed rather than tabled.
///
/// Three dimensional, because a rock's relief is a function of a DIRECTION
/// and a direction is three numbers. Hashing rather than a permutation table
/// keeps it a pure function of its inputs with no state to carry.
fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841_u32)
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
    let f = [
        smooth(p[0] - fl[0]),
        smooth(p[1] - fl[1]),
        smooth(p[2] - fl[2]),
    ];
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
        let p = [
            d[0] * freq + seed,
            d[1] * freq + seed * 1.7,
            d[2] * freq + seed * 2.3,
        ];
        sum += (noise3(p) - 0.5) * amp;
        norm += 0.5 * amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm.max(1e-4)
}

/// What a rock is made OF: the stone, the ore in it, and the crystal grown
/// against that.
///
/// Its own function rather than a block inside the generator, because it is
/// a different question from what shape a rock is, and because the one thing
/// the app has to agree with about a rock is in here.
fn surfaces() -> Vec<Surface> {
    (0..crate::voxel::SURF_COUNT)
        .map(|s| {
            if s == SURF_FRAME as usize {
                // The ore: tighter and glossier than the stone round it.
                Surface {
                    finish: "greeble".into(),
                    metal: 0.55,
                    rough: 0.38,
                }
            } else if s == CRYSTAL_SURF as usize {
                // The crystal: smooth, glassy and not metal at all. What
                // makes it glow, and what gives its faces DEPTH, is the app
                // laying a parallax mapped material over this slot, the same
                // way a carrier's drives are lit.
                Surface {
                    finish: "smooth".into(),
                    metal: 0.0,
                    rough: 0.12,
                }
            } else {
                // The stone: battered, and barely metallic at all.
                Surface {
                    finish: "battered".into(),
                    metal: 0.04,
                    rough: 0.94,
                }
            }
        })
        .collect()
}

/// Build one asteroid on a lattice `n` cells on a side.
///
/// `cell` is what one cell is worth in the world, so a rock is placed on the
/// same ladder a hull is: the app scales nothing, it asks for the lattice it
/// wants and puts the model where it wants it.
pub fn generate(n: usize, cell: f32, seed: u64) -> VoxelModel {
    generate_of(n, cell, seed, Flavour::Ore)
}

/// The same rock, carrying what its flavour says it carries.
pub fn generate_of(n: usize, cell: f32, seed: u64, flavour: Flavour) -> VoxelModel {
    let mut m = VoxelModel::new(n, n, n, cell);
    m.surfaces = surfaces();
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
    let stretch = [
        rng.range(0.72, 1.28),
        rng.range(0.72, 1.28),
        rng.range(0.72, 1.28),
    ];
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
                // Ore where a second, coarser field is high, mostly below the
                // skin and OUTCROPPING here and there: a vein smeared over the
                // whole surface reads as paint, and a vein with no outcrop at
                // all is a rock with nothing on it to look at. See `OUTCROP`.
                let deep = r < want - 1.2;
                let crop = hash3(i as i32 * 13 + 1, j as i32 * 13 + 7, k as i32 * 13 + 3);
                let seam = deep || crop < OUTCROP;
                let vein = noise3([u[0] * 3.1 + 11.0, u[1] * 3.1 + 11.0, u[2] * 3.1 + 11.0]);
                if seam && vein > 1.0 - ORE_SHARE * 4.0 && flavour != Flavour::Barren {
                    m.grid[idx] = mat::ACCENT;
                    m.surf[idx] = SURF_FRAME;
                    m.colour[idx] = ORE_COLOUR;
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
    seed_crystal(&mut m, flavour);
    m
}

/// Grow crystal against the ore, once the rock is laid.
///
/// A second pass rather than a case in the loop above, because "near the
/// ore" is not knowable while the ore is still being placed: the cell beside
/// this one may be a seam and may be stone, and which it is depends on a
/// vein this row has not reached yet.
///
/// It is hashed off the CELL rather than rolled off the rng, so a crystal is
/// a function of where it is and the same rock comes back the same way
/// however it was reached. A cell is taken only if it is stone and BURIED,
/// which is the ore's own rule a second time: a crystal with a face open to
/// space would read as paint on the outside of a rock, and what a player
/// should have to do to reach one is cut.
fn seed_crystal(m: &mut VoxelModel, flavour: Flavour) {
    let share = flavour.crystal_share();
    if share <= 0.0 {
        return;
    }
    let mut take: Vec<usize> = Vec::new();
    for k in 0..m.nz {
        for j in 0..m.ny {
            for i in 0..m.nx {
                let idx = m.index(i, j, k);
                if m.grid[idx] != mat::ACCENT {
                    continue;
                }
                for (di, dj, dk) in NEIGHBOURS {
                    let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                    if !m.inside(ni, nj, nk) {
                        continue;
                    }
                    let n = m.index(ni as usize, nj as usize, nk as usize);
                    if m.grid[n] != mat::PLATE {
                        continue;
                    }
                    // A buried cell at the full rate, a surface one at a
                    // fifth of it: the bulk of the seam stays inside the rock
                    // and a few facets reach the skin, which is what says
                    // there is crystal in here at all. See `OUTCROP`.
                    let want = if buried(m, ni, nj, nk) {
                        share
                    } else {
                        share * OUTCROP
                    };
                    if hash3(ni * 7 + 3, nj * 7 + 5, nk * 7 + 11) < want {
                        take.push(n);
                    }
                }
            }
        }
    }
    for n in take {
        m.grid[n] = mat::GLOW;
        m.surf[n] = CRYSTAL_SURF;
        m.colour[n] = CRYSTAL_COLOUR;
    }
}

/// Whether a cell has stone on all six faces, which is the one thing that
/// keeps a seam under the skin.
fn buried(m: &VoxelModel, i: i32, j: i32, k: i32) -> bool {
    NEIGHBOURS.iter().all(|(di, dj, dk)| {
        let (a, b, c) = (i + di, j + dj, k + dk);
        m.inside(a, b, c) && m.grid[m.index(a as usize, b as usize, c as usize)] != mat::EMPTY
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::mat;

    /// Every rock SHOWS what it carries, and most of it is still inside.
    ///
    /// Both halves, because each without the other is a defect this has had.
    /// Buried absolutely, a rock is a grey lump that tells a player nothing:
    /// there was no crystal anywhere anybody could see, four seeds in
    /// fourteen had not one cell of seam open to space, and "no crystal to
    /// mine" is exactly what that looks like from the cockpit. Laid over the
    /// whole skin instead, a seam reads as paint and cutting buys nothing.
    ///
    /// So: every seed outcrops SOMETHING of each seam, and the outcrop is a
    /// minority of it. Held loosely on both sides, because what the numbers
    /// have to be is "visible" and "not most of it" rather than any figure in
    /// particular, and a pin on a figure is a pin that fails the day the
    /// relief is tuned.
    #[test]
    fn every_rock_outcrops_some_of_its_seam_and_hides_most_of_it() {
        for seed in 0..14u64 {
            let m = generate(22, 0.2, seed);
            let (mut seam, mut open) = (0usize, 0usize);
            for k in 0..m.nz {
                for j in 0..m.ny {
                    for i in 0..m.nx {
                        let n = m.index(i, j, k);
                        if m.grid[n] != mat::ACCENT && m.grid[n] != mat::GLOW {
                            continue;
                        }
                        seam += 1;
                        if !buried(&m, i as i32, j as i32, k as i32) {
                            open += 1;
                        }
                    }
                }
            }
            assert!(seam > 0, "seed {seed}: a rock with no seam in it at all");
            assert!(
                open > 0,
                "seed {seed}: {seam} cells of seam and not one of them visible, \
                 so there is nothing on this rock to tell a player it is worth cutting"
            );
            // A third is the loose ceiling: at the shipped fifth the measured
            // share over these seeds is an eighth, so this catches an outcrop
            // that has turned into a coat of paint without pinning the tuning.
            assert!(
                open * 3 <= seam,
                "seed {seed}: {open} of {seam} seam cells are on the surface, \
                 which is a seam painted on rather than a rock with one in it"
            );
        }
    }

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
                let steps: [(i32, i32, i32); 6] = [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ];
                for (dx, dy, dz) in steps {
                    let (a, b, c) = (i as i32 + dx, j as i32 + dy, k as i32 + dz);
                    if a < 0
                        || b < 0
                        || c < 0
                        || a >= m.nx as i32
                        || b >= m.ny as i32
                        || c >= m.nz as i32
                    {
                        continue;
                    }
                    let q = m.index(a as usize, b as usize, c as usize);
                    if solid[q] && !seen[q] {
                        seen[q] = true;
                        stack.push(q);
                    }
                }
            }
            assert_eq!(
                count,
                solid.iter().filter(|&&s| s).count(),
                "seed {seed} came out in pieces"
            );
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
                        let edge = i == 0
                            || j == 0
                            || k == 0
                            || i == m.nx - 1
                            || j == m.ny - 1
                            || k == m.nz - 1;
                        if edge {
                            assert_eq!(
                                m.grid[m.index(i, j, k)],
                                mat::EMPTY,
                                "seed {seed} reaches its wall"
                            );
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

    /// The volume radius has to sit INSIDE the bounding one and be a fair
    /// measure of the lump, or the thing the swarm navigates round disagrees
    /// with the thing that is drawn. On a rock stretched on one axis the two
    /// differ a lot, which is exactly the case that went wrong.
    #[test]
    fn a_rock_reports_a_radius_that_matches_its_own_size() {
        for seed in 0..8u64 {
            let m = generate(22, 0.2, seed);
            let (vr, br) = (m.volume_radius(), m.radius());
            assert!(vr > 0.0, "seed {seed} has no volume");
            assert!(
                vr < br,
                "seed {seed}: volume radius {vr} is not inside the bounding {br}"
            );
            // And it is a MEAN, not a token: a rock whose volume radius were a
            // tenth of its bounding one would put the swarm inside it.
            assert!(
                vr > br * 0.45,
                "seed {seed}: {vr} against {br} is far too small"
            );
        }
    }

    /// And it carries ore, or the second surface is a material nothing is
    /// drawn in and the seam is a claim rather than a picture.
    #[test]
    fn a_rock_carries_ore() {
        let m = generate(24, 0.2, 5);
        let ore = m.grid.iter().filter(|&&x| x == mat::ACCENT).count();
        let stone = m.grid.iter().filter(|&&x| x == mat::PLATE).count();
        assert!(ore > 0, "no ore at all");
        assert!(
            ore < stone / 4,
            "{ore} ore against {stone} stone is a rock made of ore"
        );
    }
}
