//! What is drawn that is not a thing: the nav disc and the helpers that
//! build lines and rings.

mod lights;
mod nav;
mod sensors;

pub(crate) use lights::*;
pub(crate) use nav::*;
pub(crate) use sensors::*;

use crate::*;

/// A mesh with nothing in it that the renderer will still accept.
pub(crate) fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

/// One mesh under construction: the three vectors that always travel
/// together, and the one place they are turned into a `Mesh`.
///
/// The assembly was written out three times (the nav disc, the beams and the
/// sensors furniture), the same four attributes and the same empty case, and
/// this is that written once. It is the struct `#[allow(too_many_arguments)]`
/// was pointing at all along: every builder in this folder takes `pos`, `col`
/// and `idx` as three arguments because they had nowhere else to be.
#[derive(Default)]
pub(crate) struct Draw {
    pub(crate) pos: Vec<[f32; 3]>,
    pub(crate) col: Vec<[f32; 4]>,
    pub(crate) idx: Vec<u32>,
}

impl Draw {
    pub(crate) fn mesh(self) -> Mesh {
        if self.pos.is_empty() {
            return empty_mesh();
        }
        let n = self.pos.len();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh.insert_indices(Indices::U32(self.idx));
        mesh
    }
}
