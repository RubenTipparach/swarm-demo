//! A hull that tumbles: the rigid body the core weighs off its live cells,
//! the spin a hit gives it, and the pivot it is placed from.
//!
//! The target dummy on the range is the first hull to tumble. It is placed
//! the way a wreck piece is, from its centre of mass rather than from the
//! lattice origin, because rotating an off centre body about its origin
//! swings it round in an arc, and a ship kicked at the bow should turn on the
//! spot it was hit rather than orbit an invisible point.

use crate::*;

/// A hull that never moves on its own and never fires: something to shoot.
#[derive(Component)]
pub(crate) struct Dummy;

/// The body the core weighed, and how it is moving.
#[derive(Component)]
pub(crate) struct Tumble {
    pub(crate) body: Body,
    /// Angular velocity, in the world's frame, radians a second.
    pub(crate) spin: Vec3,
    /// Where the centre of mass is, in the world. The entity is placed from
    /// this every frame.
    pub(crate) pivot: Vec3,
    /// Cells have come off since it was last weighed.
    pub(crate) stale: bool,
}

/// How fast a spin dies, a share a second: the attitude thrusters fighting
/// it. Nought would be a ship that spins for ever, which is honest for
/// vacuum and useless on a range.
pub(crate) const SPIN_DAMP: f32 = 0.35;

/// And how fast a shove dies, the same way: station keeping.
pub(crate) const DRIFT_DAMP: f32 = 0.5;

pub(crate) fn weigh(hull: &Hull) -> Option<Body> {
    Body::of(&hull.model, |n| !hull.damage.is_dead(n))
}

impl Tumble {
    pub(crate) fn new(body: Body, xf: &Transform) -> Tumble {
        Tumble {
            pivot: xf.transform_point(Vec3::from(body.centre)),
            body,
            spin: Vec3::ZERO,
            stale: false,
        }
    }

    /// An impulse `j` at the world point `at`: into the model's frame, where
    /// the tensor lives, and the two velocities back out.
    pub(crate) fn kick(&mut self, hull: &mut Hull, xf: &Transform, at: Vec3, j: Vec3) {
        let inv = xf.rotation.inverse();
        let scale = xf.scale.x.max(1e-6);
        let at_local = inv * (at - xf.translation) / scale;
        let j_local = inv * j;
        let (dv, dw) = self.body.kick(at_local.to_array(), j_local.to_array());
        hull.vel += xf.rotation * Vec3::from(dv) * scale;
        self.spin += xf.rotation * Vec3::from(dw);
        self.stale = true;
    }
}

/// A dummy's hull lands through commands a frame after it is asked for;
/// this weighs it the frame it is there.
#[allow(clippy::type_complexity)]
pub(crate) fn adopt_dummies(
    mut commands: Commands,
    q: Query<(Entity, &Hull, &Transform), (With<Dummy>, Without<Tumble>)>,
) {
    for (e, hull, xf) in &q {
        if let Some(body) = weigh(hull) {
            info!(
                "target dummy weighs {} cells, centre {:.2},{:.2},{:.2}",
                body.mass, body.centre[0], body.centre[1], body.centre[2]
            );
            commands.entity(e).insert(Tumble::new(body, xf));
        }
    }
}

/// Integrate the spin and the drift, re-weighing a body that has lost cells
/// first, so a ship with its bow shot off turns about where it balances now.
pub(crate) fn tumble(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    mut q: Query<(&mut Hull, &mut Tumble, &mut Transform)>,
) {
    let dt = scene.step(&time);
    for (mut hull, mut t, mut xf) in &mut q {
        if t.stale {
            if let Some(b) = weigh(&hull) {
                t.pivot = xf.transform_point(Vec3::from(b.centre));
                t.body = b;
            }
            t.stale = false;
        }
        let step = hull.vel * dt;
        t.pivot += step;
        if t.spin.length_squared() > 0.0 {
            xf.rotation = (Quat::from_scaled_axis(t.spin * dt) * xf.rotation).normalize();
        }
        xf.translation = t.pivot - xf.rotation * (Vec3::from(t.body.centre) * xf.scale);
        let fade = |v: Vec3, k: f32| v * (1.0 - k * dt).max(0.0);
        t.spin = fade(t.spin, SPIN_DAMP);
        hull.vel = fade(hull.vel, DRIFT_DAMP);
    }
}

/// Where the dummy hangs, off the flagship's bow quarter, in its radii.
pub(crate) fn dummy_station(radius: f32) -> Vec3 {
    Vec3::new(radius * 3.2, 0.0, radius * 1.0)
}

pub(crate) fn spawn_dummy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    radius: f32,
) -> Entity {
    let (e, _) = spawn_hull(
        commands,
        meshes,
        materials,
        tex,
        &scene.target_hull,
        ShipSpec::at(Transform::from_translation(dummy_station(radius)))
            .chewers((scene.chewers / 2) as u32)
            .seed(7),
    );
    // `spawn_hull` marks a hull with no station as the flagship and selects
    // it, because a fresh flagship is the only such hull in a fight. A dummy
    // is the second, and two flagships is a camera and a nav disc that do
    // nothing: `single()` fails on both.
    commands
        .entity(e)
        .remove::<(Flagship, Selected)>()
        .insert(Dummy);
    e
}
