//! What the swarm does to a hull: bites, chunks and the smoke off a wound.

use crate::*;

/// Where one tooth's bite lands: a RAY, cast from outside the hull.
///
/// A bug attacks what it can SEE, so the cell it takes is the first live one
/// along the line it comes in on, holes included: a ray into a crater lands on
/// the crater's floor. That is what stops a tooth tunnelling. It can only ever
/// reach the surface facing it, and a hole deepens only as fast as the plating
/// around it goes.
///
/// The line is nudged off its own axis by half a cell, and no more. It is
/// tempting to spread it wide enough to make the crater by itself, and that is
/// wrong twice over: a plate cell is a hundred hit points against a bite of
/// nine, so a tooth whose bites land on a dozen different cells scratches all
/// of them and kills none, and the crater does not need the help. `bite` takes
/// the nearest EXPOSED cell to the point, so the moment one hole opens its
/// neighbours are nearer than its own floor and the hole widens on its own.
///
/// Measured over seven hundred ticks against sixty four teeth: a cell and a
/// half of spread took 312 cells off, half a cell takes 492. The tunnelling it
/// replaced took 744, and that is the price of the fix rather than a
/// regression, because a tooth that bores keeps hitting a face it has already
/// damaged and one that eats the outside is always starting on fresh plating.
fn bite_of(hull: &mut Hull, from: Vec3, n: usize, tick: u32) -> Option<Breach> {
    let reach = hull.model.radius() * 2.0;
    let seed = (n as u32).wrapping_mul(0x9E37_79B9) ^ tick;
    let jit =
        |q: u32| swarm_core::rng::hash_cell(seed.wrapping_add(q)) as f32 / u32::MAX as f32 - 0.5;
    let side = from.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = from.cross(side);
    let at = from * reach + (side * jit(1) + up * jit(7)) * hull.model.cell;
    let hit = swarm_core::ray::march(
        &hull.model,
        |i| !hull.damage.is_dead(i),
        at.to_array(),
        (-from).to_array(),
        reach * 2.0,
    )?;
    hull.damage.bite(&hull.model, hit.point, 9.0, tick)
}

/// The pieces a hull throws when cells come off it, as entities.
fn throw_chunks(
    chunks: Vec<Chunk>,
    xf: &Transform,
    cube: Handle<Mesh>,
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    chunk_mats: &mut ChunkMaterials,
) {
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
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(xf.transform_point(Vec3::from(ch.origin))),
            Debris {
                vel: Vec3::from(ch.velocity),
                born: ch.born,
            },
        ));
    }
}

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
        for n in 0..hull.chewers.len() {
            while hull.chewers[n].next <= tick.tick {
                hull.chewers[n].next += 6;
                let from = hull.chewers[n].from;
                let Some(b) = bite_of(hull, from, n, tick.tick) else {
                    continue;
                };
                // The spray comes off the FACE that opened, in world space: a
                // spark thrown in the hull's own frame would fly off in the
                // wrong direction the moment a ship moves.
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
        if breaches.is_empty() {
            continue;
        }
        hull.breaches += breaches.len();
        let cube = cube
            .get_or_insert_with(|| meshes.add(Cuboid::from_length(hull.model.cell * 0.9)))
            .clone();
        throw_chunks(
            breaches,
            xf,
            cube,
            &mut commands,
            &mut materials,
            &mut chunk_mats,
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
