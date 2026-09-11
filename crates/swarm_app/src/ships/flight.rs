//! How a capital ship flies an order: the envelope, the rocks, the heading.

use crate::*;

/// What a hull can do, in hull radii a second.
///
/// The numbers are the approved prototype's, at its frigate's own radius:
/// cruise at 1.65, accelerate at 1.15, ease the speed off over the last four
/// lengths and turn at 2.2 radians a second. A capital ship turns before it
/// moves and settles into an arrival rather than overshooting, and it is
/// still slow enough that the swarm has time to follow and a player has time
/// to watch it.
pub(crate) const HULL_SPEED: f32 = 1.65;

pub(crate) const HULL_ACCEL: f32 = 1.15;

/// Close enough to have arrived, in hull radii.
pub(crate) const ARRIVE: f32 = 0.6;

/// How fast a hull turns onto its heading, in radians a second, eased.
pub(crate) const HULL_TURN: f32 = 2.2;

/// Fly the hull to wherever it was told, and turn it to face the way it is
/// going.
///
/// A real envelope rather than a lerp: it accelerates, it has a top speed, and
/// it slows into the arrival, so a move has weight and a player can see that
/// giving an order to a capital ship is a commitment.
pub(crate) fn fly_hull(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    // WITHOUT a carrier. A mothership is a `Hull` now so that cells come off
    // it the way they come off a ship, and every system that takes hulls
    // therefore takes carriers too unless it says otherwise. This one would
    // have every carrier flying the flagship's own orders. And WITHOUT a
    // wreck, which is a hull too and drifts on its own rule.
    mut hulls: Query<(&mut Hull, &mut Transform, Option<&Escort>), (Without<Hive>, Without<Wreck>)>,
) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (mut hull, mut xf, escort) in &mut hulls {
        let radius = hull.model.radius();
        // What it can still pull. A ship that has been chewed to the stern
        // does not accelerate like a fresh one and does not cruise like one
        // either: both scale with the drives it has left, so losing them reads
        // as a ship going lame rather than as a paint job.
        let thrust = hull.thrust();
        let (speed, accel, arrive) =
            (radius * HULL_SPEED * thrust, radius * HULL_ACCEL * thrust, radius * ARRIVE);
        let mut hurry = 1.0;
        let want = match escort {
            // An escort has no order of its own: its goal is a place in the
            // formation, which MOVES, so it is never reached and never
            // cleared. Matching the leader's velocity is what makes it hold
            // station rather than trail: steering alone would put it
            // permanently behind by however far it takes to close the gap.
            Some(e) => {
                let goal = lead.pos + lead.rot * e.station;
                let to = goal - xf.translation;
                let d = to.length();
                // A reinforcement ARRIVES: three times cruise while it is far
                // out, easing back to cruise over the last eight lengths, so
                // a wave called from off the map is on station in seconds
                // rather than in the minute a capital ship's cruise would
                // take. It is the same envelope, with the cap raised.
                hurry = 1.0 + 2.0 * (d / (radius * 8.0)).min(1.0);
                if d < arrive * 0.5 {
                    lead.vel
                } else {
                    lead.vel + to / d * speed * hurry * (d / (radius * 3.0)).min(1.0)
                }
            }
            None => match hull.order {
                Some(t) => {
                    let to = t - xf.translation;
                    let d = to.length();
                    if d < arrive {
                        hull.order = None;
                        Vec3::ZERO
                    } else {
                        // Slow into it: the speed asked for falls off over the
                        // last few lengths, so it settles rather than overshooting
                        // and hunting back and forth.
                        to / d * speed * (d / (radius * 4.0)).min(1.0)
                    }
                }
                None => Vec3::ZERO,
            },
        };
        let mut want = want;
        // ---- round the rocks ----
        //
        // A ship used to fly straight through an asteroid, which is the field
        // being scenery rather than terrain. It steers on the same distance
        // field the swarm does, at the ship's own scale: a rock inside the
        // keep-out radius pushes the ship out along the normal, harder the
        // closer it is.
        for r in &cfg.rocks {
            let off = xf.translation - r.truncate();
            let d = off.length();
            let keep = r.w + radius * ROCK_CLEAR;
            if d < keep && d > 1e-4 {
                let n = off / d;
                let bite = ((keep - d) / keep).clamp(0.0, 1.0);
                want += n * speed * bite * 3.0;
            }
        }

        let dv = want - hull.vel;
        let step = accel * hurry * dt;
        let was = hull.vel;
        hull.vel += if dv.length() > step { dv.normalize() * step } else { dv };
        hull.accel = if dt > 0.0 { (hull.vel - was) / dt } else { Vec3::ZERO };
        xf.translation += hull.vel * dt;

        // And never INSIDE a rock. Steering can be beaten: an order given
        // straight through an asteroid asks for exactly that, and a capital
        // ship has the momentum to win the argument. This is the guarantee,
        // and it costs nothing in the normal case.
        for r in &cfg.rocks {
            let off = xf.translation - r.truncate();
            let d = off.length();
            let keep = r.w + radius * 0.9;
            if d < keep && d > 1e-4 {
                let n = off / d;
                xf.translation = r.truncate() + n * keep;
                // Slide along it rather than stopping dead on it.
                let into = hull.vel.dot(n).min(0.0);
                hull.vel -= n * into;
            }
        }

        // Face the way it is going, eased, and only while it is going
        // anywhere: a ship at rest keeps the heading it stopped on.
        //
        // NEGATED, and this was a real bug rather than a taste. Bevy's
        // `forward` is -Z and `looking_to` aims that at the direction it is
        // given; a hull's bow is +Z, because the lattice runs stern to bow.
        // Aimed straight, the ship flew stern first with its main engines
        // leading, which read as a retro firing at full throttle while
        // accelerating and was what made the flames look wrong.
        if hull.vel.length() > radius * 0.05 {
            let want = Transform::from_translation(xf.translation).looking_to(-hull.vel.normalize(), Vec3::Y).rotation;
            xf.rotation = xf.rotation.slerp(want, (dt * HULL_TURN).min(1.0));
        }
    }
}
