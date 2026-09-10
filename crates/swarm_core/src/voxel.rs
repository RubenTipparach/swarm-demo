//! The lattice, and a model on it.
//!
//! Ported from redux-tribes `design.ts`. A hull is 32 x 32 x 64 cells for every
//! class and what changes by rung is what one cell is worth in the world, so a
//! capital costs no more to store than a frigate. The dimensions are fields
//! rather than constants because a mote is not a hull: an alien lives on a
//! 16^3 lattice and the same mesher has to draw both.
//!
//! Index order is x fastest, then y, then z, and it is the wire format: a cell
//! index in a hull file, a damage event and a chunk record all mean the same
//! cell.

/// The hull lattice, every class.
pub const HULL_NX: usize = 32;
pub const HULL_NY: usize = 32;
pub const HULL_NZ: usize = 64;

/// What a cell is made of. Numeric so it crosses a file boundary and so the
/// mesher can key on it; the values are `Mat` in `design.ts` and must stay so,
/// because every exported hull carries them.
pub mod mat {
    pub const EMPTY: u8 = 0;
    pub const PLATE: u8 = 1;
    pub const FRAME: u8 = 2;
    pub const MACHINE: u8 = 3;
    pub const GLOW: u8 = 4;
    pub const ACCENT: u8 = 5;
    pub const CASE: u8 = 6;
    /// A frame member lying where the shell wants to be: drawn and costed as
    /// plate, frame again the moment the plate comes off.
    pub const SKINNED: u8 = 7;

    pub fn is_armour(m: u8) -> bool {
        m == PLATE || m == SKINNED
    }
}

/// Cell size in world units by rung, from `RUNG` in `design.ts`. A frigate is
/// 7 units long over 64 cells: 7 x 2^-6, exact in f32.
pub const RUNG_FRIGATE: f32 = 7.0 / 64.0;
pub const RUNG_ESCORT: f32 = 10.5 / 64.0;
pub const RUNG_CRUISER: f32 = 14.0 / 64.0;
pub const RUNG_CAPITAL: f32 = 28.0 / 64.0;

/// One material and one colour per cell, plus the two bytes the hull files
/// carry beside them (purpose and livery role) so nothing that came across the
/// boundary is thrown away.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelModel {
    pub nx: usize,
    pub ny: usize,
    pub nz: usize,
    /// World units per cell.
    pub cell: f32,
    pub grid: Vec<u8>,
    /// 0xRRGGBB, resolved. Zero where the cell is empty.
    pub colour: Vec<u32>,
    pub purp: Vec<u8>,
    pub tone: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FtvxError {
    TooShort,
    BadMagic,
    BadVersion(u32),
    BadDims,
    Truncated,
    IndexOutOfRange(u32),
}

const FTVX_MAGIC: &[u8; 4] = b"FTVX";
const FTVX_VERSION: u32 = 1;
const FTVX_HEADER: usize = 4 + 4 + 12 + 4 + 4;
const FTVX_RECORD: usize = 12;

impl VoxelModel {
    pub fn new(nx: usize, ny: usize, nz: usize, cell: f32) -> Self {
        let n = nx * ny * nz;
        VoxelModel {
            nx,
            ny,
            nz,
            cell,
            grid: vec![0; n],
            colour: vec![0; n],
            purp: vec![0; n],
            tone: vec![0; n],
        }
    }

    pub fn hull(cell: f32) -> Self {
        Self::new(HULL_NX, HULL_NY, HULL_NZ, cell)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.nx * self.ny * self.nz
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn index(&self, i: usize, j: usize, k: usize) -> usize {
        i + j * self.nx + k * self.nx * self.ny
    }

    #[inline]
    pub fn at(&self, n: usize) -> (usize, usize, usize) {
        (n % self.nx, (n / self.nx) % self.ny, n / (self.nx * self.ny))
    }

    #[inline]
    pub fn inside(&self, i: i32, j: i32, k: i32) -> bool {
        i >= 0 && j >= 0 && k >= 0 && (i as usize) < self.nx && (j as usize) < self.ny && (k as usize) < self.nz
    }

    /// The material at a cell, or empty when the cell is off the lattice.
    #[inline]
    pub fn get(&self, i: i32, j: i32, k: i32) -> u8 {
        if self.inside(i, j, k) {
            self.grid[self.index(i as usize, j as usize, k as usize)]
        } else {
            mat::EMPTY
        }
    }

    pub fn set(&mut self, i: usize, j: usize, k: usize, m: u8, colour: u32) {
        let n = self.index(i, j, k);
        self.grid[n] = m;
        self.colour[n] = if m == mat::EMPTY { 0 } else { colour };
    }

    pub fn solid_count(&self) -> usize {
        self.grid.iter().filter(|&&m| m != mat::EMPTY).count()
    }

    /// The cell a world point in the model's own frame falls in, with the
    /// model centred on its lattice the way the mesher centres it.
    pub fn cell_of_point(&self, p: [f32; 3]) -> Option<(usize, usize, usize)> {
        let i = (p[0] / self.cell + self.nx as f32 / 2.0).floor() as i32;
        let j = (p[1] / self.cell + self.ny as f32 / 2.0).floor() as i32;
        let k = (p[2] / self.cell + self.nz as f32 / 2.0).floor() as i32;
        self.inside(i, j, k).then(|| (i as usize, j as usize, k as usize))
    }

    /// The centre of a cell in the model's own frame.
    pub fn centre_of(&self, n: usize) -> [f32; 3] {
        let (i, j, k) = self.at(n);
        [
            (i as f32 + 0.5 - self.nx as f32 / 2.0) * self.cell,
            (j as f32 + 0.5 - self.ny as f32 / 2.0) * self.cell,
            (k as f32 + 0.5 - self.nz as f32 / 2.0) * self.cell,
        ]
    }

    /// Lowest and highest solid cell on each axis, inclusive, or none when the
    /// model is empty.
    pub fn bounds(&self) -> Option<([usize; 3], [usize; 3])> {
        let mut lo = [usize::MAX; 3];
        let mut hi = [0usize; 3];
        let mut any = false;
        for n in 0..self.len() {
            if self.grid[n] == mat::EMPTY {
                continue;
            }
            any = true;
            let (i, j, k) = self.at(n);
            for (a, v) in [i, j, k].into_iter().enumerate() {
                lo[a] = lo[a].min(v);
                hi[a] = hi[a].max(v);
            }
        }
        any.then_some((lo, hi))
    }

    /// The bounding radius in world units about the lattice centre, from the
    /// solid cells' own corners rather than the box diagonal.
    pub fn radius(&self) -> f32 {
        let mut r2: f32 = 0.0;
        for n in 0..self.len() {
            if self.grid[n] == mat::EMPTY {
                continue;
            }
            let c = self.centre_of(n);
            let h = self.cell * 0.5;
            let d = (c[0].abs() + h).powi(2) + (c[1].abs() + h).powi(2) + (c[2].abs() + h).powi(2);
            r2 = r2.max(d);
        }
        r2.sqrt()
    }

    /// How many pieces the model is in, on SIX neighbours: two cells meeting at
    /// an edge are touching along a line, which is not a weld. One is a ship;
    /// anything more is something hanging in space beside it.
    pub fn components(&self) -> usize {
        let n = self.len();
        let mut seen = vec![false; n];
        let mut stack = Vec::new();
        let mut count = 0;
        for start in 0..n {
            if self.grid[start] == mat::EMPTY || seen[start] {
                continue;
            }
            count += 1;
            seen[start] = true;
            stack.push(start);
            while let Some(c) = stack.pop() {
                let (i, j, k) = self.at(c);
                for (di, dj, dk) in NEIGHBOURS {
                    let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                    if !self.inside(ni, nj, nk) {
                        continue;
                    }
                    let m = self.index(ni as usize, nj as usize, nk as usize);
                    if self.grid[m] != mat::EMPTY && !seen[m] {
                        seen[m] = true;
                        stack.push(m);
                    }
                }
            }
        }
        count
    }

    /// Whether the model is the same on both sides of its x midplane.
    pub fn symmetric_x(&self) -> bool {
        self.asymmetric_cells_x() == 0
    }

    /// How many cells differ from their mirror across the x midplane.
    pub fn asymmetric_cells_x(&self) -> usize {
        (0..self.len())
            .filter(|&n| {
                let (i, j, k) = self.at(n);
                let m = self.index(self.nx - 1 - i, j, k);
                self.grid[n] != self.grid[m] || self.colour[n] != self.colour[m]
            })
            .count()
    }

    /// Read a hull file written by `tools/export_hulls.mjs`.
    ///
    /// FTVX v1, little endian: magic, u32 version, u32 nx ny nz, f32 cell, u32
    /// count, then count records of u32 index, u8 mat, u8 purp, u8 tone, u8
    /// pad, u32 rgb. Sparse, because a frigate is 65536 cells of which a few
    /// thousand are anything.
    pub fn from_ftvx(bytes: &[u8]) -> Result<Self, FtvxError> {
        if bytes.len() < FTVX_HEADER {
            return Err(FtvxError::TooShort);
        }
        if &bytes[0..4] != FTVX_MAGIC {
            return Err(FtvxError::BadMagic);
        }
        let u32_at = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let version = u32_at(4);
        if version != FTVX_VERSION {
            return Err(FtvxError::BadVersion(version));
        }
        let (nx, ny, nz) = (u32_at(8) as usize, u32_at(12) as usize, u32_at(16) as usize);
        if nx == 0 || ny == 0 || nz == 0 || nx * ny * nz > 1 << 24 {
            return Err(FtvxError::BadDims);
        }
        let cell = f32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let count = u32_at(24) as usize;
        if bytes.len() < FTVX_HEADER + count * FTVX_RECORD {
            return Err(FtvxError::Truncated);
        }
        let mut m = VoxelModel::new(nx, ny, nz, cell);
        let total = m.len();
        for r in 0..count {
            let o = FTVX_HEADER + r * FTVX_RECORD;
            let idx = u32_at(o);
            if idx as usize >= total {
                return Err(FtvxError::IndexOutOfRange(idx));
            }
            let n = idx as usize;
            m.grid[n] = bytes[o + 4];
            m.purp[n] = bytes[o + 5];
            m.tone[n] = bytes[o + 6];
            m.colour[n] = u32_at(o + 8) & 0x00FF_FFFF;
        }
        Ok(m)
    }

    pub fn to_ftvx(&self) -> Vec<u8> {
        let count = self.solid_count();
        let mut out = Vec::with_capacity(FTVX_HEADER + count * FTVX_RECORD);
        out.extend_from_slice(FTVX_MAGIC);
        out.extend_from_slice(&FTVX_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.nx as u32).to_le_bytes());
        out.extend_from_slice(&(self.ny as u32).to_le_bytes());
        out.extend_from_slice(&(self.nz as u32).to_le_bytes());
        out.extend_from_slice(&self.cell.to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        for n in 0..self.len() {
            if self.grid[n] == mat::EMPTY {
                continue;
            }
            out.extend_from_slice(&(n as u32).to_le_bytes());
            out.extend_from_slice(&[self.grid[n], self.purp[n], self.tone[n], 0]);
            out.extend_from_slice(&self.colour[n].to_le_bytes());
        }
        out
    }
}

/// The six face neighbours, in the order the mesher walks them.
pub const NEIGHBOURS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips() {
        let m = VoxelModel::hull(RUNG_FRIGATE);
        for n in [0usize, 1, 31, 32, 1023, 1024, 65535] {
            let (i, j, k) = m.at(n);
            assert_eq!(m.index(i, j, k), n);
        }
        assert_eq!(m.index(31, 31, 63), 65535);
    }

    #[test]
    fn ftvx_round_trips() {
        let mut m = VoxelModel::new(4, 4, 4, 0.5);
        m.set(1, 2, 3, mat::PLATE, 0x0095E9);
        m.set(0, 0, 0, mat::GLOW, 0xFFE0BC);
        let n = m.index(1, 2, 3);
        m.purp[n] = 3;
        m.tone[n] = 5;
        let bytes = m.to_ftvx();
        let back = VoxelModel::from_ftvx(&bytes).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn ftvx_refuses_what_it_does_not_understand() {
        assert_eq!(VoxelModel::from_ftvx(b"FTV"), Err(FtvxError::TooShort));
        let mut bad = VoxelModel::new(2, 2, 2, 1.0).to_ftvx();
        bad[0] = b'X';
        assert_eq!(VoxelModel::from_ftvx(&bad), Err(FtvxError::BadMagic));
        let mut v2 = VoxelModel::new(2, 2, 2, 1.0).to_ftvx();
        v2[4] = 2;
        assert_eq!(VoxelModel::from_ftvx(&v2), Err(FtvxError::BadVersion(2)));
        let mut m = VoxelModel::new(2, 2, 2, 1.0);
        m.set(1, 1, 1, mat::PLATE, 1);
        let mut oob = m.to_ftvx();
        oob[FTVX_HEADER] = 200; // index 200 on an 8 cell lattice
        assert_eq!(VoxelModel::from_ftvx(&oob), Err(FtvxError::IndexOutOfRange(200)));
        let mut trunc = m.to_ftvx();
        trunc.truncate(trunc.len() - 1);
        assert_eq!(VoxelModel::from_ftvx(&trunc), Err(FtvxError::Truncated));
    }

    #[test]
    fn components_counts_pieces_on_six_neighbours() {
        let mut m = VoxelModel::new(4, 4, 4, 1.0);
        m.set(0, 0, 0, mat::PLATE, 1);
        m.set(1, 0, 0, mat::PLATE, 1);
        assert_eq!(m.components(), 1);
        // Diagonal only: an edge contact, not a weld.
        m.set(2, 1, 0, mat::PLATE, 1);
        assert_eq!(m.components(), 2);
        m.set(2, 0, 0, mat::PLATE, 1);
        assert_eq!(m.components(), 1);
    }

    #[test]
    fn point_and_centre_agree() {
        let m = VoxelModel::hull(RUNG_FRIGATE);
        for n in [0usize, 500, 33_000, 65535] {
            let c = m.centre_of(n);
            let (i, j, k) = m.cell_of_point(c).unwrap();
            assert_eq!(m.index(i, j, k), n);
        }
        assert!(m.cell_of_point([100.0, 0.0, 0.0]).is_none());
    }

    /// The real thing, straight off the redux-tribes export: the terran
    /// frigate is 8938 cells on the frigate rung, in one piece.
    #[test]
    fn loads_the_terran_frigate() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/terran_frigate.ftvx");
        let bytes = std::fs::read(path).expect("assets/hulls/terran_frigate.ftvx");
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        assert_eq!((m.nx, m.ny, m.nz), (HULL_NX, HULL_NY, HULL_NZ));
        assert_eq!(m.cell, RUNG_FRIGATE);
        assert_eq!(m.solid_count(), 8938);
        assert!(m.grid.iter().filter(|&&x| mat::is_armour(x)).count() > 6000);
        // NOT asserted mirrored. The export is the hull exactly as redux-tribes
        // rasterises it, and that rasteriser is not symmetric: its CLAUDE.md
        // records that CX = 16 is a cell boundary on a lattice of 32, so
        // `round(CX + u*hw)` and `round(CX - u*hw)` do not land the same
        // distance out. Measured here: 1406 of 8938 cells differ across the
        // plane between cells 15 and 16, and the civil boxship's solid cells
        // average x = 16.5. A loader that "fixed" that would draw a different
        // ship from the one in the shipyard, so it is reported and left alone.
        let off = m.asymmetric_cells_x();
        assert!(off < m.solid_count() / 4, "{off} of {} cells unmirrored: more than the rasteriser's own skew", m.solid_count());
        eprintln!("terran_frigate: {off} cells differ across the keel plane");
        assert_eq!(m.components(), 1, "nothing may float");
        let r = m.radius();
        assert!(r > 3.0 && r < 4.0, "frigate radius {r}, class radius is 3.5");
    }
}
