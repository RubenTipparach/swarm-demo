//! The retreat's panel: the clock you are playing against, what the fleet
//! has gathered, and the button that leaves.

use crate::*;

/// One live number on the panel.
#[derive(Component, Clone, Copy)]
pub(crate) enum RetreatStat {
    Tide,
    /// What is still in the field's rocks, which is the pair of numbers that
    /// answers "is there anything left here worth staying for". Two rows and
    /// not one, because they are not interchangeable: the crystal is the way
    /// out and the ore is everything after it.
    Ore,
    Crystal,
    Materials,
    Volatiles,
    Fuel,
    Data,
    /// The wreck a salvager has recovered most of, which is what the
    /// rebuild button would put back.
    Salvage,
    /// Cubes riding on the support ships, which is what is not in the bank
    /// YET: a full miner a long way from home is a hold a player should be
    /// able to see before the swarm gets to it.
    Cargo,
    Crews,
}

/// The button that says the same thing the J key does.
#[derive(Component)]
pub(crate) struct JumpButton;

/// One support ship as the panel reads it: what it is, what it was told to
/// do, what is in its hold and what state the hull is in.
pub(crate) type CrewRow<'a> = (&'a Support, &'a Job, &'a Hold, &'a Hull);

/// What each crew is doing, one line each.
///
/// The only thing on this panel that is a sentence rather than a number, and
/// its own function for that reason: a readout of eight numbers and a roster
/// is two things, and the second one is the only one anybody has to read.
fn crew_lines(crews: &Query<CrewRow>) -> String {
    crews
        .iter()
        .map(|(s, j, h, hull)| {
            let doing = if hull.dead_hull {
                "lost".to_string()
            } else {
                match j {
                    Job::Idle => "idle".into(),
                    Job::Work(_) if s.role.scans() => {
                        format!("scanning {:.0}%", h.share() * 100.0)
                    }
                    Job::Work(_) if s.role.mends() => "standing by".into(),
                    Job::Work(_) => format!("cutting {:.0}%", h.share() * 100.0),
                    Job::Unload(_) => "unloading".into(),
                }
            };
            format!("{}: {doing}", s.role.label())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the fleet is carrying against what it could carry, in cubes.
fn hold_line(crews: &Query<CrewRow>) -> String {
    let (aboard, room): (u32, u32) = crews
        .iter()
        .fold((0, 0), |(a, r), (_, _, h, _)| (a + h.carrying, r + h.cap));
    format!("{aboard} of {room}")
}

/// The wreck the salvagers have most of, which is the one a rebuild would
/// take: a panel naming every hulk would be a roster of the dead.
fn salvage_line(run: &RunState) -> String {
    match run
        .hulks
        .iter()
        .max_by(|a, b| a.share().total_cmp(&b.share()))
    {
        Some(h) => format!("{} {:.0}%", hull_label(&h.class), h.share() * 100.0),
        None => "none".into(),
    }
}

/// Everything the panel reads that is not a query, as ONE parameter.
///
/// Five resources in an argument list is the smell this project names: the
/// missing struct is "the state of the run this frame", and Bevy's own
/// `SystemParam` is what lets it be one without any of them being copied.
#[derive(SystemParam)]
pub(crate) struct RunView<'w> {
    tick: Res<'w, Tick>,
    scene: Res<'w, SceneSpec>,
    bank: Res<'w, Bank>,
    drive: Res<'w, JumpDrive>,
    run: Res<'w, RunState>,
}

/// The panel's numbers, and the one line that says what the fleet is doing.
pub(crate) fn retreat_readouts(
    view: RunView,
    crews: Query<CrewRow>,
    rocks: Query<&Rock>,
    mut stats: Query<(&RetreatStat, &mut Text, &mut TextColor)>,
) {
    let RunView {
        tick,
        scene,
        bank,
        drive,
        run,
    } = view;
    // What it would cost to take the fleet that is actually standing here
    // out of the system, which is the number the button is judged against.
    let cost = drive.cost;
    let ore: u32 = rocks.iter().map(|r| r.ore).sum();
    let crystal: u32 = rocks.iter().map(|r| r.crystal).sum();
    let left = scene.tide.until_fleet(tick.tick);
    let phase = scene.tide.phase_at(tick.tick);
    for (stat, mut t, mut colour) in &mut stats {
        let want = match stat {
            RetreatStat::Tide => match drive.ready_at {
                Some(at) => {
                    let s = at.saturating_sub(tick.tick) / 60;
                    format!("JUMPING IN {}:{:02}", s / 60, s % 60)
                }
                None if phase == Phase::Fleet => "THE FLEET IS IN".into(),
                None => format!(
                    "{} {}:{:02}",
                    phase.label(),
                    left / 60 / 60,
                    (left / 60) % 60
                ),
            },
            RetreatStat::Ore => ore.to_string(),
            RetreatStat::Crystal => crystal.to_string(),
            RetreatStat::Materials => bank.materials.to_string(),
            RetreatStat::Volatiles => bank.volatiles.to_string(),
            RetreatStat::Fuel => format!("{:.0} of {cost}", bank.fuel),
            RetreatStat::Data => bank.data.to_string(),
            RetreatStat::Salvage => salvage_line(&run),
            RetreatStat::Cargo => hold_line(&crews),
            RetreatStat::Crews => crew_lines(&crews),
        };
        if t.0 != want {
            t.0 = want;
        }
        // The clock goes red when the fleet is in, because that is the one
        // thing on this panel a player has to see without reading it.
        if matches!(stat, RetreatStat::Tide) {
            let want = if drive.spooling() {
                GREEN
            } else if phase == Phase::Fleet {
                RED_TEXT
            } else {
                GOLD_TEXT
            };
            if colour.0 != want {
                colour.0 = want;
            }
        }
        if matches!(stat, RetreatStat::Fuel) {
            let want = if bank.fuel as u32 >= cost {
                GREEN
            } else {
                TEXT
            };
            if colour.0 != want {
                colour.0 = want;
            }
        }
    }
}
