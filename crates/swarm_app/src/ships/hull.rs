//! A hull as the app holds it: its model, its bricks, its damage and the
//! meshing that puts a brick back on screen.

use crate::*;

/// The one entity per (brick, surface) that draws a piece of a hull, created
/// the first time that surface has a face in that brick and hidden when it
/// stops having one, so a hole that reaches the frame under a plate gets a
/// frame mesh where there was none.
#[derive(Default)]
pub(crate) struct Piece {
    pub(crate) entity: Option<Entity>,
    pub(crate) mesh: Option<Handle<Mesh>>,
}

#[derive(Default)]
pub(crate) struct Brick {
    pub(crate) skin: Vec<Piece>,
    /// The machinery a hole uncovered, the burn over it, and the soot on the
    /// plating round the rim. Three layers on the same faces, and they are
    /// three materials because they are three different things.
    pub(crate) inner: Piece,
    pub(crate) wound: Piece,
    pub(crate) scorch: Piece,
    pub(crate) windows: Vec<Piece>,
}

/// A ship: its cells, its damage, its materials, and every piece of it on
/// the map, so a dirty brick is a mesh swap and nothing else.
#[derive(Component)]
pub(crate) struct Hull {
    /// Which class it was built from, when it is one of the fleet's own.
    ///
    /// Carried rather than only logged, because two things ask a hull what
    /// it IS rather than what it is made of: what a jump costs by its rung,
    /// and what a salvager would be rebuilding when it works its wreck.
    pub(crate) class: Option<String>,
    pub(crate) model: VoxelModel,
    pub(crate) damage: DamageGrid,
    pub(crate) bricks: Vec<Brick>,
    pub(crate) surface_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) window_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) inner_mat: Handle<StandardMaterial>,
    pub(crate) wound_mat: Handle<StandardMaterial>,
    pub(crate) scorch_mat: Handle<StandardMaterial>,
    pub(crate) chewers: Vec<Chewer>,
    /// Where this hull's weapons are and which way they look, read off the
    /// cells the export says are gunnery.
    pub(crate) guns: Vec<Gun>,
    /// And its engines, read off the cells the export says are propulsion.
    /// Not the thrusters: those wear the same surface and are all over a
    /// hull, and a thruster plumed constantly is a ship that never stops
    /// spinning.
    ///
    /// With their cells, so a drive that has been eaten stops being one.
    pub(crate) engines: Vec<Drive>,
    /// Where it has been told to go, in world units, or nothing.
    pub(crate) order: Option<Vec3>,
    pub(crate) vel: Vec3,
    /// Last frame's change in velocity, which is what decides whether the
    /// main engines are burning or the retros are.
    pub(crate) accel: Vec3,
    pub(crate) breaches: usize,
    pub(crate) last_heat_key: u32,
    /// How many cells it started with, so "enough of it is gone" is a share
    /// rather than a number that means something different on every class.
    pub(crate) cells: usize,
    pub(crate) dead_hull: bool,
    /// The cells of its reactor, derived from the hull's own geometry: the
    /// most buried place in the ship and a ball around it. Half of these gone
    /// is what takes the ship, and nothing else does.
    pub(crate) reactor: Vec<usize>,
    /// This SHIP's own seed, mixed into everything hashed off a cell.
    ///
    /// Two ships of a class have the same cells, so a gun's firing phase,
    /// hashed off its cell alone, came out the same on every hull in the
    /// formation: four frigates fired in one volley, on the same tick, for
    /// ever. A seed per ship is what staggers them.
    pub(crate) seed: u32,
    /// The sandbox's toggle: bites and the reactor rule leave it alone.
    pub(crate) invulnerable: bool,
}

/// One engine cluster, and the cells it was read off.
///
/// The CELLS are the point. An engine used to be a position and a direction
/// taken off the intact model at spawn and never looked at again, so a ship
/// whose drives had been eaten went on burning them at full throttle and
/// flying as if nothing had happened: the damage was drawn on the hull and
/// meant nothing to it. An engine is only an engine while the cells it is
/// made of are still there.
pub(crate) struct Drive {
    pub(crate) gun: Gun,
    pub(crate) cells: Vec<usize>,
}

impl Drive {
    /// What share of it is left, nought to one.
    pub(crate) fn left(&self, dmg: &DamageGrid) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let live = self.cells.iter().filter(|&&c| !dmg.is_dead(c)).count();
        live as f32 / self.cells.len() as f32
    }
}

impl Hull {
    /// What the ship can still PULL, nought to one.
    ///
    /// The share of its propulsion cells that are still there, and a floor
    /// under it, because a hull's attitude thrusters are scattered all over
    /// it and are not what `engine_clusters` counts: a ship that has lost
    /// every main drive can still shove itself about, slowly, and one that has
    /// lost them all and can do nothing at all would coast out of the map for
    /// ever on whatever velocity it happened to have.
    ///
    /// This is what was missing. The plating was coming off all along (three
    /// hundred and eighty five cells in four hundred ticks against forty eight
    /// chewers, measured), and none of it meant anything: the only thing that
    /// read damage was the reactor, so a ship with its whole stern eaten flew
    /// exactly as well as one fresh out of the yard.
    pub(crate) fn thrust(&self) -> f32 {
        let (mut live, mut all) = (0usize, 0usize);
        for e in &self.engines {
            all += e.cells.len();
            live += e.cells.iter().filter(|&&c| !self.damage.is_dead(c)).count();
        }
        if all == 0 {
            return 1.0;
        }
        THRUSTERS + (1.0 - THRUSTERS) * (live as f32 / all as f32)
    }
}

/// What is left of a ship's push when every main drive is gone: its attitude
/// thrusters, which are all over the hull and are not drives.
pub(crate) const THRUSTERS: f32 = 0.18;

/// One of the cloud's teeth: the side it comes in from, and when it bites
/// next.
///
/// A DIRECTION rather than a place, because the place is worked out by a ray
/// every bite. It used to be a point that started on a random exposed quad and
/// then followed its own hole inward, and both halves of that were wrong. A
/// random QUAD is not a random part of a ship: the greedy mesher merges flat
/// plating into a handful of big quads and leaves greebled work as dozens of
/// small ones, so picking one uniformly put most of the swarm's teeth on the
/// most detailed end of the hull, which on every one of these classes is the
/// stern. A frigate was eaten from the engines forward, every time, whatever
/// side the cloud was actually on.
pub(crate) struct Chewer {
    /// Unit, in the hull's OWN frame, pointing out from the middle toward the
    /// side this one attacks from.
    pub(crate) from: Vec3,
    pub(crate) next: u32,
}

/// What a warship's plating is worth, against the bare material.
///
/// ONE, which is what it always was before a hundred was tried. The hundred
/// made a ship that could not be hurt, and what actually needed fixing was
/// never the plating: it was that losing a tenth of your cells anywhere at all
/// blew the ship up. `REACTOR_LOSS` is what fixed that, and it does the job on
/// its own. A cell comes off in a few bites again, so the swarm visibly eats a
/// hull, and the ship still survives it, because a hole in the plating is not
/// a hole in the reactor.
pub const ARMOUR: f32 = 1.0;

pub(crate) fn to_mesh(md: &MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    if md.is_empty() {
        // A piece with nothing to draw still owns a handle, so give the
        // renderer one degenerate triangle rather than an empty buffer.
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        return mesh;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, md.uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, md.tangents.clone());
    let colours: Vec<[f32; 4]> = md
        .colours
        .iter()
        .map(|c| {
            [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
                c[3],
            ]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(md.indices.clone()));
    mesh
}

/// Concatenate the surfaces a predicate keeps, as one mesh.
/// Move every vertex of a mesh, so a part built in the ship's coordinates can
/// be drawn about its own pivot instead.
pub(crate) fn shift_mesh(mesh: &mut Mesh, by: Vec3) {
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for v in p.iter_mut() {
            v[0] += by.x;
            v[1] += by.y;
            v[2] += by.z;
        }
    }
}

pub(crate) fn to_mesh_where(s: &Surfaces, keep: impl Fn(usize) -> bool) -> Mesh {
    let mut all = MeshData::default();
    for (i, md) in s.skin.iter().enumerate() {
        if keep(i) {
            all.append(md);
        }
    }
    to_mesh(&all)
}

/// Put a mesh on a piece: spawn it the first time, swap the mesh after, hide
/// it when there is nothing to draw.
pub(crate) fn upsert(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    piece: &mut Piece,
    md: &MeshData,
    material: &Handle<StandardMaterial>,
    parent: Entity,
) {
    if md.is_empty() {
        if let Some(e) = piece.entity {
            commands.entity(e).insert(Visibility::Hidden);
        }
        return;
    }
    let mesh = to_mesh(md);
    match (&piece.entity, &piece.mesh) {
        (Some(e), Some(h)) => {
            let _ = meshes.insert(h.id(), mesh);
            commands.entity(*e).insert(Visibility::Inherited);
        }
        _ => {
            let h = meshes.add(mesh);
            let e = commands
                .spawn((
                    Mesh3d(h.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    ChildOf(parent),
                ))
                .id();
            piece.entity = Some(e);
            piece.mesh = Some(h);
        }
    }
}

pub(crate) fn place_brick(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    hull: &mut Hull,
    b: usize,
    s: &Surfaces,
    parent: Entity,
) {
    let brick = &mut hull.bricks[b];
    if brick.skin.len() < SURF_COUNT {
        brick.skin.resize_with(SURF_COUNT, Piece::default);
    }
    if brick.windows.len() < s.windows.len() {
        brick.windows.resize_with(s.windows.len(), Piece::default);
    }
    for (i, md) in s.skin.iter().enumerate() {
        upsert(
            commands,
            meshes,
            &mut brick.skin[i],
            md,
            &hull.surface_mats[i],
            parent,
        );
    }
    upsert(
        commands,
        meshes,
        &mut brick.inner,
        &s.inner,
        &hull.inner_mat,
        parent,
    );
    upsert(
        commands,
        meshes,
        &mut brick.wound,
        &s.wound,
        &hull.wound_mat,
        parent,
    );
    upsert(
        commands,
        meshes,
        &mut brick.scorch,
        &s.scorch,
        &hull.scorch_mat,
        parent,
    );
    for (k, md) in s.windows.iter().enumerate() {
        upsert(
            commands,
            meshes,
            &mut brick.windows[k],
            md,
            &hull.window_mats[k],
            parent,
        );
    }
}

/// Re-mesh what the chewers reached this frame, and repaint the whole wound
/// when its heat has moved a bucket.
pub(crate) fn remesh_dirty(
    tick: Res<Tick>,
    mut hulls: Query<(Entity, &mut Hull)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, mut hull) in &mut hulls {
        let hull = &mut *hull;
        let mut dirty = hull.damage.take_dirty();
        let key = hull.damage.heat_key(tick.tick);
        if key != hull.last_heat_key {
            hull.last_heat_key = key;
            for b in 0..hull.damage.brick_count() {
                if hull.bricks[b].wound.entity.is_some() && !dirty.contains(&b) {
                    dirty.push(b);
                }
            }
        }
        for b in dirty {
            let (lo, hi) = hull.damage.brick_bounds(b);
            let s = mesh_region(&hull.model, Some((&hull.damage, tick.tick)), lo, hi);
            place_brick(&mut commands, &mut meshes, hull, b, &s, entity);
        }
    }
}
