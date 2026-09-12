//! A cube: the piece a cut takes off, and the thing a harvester carries.
//!
//! The bank used to go up the moment a cutter got home, which is an economy
//! with nothing in the world to look at. A cube is that number made into an
//! object: it comes off the rock where the shaft is, it is pulled in by the
//! ship that cut it, it rides in a line behind that ship all the way back,
//! and it is gone when it is landed. A miner killed on the way home is
//! carrying something a player can watch it lose, which is the whole reason
//! for doing it this way.

use crate::*;

/// One cube, and who it belongs to.
///
/// `to` is the ship that cut it. A cube whose ship is gone goes LOOSE rather
/// than vanishing, so the wreck of a full miner leaves its load floating
/// where it died and anything with room can go and get it.
#[derive(Component)]
pub(crate) struct Cargo {
    pub(crate) kind: Cube,
    pub(crate) to: Option<Entity>,
    /// Riding, rather than still on its way in. Held cubes are PLACED every
    /// frame; loose ones fly.
    pub(crate) held: bool,
    pub(crate) vel: Vec3,
}

/// How big a cube is drawn, in the cutter's own radii. Big enough to count
/// from across the field, which is what makes "four cubes" a thing a player
/// reads off the picture rather than off the panel.
pub(crate) const CUBE_SIZE: f32 = 0.36;

/// How fast a loose cube is drawn in, in its own sizes a second, and how
/// near it has to be to be taken aboard.
pub(crate) const HAUL_SPEED: f32 = 9.0;
pub(crate) const HAUL_REACH: f32 = 0.9;

/// Where the tail of cubes rides: behind and below, in the ship's own frame
/// and its own radii, one cube behind the next.
pub(crate) const TAIL_AT: Vec3 = Vec3::new(0.0, -0.55, -1.25);
pub(crate) const TAIL_STEP: f32 = 0.5;

/// One cube, cut loose at a point in the world.
///
/// Thrown OUT of the shaft, because it came out of a hole: a cube that
/// appeared already drifting home would read as a spawn rather than as
/// something taken off a rock.
pub(crate) fn spawn_cube(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cube_mats: &mut ChunkMaterials,
    mesh: &mut Option<Handle<Mesh>>,
    kind: Cube,
    at: Vec3,
    out: Vec3,
    size: f32,
    to: Entity,
) {
    let mesh = mesh
        .get_or_insert_with(|| meshes.add(Cuboid::from_length(size)))
        .clone();
    // The cube wears its seam's own colour, through the same cache the
    // debris uses: a rock is a few colours and one material each is enough.
    let [r, g, b] = [
        ((kind.colour() >> 16) & 0xFF) as f32 / 255.0,
        ((kind.colour() >> 8) & 0xFF) as f32 / 255.0,
        (kind.colour() & 0xFF) as f32 / 255.0,
    ];
    let mat = cube_mats
        .0
        .entry(kind.colour() | 0x0100_0000)
        .or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                // Lit from inside a little, so a cube in a ship's shadow is
                // still a cube: this is cargo a player has to be able to
                // count at the range a fleet is watched from.
                emissive: LinearRgba::rgb(r * 0.9, g * 0.9, b * 0.9),
                perceptual_roughness: 0.35,
                metallic: 0.2,
                ..default()
            })
        })
        .clone();
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_translation(at),
        Cargo {
            kind,
            to: Some(to),
            held: false,
            vel: out * size * 3.0,
        },
    ));
}

/// A ship a cube can ride on: where it is and how big it is. `Without<Cargo>`
/// is what proves this query disjoint from the cubes' own, since Bevy reads
/// the FILTERS rather than what anybody knows about the data.
pub(crate) type HaulShip<'a> = (&'a Transform, &'a Hull);
pub(crate) type HaulFilter = (With<Support>, Without<Cargo>);

/// Loose cubes fly to the ship that cut them, and held ones ride behind it.
///
/// One system for both halves because they are one story: a cube is on its
/// way in or it is aboard, and the only thing that changes is whether it is
/// steered or placed.
pub(crate) fn haul_cargo(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    ships: Query<HaulShip, HaulFilter>,
    holds: Query<&Hold>,
    mut cubes: Query<(Entity, &mut Cargo, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = scene.step(&time);
    // Where each ship's next cube rides, counted as they are placed: two
    // cubes in one slot is a ship carrying one cube as far as anybody can
    // see.
    let mut slot: Vec<(Entity, usize)> = Vec::new();
    for (e, mut cargo, mut xf) in &mut cubes {
        let Some(to) = cargo.to else {
            drift(&mut cargo, &mut xf, dt);
            continue;
        };
        let Ok((ship, hull)) = ships.get(to) else {
            // Its ship is gone: the load is loose now, where it was.
            cargo.to = None;
            cargo.held = false;
            continue;
        };
        let radius = hull.model.radius();
        if cargo.held {
            let n = slot
                .iter_mut()
                .find(|(s, _)| *s == to)
                .map(|(_, n)| {
                    *n += 1;
                    *n - 1
                })
                .unwrap_or_else(|| {
                    slot.push((to, 1));
                    0
                });
            // In the SHIP's frame, so a tail of cubes turns with the ship it
            // is behind instead of sliding round it. The same rule an escort
            // keeps station by.
            let at = TAIL_AT - Vec3::Z * TAIL_STEP * n as f32;
            xf.translation = ship.translation + ship.rotation * (at * radius);
            xf.rotation = ship.rotation;
            continue;
        }
        // Still on its way in. A hold that filled while this one was in the
        // air leaves it loose rather than overfilling: a cube is a piece.
        if holds.get(to).is_ok_and(|h| h.full()) {
            cargo.to = None;
            drift(&mut cargo, &mut xf, dt);
            continue;
        }
        let gap = ship.translation - xf.translation;
        let size = CUBE_SIZE * radius;
        if gap.length() < HAUL_REACH * radius {
            cargo.held = true;
            continue;
        }
        let want = gap.normalize_or_zero() * size * HAUL_SPEED;
        cargo.vel = cargo.vel.lerp(want, (dt * 3.0).min(1.0));
        let step = cargo.vel * dt;
        xf.translation += step;
        xf.rotate_y(dt * 1.1);
        if !xf.translation.is_finite() {
            commands.entity(e).despawn();
        }
    }
}

/// A cube nobody is coming for: it keeps the speed it was thrown with and
/// slows, so a load spilled by a dead miner stays where it was spilled.
fn drift(cargo: &mut Cargo, xf: &mut Transform, dt: f32) {
    xf.translation += cargo.vel * dt;
    cargo.vel *= 1.0 - (dt * 0.8).min(1.0);
    xf.rotate_y(dt * 0.5);
}

/// What a ship is carrying, for the one place that has to know: the moment
/// it is landed.
pub(crate) fn cubes_of(cubes: &Query<(Entity, &Cargo)>, ship: Entity) -> Vec<(Entity, Cube)> {
    cubes
        .iter()
        .filter(|(_, c)| c.held && c.to == Some(ship))
        .map(|(e, c)| (e, c.kind))
        .collect()
}
