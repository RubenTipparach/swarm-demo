//! The scenery: where the sky's bands sit, where the sun is, and the
//! showcase of alien archetypes.

use crate::*;

/// Bodies sit in this band, outside the fight and inside the far plane.
pub(crate) const NEAR_BAND: f32 = 250.0;

pub(crate) const FAR_BAND: f32 = 660.0;

pub(crate) const FAR_PLANE: f32 = 6000.0;

pub(crate) const STAR_RADIUS: f32 = 4500.0;

pub(crate) const SKY_SIZE: usize = 256;

#[derive(Component)]
pub(crate) struct Showcase;

/// Where the sun is, pointing AT it.
///
/// One vector with two consumers, and they have to be the same one. The key
/// light is aimed along it, and the swarm marches its density field along it
/// to work out what of that light reaches a mote buried in the cloud. Lit
/// from one side of the sky and shadowed from the other is the single thing
/// an eye will not forgive, and it is exactly what two numbers written down
/// in two places drift into.
pub(crate) const SUN: Vec3 = Vec3::new(0.42, 0.66, -0.62);

pub(crate) fn spin_showcase(time: Res<Time>, mut q: Query<&mut Transform, With<Showcase>>) {
    for mut xf in &mut q {
        xf.rotate_y(time.delta_secs() * 0.5);
    }
}
