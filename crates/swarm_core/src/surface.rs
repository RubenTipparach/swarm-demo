//! What a meshed hull IS: the vertex buffer one draw is made of, and the
//! layers a hull is drawn in.
//!
//! Split out of `mesh` when that file went over this project's own nine
//! hundred lines, along the line the module docs there already drew: `mesh`
//! answers WHICH faces exist and how they merge, and this answers what the
//! answer is kept in. It is the same cut `damage` made when `heat` and
//! `wound` came out of it, and `build` when `rung` did, and `mesh` re-exports
//! every name here for the same reason: nothing outside the crate learns a
//! new path because a file got long.

use crate::voxel::SURF_COUNT;

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

    pub(crate) fn push_quad(
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
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.quad_cell.push(owner);
        if self.quad_at.is_empty() {
            self.quad_at.push(0);
        }
    }

    pub(crate) fn close_quad(&mut self) {
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
        self.quad_at
            .extend(other.quad_at.iter().skip(1).map(|a| a + cbase));
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
    pub(crate) fn sized(kinds: usize) -> Self {
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
