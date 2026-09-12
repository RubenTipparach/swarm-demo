//! The verdict: how it ended, how long it took, what it cost, and the two
//! ways out.

use crate::*;

#[derive(Component)]
pub(crate) struct AgainButton;

#[derive(Component)]
pub(crate) struct ToMenuButton;

/// What the screen says happened, which is three different things now: a
/// skirmish is won by killing every carrier, a system of a retreat is left
/// by jumping out of it, and the run itself ends at the gate.
fn verdict_of(outcome: &Outcome) -> (&'static str, String, Color) {
    if outcome.escaped {
        return (
            "CLEAR",
            "the fleet is out, and the swarm has nothing to follow".into(),
            GREEN,
        );
    }
    if outcome.jumped {
        let why = match outcome.left {
            0 => "the whole fleet made the jump".into(),
            1 => "one ship was outside the field".to_string(),
            n => format!("{n} ships were outside the field"),
        };
        return ("JUMPED", why, GOLD_TEXT);
    }
    if outcome.won {
        ("VICTORY", "every carrier is a wreck".into(), GREEN)
    } else {
        ("DEFEAT", "the flagship's reactor is gone".into(), RED_TEXT)
    }
}

/// What the screen counts, which is not the same question in the two modes.
///
/// A retreat is judged on what it carried OUT, not on what it killed: the
/// carriers are a tide, killing them all is not the point, and a system left
/// with a full hold is a system played well.
fn result_rows(
    outcome: &Outcome,
    scene: &SceneSpec,
    bank: &Bank,
    run: &RunState,
) -> Vec<(&'static str, String)> {
    let lost = format!("{} of {}", outcome.ships_lost, outcome.ships);
    if scene.retreat {
        vec![
            ("systems left behind", run.systems.to_string()),
            ("materials", bank.materials.to_string()),
            ("jump fuel", format!("{:.0}", bank.fuel)),
            ("data", bank.data.to_string()),
            ("ships lost", lost),
        ]
    } else {
        vec![
            ("ships lost", lost),
            ("cells lost", outcome.cells_lost.to_string()),
            (
                "motherships",
                format!("{} of {}", outcome.hives_killed, outcome.hives),
            ),
        ]
    }
}

pub(crate) fn build_result(
    mut commands: Commands,
    outcome: Res<Outcome>,
    scene: Res<SceneSpec>,
    bank: Res<Bank>,
    run: Res<RunState>,
) {
    let (word, why, colour) = verdict_of(&outcome);
    let why = why.clone();
    let secs = outcome.ticks / 60;
    let stat = |p: &mut ChildSpawnerCommands, k: &str, v: String| {
        row(p, k, move |r| {
            text(r, &v, 14.0, TEXT);
        });
    };
    // What the button means depends on where the run stands: a skirmish is
    // replayed, a system is followed by the next one, and a finished run is
    // followed by a fresh one.
    let again = match (scene.retreat, outcome.escaped) {
        (true, false) => "Next system",
        (true, true) => "New run",
        _ => "Again",
    };
    let rows = result_rows(&outcome, &scene, &bank, &run);
    commands
        .spawn((
            DespawnOnExit(AppState::Result),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn(panel(Node {
                min_width: Val::Px(320.0),
                align_items: AlignItems::Center,
                ..default()
            }))
            .with_children(|p| {
                text(p, word, 34.0, colour);
                text(p, &why, 12.0, MUTED);
                p.spawn(Node {
                    height: Val::Px(8.0),
                    ..default()
                });
                p.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Px(280.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|p| {
                    stat(p, "time", format!("{}:{:02}", secs / 60, secs % 60));
                    for (k, v) in rows {
                        stat(p, k, v);
                    }
                });
                p.spawn(Node {
                    height: Val::Px(8.0),
                    ..default()
                });
                p.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(10.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|p| {
                    button(p, again, GOLD_TEXT, AgainButton);
                    button(p, "Menu", TEXT, ToMenuButton);
                });
            });
        });
}

pub(crate) fn result_input(
    again: Query<&Interaction, (Changed<Interaction>, With<AgainButton>)>,
    menu: Query<&Interaction, (Changed<Interaction>, With<ToMenuButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    outcome: Res<Outcome>,
    mut run: ResMut<RunState>,
    mut next: ResMut<NextState<AppState>>,
) {
    if pressed(&again) || keys.just_pressed(KeyCode::Enter) {
        // A finished run is not replayed, it is started again: a new seed,
        // an empty bank and the two ships a run opens with. `run.advance`
        // has already moved a run that is only between systems.
        if outcome.escaped {
            *run = RunState::new(run.seed.wrapping_add(1), &run.flagship.clone());
        }
        next.set(AppState::Playing);
    } else if pressed(&menu) || keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Menu);
    }
}
