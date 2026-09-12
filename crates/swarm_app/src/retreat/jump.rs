//! The way out: volatiles become fuel, fuel spools the drive, and the drive
//! takes what is standing near it.

use crate::*;

/// What one jump costs, in refined fuel. Two ice rocks' seams and the time
/// to refine them, which is the shape of a system: gather, or leave.
pub(crate) const JUMP_FUEL: f32 = 60.0;

/// How long the drive spools, in ticks. Thirty seconds, which is long
/// enough to be a decision and short enough to be a relief.
pub(crate) const SPOOL_TICKS: u32 = 30 * 60;

/// How far the jump field reaches from the command ship, in its own radii.
/// Everything inside goes. Everything outside is left in the system.
pub(crate) const JUMP_FIELD: f32 = 14.0;

/// How often the tanker turns one cell of ice into one unit of fuel, in
/// ticks. Two a second, so a full tank is half a minute of refining after
/// the ice is aboard, and that delay is the whole reason fuel is not simply
/// a second pile of materials: it starts when the first ice is landed, so
/// mining the ice EARLY is worth doing.
///
/// A whole cell on a cadence rather than a rate times the frame's step,
/// which is what this was and was wrong: at a fiftieth of a cell a frame,
/// `volatiles -= take.floor()` took nothing for ever while the fuel went up
/// anyway, so the tank filled itself out of a pile of ice that never
/// shrank. A rate in a resource counted in whole cells needs somewhere to
/// keep the remainder, and a cadence needs none.
pub(crate) const REFINE_TICKS: u32 = 30;

/// The command ship's own tank, which is what stops one bad minute from
/// ending a good run: enough for one hop, and never enough for two.
pub(crate) const RESERVE: f32 = 18.0;

#[derive(Resource, Default, Debug)]
pub(crate) struct JumpDrive {
    /// The tick the spool completes, once it has started.
    pub(crate) ready_at: Option<u32>,
    /// Ships left behind by the last jump, for the screen that says so.
    pub(crate) left: u32,
    pub(crate) jumped: bool,
}

impl JumpDrive {
    pub(crate) fn spooling(&self) -> bool {
        self.ready_at.is_some()
    }
}

/// The tanker at work. No tanker, no refining: a hold of volatiles with
/// nothing to process it is a hold of rock.
pub(crate) fn refine(tick: Res<Tick>, crews: Query<(&Support, &Hull)>, mut bank: ResMut<Bank>) {
    if bank.volatiles == 0 || !tick.tick.is_multiple_of(REFINE_TICKS) {
        return;
    }
    let tanker = crews
        .iter()
        .any(|(s, h)| s.role == Role::Tanker && !h.dead_hull);
    if !tanker {
        return;
    }
    bank.volatiles -= 1;
    bank.fuel += 1.0;
}

/// Start the spool: the J key, or the button that says the same thing.
pub(crate) fn jump_input(
    keys: Res<ButtonInput<KeyCode>>,
    presses: Query<&Interaction, (Changed<Interaction>, With<JumpButton>)>,
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    bank: Res<Bank>,
    mut drive: ResMut<JumpDrive>,
    mut ack: ResMut<Ack>,
) {
    if !scene.retreat || drive.spooling() || drive.jumped {
        return;
    }
    let asked = keys.just_pressed(KeyCode::KeyJ) || pressed(&presses);
    if !asked {
        return;
    }
    if bank.fuel < JUMP_FUEL {
        ack.text = format!(
            "not enough fuel to jump: {:.0} of {JUMP_FUEL:.0}",
            bank.fuel
        );
        ack.left = ACK_LIFE * 2.0;
        return;
    }
    drive.ready_at = Some(tick.tick + SPOOL_TICKS);
    ack.text = "the drive is spooling: everything inside the field jumps".into();
    ack.left = ACK_LIFE * 2.0;
    info!(
        "jump drive spooling, ready at tick {}",
        tick.tick + SPOOL_TICKS
    );
}

/// `--jump TICK`: the drive is ready at that tick, paid for or not.
pub(crate) fn script_jump(tick: Res<Tick>, script: Res<Script>, mut drive: ResMut<JumpDrive>) {
    let Some(at) = script.jump else { return };
    if tick.tick == 0 {
        drive.ready_at = Some(at);
    }
}

/// The jump itself: what is inside the field goes, what is outside stays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn jump_spool(
    tick: Res<Tick>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    mut drive: ResMut<JumpDrive>,
    mut bank: ResMut<Bank>,
    ships: Query<
        (Entity, &Transform, Option<&Support>),
        (With<Hull>, Without<Hive>, Without<Rock>, Without<Wreck>),
    >,
    mut run: ResMut<RunState>,
    mut outcome: ResMut<Outcome>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let Some(at) = drive.ready_at else { return };
    if tick.tick < at {
        return;
    }
    let reach = cfg.hull_radius * JUMP_FIELD;
    let mut went = Vec::new();
    let mut left = 0;
    for (e, xf, support) in &ships {
        if xf.translation.distance(lead.pos) <= reach {
            went.push(support.map(|s| s.role));
        } else {
            left += 1;
            commands.entity(e).despawn();
        }
    }
    // Never below nought: a scripted jump is not paid for, and a negative
    // tank would carry into the next system as a debt nothing can settle.
    bank.fuel = (bank.fuel - JUMP_FUEL).max(0.0);
    drive.ready_at = None;
    drive.jumped = true;
    drive.left = left;
    // What survived is the fleet the next system starts with.
    run.fleet = went.iter().filter_map(|r| *r).collect();
    run.bank = bank.clone();
    run.systems += 1;
    outcome.won = true;
    outcome.jumped = true;
    outcome.ticks = tick.tick;
    outcome.left = left;
    // A run standing on the gate has nowhere left to jump to, so this jump
    // was the last one and it is the run that ended rather than the system.
    outcome.escaped = run.done();
    run.advance();
    info!(
        "jumped with {} ships, leaving {left} behind, {:.0} fuel left; {}",
        went.len(),
        bank.fuel,
        run.brief()
    );
    next.set(AppState::Result);
}
