//! What the swarm does to a hull: bites, chunks and the smoke off a wound.

use crate::*;

pub(crate) fn chew(
    tick: Res<Tick>,
    cfg: Res<SwarmConfig>,
    mut hulls: Query<(&mut Hull, &Transform)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
    mut sparks: ResMut<SparkQueue>,
    mut cube: Local<Option<Handle<Mesh>>>,
) {
    // A frozen swarm bites nothing: the chewers are the cloud's teeth.
    if cfg.frozen {
        return;
    }
    for (mut hull, xf) in &mut hulls {
        let hull = &mut *hull;
        if hull.dead_hull || hull.invulnerable {
            continue;
        }
        let mut breaches: Vec<Chunk> = Vec::new();
        for c in &mut hull.chewers {
            while c.next <= tick.tick {
                c.next += 6;
                if let Some(b) = hull
                    .damage
                    .bite(&hull.model, c.at.to_array(), 9.0, tick.tick)
                {
                    // Follow the hole in: stand where the cell was.
                    c.at = Vec3::from(hull.model.centre_of(b.cell as usize));
                    // The spray comes off the FACE that opened, in world
                    // space: a spark thrown in the hull's own frame would
                    // fly off in the wrong direction the moment a ship moves.
                    let at = xf.transform_point(Vec3::from(hull.model.centre_of(b.cell as usize)));
                    let out = xf.rotation * Vec3::from(b.outward);
                    let mut list = Vec::new();
                    breach_sparks(
                        b.cell,
                        b.tick,
                        at.to_array(),
                        out.to_array(),
                        hull.model.cell,
                        &mut list,
                    );
                    sparks.extend(list);
                    breaches.push(chunk_for(&hull.model, &b));
                }
            }
        }
        if breaches.is_empty() {
            continue;
        }
        hull.breaches += breaches.len();
        throw_chunks(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut chunk_mats,
            &mut cube,
            hull.model.cell,
            xf,
            &breaches,
        );
    }
}

/// Smoke out of the holes: slow, dark, and only a few at a time, because a
/// plume from every vent on a chewed hull is a fog bank.
pub(crate) fn vent_smoke(
    tick: Res<Tick>,
    hulls: Query<(&Hull, &Transform)>,
    mut sparks: ResMut<SparkQueue>,
) {
    if tick.tick % 4 != 0 {
        return;
    }
    for (hull, xf) in &hulls {
        let vents: Vec<Vent> = hull.damage.vents(&hull.model, 120);
        if vents.is_empty() {
            continue;
        }
        for n in 0..vents.len().min(6) {
            let v = vents[(tick.tick as usize / 4 + n * 17) % vents.len()];
            let d = drift_of(v.cell, tick.tick.wrapping_add(n as u32));
            let at = xf.transform_point(Vec3::from(v.at));
            // Outward is the way INTO the hole, so smoke leaves along its
            // opposite: a plume that went the other way would go through the
            // ship.
            let out = -(xf.rotation * Vec3::from(v.outward));
            let vel = out * hull.model.cell * 2.4 + Vec3::from(d) * hull.model.cell * 1.2;
            sparks.push(Spark {
                pos: at.to_array(),
                vel: vel.to_array(),
                // Barely over black: smoke is what a fire leaves, and it is
                // the one thing here that must NOT bloom.
                colour: [0.30, 0.20, 0.16],
                size: hull.model.cell * (2.0 + 1.5 * (d[0] * 0.5 + 0.5)),
                life: 1.4 + 1.2 * (d[1] * 0.5 + 0.5),
                kind: SparkKind::Breach,
            });
        }
    }
}

/// Throw the pieces a cut took off, wherever the cut came from.
///
/// One implementation, because a chewer's bite, a miner's shaft and a
/// salvager's cut all take cells off a voxel model and all leave the same
/// thing behind: a cube of that cell's own colour, drifting. The cube mesh
/// is built once per caller and the materials are cached by colour, since a
/// rock is two colours and a hull is a dozen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn throw_chunks(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    chunk_mats: &mut ChunkMaterials,
    cube: &mut Option<Handle<Mesh>>,
    cell: f32,
    xf: &Transform,
    chunks: &[Chunk],
) {
    if chunks.is_empty() {
        return;
    }
    let cube = cube
        .get_or_insert_with(|| meshes.add(Cuboid::from_length(cell * 0.9)))
        .clone();
    for ch in chunks {
        let mat = chunk_mats
            .0
            .entry(ch.colour)
            .or_insert_with(|| {
                let [r, g, b, _] = swarm_core::mesh::rgb_of(ch.colour);
                materials.add(StandardMaterial {
                    base_color: Color::srgb(r, g, b),
                    perceptual_roughness: 0.8,
                    ..default()
                })
            })
            .clone();
        let origin = xf.transform_point(Vec3::from(ch.origin));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(origin),
            Debris {
                vel: Vec3::from(ch.velocity),
                born: ch.born,
            },
        ));
    }
}
