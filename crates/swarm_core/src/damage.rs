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

use crate::fx::Blast;
pub use crate::heat::*;
use crate::voxel::{mat, VoxelModel, NEIGHBOURS};
pub use crate::wound::*;

/// Cells on a side of one re-mesh block.
pub const BRICK: usize = 8;

/// Not yet dead.
const ALIVE: u32 = u32::MAX;

/// A cell that died BEFORE this action began: dead, and cold from the first
/// frame it is drawn.
///
/// A run carries its flagship's wounds from one system to the next, and a
/// ship that arrived scarred must not arrive glowing. There is no way to say
/// that with a tick, because `heat_of` measures from a tick in THIS action
/// and there is no tick before nought, so it is a sentinel rather than an
/// old number.
const COLD: u32 = u32::MAX - 1;

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
        Self::with_armour(m, 1.0)
    }

    /// The same grid with every cell's hit points multiplied.
    ///
    /// A SCALE rather than a second table, because what a cell is made of and
    /// how much of a beating the ship it is part of can take are two
    /// different facts: `hp_for` stays the material's answer, and the ship
    /// says how heavily it is armoured. The tests below drive the bare grid,
    /// so a number tuned for the game cannot quietly move what they pin.
    pub fn with_armour(m: &VoxelModel, armour: f32) -> Self {
        let n = m.len();
        let armour = armour.max(0.0);
        let max_hp: Vec<f32> = m.grid.iter().map(|&x| hp_for(x) * armour).collect();
        let bricks = [
            m.nx.div_ceil(BRICK),
            m.ny.div_ceil(BRICK),
            m.nz.div_ceil(BRICK),
        ];
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
            Some(COLD) => 0.0,
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
            if t == ALIVE || t == COLD {
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
        (
            n % self.nx,
            (n / self.nx) % self.ny,
            n / (self.nx * self.ny),
        )
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
            if ni < 0
                || nj < 0
                || nk < 0
                || ni as usize >= self.nx
                || nj as usize >= self.ny
                || nk as usize >= self.nz
            {
                continue;
            }
            let b = self.brick_of(self.index(ni as usize, nj as usize, nk as usize));
            self.dirty[b] = true;
        }
        Some(Breach {
            cell: n as u32,
            tick,
            outward,
        })
    }

    /// Kill a cell outright, whatever its hit points. What a reactor does to
    /// the ball around it, and what a wreck's cut does to the cells on the
    /// other side of it. A cell already dead or never there is left alone.
    pub fn kill(&mut self, n: usize, tick: u32) -> Option<Breach> {
        self.chip(n, f32::MAX, tick, [0.0; 3])
    }

    /// Put one dead cell back: whole, live, and no longer a hole.
    ///
    /// The exact undoing of `chip`, which is why it lives beside it: the
    /// same brick and the same neighbours' bricks are dirtied, because a
    /// cell coming BACK closes the faces its neighbours had against the gap
    /// exactly as its dying opened them. Nothing else in the game does this
    /// yet: a tender is the one thing that mends rather than breaks, and a
    /// wound that could only ever get worse is a run with one direction in
    /// it.
    ///
    /// Answers whether it mended anything, so a caller paying by the cell
    /// pays for work that was done.
    pub fn mend(&mut self, n: usize) -> bool {
        if n >= self.died_at.len() || !self.is_dead(n) || self.max_hp[n] <= 0.0 {
            return false;
        }
        self.died_at[n] = ALIVE;
        self.hp[n] = self.max_hp[n];
        self.dead -= 1;
        let own = self.brick_of(n);
        self.dirty[own] = true;
        let (i, j, k) = self.at(n);
        for (di, dj, dk) in NEIGHBOURS {
            let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
            if ni < 0
                || nj < 0
                || nk < 0
                || ni as usize >= self.nx
                || nj as usize >= self.ny
                || nk as usize >= self.nz
            {
                continue;
            }
            let b = self.brick_of(self.index(ni as usize, nj as usize, nk as usize));
            self.dirty[b] = true;
        }
        true
    }

    /// The dead cells a tender should close first, at most `limit` of them.
    ///
    /// What a tender works through: a hull is mended from the OUTSIDE in,
    /// because that is the order it was eaten and because plating closed
    /// over a hole nothing has filled would be a picture nobody believes.
    pub fn holes(&self, m: &VoxelModel, limit: usize) -> Vec<usize> {
        let mut out: Vec<usize> = (0..m.len())
            .filter(|&n| self.is_dead(n) && self.max_hp[n] > 0.0)
            .collect();
        // Nearest the skin first, which on this lattice is the cell with the
        // most empty neighbours: a crater's rim before its floor.
        out.sort_by_key(|&n| {
            let (i, j, k) = self.at(n);
            let open = NEIGHBOURS
                .iter()
                .filter(|(di, dj, dk)| {
                    let (a, b, c) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                    !m.inside(a, b, c)
                        || m.grid[m.index(a as usize, b as usize, c as usize)] == mat::EMPTY
                        || self.is_dead(m.index(a as usize, b as usize, c as usize))
                })
                .count();
            (6 - open, n)
        });
        out.truncate(limit);
        out
    }

    /// Kill a cell as an OLD wound: dead, charred, and never hot.
    ///
    /// This is what a run's scars are made of. Everything downstream sees an
    /// ordinary dead cell, so the four layer wound, the bricks, the vents and
    /// the mesher are untouched: only the heat ramp is told the fire went out
    /// before this system started.
    pub fn scar(&mut self, n: usize) -> Option<Breach> {
        let b = self.kill(n, 0);
        if b.is_some() {
            self.died_at[n] = COLD;
        }
        b
    }

    /// The nearest live cell with a face open to space, within `reach` cells
    /// of a point in the model's frame. This is where a bite lands: a mote
    /// chews the surface it is standing on, and the surface is whatever is
    /// exposed now, plate at first and the machinery behind it once the plate
    /// is gone.
    pub fn nearest_exposed(
        &self,
        m: &VoxelModel,
        p: [f32; 3],
        reach: i32,
    ) -> Option<(usize, [f32; 3])> {
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
                    if best.is_none_or(|(_, _, bd)| d2 < bd) {
                        best = Some((n, outward, d2));
                    }
                }
            }
        }
        best.map(|(n, o, _)| (n, o))
    }

    /// How much soot is on a live cell, from how far it is along solid cells
    /// from the nearest dead one, or none.
    ///
    /// Asked per cell rather than derived by one walk over the whole dead set,
    /// because a brick re-meshes on its own and a ring crosses a brick
    /// boundary: a global walk would have to be redone whole every time one
    /// cell died. Two steps is a short enough march to do here.
    pub fn scorch_at(&self, m: &VoxelModel, n: usize) -> Option<f32> {
        if m.grid[n] == mat::EMPTY || self.is_dead(n) {
            return None;
        }
        let (i, j, k) = m.at(n);
        let solid_live = |i: i32, j: i32, k: i32| -> Option<(usize, bool)> {
            if !m.inside(i, j, k) {
                return None;
            }
            let c = m.index(i as usize, j as usize, k as usize);
            (m.grid[c] != mat::EMPTY).then(|| (c, self.is_dead(c)))
        };
        for (a, b, c) in NEIGHBOURS {
            if let Some((_, true)) = solid_live(i as i32 + a, j as i32 + b, k as i32 + c) {
                return Some(SCORCH_RINGS[0]);
            }
        }
        for (a, b, c) in NEIGHBOURS {
            let Some((mid, false)) = solid_live(i as i32 + a, j as i32 + b, k as i32 + c) else {
                continue;
            };
            let (x, y, z) = m.at(mid);
            for (d, e, f) in NEIGHBOURS {
                if let Some((_, true)) = solid_live(x as i32 + d, y as i32 + e, z as i32 + f) {
                    return Some(SCORCH_RINGS[1]);
                }
            }
        }
        None
    }

    /// Everything inside a blast dies at once, and every cell that does is
    /// answered so the app can throw it.
    ///
    /// The blast is measured at its FULL radius rather than at its radius this
    /// tick: a hull inside an explosion is gone, and staging the cells over
    /// the two dozen ticks the fireball takes to open would re-mesh the same
    /// bricks twenty four times to no visible end.
    pub fn blast_cells(&mut self, m: &VoxelModel, b: &Blast, tick: u32) -> Vec<Breach> {
        let mut out = Vec::new();
        let r2 = b.radius * b.radius;
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY || self.is_dead(n) {
                continue;
            }
            let p = m.centre_of(n);
            let d = [p[0] - b.at[0], p[1] - b.at[1], p[2] - b.at[2]];
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > r2 {
                continue;
            }
            let outward = crate::fx::normalise(d);
            if let Some(br) = self.chip(n, f32::MAX, tick, outward) {
                out.push(br);
            }
        }
        out
    }

    /// Bite the hull at a point: find the exposed cell there and chip it.
    pub fn bite(&mut self, m: &VoxelModel, p: [f32; 3], amount: f32, tick: u32) -> Option<Breach> {
        let (n, outward) = self.nearest_exposed(m, p, 2)?;
        self.chip(n, amount, tick, outward)
    }

    /// A column of cells killed along a line, `depth` model units deep from
    /// `from` along the unit vector `dir`: what a kinetic round does, one
    /// deep crater rather than a wide one. Every breach it opens faces back
    /// up the hole, which is the way the chunks come out of a bore.
    pub fn bore(
        &mut self,
        m: &VoxelModel,
        from: [f32; 3],
        dir: [f32; 3],
        depth: f32,
        tick: u32,
    ) -> Vec<Breach> {
        let mut out = Vec::new();
        let outward = [-dir[0], -dir[1], -dir[2]];
        let mut last = usize::MAX;
        let step = m.cell * 0.5;
        let mut t = 0.0;
        while t <= depth {
            let p = [
                from[0] + dir[0] * t,
                from[1] + dir[1] * t,
                from[2] + dir[2] * t,
            ];
            t += step;
            let Some((i, j, k)) = m.cell_of_point(p) else {
                continue;
            };
            let n = m.index(i, j, k);
            if n == last {
                continue;
            }
            last = n;
            if let Some(br) = self.chip(n, f32::MAX, tick, outward) {
                out.push(br);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bore_takes_a_column_and_nothing_beside_it() {
        let mut m = VoxelModel::new(6, 6, 6, 0.25);
        for i in 0..6 {
            for j in 0..6 {
                for k in 0..6 {
                    m.set(i, j, k, mat::PLATE, 0x808080);
                }
            }
        }
        let mut d = DamageGrid::new(&m);
        // From the centre of the near x face, three cells deep along +x.
        let from = m.centre_of(m.index(0, 3, 3));
        let got = d.bore(&m, from, [1.0, 0.0, 0.0], 3.0 * m.cell, 7);
        assert_eq!(got.len(), 4, "a start cell and three more: {}", got.len());
        for br in &got {
            let (_, j, k) = m.at(br.cell as usize);
            assert_eq!((j, k), (3, 3));
            assert_eq!(br.outward, [-1.0, 0.0, 0.0]);
        }
        assert_eq!(d.dead_count(), 4);
    }
    use crate::mesh::{exposed_faces, greedy_mesh, mesh_region};

    fn slab() -> VoxelModel {
        let mut m = VoxelModel::new(16, 16, 16, 0.25);
        for k in 2..14 {
            for j in 6..10 {
                for i in 2..14 {
                    m.set(
                        i,
                        j,
                        k,
                        if j == 6 || j == 9 {
                            mat::PLATE
                        } else {
                            mat::MACHINE
                        },
                        0x0095E9,
                    );
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
        assert_eq!(
            heat_of(500, 100),
            1.0,
            "a cell that has not died yet is not cold"
        );
        assert_eq!(ramp(1.0), [1.0, 1.0, 1.0]);
        let cold = ramp(0.0);
        assert!(
            cold[0] > 0.0 && cold[0] < 0.2,
            "char is nearly black, not black"
        );
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
        assert!(
            d.take_dirty().is_empty(),
            "pitting is a repaint, not a re-mesh"
        );
        let b = d
            .chip(n, 70.0, 5, [0.0, 1.0, 0.0])
            .expect("the killing bite");
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
        assert_eq!(
            after.wound.colours[0],
            [1.0, 1.0, 1.0, 1.0],
            "fresh is white hot"
        );
        assert_eq!(
            after.skin_all().quad_cells.len() + after.wound.quad_cells.len(),
            exposed_faces(&m, Some(&d))
        );
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
        assert!(
            dirty.len() < total_bricks,
            "a hit dirtied {} of {}",
            dirty.len(),
            total_bricks
        );
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
    fn soot_rings_a_hole_and_never_lands_in_it() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        assert!(
            (0..m.len()).all(|n| d.scorch_at(&m, n).is_none()),
            "an unhit hull is clean"
        );
        let n = m.index(7, 9, 7);
        d.chip(n, 1000.0, 1, [0.0, 1.0, 0.0]);
        assert_eq!(
            d.scorch_at(&m, n),
            None,
            "a dead cell is a hole, not a stain"
        );
        // Its five solid neighbours are the near ring.
        for (a, b, c) in [(6, 9, 7), (8, 9, 7), (7, 8, 7), (7, 9, 6), (7, 9, 8)] {
            assert_eq!(
                d.scorch_at(&m, m.index(a, b, c)),
                Some(SCORCH_RINGS[0]),
                "({a},{b},{c})"
            );
        }
        // Two steps out is the smudge, three is clean.
        assert_eq!(d.scorch_at(&m, m.index(5, 9, 7)), Some(SCORCH_RINGS[1]));
        assert_eq!(d.scorch_at(&m, m.index(4, 9, 7)), None);
        // Distance is measured along SOLID cells, so soot does not jump a gap:
        // the slab is four cells deep and nothing below it is scorched.
        assert_eq!(
            d.scorch_at(&m, m.index(7, 5, 7)),
            None,
            "below the slab is empty"
        );
        assert!(
            CHAR.iter().all(|&c| c > 0.0 && c < 0.2),
            "char is nearly black, not black"
        );
    }

    #[test]
    fn a_hole_vents_along_its_whole_rim_and_every_plume_goes_out() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        assert!(d.vents(&m, 99).is_empty());
        let n = m.index(7, 9, 7);
        d.chip(n, 1000.0, 1, [0.0, 1.0, 0.0]);
        let v = d.vents(&m, 99);
        // Five solid neighbours, so five faces to vent from; the sixth looks
        // at space and is where the cell went.
        assert_eq!(v.len(), 5);
        let centre = m.centre_of(n);
        for x in &v {
            assert_eq!(x.cell, n as u32);
            // Each sits half a cell off the dead cell's centre, on one axis.
            let off = [
                x.at[0] - centre[0],
                x.at[1] - centre[1],
                x.at[2] - centre[2],
            ];
            let far = off.iter().map(|o| o.abs()).fold(0.0f32, f32::max);
            assert!((far - m.cell * 0.5).abs() < 1e-6, "{off:?}");
            // And it looks back INTO the hole, away from the live cell.
            let dotp: f32 = (0..3).map(|a| off[a] * x.outward[a]).sum();
            assert!(dotp < 0.0, "plume goes through the ship: {x:?}");
        }
        assert_eq!(d.vents(&m, 2).len(), 2, "the limit is honoured");
    }

    #[test]
    fn a_blast_takes_a_sphere_of_hull_at_once() {
        let m = slab();
        let mut d = DamageGrid::new(&m);
        let at = m.centre_of(m.index(7, 8, 7));
        let b = crate::fx::Blast {
            at,
            radius: m.cell * 2.5,
            born: 4,
        };
        let breaches = d.blast_cells(&m, &b, 4);
        assert!(!breaches.is_empty());
        // Every cell within the radius is gone, and nothing outside it is.
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY {
                continue;
            }
            let p = m.centre_of(n);
            let dist =
                ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2)).sqrt();
            assert_eq!(d.is_dead(n), dist <= b.radius, "cell {n} at {dist}");
        }
        assert_eq!(d.dead_count(), breaches.len());
        // Thrown outward from the centre, and the one AT the centre is not
        // thrown nowhere: `normalise` answers a direction either way.
        for br in &breaches {
            let l: f32 = br.outward.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((l - 1.0).abs() < 1e-4, "{:?}", br.outward);
        }
        assert!(d.blast_cells(&m, &b, 5).is_empty(), "nothing dies twice");
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
        let plate_dead = breaches
            .iter()
            .filter(|b| m.grid[b.cell as usize] == mat::PLATE)
            .count();
        let machine_dead = breaches
            .iter()
            .filter(|b| m.grid[b.cell as usize] == mat::MACHINE)
            .count();
        assert_eq!((plate_dead, machine_dead), (9, 1), "{breaches:?}");
        assert_eq!(
            breaches[9].cell as usize,
            m.index(7, 8, 7),
            "the machinery goes last, under the first cell"
        );
        // Every dead cell is within reach of the bite point, and the crater is
        // one connected hole.
        let mut hole = VoxelModel::new(16, 16, 16, m.cell);
        for b in &breaches {
            let (i, j, k) = m.at(b.cell as usize);
            assert!(
                (i as i32 - 7).abs() <= 2 && (j as i32 - 9).abs() <= 3 && (k as i32 - 7).abs() <= 2
            );
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
        assert_eq!(
            c,
            chunk_for(&m, &breaches[0]),
            "a chunk is a function of its breach"
        );
    }

    #[test]
    fn a_tender_mends_a_hole_and_the_brick_knows() {
        let mut m = VoxelModel::new(8, 8, 8, 0.1);
        for i in 1..7 {
            for j in 1..7 {
                for k in 1..7 {
                    m.set(i, j, k, mat::PLATE, 0x808080);
                }
            }
        }
        let mut d = DamageGrid::new(&m);
        let whole = d.dead_count();
        let n = m.index(1, 3, 3);
        d.kill(n, 4);
        d.take_dirty();
        assert!(d.is_dead(n) && d.dead_count() == whole + 1);
        // Mending is the exact undoing of the kill: live, whole, cold, and
        // the bricks that have to be rebuilt are the same ones.
        assert!(d.mend(n), "a hole mends");
        assert!(!d.is_dead(n));
        assert_eq!(d.health(n), 1.0);
        assert_eq!(d.heat(n, 4), 0.0);
        assert_eq!(d.dead_count(), whole);
        assert!(!d.take_dirty().is_empty(), "the hull has to be re-meshed");
        // And it is only ever done once.
        assert!(!d.mend(n), "a live cell is not a hole");
        assert!(
            !d.mend(m.index(0, 0, 0)),
            "nor is a cell that was never there"
        );
        // The holes it works through come rim first: a crater's own floor is
        // the last thing closed over.
        let mut d = DamageGrid::new(&m);
        for k in 2..6 {
            d.kill(m.index(1, 3, k), 0);
        }
        d.kill(m.index(2, 3, 3), 0);
        let holes = d.holes(&m, 9);
        assert_eq!(holes.len(), 5);
        assert_eq!(
            holes[holes.len() - 1],
            m.index(2, 3, 3),
            "the deepest cell is mended last"
        );
    }

    #[test]
    fn a_scar_is_dead_and_cold_from_the_first_frame() {
        let mut m = VoxelModel::new(8, 8, 8, 0.1);
        for i in 2..6 {
            for j in 2..6 {
                for k in 2..6 {
                    m.set(i, j, k, mat::PLATE, 0x808080);
                }
            }
        }
        let mut d = DamageGrid::new(&m);
        let n = m.index(2, 3, 3);
        assert!(d.scar(n).is_some(), "a live cell with a face out scars");
        assert!(d.is_dead(n), "and it is dead, like any other hole");
        // The point of it: a run carries its flagship's wounds from system to
        // system, and the tick a new system starts on is nought, so a scar
        // killed at nought would arrive white hot and burn for fifteen
        // seconds. It is cold at the first frame and stays cold.
        assert_eq!(d.heat(n, 0), 0.0, "cold on the frame it arrives");
        assert_eq!(d.heat(n, COOL_TICKS * 9), 0.0, "and cold for ever after");
        assert_eq!(d.heat_key(0), 0, "so it asks for no repaint");
        // And a fresh hole beside it still burns: the sentinel is per cell.
        let hot = m.index(5, 3, 3);
        d.kill(hot, 0);
        assert_eq!(d.heat(hot, 0), 1.0);
        assert!(d.heat_key(0) > 0);
        // A scar on a cell that is not there is not a scar.
        assert!(d.scar(m.index(0, 0, 0)).is_none());
        assert!(d.scar(n).is_none(), "and a cell only dies once");
    }
}
