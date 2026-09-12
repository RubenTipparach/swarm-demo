//! The way out: volatiles become fuel, fuel spools the drive, and the drive
//! takes what is standing near it.

use crate::*;

/// What the DRIVE itself costs to spin up, in refined fuel, and what every
/// other hull in the field adds to that.
///
/// A jump moves a FLEET, so it is priced by the fleet: the command ship
/// carries the drive and costs the most, and every hull standing inside the
/// field when it goes costs its own rung. That is what makes calling in an
/// escort a decision with two sides to it, because the ship that helps you
/// hold a system is the ship you then have to pay to take out of it.
///
/// The two the owner set are the command ship at 250 and a frigate at 50;
/// the rest is the rung ladder those two imply, doubling a rung. A civil
/// trade is frigate sized and is priced there.
pub(crate) const DRIVE_COST: u32 = 250;

pub(crate) fn rung_cost(class: &str) -> u32 {
    match class.rsplit_once('_').map(|(_, rung)| rung) {
        Some("corvette") => 25,
        Some("destroyer") => 100,
        Some("cruiser") => 200,
        _ => 50,
    }
}

/// What it costs to take what is actually in the field out of it.
///
/// Counted off the LIVE ships rather than off the run's roster, so an escort
/// that died is an escort you no longer pay for, and a player watching the
/// number drop as a ship goes is being told exactly what a jump is.
pub(crate) fn jump_cost(run: &RunState, ships: &Query<JumpShip, JumpFilter>) -> u32 {
    let mut cost = 0;
    for (_, _, hull, support, flag) in ships.iter() {
        if hull.dead_hull {
            continue;
        }
        cost += match (flag.is_some(), support) {
            (true, _) => DRIVE_COST,
            (false, Some(s)) => rung_cost(s.role.hull()),
            // An escort is a copy of the flagship's class by construction.
            (false, None) => rung_cost(&run.flagship),
        };
    }
    cost
}

/// The ships a jump takes and prices: everything of the fleet's that is not
/// a carrier, a rock or a wreck. Named once, because the cost and the jump
/// itself have to ask the same question of the same set.
pub(crate) type JumpShip<'a> = (
    Entity,
    &'a Transform,
    &'a Hull,
    Option<&'a Support>,
    Option<&'a Flagship>,
);
pub(crate) type JumpFilter = (Without<Hive>, Without<Rock>, Without<Wreck>);

/// How long the drive spools, in ticks. Thirty seconds, which is long
/// enough to be a decision and short enough to be a relief.
pub(crate) const SPOOL_TICKS: u32 = 30 * 60;

/// How far the jump field reaches from the command ship, in its own radii.
/// Everything inside goes. Everything outside is left in the system.
pub(crate) const JUMP_FIELD: f32 = 14.0;

/// How often the tanker turns one unit of volatiles into one of fuel, in
/// ticks. Ten a second, so a fleet's four hundred is most of a minute of
/// refining after the crystal is landed, and that delay is the whole reason
/// fuel is not simply a second pile of materials: it starts when the first
/// cube is landed, so mining the crystal EARLY is what opens the window.
///
/// A whole cell on a cadence rather than a rate times the frame's step,
/// which is what this was and was wrong: at a fiftieth of a cell a frame,
/// `volatiles -= take.floor()` took nothing for ever while the fuel went up
/// anyway, so the tank filled itself out of a pile of ice that never
/// shrank. A rate in a resource counted in whole cells needs somewhere to
/// keep the remainder, and a cadence needs none.
pub(crate) const REFINE_TICKS: u32 = 6;

/// What the command ship arrives with in its own tank: the DRIVE's own cost
/// exactly, and none of the fleet's.
///
/// So you can always leave alone. A system that goes badly wrong costs the
/// escorts and the support ships standing outside the field when it fires,
/// and never the run itself, which is the answer the owner gave to "does
/// losing the tanker strand you": total loss is the better disaster, and a
/// disaster you cannot come back from at all is not a disaster, it is a
/// reload. It is written as the drive's cost rather than beside it, because
/// two numbers that have to be equal are one number.
pub(crate) const RESERVE: f32 = DRIVE_COST as f32;

#[derive(Resource, Default, Debug)]
pub(crate) struct JumpDrive {
    /// What it would cost to take the fleet that is standing here out of
    /// this system, in refined fuel.
    ///
    /// Worked out once a frame by `price_jump` rather than at each of the
    /// three places that want it (the panel, the button and the jump), which
    /// is the same rule the one clock keeps: a number three systems compute
    /// for themselves is a number two of them will compute differently.
    pub(crate) cost: u32,
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

/// What the fleet in this system would cost to jump out, every frame.
pub(crate) fn price_jump(
    run: Res<RunState>,
    ships: Query<JumpShip, JumpFilter>,
    mut drive: ResMut<JumpDrive>,
) {
    drive.cost = jump_cost(&run, &ships);
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
    let cost = drive.cost;
    if (bank.fuel as u32) < cost {
        let short = cost - bank.fuel as u32;
        ack.text = format!(
            "not enough fuel to take the fleet out: {:.0} of {cost}, which is {} more crystal cubes",
            bank.fuel,
            short.div_ceil(swarm_core::economy::CRYSTAL_CUBE)
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
pub(crate) fn script_jump(script: Res<Script>, mut drive: ResMut<JumpDrive>) {
    let Some(at) = script.jump else { return };
    // Whenever the drive is idle rather than on the first tick, because
    // there is no first tick to catch: the clock is advanced ahead of this
    // in the same frame, so tick nought is never a tick anything here sees.
    // It does not re-arm after a jump, because `jumped` stays set.
    if drive.ready_at.is_none() && !drive.jumped {
        drive.ready_at = Some(at);
    }
}

/// `--onward`: the map's first branch, taken without being pressed.
pub(crate) fn script_onward(
    script: Res<Script>,
    mut run: ResMut<RunState>,
    mut next: ResMut<NextState<AppState>>,
) {
    if !script.onward {
        return;
    }
    let Some(&to) = run.node().next.first() else {
        return;
    };
    run.go(to);
    next.set(AppState::Playing);
}

/// The jump itself: what is inside the field goes, what is outside stays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn jump_spool(
    tick: Res<Tick>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    mut drive: ResMut<JumpDrive>,
    mut bank: ResMut<Bank>,
    ships: Query<JumpShip, JumpFilter>,
    mut run: ResMut<RunState>,
    mut fx: ResMut<LiveFx>,
    mut outcome: ResMut<Outcome>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let Some(at) = drive.ready_at else { return };
    if tick.tick < at {
        return;
    }
    let cost = drive.cost;
    let reach = cfg.hull_radius * JUMP_FIELD;
    let mut went = Vec::new();
    let mut escorts = 0;
    let mut left = 0;
    for (e, xf, hull, support, flag) in &ships {
        if xf.translation.distance(lead.pos) > reach {
            left += 1;
            commands.entity(e).despawn();
            continue;
        }
        went.push(support.map(|s| s.role));
        // And what went is GONE from this system: the screen that follows
        // sits over the field, and a fleet still standing in it under the
        // word JUMPED is the one picture this must not leave behind.
        commands.entity(e).despawn();
        // The flagship carries its holes into the next system, which is what
        // makes a system cost anything at all: the field is built fresh every
        // time, so a run with no scars would arrive in a pristine ship
        // however the last one went.
        if flag.is_some() {
            run.scars = scars_of(hull);
        } else if support.is_none() {
            escorts += 1;
        }
    }
    // Never below nought: a scripted jump is not paid for, and a negative
    // tank would carry into the next system as a debt nothing can settle.
    // The guns that were firing went with the fleet. Everything on the
    // screen after this is frozen, so a beam left in the air is a beam that
    // hangs there for ever with nothing at the end of it.
    fx.beams.clear();
    fx.blasts.clear();
    bank.fuel = (bank.fuel - cost as f32).max(0.0);
    drive.ready_at = None;
    drive.jumped = true;
    drive.left = left;
    // What survived is the fleet the next system starts with.
    run.fleet = went.iter().filter_map(|r| *r).collect();
    run.escorts = escorts;
    run.bank = bank.clone();
    run.systems += 1;
    outcome.won = true;
    outcome.jumped = true;
    outcome.ticks = tick.tick;
    outcome.left = left;
    // The verdict never ran, because a jump is not a verdict: what a system
    // cost is what the jump itself can see.
    outcome.ships = (went.len() + left as usize) as u32;
    outcome.ships_lost = left;
    // A run standing on the gate has nowhere left to jump to, so this jump
    // was the last one and it is the run that ended rather than the system.
    outcome.escaped = run.done();
    // What it jumped OUT of, not where it is going: the branch is the map
    // screen's to pick, and until it is picked the run is still standing in
    // the system it just left.
    info!(
        "jumped out of the {} system with {} ships, leaving {left} behind, {:.0} fuel left, {} cells open",
        run.node().tag.label(),
        went.len(),
        bank.fuel,
        run.scars.len(),
    );
    next.set(AppState::Result);
}
