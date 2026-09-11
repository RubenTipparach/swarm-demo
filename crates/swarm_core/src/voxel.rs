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

/// How many surfaces a hull draws in: three bands of plating, the frame,
/// drives, weapons, other machinery, and eight brush slots. `SURF_*` in
/// redux-tribes `hull.ts`, and the order is a wire value.
pub const SURF_ARMOUR: u8 = 0;
pub const SURF_FRAME: u8 = 3;
pub const SURF_DRIVE: u8 = 4;
pub const SURF_WEAPON: u8 = 5;
pub const SURF_PART: u8 = 6;
pub const SURF_SLOT: u8 = 7;
pub const SURF_COUNT: usize = 15;

/// What a cell is FOR, as the export writes it.
///
/// redux-tribes keeps two words apart on purpose and so does this: ENGINES are
/// the main drive, the thing that makes speed; THRUSTERS are attitude
/// authority, the thing that makes heading. They wear one surface because both
/// are bells and a finish cannot tell them apart, and they are not the same
/// thing: a plume comes out of an engine, and a thruster firing constantly
/// would be a ship that never stops spinning. "Jets" is wrong and is not used.
pub mod purpose {
    pub const NONE: u8 = 0;
    /// Engines.
    pub const PROPULSION: u8 = 1;
    /// Thrusters.
    pub const ATTITUDE: u8 = 2;
    pub const GUN: u8 = 3;
    pub const ORDNANCE: u8 = 4;
    pub const COMMAND: u8 = 5;
    pub const CREW: u8 = 6;
    pub const BOARDING: u8 = 7;
    pub const STRUCTURE: u8 = 8;
}

/// What one surface is made of: the finish is the key of a normal map
/// (`armour_<finish>_n.png`, or `smooth` for none), and the pair is what a
/// PBR shader calls metalness and roughness.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub finish: String,
    pub metal: f32,
    pub rough: f32,
}

/// A window: a hole cut in the plating on one face of one cell, wearing a
/// decal. Derived by redux-tribes from the room behind the plate, the navy's
/// own rows, and whatever a player painted, and exported as the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub cell: u32,
    /// Face, in `mesh::DIRS` order: +x -x +y -y +z -z.
    pub dir: u8,
    /// Index into `VoxelModel::window_kinds`.
    pub kind: u8,
    /// How many variants sit side by side in that decal's strip.
    pub variants: u16,
}

/// No window on this face.
pub const NO_WINDOW: u16 = 0xFFFF;

/// One material and one colour per cell, plus the bytes the hull files carry
/// beside them (purpose, livery role, surface) so nothing that came across the
/// boundary is thrown away; and the hull's surfaces and windows.
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
    /// Which surface each cell draws in, `SURF_*`.
    pub surf: Vec<u8>,
    /// In `SURF_*` order. Empty for a model that has no finishes, such as a
    /// mote, which draws every surface alike.
    pub surfaces: Vec<Surface>,
    pub window_kinds: Vec<String>,
    pub windows: Vec<Window>,
    /// `cell * 6 + dir` to the index of the window on that face, or
    /// `NO_WINDOW`. Built from `windows` by `rebuild_window_lut`, which the
    /// loader calls. Sixteen bits: a liner carries 801 windows.
    pub window_lut: Vec<u16>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FtvxError {
    TooShort,
    BadMagic,
    BadVersion(u32),
    BadDims,
    Truncated,
    IndexOutOfRange(u32),
    BadString,
    BadSurface,
    BadWindow,
}

const FTVX_MAGIC: &[u8; 4] = b"FTVX";
const FTVX_VERSION: u32 = 2;
const FTVX_HEADER: usize = 4 + 4 + 12 + 4;
const FTVX_RECORD: usize = 12;
const FTVX_WINDOW: usize = 8;

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
            surf: vec![0; n],
            surfaces: Vec::new(),
            window_kinds: Vec::new(),
            windows: Vec::new(),
            window_lut: vec![NO_WINDOW; n * 6],
        }
    }

    /// Rebuild the face lookup from the window list. A window on a cell that
    /// is not solid is dropped: there is no plate there to cut it into.
    pub fn rebuild_window_lut(&mut self) {
        let n = self.len();
        self.window_lut = vec![NO_WINDOW; n * 6];
        for (w, win) in self.windows.iter().enumerate() {
            let c = win.cell as usize;
            if c < n && win.dir < 6 && self.grid[c] != mat::EMPTY && w < NO_WINDOW as usize {
                self.window_lut[c * 6 + win.dir as usize] = w as u16;
            }
        }
    }

    /// The window on a face, if any.
    #[inline]
    pub fn window_at(&self, cell: usize, dir: usize) -> Option<&Window> {
        let w = self.window_lut[cell * 6 + dir];
        (w != NO_WINDOW).then(|| &self.windows[w as usize])
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
                self.grid[n] != self.grid[m] || self.colour[n] != self.colour[m] || self.surf[n] != self.surf[m]
            })
            .count()
    }

    /// Read a hull file written by `tools/export_hulls.mjs`.
    ///
    /// FTVX v2, little endian: magic, u32 version, u32 nx ny nz, f32 cell;
    /// a string table (u32 count, then u16 length + utf8 each); a surface
    /// table (u32 count, then u16 finish string, u16 pad, f32 metal, f32
    /// rough, in `SURF_*` order); the cells (u32 count, then u32 index, u8
    /// mat, u8 purp, u8 tone, u8 surf, u32 rgb); and the windows (u32 count,
    /// then u32 cell, u8 dir, u8 kind string, u16 variants). Sparse, because
    /// a frigate is 65536 cells of which a few thousand are anything, and
    /// refused by name when it is not what this build understands: an index
    /// off the lattice, a string off the table, a surface count that is not
    /// the fifteen this build draws.
    pub fn from_ftvx(bytes: &[u8]) -> Result<Self, FtvxError> {
        let mut r = Reader { b: bytes, o: 0 };
        if bytes.len() < FTVX_HEADER {
            return Err(FtvxError::TooShort);
        }
        if &bytes[0..4] != FTVX_MAGIC {
            return Err(FtvxError::BadMagic);
        }
        r.o = 4;
        let version = r.u32()?;
        if version != FTVX_VERSION {
            return Err(FtvxError::BadVersion(version));
        }
        let (nx, ny, nz) = (r.u32()? as usize, r.u32()? as usize, r.u32()? as usize);
        if nx == 0 || ny == 0 || nz == 0 || nx * ny * nz > 1 << 24 {
            return Err(FtvxError::BadDims);
        }
        let cell = r.f32()?;
        let nstr = r.u32()? as usize;
        let mut strings = Vec::with_capacity(nstr);
        for _ in 0..nstr {
            let len = r.u16()? as usize;
            let raw = r.bytes(len)?;
            strings.push(std::str::from_utf8(raw).map_err(|_| FtvxError::BadString)?.to_string());
        }
        let nsurf = r.u32()? as usize;
        if nsurf != 0 && nsurf != SURF_COUNT {
            return Err(FtvxError::BadSurface);
        }
        let mut surfaces = Vec::with_capacity(nsurf);
        for _ in 0..nsurf {
            let finish = r.u16()? as usize;
            let _pad = r.u16()?;
            let metal = r.f32()?;
            let rough = r.f32()?;
            let finish = strings.get(finish).ok_or(FtvxError::BadSurface)?.clone();
            surfaces.push(Surface { finish, metal, rough });
        }
        let count = r.u32()? as usize;
        if r.b.len() < r.o + count * FTVX_RECORD {
            return Err(FtvxError::Truncated);
        }
        let mut m = VoxelModel::new(nx, ny, nz, cell);
        m.surfaces = surfaces;
        let total = m.len();
        for _ in 0..count {
            let idx = r.u32()?;
            if idx as usize >= total {
                return Err(FtvxError::IndexOutOfRange(idx));
            }
            let n = idx as usize;
            let rec = r.bytes(4)?;
            m.grid[n] = rec[0];
            m.purp[n] = rec[1];
            m.tone[n] = rec[2];
            m.surf[n] = rec[3];
            if m.surf[n] as usize >= SURF_COUNT {
                return Err(FtvxError::BadSurface);
            }
            m.colour[n] = r.u32()? & 0x00FF_FFFF;
        }
        let nwin = r.u32()? as usize;
        if r.b.len() < r.o + nwin * FTVX_WINDOW {
            return Err(FtvxError::Truncated);
        }
        let mut kinds: Vec<String> = Vec::new();
        for _ in 0..nwin {
            let cell = r.u32()?;
            let rec = r.bytes(2)?;
            let (dir, kind) = (rec[0], rec[1] as usize);
            let variants = r.u16()?;
            if cell as usize >= total || dir >= 6 || variants == 0 {
                return Err(FtvxError::BadWindow);
            }
            let name = strings.get(kind).ok_or(FtvxError::BadWindow)?;
            let k = match kinds.iter().position(|s| s == name) {
                Some(k) => k,
                None => {
                    kinds.push(name.clone());
                    kinds.len() - 1
                }
            };
            m.windows.push(Window { cell, dir, kind: k as u8, variants });
        }
        m.window_kinds = kinds;
        m.rebuild_window_lut();
        Ok(m)
    }

    pub fn to_ftvx(&self) -> Vec<u8> {
        let mut strings: Vec<String> = Vec::new();
        let intern = |s: &str, strings: &mut Vec<String>| -> u16 {
            match strings.iter().position(|x| x == s) {
                Some(i) => i as u16,
                None => {
                    strings.push(s.to_string());
                    (strings.len() - 1) as u16
                }
            }
        };
        let surf_ids: Vec<u16> = self.surfaces.iter().map(|s| intern(&s.finish, &mut strings)).collect();
        let kind_ids: Vec<u16> = self.window_kinds.iter().map(|k| intern(k, &mut strings)).collect();
        let count = self.solid_count();
        let mut out = Vec::new();
        out.extend_from_slice(FTVX_MAGIC);
        out.extend_from_slice(&FTVX_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.nx as u32).to_le_bytes());
        out.extend_from_slice(&(self.ny as u32).to_le_bytes());
        out.extend_from_slice(&(self.nz as u32).to_le_bytes());
        out.extend_from_slice(&self.cell.to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        for s in &strings {
            out.extend_from_slice(&(s.len() as u16).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        out.extend_from_slice(&(self.surfaces.len() as u32).to_le_bytes());
        for (s, id) in self.surfaces.iter().zip(&surf_ids) {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&s.metal.to_le_bytes());
            out.extend_from_slice(&s.rough.to_le_bytes());
        }
        out.extend_from_slice(&(count as u32).to_le_bytes());
        for n in 0..self.len() {
            if self.grid[n] == mat::EMPTY {
                continue;
            }
            out.extend_from_slice(&(n as u32).to_le_bytes());
            out.extend_from_slice(&[self.grid[n], self.purp[n], self.tone[n], self.surf[n]]);
            out.extend_from_slice(&self.colour[n].to_le_bytes());
        }
        out.extend_from_slice(&(self.windows.len() as u32).to_le_bytes());
        for w in &self.windows {
            out.extend_from_slice(&w.cell.to_le_bytes());
            out.push(w.dir);
            out.push(kind_ids[w.kind as usize] as u8);
            out.extend_from_slice(&w.variants.to_le_bytes());
        }
        out
    }
}

/// A cursor over a hull file that refuses to read past its end.
struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], FtvxError> {
        if self.o + n > self.b.len() {
            return Err(FtvxError::Truncated);
        }
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, FtvxError> {
        let s = self.bytes(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u16(&mut self) -> Result<u16, FtvxError> {
        let s = self.bytes(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn f32(&mut self) -> Result<f32, FtvxError> {
        Ok(f32::from_bits(self.u32()?))
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
        m.surf[n] = 1;
        m.surfaces = (0..SURF_COUNT)
            .map(|i| Surface { finish: if i % 2 == 0 { "plate".into() } else { "hex".into() }, metal: 0.1 * i as f32, rough: 0.5 })
            .collect();
        // Kinds are numbered in the order the file first names them, so a
        // model that numbers them that way round trips to itself exactly.
        m.window_kinds = vec!["panes".into(), "bridge".into()];
        m.windows = vec![Window { cell: n as u32, dir: 4, kind: 0, variants: 7 }, Window { cell: n as u32, dir: 0, kind: 1, variants: 1 }];
        m.rebuild_window_lut();
        let bytes = m.to_ftvx();
        let back = VoxelModel::from_ftvx(&bytes).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.window_at(n, 0).map(|w| w.kind), Some(1));
        assert_eq!(back.window_at(n, 4).map(|w| w.variants), Some(7));
        assert!(back.window_at(n, 1).is_none());
    }

    #[test]
    fn a_window_on_an_empty_cell_is_dropped() {
        let mut m = VoxelModel::new(4, 4, 4, 0.5);
        m.window_kinds = vec!["panes".into()];
        m.windows = vec![Window { cell: 5, dir: 0, kind: 0, variants: 1 }];
        m.rebuild_window_lut();
        assert!(m.window_at(5, 0).is_none(), "no plate, no hole in it");
    }

    #[test]
    fn ftvx_refuses_what_it_does_not_understand() {
        assert_eq!(VoxelModel::from_ftvx(b"FTV"), Err(FtvxError::TooShort));
        let mut bad = VoxelModel::new(2, 2, 2, 1.0).to_ftvx();
        bad[0] = b'X';
        assert_eq!(VoxelModel::from_ftvx(&bad), Err(FtvxError::BadMagic));
        let mut v3 = VoxelModel::new(2, 2, 2, 1.0).to_ftvx();
        v3[4] = 3;
        assert_eq!(VoxelModel::from_ftvx(&v3), Err(FtvxError::BadVersion(3)));
        let mut m = VoxelModel::new(2, 2, 2, 1.0);
        m.set(1, 1, 1, mat::PLATE, 1);
        let mut oob = m.to_ftvx();
        // No strings, no surfaces: the first cell record starts right after
        // the two zero counts and the cell count.
        let first = FTVX_HEADER + 4 + 4 + 4;
        oob[first] = 200; // index 200 on an 8 cell lattice
        assert_eq!(VoxelModel::from_ftvx(&oob), Err(FtvxError::IndexOutOfRange(200)));
        let mut sur = m.clone();
        sur.surfaces = vec![Surface { finish: "plate".into(), metal: 0.0, rough: 0.0 }];
        assert_eq!(VoxelModel::from_ftvx(&sur.to_ftvx()), Err(FtvxError::BadSurface), "fifteen or none");
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
        // Its surfaces, as hull.ts builds its materials: the Terran plates
        // riveted, its frame in composite weave, its machinery greebled.
        assert_eq!(m.surfaces.len(), SURF_COUNT);
        assert_eq!(m.surfaces[SURF_ARMOUR as usize].finish, "plate");
        assert_eq!(m.surfaces[SURF_FRAME as usize].finish, "weave");
        assert_eq!(m.surfaces[SURF_PART as usize].finish, "greeble");
        assert!((m.surfaces[SURF_FRAME as usize].metal - 0.45).abs() < 1e-6);
        // Every armour cell draws in a band, every frame cell as frame.
        for n in 0..m.len() {
            match m.grid[n] {
                x if mat::is_armour(x) => assert!(m.surf[n] < SURF_FRAME || m.surf[n] >= SURF_SLOT, "cell {n}"),
                mat::FRAME => assert_eq!(m.surf[n], SURF_FRAME, "cell {n}"),
                mat::EMPTY => {}
                _ => assert!((SURF_DRIVE..=SURF_PART).contains(&m.surf[n]), "cell {n}"),
            }
        }
        // And its windows, the ones hull.ts derives: 240 cabin panes, 30
        // running lights, 13 bridge viewport cells, 9 portholes, every one on
        // a solid cell and none looking up or down.
        assert_eq!(m.windows.len(), 292);
        let by_kind = |k: &str| m.windows.iter().filter(|w| m.window_kinds[w.kind as usize] == k).count();
        assert_eq!((by_kind("panes"), by_kind("beacons"), by_kind("bridge"), by_kind("porthole")), (240, 30, 13, 9));
        for w in &m.windows {
            assert_ne!(m.grid[w.cell as usize], mat::EMPTY);
            assert!(w.dir != 2 && w.dir != 3, "a window looks along or across, never up or down");
            assert!(m.window_at(w.cell as usize, w.dir as usize).is_some());
        }
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
