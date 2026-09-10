//! What a hit does to the cells, and what a hole looks like as it cools.
//!
//! Ported from the model behind redux-tribes `wound.ts`, on a real time footing:
//! there, cells came off a hull because a turn's event stream said so, and here
//! a mote bites a cell and the cell's own hit points decide. The picture is the
//! same idea either way. A cell that dies stops being drawn, the faces its
//! neighbours had against it are the inside of the ship and are drawn burning,
//! and the burn cools along one ramp.
//!
//! Bricks are how a hull re-meshes what a hit reached and nothing else: the
//! lattice is cut into 8^3 blocks, a dead cell dirties its own block and the
//! neighbour blocks whose cells now face it, and the renderer rebuilds only
//! those.

use crate::rng::drift_of;
use crate::voxel::{mat, VoxelModel, NEIGHBOURS};

/// How long a fresh wound burns, in ticks. Sixty ticks a second, so a hole is
/// glowing for fifteen seconds and is char after that.
pub const COOL_TICKS: u32 = 900;

/// The ramp is quantised so a cooling wound is a repaint every 28 ticks rather
/// than every frame, and so two faces at nearly the same heat merge.
pub const HEAT_STEPS: u32 = 32;

/// Cells on a side of one re-mesh block.
pub const BRICK: usize = 8;

/// Not yet dead.
const ALIVE: u32 = u32::MAX;

/// The heat ramp, hottest first, from `wound.ts`. White hot cores through
/// orange to char, and the last stop is not black: a cold wound is a hole with
/// a burnt edge, and flat black reads as a gap in the mesh.
const HEAT: [[f32; 4]; 5] = [
    [1.00, 1.00, 1.00, 1.00],
    [0.70, 1.00, 0.82, 0.62],
    [0.40, 0.88, 0.44, 0.22],
    [0.18, 0.52, 0.20, 0.09],
    [0.00, 0.10, 0.085, 0.08],
];

/// Where a cell sits on the ramp, given how long ago it died.
pub fn heat_of(died_at: u32, tick: u32) -> f32 {
    let age = tick.saturating_sub(died_at) as f32;
    (1.0 - age / COOL_TICKS as f32).clamp(0.0, 1.0)
}

/// The colour at a heat.
pub fn ramp(heat: f32) -> [f32; 3] {
    for i in 1..HEAT.len() {
        let hi = HEAT[i - 1];
        let lo = HEAT[i];
        if heat > lo[0] || i == HEAT.len() - 1 {
            let span = hi[0] - lo[0];
            let t = if span > 0.0 { ((heat - lo[0]) / span).clamp(0.0, 1.0) } else { 0.0 };
            return [
                lo[1] + (hi[1] - lo[1]) * t,
                lo[2] + (hi[2] - lo[2]) * t,
                lo[3] + (hi[3] - lo[3]) * t,
            ];
        }
    }
    [HEAT[4][1], HEAT[4][2], HEAT[4][3]]
}

/// How much of the char crust still covers what it burned. A third stays once
/// the embers are gone: a crust does not un-char itself.
pub const COLD_CRUST: f32 = 0.34;
pub fn crust_alpha(heat: f32) -> f32 {
    COLD_CRUST + heat * (1.0 - COLD_CRUST)
}

/// Hit points a cell starts with, by what it is made of. Plate is what armour
/// is for; a glowing cell is a light and a light is fragile.
pub fn hp_for(m: u8) -> f32 {
    match m {
        mat::PLATE | mat::SKINNED => 100.0,
        mat::FRAME => 60.0,
        mat::CASE => 50.0,
        mat::MACHINE | mat::ACCENT => 40.0,
        mat::GLOW => 30.0,
        _ => 0.0,
    }
}

/// A cell that died this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breach {
    pub cell: u32,
    pub tick: u32,
    /// Which way out of the hull the face it was bitten from looks.
    pub outward: [f32; 3],
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

#[derive(Clone, Debug)]
pub struct DamageGrid {
    nx: usize,
    ny: usize,
    nz: usize,
    hp: Vec<f32>,
    max_hp: Vec<f32>,
    died_at: Vec<u32>,
    bricks: [usize; 3],
    dirty: Vec<bool>,
    dead: usize,
}

impl DamageGrid {
    pub fn new(m: &VoxelModel) -> Self {
        let n = m.len();
        let max_hp: Vec<f32> = m.grid.iter().map(|&x| hp_for(x)).collect();
        let bricks = [m.nx.div_ceil(BRICK), m.ny.div_ceil(BRICK), m.nz.div_ceil(BRICK)];
        DamageGrid {
            nx: m.nx,
            ny: m.ny,
            nz: m.nz,
            hp: max_hp.clone(),
            max_hp,
            died_at: vec![ALIVE; n],
            dirty: vec![false; bricks[0] * bricks[1] * bricks[2]],
            bricks,
            dead: 0,
        }
    }

    #[inline]
    pub fn is_dead(&self, n: usize) -> bool {
        self.died_at[n] != ALIVE
    }

    pub fn died_at(&self, n: usize) -> Option<u32> {
        (self.died_at[n] != ALIVE).then_some(self.died_at[n])
    }

    /// Hit points left as a share of what the cell started with. One for a
    /// cell never touched, nought for a dead one, and what the renderer pits a
    /// surface by before it breaks.
    pub fn health(&self, n: usize) -> f32 {
        if self.max_hp[n] <= 0.0 {
            0.0
        } else {
            (self.hp[n] / self.max_hp[n]).clamp(0.0, 1.0)
        }
    }

    pub fn heat(&self, n: usize, tick: u32) -> f32 {
        match self.died_at(n) {
            Some(t) => heat_of(t, tick),
            None => 0.0,
        }
    }

    pub fn dead_count(&self) -> usize {
        self.dead
    }

    /// The hottest cell on the hull, quantised: when this changes the wound
    /// needs a repaint and not before.
    pub fn heat_key(&self, tick: u32) -> u32 {
        let mut hottest: f32 = 0.0;
        for &t in &self.died_at {
            if t == ALIVE {
                continue;
            }
            hottest = hottest.max(heat_of(t, tick));
            if hottest >= 1.0 {
                break;
            }
        }
        (hottest * HEAT_STEPS as f32).round() as u32
    }

    #[inline]
    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        i + j * self.nx + k * self.nx * self.ny
    }

    #[inline]
    fn at(&self, n: usize) -> (usize, usize, usize) {
        (n % self.nx, (n / self.nx) % self.ny, n / (self.nx * self.ny))
    }

    pub fn brick_count(&self) -> usize {
        self.dirty.len()
    }

    pub fn brick_of(&self, n: usize) -> usize {
        let (i, j, k) = self.at(n);
        (i / BRICK) + (j / BRICK) * self.bricks[0] + (k / BRICK) * self.bricks[0] * self.bricks[1]
    }

    /// The cells a brick covers, `lo <= c < hi`, clipped to the lattice.
    pub fn brick_bounds(&self, b: usize) -> ([usize; 3], [usize; 3]) {
        let bi = b % self.bricks[0];
        let bj = (b / self.bricks[0]) % self.bricks[1];
        let bk = b / (self.bricks[0] * self.bricks[1]);
        let lo = [bi * BRICK, bj * BRICK, bk * BRICK];
        let hi = [
            (lo[0] + BRICK).min(self.nx),
            (lo[1] + BRICK).min(self.ny),
            (lo[2] + BRICK).min(self.nz),
        ];
        (lo, hi)
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = true);
    }

    /// The bricks that need re-meshing, and clear them.
    pub fn take_dirty(&mut self) -> Vec<usize> {
        let out: Vec<usize> = (0..self.dirty.len()).filter(|&b| self.dirty[b]).collect();
        self.dirty.iter_mut().for_each(|d| *d = false);
        out
    }

    /// Take `amount` off a cell. Answers the breach when this is the bite that
    /// kills it; a cell already dead or never there is not damaged twice.
    pub fn chip(&mut self, n: usize, amount: f32, tick: u32, outward: [f32; 3]) -> Option<Breach> {
        if self.max_hp[n] <= 0.0 || self.is_dead(n) {
            return None;
        }
        self.hp[n] -= amount;
        if self.hp[n] > 0.0 {
            return None;
        }
        self.hp[n] = 0.0;
        self.died_at[n] = tick;
        self.dead += 1;
        // Its own brick, and every neighbour's brick: the neighbour's face
        // toward this cell is exposed now and is meshed where the neighbour is.
        let own = self.brick_of(n);
        self.dirty[own] = true;
        let (i, j, k) = self.at(n);
        for (di, dj, dk) in NEIGHBOURS {
            let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
            if ni < 0 || nj < 0 || nk < 0 || ni as usize >= self.nx || nj as usize >= self.ny || nk as usize >= self.nz {
                continue;
            }
            let b = self.brick_of(self.index(ni as usize, nj as usize, nk as usize));
            self.dirty[b] = true;
        }
        Some(Breach { cell: n as u32, tick, outward })
    }

    /// The nearest live cell with a face open to space, within `reach` cells
    /// of a point in the model's frame. This is where a bite lands: a mote
    /// chews the surface it is standing on, and the surface is whatever is
    /// exposed now, plate at first and the machinery behind it once the plate
    /// is gone.
    pub fn nearest_exposed(&self, m: &VoxelModel, p: [f32; 3], reach: i32) -> Option<(usize, [f32; 3])> {
        let ci = (p[0] / m.cell + m.nx as f32 / 2.0).floor() as i32;
        let cj = (p[1] / m.cell + m.ny as f32 / 2.0).floor() as i32;
        let ck = (p[2] / m.cell + m.nz as f32 / 2.0).floor() as i32;
        let live = |i: i32, j: i32, k: i32| -> bool {
            m.inside(i, j, k) && {
                let n = m.index(i as usize, j as usize, k as usize);
                m.grid[n] != mat::EMPTY && !self.is_dead(n)
            }
        };
        let mut best: Option<(usize, [f32; 3], f32)> = None;
        for dk in -reach..=reach {
            for dj in -reach..=reach {
                for di in -reach..=reach {
                    let (i, j, k) = (ci + di, cj + dj, ck + dk);
                    if !live(i, j, k) {
                        continue;
                    }
                    // Which face is open, and therefore which way is out.
                    let mut out = None;
                    for (a, b, c) in NEIGHBOURS {
                        if !live(i + a, j + b, k + c) {
                            out = Some([a as f32, b as f32, c as f32]);
                            break;
                        }
                    }
                    let Some(outward) = out else { continue };
                    let n = m.index(i as usize, j as usize, k as usize);
                    let c = m.centre_of(n);
                    let d2 = (c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) + (c[2] - p[2]).powi(2);
                    if best.map_or(true, |(_, _, bd)| d2 < bd) {
                        best = Some((n, outward, d2));
                    }
                }
            }
        }
        best.map(|(n, o, _)| (n, o))
    }

    /// Bite the hull at a point: find the exposed cell there and chip it.
    pub fn bite(&mut self, m: &VoxelModel, p: [f32; 3], amount: f32, tick: u32) -> Option<Breach> {
        let (n, outward) = self.nearest_exposed(m, p, 2)?;
        self.chip(n, amount, tick, outward)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{exposed_faces, greedy_mesh, mesh_region};

    fn slab() -> VoxelModel {
        let mut m = VoxelModel::new(16, 16, 16, 0.25);
        for k in 2..14 {
            for j in 6..10 {
                for i in 2..14 {
                    m.set(i, j, k, if j == 6 || j == 9 { mat::PLATE } else { mat::MACHINE }, 0x0095E9);
                }
            }
        }
        m
    }

    #[test]
    fn heat_cools_to_char_and_no_further() {
        assert_eq!(heat_of(100, 100), 1.0);
        assert!((heat_of(100, 100 + COOL_TICKS / 2) - 0.5).abs() < 1e-6);
        assert_eq!(heat_of(100, 100 + COOL_TICKS), 0.0);
        assert_eq!(heat_of(100, 100 + COOL_TICKS * 3), 0.0);
        assert_eq!(heat_of(500, 100), 1.0, "a cell that has not died yet is not cold");
        assert_eq!(ramp(1.0), [1.0, 1.0, 1.0]);
        let cold = ramp(0.0);
        assert!(cold[0] > 0.0 && cold[0] < 0.2, "char is nearly black, not black");
        let mut last = 3.0;
        for s in (0..=32).rev() {
            let c = ramp(s as f32 / 32.0);
            let lum = c[0] + c[1] + c[2];
            assert!(lum <= last + 1e-6, "ramp brightens as it cools at {s}");
            last = lum;
        }
        assert!((crust_alpha(1.0) - 1.0).abs() < 1e-6);
        assert!((crust_alpha(0.0) - COLD_CRUST).abs() < 1e-6);
    }

    #[test]
    fn a_cell_dies_once_and_dirties_its_neighbours_bricks() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        assert!(d.take_dirty().is_empty());
        let n = m.index(7, 9, 7);
        assert_eq!(d.chip(n, 40.0, 1, [0.0, 1.0, 0.0]), None);
        assert!((d.health(n) - 0.6).abs() < 1e-6);
        assert!(d.take_dirty().is_empty(), "pitting is a repaint, not a re-mesh");
        let b = d.chip(n, 70.0, 5, [0.0, 1.0, 0.0]).expect("the killing bite");
        assert_eq!(b.cell, n as u32);
        assert_eq!(b.tick, 5);
        assert!(d.is_dead(n));
        assert_eq!(d.dead_count(), 1);
        assert_eq!(d.chip(n, 1000.0, 9, [0.0, 1.0, 0.0]), None, "not twice");
        assert_eq!(d.dead_count(), 1);
        let dirty = d.take_dirty();
        // (7,9,7) sits in brick (0,1,0). Its +x neighbour (8,9,7) is in brick
        // (1,1,0) and its +z neighbour (7,9,8) in (0,1,1); the other four are
        // in its own. Three bricks, and not the whole hull.
        assert!(dirty.contains(&d.brick_of(n)));
        assert!(dirty.contains(&d.brick_of(m.index(8, 9, 7))));
        assert!(dirty.contains(&d.brick_of(m.index(7, 9, 8))));
        assert_eq!(dirty.len(), 3, "{dirty:?}");
        assert!(d.take_dirty().is_empty());
    }

    #[test]
    fn a_hole_exposes_the_inside_and_the_wound_is_drawn_hot() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        let before = greedy_mesh(&m, Some((&d, 0)));
        assert!(before.wound.is_empty());
        let n = m.index(7, 9, 7);
        d.chip(n, 1000.0, 10, [0.0, 1.0, 0.0]);
        let after = greedy_mesh(&m, Some((&d, 10)));
        // The dead plate cell had one face out. Its five live neighbours now
        // each show one face into the hole, and those are wound faces.
        assert_eq!(after.wound.quads(), 5);
        assert_eq!(after.wound.colours[0], [1.0, 1.0, 1.0, 1.0], "fresh is white hot");
        assert_eq!(after.skin_all().quad_cells.len() + after.wound.quad_cells.len(), exposed_faces(&m, Some(&d)));
        let cooled = greedy_mesh(&m, Some((&d, 10 + COOL_TICKS)));
        assert_eq!(cooled.wound.quads(), 5);
        assert!(cooled.wound.colours[0][0] < 0.2, "char after COOL_TICKS");
        assert_eq!(d.heat_key(10), HEAT_STEPS);
        assert_eq!(d.heat_key(10 + COOL_TICKS), 0);
    }

    #[test]
    fn remeshing_only_dirty_bricks_equals_a_full_remesh() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        let tick = 3;
        let total_bricks = d.brick_count();
        // Mesh every brick once, the way a renderer holds them.
        let mut bricks: Vec<_> = (0..total_bricks)
            .map(|b| {
                let (lo, hi) = d.brick_bounds(b);
                mesh_region(&m, Some((&d, tick)), lo, hi)
            })
            .collect();
        for (i, j, k) in [(7, 9, 7), (8, 9, 7), (7, 8, 7), (3, 6, 3)] {
            d.chip(m.index(i, j, k), 1000.0, tick, [0.0, 1.0, 0.0]);
        }
        let dirty = d.take_dirty();
        assert!(dirty.len() < total_bricks, "a hit dirtied {} of {}", dirty.len(), total_bricks);
        for &b in &dirty {
            let (lo, hi) = d.brick_bounds(b);
            bricks[b] = mesh_region(&m, Some((&d, tick)), lo, hi);
        }
        let mut skin = 0;
        let mut wound = 0;
        for s in &bricks {
            skin += s.skin_all().quad_cells.len();
            wound += s.wound.quad_cells.len();
        }
        let whole = greedy_mesh(&m, Some((&d, tick)));
        assert_eq!(skin, whole.skin_all().quad_cells.len());
        assert_eq!(wound, whole.wound.quad_cells.len());
        assert_eq!(skin + wound, exposed_faces(&m, Some(&d)));
    }

    #[test]
    fn a_bite_lands_on_the_nearest_exposed_cell_and_chews_inward() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        // Just above the top plate at (7, 9, 7): centre of that cell plus half a cell up.
        let top = m.centre_of(m.index(7, 9, 7));
        let p = [top[0], top[1] + m.cell, top[2]];
        let (n, out) = d.nearest_exposed(&m, p, 2).unwrap();
        assert_eq!(n, m.index(7, 9, 7));
        assert_eq!(out, [0.0, 1.0, 0.0]);
        let mut breaches = Vec::new();
        for t in 0..40 {
            if let Some(b) = d.bite(&m, p, 25.0, t) {
                breaches.push(b);
            }
        }
        // A mote parked over one cell eats a CRATER, not a drill hole. The
        // plate under it goes in four bites, and then the nearest exposed cell
        // is the plate BESIDE the hole (1.4 cells off) rather than the machine
        // cell under it (2 off), so the dish widens to three by three before
        // the first cell of machinery goes. Forty bites of 25 is a thousand
        // points: nine plate cells and one of machinery.
        assert_eq!(breaches.len(), 10, "{breaches:?}");
        assert_eq!(breaches[0].cell as usize, m.index(7, 9, 7));
        let plate_dead = breaches.iter().filter(|b| m.grid[b.cell as usize] == mat::PLATE).count();
        let machine_dead = breaches.iter().filter(|b| m.grid[b.cell as usize] == mat::MACHINE).count();
        assert_eq!((plate_dead, machine_dead), (9, 1), "{breaches:?}");
        assert_eq!(breaches[9].cell as usize, m.index(7, 8, 7), "the machinery goes last, under the first cell");
        // Every dead cell is within reach of the bite point, and the crater is
        // one connected hole.
        let mut hole = VoxelModel::new(16, 16, 16, m.cell);
        for b in &breaches {
            let (i, j, k) = m.at(b.cell as usize);
            assert!((i as i32 - 7).abs() <= 2 && (j as i32 - 9).abs() <= 3 && (k as i32 - 7).abs() <= 2);
            hole.set(i, j, k, mat::PLATE, 1);
        }
        assert_eq!(hole.components(), 1);
        // And the same bites on a fresh grid land on the same cells.
        let mut again = DamageGrid::new(&m);
        let replay: Vec<_> = (0..40).filter_map(|t| again.bite(&m, p, 25.0, t)).collect();
        assert_eq!(replay, breaches);
        let c = chunk_for(&m, &breaches[0]);
        assert_eq!(c.colour, 0x0095E9);
        assert!(c.velocity[1] > 0.0, "thrown outward");
        assert_eq!(c, chunk_for(&m, &breaches[0]), "a chunk is a function of its breach");
    }
}
