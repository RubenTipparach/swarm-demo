//! A cube: the piece a cut takes off, and the thing a collector carries.
//!
//! The bank used to go up the moment a cutter got home, which is an economy
//! with nothing in the world to look at. A cube is that number made into an
//! object: it comes off the rock where the shaft is, it lies where it was
//! cut until a collector comes for it, and it is gone when it is landed.
//! What a player watches is a pile growing at a shaft and craft ferrying it
//! away, which is the whole reason for doing it this way.

use crate::*;

/// One cube, and who is coming for it.
///
/// `to` is the COLLECTOR that has claimed it, which is what stops two craft
/// flying at the same cube: the claim lives on the thing being come for, so
/// there is one answer to "who is coming for this" rather than one per
/// craft. A cube whose collector is gone goes loose again and the next one
/// free claims it.
#[derive(Component)]
pub(crate) struct Cargo {
    pub(crate) kind: Cube,
    pub(crate) to: Option<Entity>,
    /// In the claws, rather than lying where it was cut. A held cube is
    /// PLACED by the craft holding it every frame; a loose one drifts.
    pub(crate) held: bool,
    pub(crate) vel: Vec3,
}

/// How big a cube is drawn, in the cutter's own radii. Big enough to count
/// from across the field, which is what makes "four cubes" a thing a player
/// reads off the picture rather than off the panel.
pub(crate) const CUBE_SIZE: f32 = 0.36;

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
            to: None,
            held: false,
            vel: out * size * 3.0,
        },
    ));
}

/// Cubes nobody is holding drift and slow, and a claim whose craft is gone
/// is dropped.
///
/// The one thing this must NOT do is move a cube a collector is carrying:
/// that cube is placed at the claws by the craft holding it, and two systems
/// deciding where one cube rides is two answers. The filter IS the check,
/// which is this project's own rule: a claim naming something that is not a
/// collector any more is a collector that died with the load.
pub(crate) fn drift_cargo(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    craft: Query<(), With<Collector>>,
    mut cubes: Query<(Entity, &mut Cargo, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = scene.step(&time);
    for (e, mut cargo, mut xf) in &mut cubes {
        if let Some(to) = cargo.to {
            if craft.get(to).is_ok() {
                continue;
            }
            // Its craft is gone: the load is loose now, where it was.
            cargo.to = None;
            cargo.held = false;
        }
        drift(&mut cargo, &mut xf, dt);
        if !xf.translation.is_finite() {
            commands.entity(e).despawn();
        }
    }
}

/// A cube nobody is coming for: it keeps the speed it was thrown with and
/// slows, so a pile at a shaft mouth stays at the shaft mouth.
fn drift(cargo: &mut Cargo, xf: &mut Transform, dt: f32) {
    xf.translation += cargo.vel * dt;
    cargo.vel *= 1.0 - (dt * 0.8).min(1.0);
    xf.rotate_y(dt * 0.5);
}
