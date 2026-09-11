//! A gun that turns: the child entity and the slew that aims it.

use crate::*;

/// One gun, drawn as its own object so it can turn.
///
/// A turret's cells are lifted OUT of the hull's own mesh and given a child
/// entity that pivots on the cluster's middle. The alternative is redux-tribes'
/// approach of rewriting the turret's quads inside the hull's geometry every
/// frame, which it does for a reason that does not apply here: its hulls carve
/// holes through the same buffers. Here a gun is three to a ship and a child
/// transform is free, so the mesh is built once and only a rotation changes.
#[derive(Component)]
pub(crate) struct Turret {
    /// Which gun of the hull's list this is, so the aim matches the fire.
    pub(crate) slot: usize,
    /// Where it rests, in the hull's frame, when it has nothing to shoot at.
    pub(crate) rest: Vec3,
}

/// How fast a turret slews, in radians a second. Slow enough to watch.
pub(crate) const TURRET_SLEW: f32 = 1.9;

/// Swing every turret onto whatever its ship is shooting at.
///
/// The aim is the SAME answer `fire_guns` uses, so the barrel and the beam
/// agree: a turret that pointed somewhere the beam did not come out of would
/// be a decoration rather than a gun. It eases on a slew cap so a gun takes
/// time to come round, and it stands down to the facing its own cluster looks
/// out along when there is nothing in reach.
///
/// Everything is in the HULL's frame. The child's transform is relative to its
/// parent already, so the target has to be taken into that frame first, and
/// the rotation is then a plain `looking_to` with no ship pose in it at all.
pub(crate) fn aim_turrets(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    // Both of these must say `Without<Turret>`. Bevy proves two queries
    // disjoint from their FILTERS, not from what you know about the data: it
    // cannot tell that nothing is both a carrier and a turret, so a plain
    // `&Transform` on the carriers conflicts with the `&mut Transform` here.
    hulls: Query<(&Hull, &Transform), Without<Turret>>,
    hives: Query<(&Hive, &Transform, &Hull), Without<Turret>>,
    mut turrets: Query<(&Turret, &ChildOf, &mut Transform)>,
) {
    let dt = scene.step(&time);
    for (t, parent, mut xf) in &mut turrets {
        let Ok((hull, ship)) = hulls.get(parent.parent()) else {
            continue;
        };
        if hull.dead_hull {
            continue;
        }
        let Some(gun) = hull.guns.get(t.slot) else {
            continue;
        };
        let radius = hull.model.radius();
        let muzzle = ship.transform_point(Vec3::from(gun.at));

        // The nearest carrier it could reach, which is what `fire_guns` picks.
        let mut best: Option<(f32, Vec3)> = None;
        for (h, hxf, hhull) in &hives {
            if hhull.dead_hull {
                continue;
            }
            let d = hxf.translation.distance(muzzle) - h.radius;
            if d < radius * BEAM_RANGE && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, hxf.translation));
            }
        }

        // Into the hull's own frame, because a child's rotation is relative to
        // its parent and the ship is turning underneath it.
        let want_world = match best {
            Some((_, to)) => (to - muzzle).normalize_or(ship.rotation * t.rest),
            None => ship.rotation * t.rest,
        };
        let want_local = (ship.rotation.inverse() * want_world).normalize_or(t.rest);
        // Negated for the reason every hull here is negated: Bevy's forward is
        // minus Z and a barrel points along plus Z.
        let goal = Transform::IDENTITY
            .looking_to(-want_local, Vec3::Y)
            .rotation;
        xf.rotation = xf.rotation.slerp(goal, (dt * TURRET_SLEW).min(1.0));
    }
}
