//! Flak: a round that flies, and the blast where it lands.

use crate::*;

/// How many ticks a round takes to cross to where it is going.
pub(crate) const TRACER_TICKS: u32 = 7;

/// How long a bolt is drawn, as a share of its whole flight, and how wide.
pub(crate) const TRACER_LEN: f32 = 0.22;

pub(crate) const TRACER_WIDTH: f32 = 0.06;

/// Fly the rounds, and set off the ones that have arrived.
///
/// The blast is pushed HERE rather than where the trigger was pulled, so what
/// kills a mote is the shell reaching it. That also means a player can watch a
/// burst travel into the cloud and see the hole it makes appear at the end of
/// its own flight, which is the whole reason for having a shell at all.
pub(crate) fn fly_tracers(tick: Res<Tick>, mut fx: ResMut<LiveFx>, mut sparks: ResMut<SparkQueue>) {
    let mut landed: Vec<Tracer> = Vec::new();
    for t in fx.tracers.iter_mut() {
        t.t += t.rate;
        if t.t >= 1.0 {
            landed.push(*t);
        }
    }
    fx.tracers.retain(|t| t.t < 1.0);
    for t in landed {
        fx.blasts.push(Blast {
            at: t.to.to_array(),
            radius: t.burst,
            born: tick.tick,
        });
        let mut list = Vec::new();
        blast_sparks(
            tick.tick.wrapping_add(t.to.x.to_bits()),
            t.to.to_array(),
            t.burst,
            26,
            &mut list,
        );
        for s in &mut list {
            s.life *= 0.34;
            s.size *= 0.9;
        }
        fx.sparked += list.len();
        sparks.extend(list);
    }
}

/// Point defence: the ship rakes the cloud around itself, constantly.
///
/// A flak burst is a `Blast`, which is to say a capsule of ZERO LENGTH, which
/// is the shape the shot path already carries. So this needed no new kind of
/// anything: it is placed out along a gun's own line at the standoff the
/// swarm holds, the shader kills whatever is inside it, and the CPU never
/// learns where a mote was. That is the whole point of resolving a shot as a
/// volume rather than as a target.
///
/// Fast and weak against the long slow beams that go for the carriers, so the
/// two read as two different weapons doing two different jobs.
pub(crate) fn fire_flak(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    // Not the carriers, which are hulls now: a mothership does not
    // carry the fleet's guns and would otherwise open fire on its own side.
    hulls: Query<(&Hull, &Transform), Without<Hive>>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    // A quarter of the beam cadence, so a hull is always putting something up.
    let every = (scene.cadence / 4).max(2);
    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let radius = hull.model.radius();
        for (n, g) in hull.guns.iter().enumerate() {
            let phase = (swarm_core::rng::hash_cell(g.cell ^ 0x51A7 ^ hull.seed) % every) as u32;
            if (tick.tick + phase) % every != 0 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(g.at));
            let out = (xf.rotation * Vec3::from(g.out)).normalize_or_zero();
            // Swept fast and wide, so the bursts walk round the hull rather
            // than punching the same hole in the cloud.
            let t = tick.tick as f32 * 0.21 + n as f32 * 2.3;
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(out);
            let dir = (out + side * (t.sin() * 0.8) + up * ((t * 1.3).cos() * 0.55)).normalize();
            // Out where the swarm actually holds: a burst inside the standoff
            // would go off in clear space every time.
            let reach = radius * (1.2 + 0.7 * ((t * 0.37).sin() * 0.5 + 0.5));
            let centre = at + dir * reach;
            // A ROUND, not an explosion at the far end. A flak burst used to
            // appear where it was going to go off, which is a gun with no
            // shell in it: the muzzle flashed, the target flashed, and
            // nothing at all crossed the gap between them. It travels now,
            // and the blast is pushed when it ARRIVES.
            fx.tracers.push(Tracer {
                from: at,
                to: centre,
                t: 0.0,
                rate: 1.0 / TRACER_TICKS as f32,
                burst: radius * 0.42,
                colour: [4.2, 2.1, 0.55],
            });
            fx.flak += 1;

            // The muzzle flash, where the gun is. Bigger than it was, because
            // a capital ship's point defence going off ought to light the
            // plating round it.
            let mut list = Vec::new();
            blast_sparks(
                tick.tick.wrapping_add(g.cell),
                at.to_array(),
                radius * 0.20,
                14,
                &mut list,
            );
            for s in &mut list {
                s.life *= 0.22;
                s.size *= 0.7;
            }
            sparks.extend(list);
            let mut flash = Vec::new();
            muzzle_sparks(
                g.cell ^ hull.seed ^ tick.tick,
                at.to_array(),
                dir.to_array(),
                hull.model.cell * 0.7,
                &mut flash,
            );
            sparks.extend(flash);
        }
    }
}
