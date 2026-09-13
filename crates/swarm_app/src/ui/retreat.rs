//! The retreat's panel: the clock you are playing against, what the fleet
//! has gathered, and the button that leaves.

use crate::*;

/// The one number a run is played against: how long this system has.
///
/// It used to be eighteen of these on a panel of its own, and the owner is
/// right that a campaign is not played off a debug readout: what a player
/// needs in a system is the clock and the button that leaves. The rest of the
/// run's numbers are in the headless report, where they were always the thing
/// actually reading them.
#[derive(Component, Clone, Copy)]
pub(crate) enum RetreatStat {
    Tide,
}

/// The button that says the same thing the J key does.
#[derive(Component)]
pub(crate) struct JumpButton;

/// Everything the clock reads that is not a query, as ONE parameter.
#[derive(SystemParam)]
pub(crate) struct RunView<'w> {
    tick: Res<'w, Tick>,
    scene: Res<'w, SceneSpec>,
    drive: Res<'w, JumpDrive>,
}

/// The clock a system is played against, on the resource strip.
pub(crate) fn retreat_readouts(
    view: RunView,
    mut stats: Query<(&RetreatStat, &mut Text, &mut TextColor)>,
) {
    let RunView { tick, scene, drive } = view;
    let left = scene.tide.until_fleet(tick.tick);
    let phase = scene.tide.phase_at(tick.tick);
    for (_, mut t, mut colour) in &mut stats {
        let want = match drive.ready_at {
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
        };
        if t.0 != want {
            t.0 = want;
        }
        // Red when the fleet is in, because that is the one thing on the strip
        // a player has to see without reading it.
        let ink = if drive.spooling() {
            GREEN
        } else if phase == Phase::Fleet {
            RED_TEXT
        } else {
            GOLD_TEXT
        };
        if colour.0 != ink {
            colour.0 = ink;
        }
    }
}
