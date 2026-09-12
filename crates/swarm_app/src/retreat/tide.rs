//! The swarm arriving: one carrier, then a ramp, then the rest at once.
//!
//! The core answers how many should be out at tick N and this spawns the
//! difference. Killing one does NOT bring another: the count it compares
//! against is how many have ever been spawned, so a carrier you killed is a
//! carrier that is not in the system, which is the whole reason to shoot at
//! them while you gather.

use crate::*;

#[derive(Resource, Default)]
pub(crate) struct TideState {
    pub(crate) spawned: usize,
    /// The phase last reported, so the log says something only when it
    /// changes.
    pub(crate) said: Option<Phase>,
}

pub(crate) fn tide_carriers(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    mut state: ResMut<TideState>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    cfg: Res<SwarmConfig>,
) {
    if !scene.retreat {
        return;
    }
    let phase = scene.tide.phase_at(tick.tick);
    if state.said != Some(phase) {
        info!(
            "the tide is {} ({} carriers by now)",
            phase.label(),
            scene.tide.carriers_at(tick.tick)
        );
        state.said = Some(phase);
    }
    let want = scene.tide.carriers_at(tick.tick);
    while state.spawned < want {
        spawn_hive(
            &mut commands,
            &mut meshes,
            &mut materials,
            &tex,
            HiveSeat {
                n: state.spawned,
                of: scene.tide.carriers.max(1),
                radius: cfg.hull_radius,
                stand: scene.stand,
            },
        );
        state.spawned += 1;
    }
}
