//! What the sandbox says about the target it is shooting at.
//!
//! The toggles and the weapons are on the command deck now, on its Strike and
//! Tactics pages, because they were the same five buttons twice: one set here
//! and one there is two writers for one state, and the panel's own copy also
//! carried a second table of which key arms what. What is left is the part the
//! deck has no other home for, which is what the dummy weighs and how fast the
//! last shot set it turning.

use crate::*;

/// Which live number a readout shows.
#[derive(Component, Clone, Copy)]
pub(crate) enum Stat {
    Target,
    Mass,
    Centre,
    Spin,
    Cells,
    Shots,
}

/// The labels say what the toggles are at and which weapon is armed, and
/// the readouts say what the target weighs and how fast it turns.
pub(crate) fn sandbox_readouts(
    sb: Res<Sandbox>,
    scene: Res<SceneSpec>,
    fleet: Res<Fleet>,
    mut stats: Query<(&Stat, &mut Text)>,
    dummies: Query<(&Hull, &Tumble), With<Dummy>>,
) {
    let dummy = dummies.iter().next();
    for (stat, mut t) in &mut stats {
        let want = match (stat, dummy) {
            (Stat::Target, _) => fleet.0[fleet.index_of(&scene.target_hull)].label.clone(),
            (Stat::Shots, _) => sb.shots.to_string(),
            (_, None) => "none".into(),
            (Stat::Mass, Some((_, tb))) => format!("{:.0} cells", tb.body.mass),
            (Stat::Centre, Some((_, tb))) => format!(
                "{:+.2} {:+.2} {:+.2}",
                tb.body.centre[0], tb.body.centre[1], tb.body.centre[2]
            ),
            (Stat::Spin, Some((_, tb))) => format!("{:.0} deg/s", tb.spin.length().to_degrees()),
            (Stat::Cells, Some((h, _))) => h.damage.dead_count().to_string(),
        };
        if t.0 != want {
            t.0 = want;
        }
    }
}
