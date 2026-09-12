//! What the app is doing: showing the menu, taking a setup, playing a scene
//! or showing how it ended. The field is spawned on the way into `Playing`
//! and torn down on the way out of it, and what survives a teardown is
//! exactly what was spawned before any scene existed.

use crate::*;

/// The front door, the form, the fight and the verdict.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum AppState {
    /// The first frame, before any screen. The initial state's `OnEnter`
    /// runs BEFORE `PreStartup`, so a run that opened straight on the fight
    /// would spawn its field before the textures or the camera existed;
    /// `boot` moves on from here once startup has run.
    #[default]
    Boot,
    Menu,
    Setup,
    Playing,
    Result,
}

/// Where the arguments said to open.
#[derive(Resource)]
pub(crate) struct Initial(pub(crate) AppState);

pub(crate) fn boot(initial: Res<Initial>, mut next: ResMut<NextState<AppState>>) {
    next.set(initial.0);
}

/// Spawned once, before any scene, and kept across all of them: the camera,
/// the sky, the lights, the meshes drawn every frame from state.
///
/// A marker on the FEW things that persist rather than on the many that do
/// not, because the field is spawned from a dozen places (a wave, a fighter,
/// a wreck, a chunk) and a marker every one of them had to remember is a
/// marker one of them would forget.
#[derive(Component)]
pub(crate) struct Keep;

/// Runs after the backdrop has been spawned and before any field has: tags
/// everything in the world as kept.
pub(crate) fn mark_keep(
    mut commands: Commands,
    q: Query<Entity, (With<Transform>, Without<Keep>)>,
) {
    for e in &q {
        commands.entity(e).insert(Keep);
    }
}

/// Take the field down: every top level entity that is not kept and is not a
/// UI node. Children go with their parents, which is how a turret leaves with
/// its hull and a rock's surfaces with the rock.
// The filter IS the interface here: four components say what a field is.
#[allow(clippy::type_complexity)]
pub(crate) fn teardown_field(
    mut commands: Commands,
    q: Query<
        Entity,
        (
            With<Transform>,
            Without<Keep>,
            Without<Node>,
            Without<ChildOf>,
        ),
    >,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Everything a scene leaves behind in a resource, put back to its start.
///
/// The swarm's lists are emptied and its generation bumped, which is what
/// makes the render world build a NEW cloud for this scene instead of
/// carrying the last one in. The tick starts from nought, so `--explode 90`
/// means the same thing on the third scene as on the first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reset_run(
    scene: Res<SceneSpec>,
    mut cfg: ResMut<SwarmConfig>,
    mut fx: ResMut<LiveFx>,
    mut shots: ResMut<Shots>,
    mut tick: ResMut<Tick>,
    mut order: ResMut<NavOrder>,
    mut mode: ResMut<OrderMode>,
    mut pings: ResMut<Pings>,
    mut ack: ResMut<Ack>,
    mut lead: ResMut<Lead>,
    mut marquee: ResMut<Marquee>,
    mut hud: ResMut<Hud>,
    mut outcome: ResMut<Outcome>,
) {
    cfg.hives.clear();
    cfg.targets.clear();
    cfg.rocks.clear();
    cfg.paused = false;
    cfg.frozen = false;
    cfg.time_scale = scene.time_scale;
    *fx = LiveFx::default();
    *shots = Shots::default();
    *tick = Tick::default();
    *order = NavOrder::default();
    *mode = OrderMode::default();
    *pings = Pings::default();
    *ack = Ack::default();
    *lead = Lead::default();
    *marquee = Marquee::default();
    hud.paused = false;
    *outcome = Outcome::default();
}

/// What a RUN carries into the system it is entering, and what it does not.
///
/// A second system rather than four more arguments on `reset_run`, because
/// that one is already at Bevy's limit on how many parameters a system
/// function may take, and past it `.chain()` simply stops compiling with
/// nothing pointing at the count. Which is the right split anyway: one of
/// these resets a SCENE and the other resets a LEG of a run, and they are
/// only ever ordered together because a leg is played as a scene.
pub(crate) fn reset_retreat(
    run: Res<RunState>,
    mut scene: ResMut<SceneSpec>,
    mut bank: ResMut<Bank>,
    mut drive: ResMut<JumpDrive>,
    mut tide: ResMut<TideState>,
) {
    if !scene.retreat {
        return;
    }
    // The scene is written from the RUN on the way in, which is what makes
    // the next system the next system: the node decides the seed, the rocks,
    // the tide and which support ships arrive, and `spawn_field` after this
    // reads exactly that.
    run.write(&mut scene);
    // The bank CARRIES between systems, because that is what a run is: the
    // drive and the tide do not, because each system has its own.
    *bank = run.bank.clone();
    *drive = JumpDrive::default();
    *tide = TideState::default();
    info!("{}", run.brief());
}

/// How a scene ended, and what it cost.
#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct Outcome {
    pub(crate) won: bool,
    /// Ticks from the first frame to the verdict.
    pub(crate) ticks: u32,
    /// Player hulls that went critical, counted by `go_critical`.
    pub(crate) ships_lost: u32,
    /// Cells taken off player hulls, the dead ones included.
    pub(crate) cells_lost: usize,
    /// Carriers killed of the carriers there were.
    pub(crate) hives_killed: usize,
    pub(crate) hives: usize,
    /// Player hulls there were, the flagship and its wave.
    pub(crate) ships: u32,
    /// The tick the verdict was reached, if it has been. The screen follows
    /// `VERDICT_GRACE` later, so the fireball and the wreck are seen first.
    pub(crate) decided: Option<u32>,
    /// The system was left on the jump drive rather than won or lost, which
    /// is how a retreat ends when it goes well.
    pub(crate) jumped: bool,
    /// Ships that were outside the field when it went.
    pub(crate) left: u32,
    /// The jump was out of the LAST system, which is the run won rather than
    /// a leg of it finished.
    pub(crate) escaped: bool,
}

/// Four seconds between the verdict and the screen that says it.
pub(crate) const VERDICT_GRACE: u32 = 240;

/// The verdict. Every carrier a wreck is a win; every player hull a wreck is
/// a loss; a sandbox never ends. Wrecks are hulls too and count as neither,
/// which is what `dead_hull` is for.
pub(crate) fn judge(
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    hulls: Query<(&Hull, Option<&Hive>), Without<Wreck>>,
    mut outcome: ResMut<Outcome>,
    mut next: ResMut<NextState<AppState>>,
) {
    // The field is spawned through commands and lands a frame late, and a
    // scene with no carriers in it is a playground rather than a fight.
    if scene.sandbox || tick.tick < 60 || scene.hives == 0 {
        return;
    }
    if let Some(at) = outcome.decided {
        if tick.tick >= at + VERDICT_GRACE {
            next.set(AppState::Result);
        }
        return;
    }
    let mut ships = 0;
    let mut hives = 0;
    let mut cells = 0;
    for (h, hive) in &hulls {
        if h.dead_hull {
            continue;
        }
        if hive.is_some() {
            hives += 1;
        } else {
            ships += 1;
            cells += h.damage.dead_count();
        }
    }
    // In a retreat the way out is the drive: killing every carrier in a
    // system is worth doing and is not a victory, because the tide brings
    // more and the fleet is still coming. Only the loss applies.
    let verdict = if hives == 0 && !scene.retreat {
        Some(true)
    } else if ships == 0 {
        Some(false)
    } else {
        None
    };
    let Some(won) = verdict else { return };
    outcome.won = won;
    outcome.ticks = tick.tick;
    outcome.cells_lost += cells;
    outcome.hives = scene.hives;
    outcome.hives_killed = scene.hives - hives;
    outcome.ships = scene.reinforce + 1;
    outcome.decided = Some(tick.tick);
    info!("{}: {:?}", if won { "victory" } else { "defeat" }, *outcome);
}
