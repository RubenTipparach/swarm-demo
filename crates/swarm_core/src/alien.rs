//! The swarm's bodies, as voxels.
//!
//! A mote is drawn by the same mesher as a hull so that it is made of the same
//! stuff on screen: a cell is filled or it is not, faces are where a cell meets
//! space, and a hit takes cells off. Everything here is built from a seed and
//! nothing else, mirrored about x, and connected on six neighbours, and the
//! tests hold it to all three.
//!
//! Four archetypes. A DRONE is the bulk of the swarm: a body, a head, legs, a
//! pair of eyes. A LANCER is long, with a spike forward and swept fins aft, and
//! it rams. A CHEWER is fat and short with mandibles as big as its head: it is
//! the one that eats armour. A MOTHER is three segments long, carries egg sacs
//! that glow, and is the thing worth killing.

use crate::rng::Rng;
use crate::voxel::{mat, purpose, VoxelModel, SURF_DRIVE};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Archetype {
    Drone,
    Lancer,
    Chewer,
    Mother,
}

impl Archetype {
    pub const ALL: [Archetype; 4] = [
        Archetype::Drone,
        Archetype::Lancer,
        Archetype::Chewer,
        Archetype::Mother,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Archetype::Drone => "drone",
            Archetype::Lancer => "lancer",
            Archetype::Chewer => "chewer",
            Archetype::Mother => "mother",
        }
    }

    /// Lattice side, in cells.
    pub fn lattice(self) -> usize {
        match self {
            Archetype::Mother => 24,
            _ => 16,
        }
    }

    /// World units per cell. A drone is about 0.6 units long against a
    /// frigate's 7; a mother is about 2.4.
    pub fn cell(self) -> f32 {
        match self {
            Archetype::Drone => 0.0375,
            Archetype::Lancer => 0.045,
            Archetype::Chewer => 0.05,
            Archetype::Mother => 0.1,
        }
    }
}

/// Chitin, bone and acid. The hull palettes are the navies'; these are nobody's.
pub mod palette {
    pub const CHITIN_DARK: u32 = 0x2A1638;
    pub const CHITIN: u32 = 0x4A2A66;
    pub const CHITIN_LIT: u32 = 0x7A4DA8;
    pub const BONE: u32 = 0xD9C7A8;
    /// What is ALIVE about a mote is lit with this: eyes, and the lights down
    /// a carrier's flank. Taken down from 0x9BFF4A, which at the emissive a
    /// lit cell carries came out as a lamp rather than as an eye once the
    /// bodies round it went dark.
    pub const GLOW: u32 = 0x6FB835;
    pub const GLOW_HOT: u32 = 0xE8FFB0;
    pub const MAW: u32 = 0x1A0A22;
    /// What a drive burns. Cold blue against the green everything ALIVE about
    /// a mote is lit with, so an engine reads as machinery and an eye reads as
    /// an animal even at one pixel.
    pub const DRIVE: u32 = 0x6FE8FF;
    pub const DRIVE_HOT: u32 = 0xD8FBFF;
}

struct Builder {
    m: VoxelModel,
}

impl Builder {
    fn new(n: usize, cell: f32) -> Self {
        Builder {
            m: VoxelModel::new(n, n, n, cell),
        }
    }

    fn put(&mut self, i: i32, j: i32, k: i32, mt: u8, colour: u32) -> bool {
        if !self.m.inside(i, j, k) {
            return false;
        }
        self.m.set(i as usize, j as usize, k as usize, mt, colour);
        true
    }

    fn filled(&self, i: i32, j: i32, k: i32) -> bool {
        self.m.get(i, j, k) != mat::EMPTY
    }

    /// An ellipsoid about a CELL CENTRE frame: `c` is measured in cells with
    /// the lattice's own centre at n/2, so an ellipsoid centred on x = n/2 is
    /// exactly mirrored about the midplane between cells n/2-1 and n/2.
    fn ellipsoid(&mut self, c: [f32; 3], r: [f32; 3], mt: u8, colour: u32) {
        let n = self.m.nx as i32;
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let dx = (i as f32 + 0.5 - c[0]) / r[0];
                    let dy = (j as f32 + 0.5 - c[1]) / r[1];
                    let dz = (k as f32 + 0.5 - c[2]) / r[2];
                    if dx * dx + dy * dy + dz * dz <= 1.0 {
                        self.put(i, j, k, mt, colour);
                    }
                }
            }
        }
    }

    /// A limb: from a cell that is already filled, a run of steps each one
    /// cell from the last, so it is connected by construction.
    fn walk(&mut self, from: [i32; 3], steps: &[[i32; 3]], mt: u8, colour: u32) {
        debug_assert!(
            self.filled(from[0], from[1], from[2]),
            "a limb starts on the body"
        );
        let mut p = from;
        for s in steps {
            debug_assert_eq!(
                s[0].abs() + s[1].abs() + s[2].abs(),
                1,
                "a limb step is one face, not a corner"
            );
            p = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
            if !self.put(p[0], p[1], p[2], mt, colour) {
                return;
            }
        }
    }

    /// Copy the x >= n/2 half onto the other, so what was built on one side is
    /// on both. Mirroring i to n-1-i is exact on an even lattice.
    fn mirror_x(&mut self) {
        let n = self.m.nx;
        for k in 0..n {
            for j in 0..n {
                for i in n / 2..n {
                    let src = self.m.index(i, j, k);
                    let dst = self.m.index(n - 1 - i, j, k);
                    self.m.grid[dst] = self.m.grid[src];
                    self.m.colour[dst] = self.m.colour[src];
                    // The SURFACE too, or a mote's port engines come back
                    // as chitin and only one side of it lights up.
                    self.m.surf[dst] = self.m.surf[src];
                    self.m.purp[dst] = self.m.purp[src];
                }
            }
        }
    }

    /// The outermost filled cell along +x at a given (y, z) row, on the right
    /// half, for hanging a limb off the flank.
    fn flank(&self, j: i32, k: i32) -> Option<i32> {
        let n = self.m.nx as i32;
        (n / 2..n).rev().find(|&i| self.filled(i, j, k))
    }

    /// The top filled cell in a column.
    fn crown(&self, i: i32, k: i32) -> Option<i32> {
        let n = self.m.ny as i32;
        (0..n).rev().find(|&j| self.filled(i, j, k))
    }

    /// The bottom filled cell in a column.
    fn keel(&self, i: i32, k: i32) -> Option<i32> {
        let n = self.m.ny as i32;
        (0..n).find(|&j| self.filled(i, j, k))
    }

    /// The aftmost filled cell in a line along the hull.
    fn stern(&self, i: i32, j: i32) -> Option<i32> {
        let n = self.m.nz as i32;
        (0..n).find(|&k| self.filled(i, j, k))
    }

    /// Mark a cell as SELF LIT.
    ///
    /// `surf` is what the mesher keys its passes on, so a cell marked here
    /// comes back in its own mesh for nothing: the app draws `skin[SURF_DRIVE]`
    /// with an unlit emissive material and the rest with chitin, and neither
    /// has to know which cells those were.
    fn light(&mut self, i: i32, j: i32, k: i32) {
        if self.m.inside(i, j, k) {
            let n = self.m.index(i as usize, j as usize, k as usize);
            self.m.surf[n] = SURF_DRIVE;
        }
    }

    /// An engine: the aftmost cell of its column, relit and marked.
    ///
    /// Marked as PROPULSION as well as lit, because that is what makes it an
    /// engine rather than a light: `engines_of` clusters on the purpose, so a
    /// mote's plume comes off the same query a ship's does.
    fn engine(&mut self, i: i32, j: i32, colour: u32) -> Option<i32> {
        let k = self.stern(i, j)?;
        if !self.put(i, j, k, mat::GLOW, colour) {
            return None;
        }
        self.light(i, j, k);
        let n = self.m.index(i as usize, j as usize, k as usize);
        self.m.purp[n] = purpose::PROPULSION;
        Some(k)
    }

    /// Wall a lit cell in on every side but the one it shines out of.
    ///
    /// A drive is the aftmost cell of its column, so its AFT face is open by
    /// construction and so is whichever of the other five the body happened
    /// not to reach. That is a light that wraps round a corner: three faces
    /// lit reads as a lamp stuck on the outside, and what an exhaust looks
    /// like is one face at the bottom of a recess. Anything empty beside it
    /// becomes chitin, so the only face left open is the one facing aft.
    ///
    /// Five directions and never the aft one, which is the whole point of it.
    fn shroud(&mut self, i: i32, j: i32, k: i32, colour: u32) {
        for (di, dj, dk) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1)] {
            let (a, b, c) = (i + di, j + dj, k + dk);
            if self.m.get(a, b, c) == mat::EMPTY {
                self.put(a, b, c, mat::CASE, colour);
            }
        }
    }

    /// An eye is a body cell relit, on the crown of its column, so it is in
    /// the skin and cannot float beside it. Asked for a column with nothing
    /// in it, it walks inboard until it finds one: a narrow head still gets
    /// its pair, one cell closer to the midline.
    fn eye(&mut self, i: i32, k: i32, colour: u32) -> bool {
        let mid = (self.m.nx / 2) as i32;
        for ii in (mid..=i).rev() {
            if let Some(j) = self.crown(ii, k) {
                let ok = self.put(ii, j, k, mat::GLOW, colour);
                if ok {
                    self.light(ii, j, k);
                }
                return ok;
            }
        }
        false
    }

    /// A limb from the keel of a column, hanging down and forward.
    fn hang(&mut self, i: i32, k: i32, steps: &[[i32; 3]], mt: u8, colour: u32) {
        if let Some(j) = self.keel(i, k) {
            self.walk([i, j, k], steps, mt, colour);
        }
    }

    /// Recolour a share of one material's cells to another colour, for the
    /// mottling that keeps two drones from one seed apart.
    fn mottle(&mut self, rng: &mut Rng, of: u8, share: f32, colour: u32) {
        for n in 0..self.m.len() {
            if self.m.grid[n] == of && rng.chance(share) {
                self.m.colour[n] = colour;
            }
        }
    }
}

/// Build a mote. Same archetype and seed, same mote, on any machine.
pub fn generate(arch: Archetype, seed: u64) -> VoxelModel {
    let mut rng = Rng::new(seed ^ ((arch as u64) << 32));
    let n = arch.lattice();
    let mut b = Builder::new(n, arch.cell());
    let c = n as f32 / 2.0;
    match arch {
        Archetype::Drone => drone(&mut b, &mut rng, c),
        Archetype::Lancer => lancer(&mut b, &mut rng, c),
        Archetype::Chewer => chewer(&mut b, &mut rng, c),
        Archetype::Mother => mother(&mut b, &mut rng, c),
    }
    b.mirror_x();
    b.m
}

fn drone(b: &mut Builder, rng: &mut Rng, c: f32) {
    use palette::*;
    let rx = rng.range(2.2, 2.9);
    let rz = rng.range(3.0, 3.8);
    b.ellipsoid([c, c, c - 0.5], [rx, 2.0, rz], mat::CASE, CHITIN);
    b.ellipsoid(
        [c, c + 0.3, c + 3.5],
        [1.8, 1.6, 1.9],
        mat::CASE,
        CHITIN_LIT,
    );
    b.mottle(rng, mat::CASE, 0.18, CHITIN_DARK);
    // Eyes, a mirrored pair, forward and high on the head.
    let hz = (c + 4.5) as i32;
    b.eye(c as i32 + 1, hz, GLOW);
    // Mandibles hang under the head and reach forward.
    b.hang(
        c as i32 + 1,
        hz - 1,
        &[[0, -1, 0], [0, 0, 1], [0, 0, 1], [1, 0, 0]],
        mat::MACHINE,
        BONE,
    );
    // The drive: ONE light, on the centreline, a row down from the middle.
    //
    // It was three calls and six cells, two of them the hot white, and against
    // a swarm that shades itself that made every bug a bank of headlamps seen
    // from behind: a stern that outshines the eyes is a bug flying backwards
    // as far as the picture is concerned. The eyes read first now and this
    // reads second.
    //
    // Down a row rather than on the middle line, which is what INSETS it: the
    // body is an ellipsoid, so the row above and the columns either side stand
    // further aft than this one does, and the light sits in the notch they
    // leave with chitin over it and chitin down both sides. A light flush with
    // the widest part of the stern is a lamp stuck on the back; one in a
    // recess is an exhaust.
    //
    // Two cells wide and not one, and that is the mirror rather than a choice:
    // the lattice is folded about the plane between the two centre columns, so
    // anything on the centreline is a pair by construction. They are adjacent,
    // so they read as one light.
    //
    // And it is WALLED IN afterwards. The stern of a column is open aft and
    // open wherever the ellipsoid stopped short, which was two faces on a good
    // seed and three on most: a light round a corner is a lamp stuck on the
    // back. `shroud` fills whatever is empty on the other five, so one face
    // shows and it is the one pointing the way the mote came from.
    if let Some(k) = b.engine(c as i32, c as i32 - 1, DRIVE) {
        b.shroud(c as i32, c as i32 - 1, k, CHITIN_DARK);
    }
    // Legs: two or three pairs off the flank, out then down.
    let pairs = rng.int(2, 3);
    for p in 0..pairs {
        let k = (c - 2.5 + p as f32 * 2.5) as i32;
        let j = c as i32 - 1;
        if let Some(i) = b.flank(j, k) {
            let reach = rng.int(1, 2);
            let mut steps = vec![[1, 0, 0]; reach as usize];
            steps.extend([[0, -1, 0], [0, -1, 0]]);
            b.walk([i, j, k], &steps, mat::ACCENT, CHITIN_DARK);
        }
    }
}

fn lancer(b: &mut Builder, rng: &mut Rng, c: f32) {
    use palette::*;
    let rz = rng.range(3.6, 4.4);
    b.ellipsoid([c, c, c - 1.0], [1.7, 1.5, rz], mat::CASE, CHITIN);
    b.ellipsoid([c, c, c + 2.5], [1.4, 1.3, 1.6], mat::CASE, CHITIN_LIT);
    b.mottle(rng, mat::CASE, 0.12, CHITIN_DARK);
    // The lance, from the nose to the front of the lattice.
    let nose = (0..b.m.nz as i32)
        .rev()
        .find(|&k| b.filled(c as i32, c as i32, k))
        .unwrap();
    let len = (b.m.nz as i32 - 1 - nose).min(rng.int(4, 6));
    b.walk(
        [c as i32, c as i32, nose],
        &vec![[0, 0, 1]; len as usize],
        mat::MACHINE,
        BONE,
    );
    // Eyes either side of the lance root.
    b.eye(c as i32 + 1, nose - 1, GLOW);
    // Fins swept back from the tail: a wedge that widens aft.
    let tail = (c - rz + 1.0) as i32;
    for step in 0..3 {
        let k = tail + step;
        if let Some(i) = b.flank(c as i32, k) {
            let span = 3 - step;
            b.walk(
                [i, c as i32, k],
                &vec![[1, 0, 0]; span as usize],
                mat::ACCENT,
                CHITIN_LIT,
            );
        }
    }
    // One big drive on the axis and two outriggers: a lancer is mostly engine.
    b.engine(c as i32, c as i32, DRIVE_HOT);
    b.engine(c as i32 + 1, c as i32, DRIVE_HOT);
    b.engine(c as i32, c as i32 + 1, DRIVE);
    // One leg pair, tucked.
    if let Some(i) = b.flank(c as i32 - 1, c as i32) {
        b.walk(
            [i, c as i32 - 1, c as i32],
            &[[1, 0, 0], [0, -1, 0]],
            mat::ACCENT,
            CHITIN_DARK,
        );
    }
}

fn chewer(b: &mut Builder, rng: &mut Rng, c: f32) {
    use palette::*;
    let r = rng.range(2.9, 3.4);
    b.ellipsoid([c, c, c - 0.5], [r, r * 0.85, r], mat::CASE, CHITIN);
    b.mottle(rng, mat::CASE, 0.25, CHITIN_DARK);
    // The maw: a dark recess in the front, then two mandibles that curve in.
    let front = (c - 0.5 + r) as i32;
    b.put(c as i32, c as i32 - 1, front, mat::CASE, MAW);
    b.put(c as i32, c as i32, front, mat::CASE, MAW);
    b.hang(
        c as i32 + 2,
        front - 1,
        &[[0, -1, 0], [0, 0, 1], [0, 0, 1], [0, 0, 1], [-1, 0, 0]],
        mat::MACHINE,
        BONE,
    );
    // Eyes above the maw.
    b.eye(c as i32 + 1, front - 1, GLOW);
    // Dorsal spines along the crown.
    let spines = rng.int(2, 4);
    for s in 0..spines {
        let k = (c - r + 1.5 + s as f32 * (2.0 * r - 3.0) / spines as f32) as i32;
        let i = c as i32 + rng.int(0, 1);
        if let Some(j) = b.crown(i, k) {
            b.walk([i, j, k], &[[0, 1, 0], [0, 1, 0]], mat::ACCENT, BONE);
        }
    }
    // Drives low and wide: a chewer is pushed rather than flown.
    b.engine(c as i32, c as i32 - 1, DRIVE);
    b.engine(c as i32 + 1, c as i32 - 1, DRIVE);
    b.engine(c as i32 + 2, c as i32 - 1, DRIVE_HOT);
    // Four pairs of short legs.
    for p in 0..4 {
        let k = (c - r + 1.0 + p as f32 * (2.0 * r - 2.0) / 4.0) as i32;
        let j = c as i32 - 2;
        if let Some(i) = b.flank(j, k) {
            b.walk(
                [i, j, k],
                &[[1, 0, 0], [0, -1, 0]],
                mat::ACCENT,
                CHITIN_DARK,
            );
        }
    }
}

fn mother(b: &mut Builder, rng: &mut Rng, c: f32) {
    use palette::*;
    // Three segments along z: abdomen, thorax, head.
    let ra = rng.range(4.2, 4.9);
    b.ellipsoid([c, c, c - 6.0], [ra, ra * 0.85, 5.0], mat::CASE, CHITIN);
    b.ellipsoid([c, c, c], [3.6, 3.2, 3.6], mat::CASE, CHITIN);
    b.ellipsoid(
        [c, c + 0.5, c + 5.0],
        [2.8, 2.5, 2.8],
        mat::CASE,
        CHITIN_LIT,
    );
    b.mottle(rng, mat::CASE, 0.2, CHITIN_DARK);
    // Egg sacs: glowing cells on the abdomen's skin, a seeded scatter.
    let sacs = rng.int(6, 10);
    let mut placed = 0;
    let mut tries = 0;
    while placed < sacs && tries < 200 {
        tries += 1;
        let i = rng.int(c as i32, c as i32 + ra as i32);
        let k = rng.int((c - 10.0) as i32, (c - 2.0) as i32);
        let Some(j) = b.crown(i, k) else { continue };
        if b.m.get(i, j, k) == mat::CASE {
            b.put(
                i,
                j,
                k,
                mat::GLOW,
                if rng.chance(0.3) { GLOW_HOT } else { GLOW },
            );
            placed += 1;
        }
    }
    // Eyes on the head, forward.
    let hz = (c + 6.0) as i32;
    b.eye(c as i32 + 1, hz, GLOW);
    b.eye(c as i32 + 2, hz - 1, GLOW);
    // A bone crest down the thorax.
    for k in (c as i32 - 3)..(c as i32 + 3) {
        if let Some(j) = b.crown(c as i32, k) {
            b.walk([c as i32, j, k], &[[0, 1, 0]], mat::ACCENT, BONE);
        }
    }
    // Four pairs of long legs off the thorax and abdomen.
    for p in 0..4 {
        let k = (c - 6.0 + p as f32 * 3.0) as i32;
        let j = c as i32 - 1;
        if let Some(i) = b.flank(j, k) {
            b.walk(
                [i, j, k],
                &[
                    [1, 0, 0],
                    [1, 0, 0],
                    [1, 0, 0],
                    [0, -1, 0],
                    [0, -1, 0],
                    [0, -1, 0],
                ],
                mat::ACCENT,
                CHITIN_DARK,
            );
        }
    }
    // A carrier's drive block: six across the stern, hottest on the axis.
    for (n, i) in (c as i32..c as i32 + 4).enumerate() {
        b.engine(i, c as i32, if n == 0 { DRIVE_HOT } else { DRIVE });
        b.engine(i, c as i32 - 2, DRIVE);
    }
    // Mandibles.
    b.hang(
        c as i32 + 1,
        hz,
        &[[0, -1, 0], [0, 0, 1], [0, 0, 1], [0, -1, 0], [0, 0, 1]],
        mat::MACHINE,
        BONE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_archetype_is_symmetric_connected_and_lit() {
        for arch in Archetype::ALL {
            let mut counts = std::collections::HashSet::new();
            for seed in 0..8u64 {
                let m = generate(arch, seed);
                assert!(m.symmetric_x(), "{} seed {seed} is lopsided", arch.name());
                assert_eq!(
                    m.components(),
                    1,
                    "{} seed {seed} is in pieces",
                    arch.name()
                );
                let glow = m.grid.iter().filter(|&&x| x == mat::GLOW).count();
                assert!(
                    glow >= 2,
                    "{} seed {seed} has {glow} glow cells",
                    arch.name()
                );
                // Everything self lit is on the lit SURFACE, which is what
                // gives it its own mesh, and every drive is at the stern.
                let lit: Vec<usize> = (0..m.len()).filter(|&n| m.surf[n] == SURF_DRIVE).collect();
                assert!(
                    lit.len() >= 4,
                    "{} seed {seed}: {} lit cells",
                    arch.name(),
                    lit.len()
                );
                for &n in &lit {
                    assert_eq!(m.grid[n], mat::GLOW, "a lit cell that is not a light");
                }
                let drives: Vec<usize> = (0..m.len())
                    .filter(|&n| m.purp[n] == crate::voxel::purpose::PROPULSION)
                    .collect();
                assert!(
                    drives.len() >= 2,
                    "{} seed {seed} has {} engines",
                    arch.name(),
                    drives.len()
                );
                // Aft of the middle, which is what makes them engines: one on
                // the nose would push the wrong way.
                let mid = m.nz / 2;
                for &n in &drives {
                    assert!(m.at(n).2 < mid, "{} engine at z {}", arch.name(), m.at(n).2);
                    assert_eq!(m.surf[n], SURF_DRIVE);
                }
                // And the same query the ships use finds them.
                let found = crate::fx::engines_of(&m);
                assert!(
                    !found.is_empty(),
                    "{} seed {seed}: engines_of found none",
                    arch.name()
                );
                let cells = m.solid_count();
                let (lo, hi) = match arch {
                    Archetype::Drone => (60, 400),
                    Archetype::Lancer => (50, 300),
                    Archetype::Chewer => (100, 600),
                    Archetype::Mother => (600, 3000),
                };
                assert!(
                    (lo..=hi).contains(&cells),
                    "{} seed {seed}: {cells} cells",
                    arch.name()
                );
                counts.insert(cells);
                assert_eq!(generate(arch, seed), m, "same seed, same mote");
            }
            assert!(
                counts.len() >= 2,
                "{}: eight seeds gave one body",
                arch.name()
            );
        }
    }

    /// A drone's drive shows ONE face, and it is the one pointing aft.
    ///
    /// The stern of a column is open aft by construction and open on whatever
    /// side the body stopped short of, which was two faces on a good seed and
    /// three on most: a light round a corner reads as a lamp stuck on the back
    /// rather than as an exhaust in a recess. This is what `shroud` is for, and
    /// it is held per seed because the body is an ellipsoid at a rolled radius
    /// and which side it falls short on moves with the roll.
    #[test]
    fn a_drones_drive_shows_one_face_and_it_faces_aft() {
        for seed in 0..12u64 {
            let m = generate(Archetype::Drone, seed);
            let drives: Vec<usize> = (0..m.len())
                .filter(|&n| m.purp[n] == crate::voxel::purpose::PROPULSION)
                .collect();
            assert_eq!(drives.len(), 2, "seed {seed}: {} drive cells", drives.len());
            for &n in &drives {
                let (i, j, k) = m.at(n);
                let open: Vec<[i32; 3]> = [
                    [1, 0, 0],
                    [-1, 0, 0],
                    [0, 1, 0],
                    [0, -1, 0],
                    [0, 0, 1],
                    [0, 0, -1],
                ]
                .into_iter()
                .filter(|d| m.get(i as i32 + d[0], j as i32 + d[1], k as i32 + d[2]) == mat::EMPTY)
                .collect();
                assert_eq!(
                    open,
                    vec![[0, 0, -1]],
                    "seed {seed}: drive at {i},{j},{k} is open on {open:?}"
                );
            }
        }
    }

    #[test]
    fn a_mote_meshes_like_a_hull() {
        let m = generate(Archetype::Chewer, 3);
        let s = crate::mesh::greedy_mesh(&m, None).skin_all();
        assert!(s.quads() > 40);
        assert_eq!(s.quad_cells.len(), crate::mesh::exposed_faces(&m, None));
    }
}
