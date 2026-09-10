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

use crate::damage::{crust_alpha, ramp, DamageGrid, CHAR, HEAT_STEPS};
use crate::fx::{ember_tile, ember_uv};
use crate::rng::hash_cell;
use crate::voxel::{mat, VoxelModel, SURF_COUNT};

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
    /// The direction U increases in, and in w which way V goes relative to
    /// normal cross tangent, so a normal map seats the right way up. Exact
    /// for an axis aligned quad; nothing here needs mikktspace.
    pub tangents: Vec<[f32; 4]>,
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
        tangent: [f32; 4],
        owner: u32,
    ) {
        let base = self.positions.len() as u32;
        for c in 0..4 {
            self.positions.push(corners[c]);
            self.normals.push(normal);
            self.colours.push(colour);
            self.uvs.push(uvs[c]);
            self.tangents.push(tangent);
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
        self.tangents.extend_from_slice(&other.tangents);
        self.indices.extend(other.indices.iter().map(|i| i + vbase));
        self.quad_cell.extend_from_slice(&other.quad_cell);
        self.quad_cells.extend_from_slice(&other.quad_cells);
        if self.quad_at.is_empty() {
            self.quad_at.push(0);
        }
        self.quad_at.extend(other.quad_at.iter().skip(1).map(|a| a + cbase));
    }
}

/// What one pass of the mesher produces, by material.
///
/// `skin[s]` is every face of surface `s` (`SURF_*`), which is what lets each
/// band of plating, the frame and each kind of machinery wear its own finish:
/// a normal map is a material and a material is a draw call, so the split is
/// by what a face is made of and by nothing finer. `wound` is the inside a hit
/// opened, drawn burning and cooling. `windows[k]` is every window face of
/// kind `k` (`VoxelModel::window_kinds`), one quad per cell and never merged,
/// because each picks its own slice of a variant strip.
#[derive(Clone, Debug, Default)]
pub struct Surfaces {
    pub skin: Vec<MeshData>,
    /// The inside a hit opened, drawn as the machinery it is: lit, in the
    /// surviving cell's own colour, on the plate's own UVs.
    ///
    /// This face used to exist only as ember, on a ramp that cools to char,
    /// so a hole a minute old was a black void with the reactor somewhere
    /// inside it and nothing to see. They are parts, and the yard has always
    /// drawn them with the plate off, so the battlefield draws them too.
    pub inner: MeshData,
    /// The burn ON that inside: the same faces again, unlit, on the ember
    /// atlas, white hot through orange to char. Alpha is the CRUST, which
    /// keeps a third of itself once the fire is out, so a cooled wound is a
    /// burnt hole rather than a part looking freshly built.
    pub wound: MeshData,
    /// The soot on the plating AROUND a hole, on the ember atlas too, so both
    /// halves of a burn come off one texture.
    pub scorch: MeshData,
    pub windows: Vec<MeshData>,
}

impl Surfaces {
    fn sized(kinds: usize) -> Self {
        Surfaces {
            skin: (0..SURF_COUNT).map(|_| MeshData::default()).collect(),
            inner: MeshData::default(),
            wound: MeshData::default(),
            scorch: MeshData::default(),
            windows: (0..kinds).map(|_| MeshData::default()).collect(),
        }
    }

    /// Every skin face, all surfaces together, for a picture that draws one
    /// material.
    pub fn skin_all(&self) -> MeshData {
        let mut all = MeshData::default();
        for s in &self.skin {
            all.append(s);
        }
        all
    }

    pub fn skin_quads(&self) -> usize {
        self.skin.iter().map(|s| s.quads()).sum()
    }

    pub fn window_quads(&self) -> usize {
        self.windows.iter().map(|s| s.quads()).sum()
    }

    /// Every quad in every layer, which is what a face count is measured
    /// against: the skin, the windows cut out of it, and the inside a hit
    /// opened. The wound and the scorch are the same faces again in another
    /// layer, so they are not counted twice.
    pub fn face_quads(&self) -> usize {
        self.skin.iter().map(|s| s.quad_cells.len()).sum::<usize>()
            + self.window_quads()
            + self.inner.quads()
    }
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

/// The tangent of a face in direction `d`, given the UV rule below: on the x
/// faces U runs along z, on the others along x. The w is the handedness that
/// makes `w * cross(normal, tangent)` the direction V increases in, which is
/// the hull's up on every face but the decks and keels, where it is z.
fn tangent_of(d: usize) -> [f32; 4] {
    match d {
        0 => [0.0, 0.0, 1.0, -1.0],
        1 => [0.0, 0.0, 1.0, 1.0],
        2 => [1.0, 0.0, 0.0, -1.0],
        3 => [1.0, 0.0, 0.0, 1.0],
        4 => [1.0, 0.0, 0.0, 1.0],
        _ => [1.0, 0.0, 0.0, -1.0],
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
    let mut out = Surfaces::sized(m.window_kinds.len());
    let half = [m.nx as f32 / 2.0, m.ny as f32 / 2.0, m.nz as f32 / 2.0];
    let cell = m.cell;
    let world = |p: [i32; 3]| -> [f32; 3] {
        [(p[0] as f32 - half[0]) * cell, (p[1] as f32 - half[1]) * cell, (p[2] as f32 - half[2]) * cell]
    };

    let solid = |i: i32, j: i32, k: i32| -> bool {
        if !m.inside(i, j, k) {
            return false;
        }
        let n = m.index(i as usize, j as usize, k as usize);
        m.grid[n] != mat::EMPTY && damage.map_or(true, |(d, _)| !d.is_dead(n))
    };
    // A face whose neighbour is a cell that DIED rather than one that was never
    // there is the inside of the ship, and it is drawn as such.
    // The dead cell behind a face, and how hot it still is. The CELL comes
    // back as well as the heat because the ember tile is hashed off it: a
    // wound that picked its tile from the live cell would relight a whole
    // crater the same way.
    let dead_at = |i: i32, j: i32, k: i32| -> Option<(u32, f32)> {
        let (d, tick) = damage?;
        if !m.inside(i, j, k) {
            return None;
        }
        let n = m.index(i as usize, j as usize, k as usize);
        if m.grid[n] != mat::EMPTY && d.is_dead(n) {
            // Quantised, so a wound repaints once every 28 ticks rather than
            // every frame, and two faces at nearly one heat draw alike.
            let bucket = (d.heat(n, tick) * HEAT_STEPS as f32).round();
            Some((n as u32, bucket / HEAT_STEPS as f32))
        } else {
            None
        }
    };

    for (d, (axis, step, normal)) in DIRS.into_iter().enumerate() {
        let tangent = tangent_of(d);
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
                    // The corners of this one cell's face, which three of the
                    // four layers below want and none of them may compute its
                    // own way: two answers about where a face is would draw a
                    // pane, a burn and the plate under them a hair apart.
                    let face = if step > 0 { 1 } else { 0 };
                    let ccw = (step > 0) != (axis == 1);
                    let order: [(i32, i32); 4] =
                        if ccw { [(0, 0), (1, 0), (1, 1), (0, 1)] } else { [(0, 0), (0, 1), (1, 1), (1, 0)] };
                    let mut corners = [[0.0f32; 3]; 4];
                    let mut duv = [[0.0f32; 2]; 4];
                    for (c, (du, dv)) in order.into_iter().enumerate() {
                        corners[c] = world(put(axis, u + du, v + dv, w + face));
                        duv[c] = if axis == 0 { [dv as f32, du as f32] } else { [du as f32, dv as f32] };
                    }

                    // A face whose neighbour DIED is the inside of the ship,
                    // and it is TWO layers: the machinery it is made of, and
                    // the burn over it. It leaves the greedy pass because
                    // each picks its own tile of the ember atlas by its own
                    // cell, and two heats must never merge into one quad.
                    if let Some((dc, heat)) = dead_at(ni, nj, nk) {
                        let tile = ember_tile(dc);
                        let [r, g, b] = ramp(heat);
                        let mut euv = [[0.0f32; 2]; 4];
                        for c in 0..4 {
                            euv[c] = ember_uv(tile, duv[c]);
                        }
                        out.inner.push_quad(corners, normal, rgb_of(m.colour[n]), duv, tangent, n as u32);
                        out.inner.quad_cells.push(n as u32);
                        out.inner.close_quad();
                        out.wound.push_quad(corners, normal, [r, g, b, crust_alpha(heat)], euv, tangent, dc);
                        out.wound.quad_cells.push(dc);
                        out.wound.close_quad();
                        continue;
                    }

                    // A window face leaves the plate pass entirely: it is
                    // its own quad with its own slice of the decal strip,
                    // and the hole it leaves in the plating is exactly where
                    // it goes. Not over a hole, though: a face that looks
                    // into the inside of the ship is a wound, whatever the
                    // plate there used to wear.
                    if let Some(win) = m.window_at(n, d) {
                        let variants = win.variants.max(1) as u32;
                        let slice = if variants > 1 { hash_cell(n as u32) % variants } else { 0 };
                        let span = 1.0 / variants as f32;
                        let mut uvs = [[0.0f32; 2]; 4];
                        for c in 0..4 {
                            uvs[c] = [(slice as f32 + duv[c][0]) * span, duv[c][1]];
                        }
                        let kind = win.kind as usize;
                        let target = &mut out.windows[kind];
                        target.push_quad(corners, normal, rgb_of(m.colour[n]), uvs, tangent, n as u32);
                        target.quad_cells.push(n as u32);
                        target.close_quad();
                        continue;
                    }

                    // Soot on the plating around a hole, as a layer over it.
                    // Only on faces looking into SPACE: the faces looking into
                    // the hole already carry the burn above, and stacking a
                    // third layer on them would be three decals deep on one
                    // plane for no picture anybody could read.
                    if let Some((d_grid, tick)) = damage {
                        if let Some(alpha) = d_grid.scorch_at(m, n) {
                            let tile = ember_tile(n as u32);
                            let mut euv = [[0.0f32; 2]; 4];
                            for c in 0..4 {
                                euv[c] = ember_uv(tile, duv[c]);
                            }
                            let _ = tick;
                            out.scorch.push_quad(corners, normal, [CHAR[0], CHAR[1], CHAR[2], alpha], euv, tangent, n as u32);
                            out.scorch.quad_cells.push(n as u32);
                            out.scorch.close_quad();
                        }
                    }

                    let key = (m.colour[n] as i64) | ((m.surf[n] as i64) << 24);
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
                    #[allow(clippy::needless_range_loop)]
                    for (c, (du, dv)) in order.into_iter().enumerate() {
                        corners[c] = world(put(axis, u0 + u as i32 + du, v0 + v as i32 + dv, w + face));
                        uvs[c] = if axis == 0 { [dv as f32, du as f32] } else { [du as f32, dv as f32] };
                    }
                    let own = owner[u + v * un];
                    let target = &mut out.skin[((key >> 24) & 0xFF) as usize];
                    target.push_quad(corners, normal, rgb_of((key & 0xFF_FFFF) as u32), uvs, tangent, own);
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
        let skin = s.skin_all();
        assert_eq!(skin.quads(), 6);
        assert_eq!(area(&skin), 24, "2x2x2 box has 24 cell faces");
        assert!(s.wound.is_empty());
        assert_eq!(skin.indices.len(), 36);
        assert_eq!(skin.tangents.len(), 24);
    }

    /// The tangent frame has to agree with the UVs, or a normal map seats a
    /// rivet head the wrong way up on half the faces: for every quad, the
    /// tangent is the direction U grows in and `w * cross(n, t)` the direction
    /// V grows in, measured off the quad's own corners.
    #[test]
    fn tangents_follow_the_uvs_on_every_face() {
        let m = box_model(4, 1, 3);
        let skin = greedy_mesh(&m, None).skin_all();
        for q in 0..skin.quads() {
            let p = &skin.positions[q * 4..q * 4 + 4];
            let uv = &skin.uvs[q * 4..q * 4 + 4];
            let n = skin.normals[q * 4];
            let t = skin.tangents[q * 4];
            // Find the corner that differs from corner 0 in U only, and in V only.
            let mut du = None;
            let mut dv = None;
            for c in 1..4 {
                let (su, sv) = (uv[c][0] - uv[0][0], uv[c][1] - uv[0][1]);
                let d = [p[c][0] - p[0][0], p[c][1] - p[0][1], p[c][2] - p[0][2]];
                if su > 0.0 && sv == 0.0 { du = Some(d); }
                if sv > 0.0 && su == 0.0 { dv = Some(d); }
            }
            let (du, dv) = (du.unwrap(), dv.unwrap());
            let along = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(along(du, [t[0], t[1], t[2]]) > 0.0, "quad {q}: tangent {t:?} against U {du:?}");
            let b = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
            assert!(along(dv, b) * t[3] > 0.0, "quad {q}: bitangent {b:?} w {} against V {dv:?}", t[3]);
        }
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
        let skin = s.skin_all();
        assert_eq!(skin.quads(), 12, "six outside and six inside");
        assert_eq!(area(&skin), exposed_faces(&m, None));
    }

    #[test]
    fn every_quad_winds_toward_its_normal() {
        let m = box_model(4, 1, 3);
        let s = greedy_mesh(&m, None).skin_all();
        for q in 0..s.quads() {
            let p = &s.positions[q * 4..q * 4 + 4];
            let n = s.normals[q * 4];
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
        let s = greedy_mesh(&m, None).skin_all();
        assert!(s.quads() > 6);
        assert_eq!(area(&s), 24);
    }

    /// Two cells of one colour on two SURFACES are two quads, in two meshes:
    /// a finish is a material and a material cannot straddle a quad.
    #[test]
    fn two_surfaces_do_not_merge() {
        let mut m = box_model(4, 1, 3);
        let n = m.index(1, 1, 1);
        m.surf[n] = 3;
        let s = greedy_mesh(&m, None);
        // The corner cell shows three faces. Each leaves its box face as one
        // quad of its own plus an L of three cells, which greedy lays as two.
        assert_eq!(s.skin[3].quads(), 3);
        assert_eq!(s.skin[0].quads(), 3 + 3 * 2);
        assert_eq!(area(&s.skin_all()), 24);
    }

    /// A window is a hole in the plating: the plate quad does not cover it,
    /// the window quad does, and the two together cover every face once.
    #[test]
    fn a_window_is_cut_out_of_the_plate() {
        let mut m = box_model(6, 1, 5);
        m.window_kinds = vec!["panes".into(), "porthole".into()];
        let a = m.index(4, 2, 2);
        let b = m.index(4, 3, 3);
        m.windows = vec![
            crate::voxel::Window { cell: a as u32, dir: 0, kind: 0, variants: 7 },
            crate::voxel::Window { cell: b as u32, dir: 0, kind: 1, variants: 1 },
            // Looking up: the export never writes one, and if it did the face
            // is still a face.
            crate::voxel::Window { cell: m.index(2, 4, 2) as u32, dir: 2, kind: 0, variants: 7 },
        ];
        m.rebuild_window_lut();
        let s = greedy_mesh(&m, None);
        assert_eq!(s.window_quads(), 3);
        assert_eq!(s.windows[0].quads(), 2);
        assert_eq!(s.windows[1].quads(), 1);
        assert_eq!(area(&s.skin_all()) + s.window_quads(), exposed_faces(&m, None));
        // The +x face of the box without the windows would be one quad; with
        // two holes in it the plate is several.
        assert!(s.skin_quads() > 6);
        // The variant slice: U spans one seventh of the strip for a panes
        // window, the whole strip for a porthole, and V the full height.
        let uv = &s.windows[0].uvs[0..4];
        let umin = uv.iter().map(|c| c[0]).fold(f32::MAX, f32::min);
        let umax = uv.iter().map(|c| c[0]).fold(f32::MIN, f32::max);
        assert!((umax - umin - 1.0 / 7.0).abs() < 1e-6, "{umin}..{umax}");
        assert!(umin >= 0.0 && umax <= 1.0);
        let uv1 = &s.windows[1].uvs[0..4];
        assert_eq!(uv1.iter().map(|c| c[0]).fold(f32::MIN, f32::max), 1.0);
        // A dead window cell has no window: its face is a hole, not a pane.
        let mut d = DamageGrid::new(&m);
        d.chip(a, 1000.0, 1, [1.0, 0.0, 0.0]);
        let s = greedy_mesh(&m, Some((&d, 1)));
        assert_eq!(s.window_quads(), 2);
    }

    #[test]
    fn the_terran_frigate_covers_every_exposed_face_once() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/terran_frigate.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let s = greedy_mesh(&m, None);
        let faces = exposed_faces(&m, None);
        let skin = s.skin_all();
        assert_eq!(area(&skin) + s.window_quads(), faces);
        assert_eq!(s.face_quads(), faces, "the layers cover every face once");
        assert_eq!(s.window_quads(), 292, "every window hull.ts derived is drawn");
        assert!(skin.quads() < faces / 2, "greedy merged {} faces into {} quads", faces, skin.quads());
        let mut seen = std::collections::HashSet::new();
        for q in 0..skin.quads() {
            let (from, to) = (skin.quad_at[q] as usize, skin.quad_at[q + 1] as usize);
            let n = skin.normals[q * 4];
            for &c in &skin.quad_cells[from..to] {
                assert!(seen.insert((c, n.map(|x| x as i32))), "cell {c} face {n:?} covered twice");
            }
        }
        for w in &s.windows {
            for q in 0..w.quads() {
                let n = w.normals[q * 4];
                assert!(seen.insert((w.quad_cell[q], n.map(|x| x as i32))), "window face covered twice");
            }
        }
        // Seven surfaces on a stock Terran: three bands, frame, drive, weapon, part.
        let used: Vec<usize> = (0..SURF_COUNT).filter(|&i| !s.skin[i].is_empty()).collect();
        assert_eq!(used, vec![0, 1, 2, 3, 4, 5, 6]);
        eprintln!("terran_frigate: {} exposed faces, {} skin quads, {} window quads, hull.ts drew 1611", faces, skin.quads(), s.window_quads());
    }

    #[test]
    fn bricks_cover_the_same_faces_as_the_whole() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/rogue_corvette.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let whole = greedy_mesh(&m, None);
        let mut sum = 0;
        let mut wins = 0;
        let b = 8;
        for k in (0..HULL_NZ).step_by(b) {
            for j in (0..HULL_NY).step_by(b) {
                for i in (0..HULL_NX).step_by(b) {
                    let s = mesh_region(&m, None, [i, j, k], [i + b, j + b, k + b]);
                    let skin = s.skin_all();
                    sum += area(&skin);
                    wins += s.window_quads();
                    for q in 0..skin.quads() {
                        for c in 0..4 {
                            let p = skin.positions[q * 4 + c];
                            let lo = |a: usize, o: f32| (a as f32 - o) * m.cell - 1e-4;
                            assert!(p[0] >= lo(i, HULL_NX as f32 / 2.0) && p[0] <= lo(i + b, HULL_NX as f32 / 2.0) + 2e-4);
                            assert!(p[1] >= lo(j, HULL_NY as f32 / 2.0) && p[1] <= lo(j + b, HULL_NY as f32 / 2.0) + 2e-4);
                            assert!(p[2] >= lo(k, HULL_NZ as f32 / 2.0) && p[2] <= lo(k + b, HULL_NZ as f32 / 2.0) + 2e-4);
                        }
                    }
                }
            }
        }
        assert_eq!(sum, area(&whole.skin_all()));
        assert_eq!(wins, whole.window_quads());
        let _ = RUNG_FRIGATE;
    }

    #[test]
    fn append_keeps_quad_footprints_aligned() {
        let a = greedy_mesh(&box_model(4, 1, 3), None).skin_all();
        let b = greedy_mesh(&box_model(4, 0, 2), None).skin_all();
        let mut all = a.clone();
        all.append(&b);
        assert_eq!(all.quads(), a.quads() + b.quads());
        assert_eq!(all.quad_at.len(), all.quads() + 1);
        assert_eq!(*all.quad_at.last().unwrap() as usize, all.quad_cells.len());
        assert_eq!(all.indices.len(), all.quads() * 6);
        assert_eq!(*all.indices.iter().max().unwrap() as usize, all.positions.len() - 1);
    }
}
