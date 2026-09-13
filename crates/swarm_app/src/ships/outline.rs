//! A ship's outline: one inverted shell per HULL, shown while it is selected.
//!
//! What an RTS needs is a silhouette that says THIS one, and a cyan ring on
//! the plane under a ship says where it stands rather than which it is. This
//! is the classic inverted hull: the same geometry pushed out along its own
//! normals and drawn FRONT face culled, so the ship's own faces cover the
//! middle of it and what survives is a rim the width of the push.
//!
//! **Per ship and not per brick, which is the whole reason it is affordable.**
//! A hull on the field is dozens of brick meshes over seven surfaces, and each
//! of those is re-meshed the moment a bite lands: a shell per brick would be
//! dozens of extra draws that all have to be rebuilt on damage. This is ONE
//! mesh, built once from the model at spawn, and it never changes, which is
//! also the right answer for what it is FOR: an outline is the ship's identity
//! and not its wounds, so a chewed frigate is still outlined as a frigate.
//!
//! And it costs nothing while nothing is selected, because it is hidden.

use bevy::mesh::{MeshVertexAttributeId, VertexAttributeValues};
use swarm_core::voxel::depth_from_outside;

use crate::*;

/// How far the shell stands off the hull, as a share of the hull's radius.
///
/// Along the NORMAL rather than by scaling the transform: a uniform scale
/// expands about the lattice origin, so it would stand a long way off the bow
/// of a frigate and hardly at all amidships, which is a halo rather than an
/// outline. Pushed along the normal it is one width everywhere.
const OUTLINE: f32 = 0.022;

/// The shell itself, so a selection can show and hide it.
#[derive(Component)]
pub(crate) struct Outline;

/// The model an outline is taken off: the hull with every void SEALED.
///
/// The mesher meshes a face wherever a solid cell meets one that is not,
/// which includes the faces nothing outside can see, and that is right for a
/// hull (it is what makes a hole read as a hull with a hole in it) and wrong
/// for a shell. An interior face's normal points INTO its void, so pushing it
/// drives it further in and then out through the plating on the far side: the
/// first cut of this drew cyan across the middle of the deck and down every
/// frame member, which is a wireframe of the ship's insides rather than an
/// outline of its outside.
///
/// `depth_from_outside` already answers which empty cells the outside can
/// reach, so everything else is filled in. What comes back has exactly the
/// skin and nothing behind it.
fn sealed(model: &VoxelModel) -> VoxelModel {
    let depth = depth_from_outside(model);
    let mut m = VoxelModel::new(model.nx, model.ny, model.nz, model.cell);
    for (cell, d) in m.grid.iter_mut().zip(depth.iter()) {
        // Nought is outside space itself. Anything else is either solid
        // already or a void nothing outside can see, and both are the shell's
        // inside.
        *cell = if *d == 0 { mat::EMPTY } else { mat::PLATE };
    }
    m
}

/// Build one hull's shell from its own model.
///
/// **The normals are averaged BY POSITION, and that is what closes it.** A
/// voxel mesh is hard edged: every vertex carries its own face's normal, so
/// three faces meeting at a corner push three ways and the corner comes
/// APART, leaving a shell full of gaps for the front culled draw to show the
/// far side through. Averaged, a convex corner is pushed along its own
/// diagonal and the shell stays closed, which is the whole difference between
/// an outline and a wireframe of every edge on the ship.
pub(crate) fn outline_mesh(model: &VoxelModel) -> Mesh {
    let skin = greedy_mesh(&sealed(model), None);
    let mut mesh = to_mesh_where(&skin, |_| true);
    // Nothing but the shape. A vertex colour would put the hull's own paint
    // on the rim and a tangent is for a normal map this material does not
    // have, and both are bytes uploaded for every ship in the fleet.
    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    let push = model.radius() * OUTLINE;
    let Some(normals) = float3(&mesh, Mesh::ATTRIBUTE_NORMAL.id) else {
        return mesh;
    };
    let Some(pos) = float3(&mesh, Mesh::ATTRIBUTE_POSITION.id) else {
        return mesh;
    };
    let smooth = average_normals(&pos, &normals);
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for (p, n) in p.iter_mut().zip(smooth.iter()) {
            for a in 0..3 {
                p[a] += n[a] * push;
            }
        }
    }
    mesh
}

/// One vertex attribute as three floats, or nothing if it is not there.
fn float3(mesh: &Mesh, id: MeshVertexAttributeId) -> Option<Vec<[f32; 3]>> {
    match mesh.attribute(id)? {
        VertexAttributeValues::Float32x3(v) => Some(v.clone()),
        _ => None,
    }
}

/// Sum every face normal that meets at each POSITION and normalise it.
///
/// Keyed on the position's own bits rather than on a rounding, because a
/// lattice corner is written by the mesher as the same arithmetic every time
/// and two faces meeting there agree exactly; a tolerance would be a number
/// to get wrong for no gain. A `HashMap` is fine here: this is the render
/// side, built once at spawn, and nothing about it is hashed or replayed.
fn average_normals(pos: &[[f32; 3]], normals: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut sum: std::collections::HashMap<[u32; 3], [f32; 3]> = std::collections::HashMap::new();
    let key = |p: &[f32; 3]| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
    for (p, n) in pos.iter().zip(normals.iter()) {
        let e = sum.entry(key(p)).or_insert([0.0; 3]);
        for a in 0..3 {
            e[a] += n[a];
        }
    }
    pos.iter()
        .zip(normals.iter())
        .map(|(p, n)| {
            let s = sum.get(&key(p)).copied().unwrap_or(*n);
            let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
            // A corner where opposite faces cancel sums to nought, and a
            // direction of nought is a NaN the moment it is normalised. Its
            // own face's normal is the honest answer there.
            if len > 1e-4 {
                [s[0] / len, s[1] / len, s[2] / len]
            } else {
                *n
            }
        })
        .collect()
}

/// Hang a shell on a hull, hidden until it is picked, and say how many quads
/// it came to, so the spawn line can report what the outline costs beside
/// what the hull itself costs.
pub(crate) fn add_outline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    hull_entity: Entity,
    model: &VoxelModel,
) -> usize {
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.88, 1.0),
        unlit: true,
        // FRONT culled, which is what makes an inverted hull an outline: the
        // shell's near faces are thrown away and its far ones are drawn, so
        // the ship's own geometry covers everything but the rim.
        cull_mode: Some(bevy::render::render_resource::Face::Front),
        ..default()
    });
    let mesh = outline_mesh(model);
    let quads = mesh.count_vertices() / 4;
    let shell = commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            Visibility::Hidden,
            Outline,
        ))
        .id();
    commands.entity(hull_entity).add_child(shell);
    quads
}

/// Show the shell on what is picked and hide it on what is not.
///
/// A CHILD's visibility against its parent's selection, so the rule is read
/// off the hull rather than copied onto the shell: a shell that remembered its
/// own state would be a second writer for one fact.
pub(crate) fn light_outline(
    picked: Query<(), With<Selected>>,
    parents: Query<&ChildOf>,
    mut shells: Query<(Entity, &mut Visibility), With<Outline>>,
) {
    for (e, mut vis) in &mut shells {
        let on = parents.get(e).is_ok_and(|p| picked.get(p.parent()).is_ok());
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}
