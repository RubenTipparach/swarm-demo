//! Faces, greedily merged, not a cube per cell.
//!
//! Ported from redux-tribes `hull.ts`. A box per cell is twelve triangles
//! whichever way it is turned, and four Terrans drawn that way took a headless
//! frame from 22 fps to 2.2. What can be seen is the faces between a solid cell
//! and one that is not, and merging runs of one colour into rectangles brings a
//! Terran from 4064 faces to about 1300 quads.
//!
//! The plain voxel rule, deliberately, including the faces nothing outside can
//! see: a frigate is mostly hollow, and a hull meshed by a flood fill from the
//! edge has no inside, so the first shot through its plating looks into a ship
//! with nothing behind the hole.
//!
//! [`mesh_region`] is the same pass over one brick of the lattice, which is how
//! a hit re-meshes what it reached and nothing else. A quad never crosses a
//! brick boundary, so a brick's mesh depends on its own cells and a one cell
//! halo and on nothing further away.

use crate::damage::{ramp, DamageGrid, HEAT_STEPS};
use crate::voxel::{mat, VoxelModel};

/// A mesh as parallel arrays, four vertices and six indices per quad, plus the
/// cells each quad stands on so a hit can find the quads it reached.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// sRGB, 0 to 1, straight off the 0xRRGGBB the cell carries. A renderer
    /// that wants linear asks [`srgb_to_linear`].
    pub colours: Vec<[f32; 4]>,
    /// One repeat per cell, V along the model's up axis on every face.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// The cell each quad is named for.
    pub quad_cell: Vec<u32>,
    /// Every cell under every quad, and where each quad's run begins.
    pub quad_cells: Vec<u32>,
    pub quad_at: Vec<u32>,
}

impl MeshData {
    pub fn quads(&self) -> usize {
        self.quad_cell.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quad_cell.is_empty()
    }

    fn push_quad(
        &mut self,
        corners: [[f32; 3]; 4],
        normal: [f32; 3],
        colour: [f32; 4],
        uvs: [[f32; 2]; 4],
        owner: u32,
    ) {
        let base = self.positions.len() as u32;
        for c in 0..4 {
            self.positions.push(corners[c]);
            self.normals.push(normal);
            self.colours.push(colour);
            self.uvs.push(uvs[c]);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.quad_cell.push(owner);
        if self.quad_at.is_empty() {
            self.quad_at.push(0);
        }
    }

    fn close_quad(&mut self) {
        self.quad_at.push(self.quad_cells.len() as u32);
    }

    /// Fold another mesh's quads onto this one.
    pub fn append(&mut self, other: &MeshData) {
        let vbase = self.positions.len() as u32;
        let cbase = self.quad_cells.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.colours.extend_from_slice(&other.colours);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + vbase));
        self.quad_cell.extend_from_slice(&other.quad_cell);
        self.quad_cells.extend_from_slice(&other.quad_cells);
        if self.quad_at.is_empty() {
            self.quad_at.push(0);
        }
        self.quad_at.extend(other.quad_at.iter().skip(1).map(|a| a + cbase));
    }
}

/// Two surfaces, because a wound is two things: the plate that is still there,
/// and the inside the hit opened, which is drawn as burning and cools.
#[derive(Clone, Debug, Default)]
pub struct Surfaces {
    pub skin: MeshData,
    pub wound: MeshData,
}

/// sRGB channel to linear, for a renderer that lights in linear.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn rgb_of(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
        1.0,
    ]
}

/// The six directions, as (axis, step, normal), in `hull.ts` order.
const DIRS: [(usize, i32, [f32; 3]); 6] = [
    (0, 1, [1.0, 0.0, 0.0]),
    (0, -1, [-1.0, 0.0, 0.0]),
    (1, 1, [0.0, 1.0, 0.0]),
    (1, -1, [0.0, -1.0, 0.0]),
    (2, 1, [0.0, 0.0, 1.0]),
    (2, -1, [0.0, 0.0, -1.0]),
];

/// Back into lattice order, whichever pair of axes this layer is drawn on.
#[inline]
fn put(axis: usize, u: i32, v: i32, w: i32) -> [i32; 3] {
    match axis {
        0 => [w, u, v],
        1 => [u, w, v],
        _ => [u, v, w],
    }
}

/// The whole model.
pub fn greedy_mesh(m: &VoxelModel, damage: Option<(&DamageGrid, u32)>) -> Surfaces {
    mesh_region(m, damage, [0, 0, 0], [m.nx, m.ny, m.nz])
}

/// One brick of it: cells with `lo <= c < hi` on every axis. Faces are judged
/// against the whole lattice, so a brick on the edge of a hole meshes the
/// faces that hole exposed.
pub fn mesh_region(
    m: &VoxelModel,
    damage: Option<(&DamageGrid, u32)>,
    lo: [usize; 3],
    hi: [usize; 3],
) -> Surfaces {
    let mut out = Surfaces::default();
    let half = [m.nx as f32 / 2.0, m.ny as f32 / 2.0, m.nz as f32 / 2.0];
    let cell = m.cell;

    let solid = |i: i32, j: i32, k: i32| -> bool {
        if !m.inside(i, j, k) {
            return false;
        }
        let n = m.index(i as usize, j as usize, k as usize);
        m.grid[n] != mat::EMPTY && damage.map_or(true, |(d, _)| !d.is_dead(n))
    };
    // A face whose neighbour is a cell that DIED rather than one that was never
    // there is the inside of the ship, and it is drawn as such.
    let dead_at = |i: i32, j: i32, k: i32| -> Option<u32> {
        let (d, tick) = damage?;
        if !m.inside(i, j, k) {
            return None;
        }
        let n = m.index(i as usize, j as usize, k as usize);
        if m.grid[n] != mat::EMPTY && d.is_dead(n) {
            Some((d.heat(n, tick) * HEAT_STEPS as f32).round() as u32)
        } else {
            None
        }
    };

    for (axis, step, normal) in DIRS {
        let u_axis = if axis == 0 { 1 } else { 0 };
        let v_axis = if axis == 2 { 1 } else { 2 };
        let (u0, u1) = (lo[u_axis] as i32, hi[u_axis] as i32);
        let (v0, v1) = (lo[v_axis] as i32, hi[v_axis] as i32);
        let un = (u1 - u0) as usize;
        let vn = (v1 - v0) as usize;
        if un == 0 || vn == 0 {
            continue;
        }
        // Colour and kind of the face at (u, v), or -1 for no face. The high
        // bits carry whether it is a wound and how hot, so a wound face never
        // merges into plate and two heats never merge into one quad.
        let mut mask = vec![-1i64; un * vn];
        let mut owner = vec![0u32; un * vn];

        for w in lo[axis] as i32..hi[axis] as i32 {
            mask.fill(-1);
            for v in v0..v1 {
                for u in u0..u1 {
                    let [i, j, k] = put(axis, u, v, w);
                    if !solid(i, j, k) {
                        continue;
                    }
                    let [ni, nj, nk] = put(axis, u, v, w + step);
                    if solid(ni, nj, nk) {
                        continue;
                    }
                    let n = m.index(i as usize, j as usize, k as usize);
                    let key = match dead_at(ni, nj, nk) {
                        Some(heat) => (1i64 << 32) | ((heat as i64) << 24),
                        None => m.colour[n] as i64,
                    };
                    let slot = (u - u0) as usize + (v - v0) as usize * un;
                    mask[slot] = key;
                    owner[slot] = n as u32;
                }
            }

            for v in 0..vn {
                let mut u = 0;
                while u < un {
                    let key = mask[u + v * un];
                    if key < 0 {
                        u += 1;
                        continue;
                    }
                    // How far this colour runs along u, then how many whole
                    // rows of that width follow it.
                    let mut wide = 1;
                    while u + wide < un && mask[u + wide + v * un] == key {
                        wide += 1;
                    }
                    let mut tall = 1;
                    'grow: while v + tall < vn {
                        for q in 0..wide {
                            if mask[u + q + (v + tall) * un] != key {
                                break 'grow;
                            }
                        }
                        tall += 1;
                    }
                    for b in 0..tall {
                        for a in 0..wide {
                            mask[u + a + (v + b) * un] = -1;
                        }
                    }

                    // The rectangle's four corners, on the face's own side of
                    // the cell. Which way round depends on the axis and not
                    // only on the sign: u cross v is along the normal for x and
                    // z and AGAINST it for y, so the y faces wind the other
                    // way or every deck and keel is culled.
                    let face = if step > 0 { 1 } else { 0 };
                    let ccw = (step > 0) != (axis == 1);
                    let (wide_i, tall_i) = (wide as i32, tall as i32);
                    let order: [(i32, i32); 4] = if ccw {
                        [(0, 0), (wide_i, 0), (wide_i, tall_i), (0, tall_i)]
                    } else {
                        [(0, 0), (0, tall_i), (wide_i, tall_i), (wide_i, 0)]
                    };
                    let mut corners = [[0.0f32; 3]; 4];
                    let mut uvs = [[0.0f32; 2]; 4];
                    for (c, (du, dv)) in order.into_iter().enumerate() {
                        let p = put(axis, u0 + u as i32 + du, v0 + v as i32 + dv, w + face);
                        corners[c] = [
                            (p[0] as f32 - half[0]) * cell,
                            (p[1] as f32 - half[1]) * cell,
                            (p[2] as f32 - half[2]) * cell,
                        ];
                        uvs[c] = if axis == 0 { [dv as f32, du as f32] } else { [du as f32, dv as f32] };
                    }
                    let own = owner[u + v * un];
                    let is_wound = key >> 32 != 0;
                    let colour = if is_wound {
                        let heat = ((key >> 24) & 0xFF) as f32 / HEAT_STEPS as f32;
                        let [r, g, b] = ramp(heat);
                        [r, g, b, 1.0]
                    } else {
                        rgb_of((key & 0xFF_FFFF) as u32)
                    };
                    let target = if is_wound { &mut out.wound } else { &mut out.skin };
                    target.push_quad(corners, normal, colour, uvs, own);
                    // The rectangle's whole footprint, so a hit can take the
                    // cells it reached and leave the rest of the plate standing.
                    for b in 0..tall {
                        for a in 0..wide {
                            let p = put(axis, u0 + (u + a) as i32, v0 + (v + b) as i32, w);
                            target.quad_cells.push(m.index(p[0] as usize, p[1] as usize, p[2] as usize) as u32);
                        }
                    }
                    target.close_quad();
                    u += wide;
                }
            }
        }
    }
    out
}

/// How many cell faces are exposed, counted the slow way, one at a time. The
/// greedy pass has to cover exactly these, which is what its tests check.
pub fn exposed_faces(m: &VoxelModel, damage: Option<&DamageGrid>) -> usize {
    let solid = |i: i32, j: i32, k: i32| -> bool {
        if !m.inside(i, j, k) {
            return false;
        }
        let n = m.index(i as usize, j as usize, k as usize);
        m.grid[n] != mat::EMPTY && damage.map_or(true, |d| !d.is_dead(n))
    };
    let mut count = 0;
    for n in 0..m.len() {
        let (i, j, k) = m.at(n);
        if !solid(i as i32, j as i32, k as i32) {
            continue;
        }
        for (di, dj, dk) in crate::voxel::NEIGHBOURS {
            if !solid(i as i32 + di, j as i32 + dj, k as i32 + dk) {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::{RUNG_FRIGATE, HULL_NX, HULL_NY, HULL_NZ};

    fn area(md: &MeshData) -> usize {
        md.quad_cells.len()
    }

    fn box_model(n: usize, lo: usize, hi: usize) -> VoxelModel {
        let mut m = VoxelModel::new(n, n, n, 1.0);
        for k in lo..hi {
            for j in lo..hi {
                for i in lo..hi {
                    m.set(i, j, k, mat::PLATE, 0x0095E9);
                }
            }
        }
        m
    }

    #[test]
    fn a_solid_box_is_six_quads() {
        let m = box_model(4, 1, 3);
        let s = greedy_mesh(&m, None);
        assert_eq!(s.skin.quads(), 6);
        assert_eq!(area(&s.skin), 24, "2x2x2 box has 24 cell faces");
        assert!(s.wound.is_empty());
        assert_eq!(s.skin.indices.len(), 36);
    }

    #[test]
    fn a_hollow_box_meshes_its_inside_too() {
        let mut m = box_model(6, 1, 5);
        for k in 2..4 {
            for j in 2..4 {
                for i in 2..4 {
                    m.set(i, j, k, mat::EMPTY, 0);
                }
            }
        }
        let s = greedy_mesh(&m, None);
        assert_eq!(s.skin.quads(), 12, "six outside and six inside");
        assert_eq!(area(&s.skin), exposed_faces(&m, None));
    }

    #[test]
    fn every_quad_winds_toward_its_normal() {
        let m = box_model(4, 1, 3);
        let s = greedy_mesh(&m, None);
        for q in 0..s.skin.quads() {
            let p = &s.skin.positions[q * 4..q * 4 + 4];
            let n = s.skin.normals[q * 4];
            let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
            let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let dot = c[0] * n[0] + c[1] * n[1] + c[2] * n[2];
            assert!(dot > 0.0, "quad {q} normal {n:?} winds against itself");
        }
    }

    #[test]
    fn two_colours_do_not_merge() {
        let mut m = box_model(4, 1, 3);
        m.set(1, 1, 1, mat::GLOW, 0xFFE0BC);
        let s = greedy_mesh(&m, None);
        assert!(s.skin.quads() > 6);
        assert_eq!(area(&s.skin), 24);
    }

    #[test]
    fn the_terran_frigate_covers_every_exposed_face_once() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/terran_frigate.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let s = greedy_mesh(&m, None);
        let faces = exposed_faces(&m, None);
        assert_eq!(area(&s.skin), faces);
        assert!(s.skin.quads() < faces / 2, "greedy merged {} faces into {} quads", faces, s.skin.quads());
        let mut seen = std::collections::HashSet::new();
        for q in 0..s.skin.quads() {
            let (from, to) = (s.skin.quad_at[q] as usize, s.skin.quad_at[q + 1] as usize);
            let n = s.skin.normals[q * 4];
            for &c in &s.skin.quad_cells[from..to] {
                assert!(seen.insert((c, n.map(|x| x as i32))), "cell {c} face {n:?} covered twice");
            }
        }
        eprintln!("terran_frigate: {} exposed faces, {} quads", faces, s.skin.quads());
    }

    #[test]
    fn bricks_cover_the_same_faces_as_the_whole() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/rogue_corvette.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let whole = greedy_mesh(&m, None);
        let mut sum = 0;
        let b = 8;
        for k in (0..HULL_NZ).step_by(b) {
            for j in (0..HULL_NY).step_by(b) {
                for i in (0..HULL_NX).step_by(b) {
                    let s = mesh_region(&m, None, [i, j, k], [i + b, j + b, k + b]);
                    sum += area(&s.skin);
                    for q in 0..s.skin.quads() {
                        for c in 0..4 {
                            let p = s.skin.positions[q * 4 + c];
                            let lo = |a: usize, o: f32| (a as f32 - o) * m.cell - 1e-4;
                            assert!(p[0] >= lo(i, HULL_NX as f32 / 2.0) && p[0] <= lo(i + b, HULL_NX as f32 / 2.0) + 2e-4);
                            assert!(p[1] >= lo(j, HULL_NY as f32 / 2.0) && p[1] <= lo(j + b, HULL_NY as f32 / 2.0) + 2e-4);
                            assert!(p[2] >= lo(k, HULL_NZ as f32 / 2.0) && p[2] <= lo(k + b, HULL_NZ as f32 / 2.0) + 2e-4);
                        }
                    }
                }
            }
        }
        assert_eq!(sum, area(&whole.skin));
        let _ = RUNG_FRIGATE;
    }

    #[test]
    fn append_keeps_quad_footprints_aligned() {
        let a = greedy_mesh(&box_model(4, 1, 3), None).skin;
        let b = greedy_mesh(&box_model(4, 0, 2), None).skin;
        let mut all = a.clone();
        all.append(&b);
        assert_eq!(all.quads(), a.quads() + b.quads());
        assert_eq!(all.quad_at.len(), all.quads() + 1);
        assert_eq!(*all.quad_at.last().unwrap() as usize, all.quad_cells.len());
        assert_eq!(all.indices.len(), all.quads() * 6);
        assert_eq!(*all.indices.iter().max().unwrap() as usize, all.positions.len() - 1);
    }
}
