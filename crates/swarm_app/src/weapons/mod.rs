//! What fires: the live shots, the tracers, the beams' meshes and the
//! numbers a beam is made of.

mod beams;
mod flak;
mod range;

pub(crate) use beams::*;
pub(crate) use flak::*;
pub(crate) use range::*;

use crate::*;

/// What one beam does to each cell it bites, and how many bites it takes.
///
/// A ring of bites rather than one, because a beam that took a single cell
/// off a mothership would need thousands of shots to show and the picture
/// would never change. Eight opens a crater the size of the weapon.
pub(crate) const BEAM_DAMAGE: f32 = 260.0;

pub(crate) const BEAM_BITES: u32 = 8;

/// How far a gun shoots, in hull radii. Far enough to reach the carriers,
/// which stand well off.
pub(crate) const BEAM_RANGE: f32 = 34.0;

/// Every shot and every blast that is still live.
///
/// They are here rather than as entities because they are read as one batch
/// every frame by two things (the mesh that draws them and the uniform the
/// swarm is handed) and neither wants a query: a few dozen of anything is a
/// Vec, and an entity per beam would be an archetype move per shot fired.
#[derive(Resource, Default)]
pub(crate) struct LiveFx {
    pub(crate) beams: Vec<Beam>,
    pub(crate) blasts: Vec<Blast>,
    /// Rounds in the air. A flak burst used to appear where it was going to
    /// go off, which is a gun with no shell: the muzzle flashed, the target
    /// flashed, and nothing crossed the gap between them. A tracer is that
    /// gap, drawn as a short bright bolt travelling at a real speed, and the
    /// blast is pushed when it ARRIVES rather than when the trigger is
    /// pulled.
    pub(crate) tracers: Vec<Tracer>,
    /// Totals over the whole run, for the headless report. A picture with no
    /// beam in it and a picture of a beam that was never fired look the same,
    /// and only one of them is a bug in this file.
    pub(crate) fired: usize,
    pub(crate) flak: usize,
    pub(crate) sparked: usize,
}

/// One round in flight.
#[derive(Clone, Copy)]
pub(crate) struct Tracer {
    pub(crate) from: Vec3,
    pub(crate) to: Vec3,
    /// How far along it is, nought to one.
    pub(crate) t: f32,
    /// How much of the flight one tick covers.
    pub(crate) rate: f32,
    /// What it does when it lands, in world units.
    pub(crate) burst: f32,
    /// And what else, when it is a torpedo on the range: handed to `Landed`
    /// the frame it arrives, for the range to strike with.
    pub(crate) payload: Option<Landing>,
    pub(crate) colour: [f32; 3],
}

#[derive(Component)]
pub(crate) struct BeamMesh;

#[derive(Resource)]
pub(crate) struct BeamHandle(pub(crate) Handle<Mesh>);

/// How many quads the beam mesh carried this frame.
#[derive(Resource, Default)]
pub(crate) struct BeamQuads(pub(crate) usize);

/// How wide a beam bites, in hull cells, and how far its far end SWEEPS while
/// it is alive, in radians.
///
/// A beam used to be a fixed segment for its whole life: it killed whatever
/// was on that line at the tick it went off and nothing after. Sweeping it
/// carves an ARC through the cloud over the nine ticks it lives, which is
/// what sets off a line of kills a player can watch travel.
pub(crate) const BEAM_WIDTH: f32 = 5.5;

pub(crate) const BEAM_SWEEP: f32 = 0.55;

/// Drop what has gone out, and hand what is left to the swarm as capsules.
///
/// The blast's radius is grown HERE rather than in the shader, which is what
/// lets a beam and a blast be one shape on the other side: the shader tests a
/// capsule and never learns there are two kinds.
pub(crate) fn age_fx(
    tick: Res<Tick>,
    mut fx: ResMut<LiveFx>,
    mut shots: ResMut<Shots>,
    sparks: Res<SparkQueue>,
) {
    fx.sparked += sparks.0.len();
    fx.beams.retain(|b| b.live(tick.tick));
    fx.blasts.retain(|b| b.live(tick.tick));
    shots.0.clear();
    for b in fx.beams.iter_mut() {
        // SWEPT. The far end walks across while the beam is alive, so what the
        // swarm is handed each tick is a different segment and the beam carves
        // an arc rather than cutting one thread. The mesh is rebuilt from the
        // same endpoints, so what is drawn is what kills.
        let from = Vec3::from(b.from);
        let along = Vec3::from(b.to) - from;
        let len = along.length();
        if len > 1e-4 {
            let dir = along / len;
            // About an axis of its own, so two guns firing together sweep
            // different ways instead of scything in step.
            let seed = swarm_core::rng::hash_cell(b.born ^ (len.to_bits()));
            let mut ax = Vec3::new(
                (seed & 0xFF) as f32 / 255.0 - 0.5,
                ((seed >> 8) & 0xFF) as f32 / 255.0 - 0.5,
                ((seed >> 16) & 0xFF) as f32 / 255.0 - 0.5,
            );
            ax = (ax - dir * ax.dot(dir)).normalize_or(dir.any_orthonormal_vector());
            let step = Quat::from_axis_angle(ax, BEAM_SWEEP / swarm_core::fx::BEAM_TICKS as f32);
            b.to = (from + step * (dir * len)).to_array();
        }
        shots.0.push(Capsule {
            from,
            to: Vec3::from(b.to),
            radius: b.radius,
        });
    }
    for b in &fx.blasts {
        let at = Vec3::from(b.at);
        shots.0.push(Capsule {
            from: at,
            to: at,
            radius: b.radius_at(tick.tick),
        });
    }
}
