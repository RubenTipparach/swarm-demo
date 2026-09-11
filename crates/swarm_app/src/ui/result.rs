//! The verdict: how it ended, how long it took, what it cost, and the two
//! ways out.

use crate::*;

#[derive(Component)]
pub(crate) struct AgainButton;

#[derive(Component)]
pub(crate) struct ToMenuButton;

pub(crate) fn build_result(mut commands: Commands, outcome: Res<Outcome>) {
    let (word, why, colour) = if outcome.won {
        ("VICTORY", "every carrier is a wreck", GREEN)
    } else {
        ("DEFEAT", "the flagship's reactor is gone", RED_TEXT)
    };
    let secs = outcome.ticks / 60;
    let stat = |p: &mut ChildSpawnerCommands, k: &str, v: String| {
        row(p, k, move |r| {
            text(r, &v, 14.0, TEXT);
        });
    };
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
                text(p, why, 12.0, MUTED);
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
                    stat(
                        p,
                        "ships lost",
                        format!("{} of {}", outcome.ships_lost, outcome.ships),
                    );
                    stat(p, "cells lost", outcome.cells_lost.to_string());
                    stat(
                        p,
                        "motherships",
                        format!("{} of {}", outcome.hives_killed, outcome.hives),
                    );
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
                    button(p, "Again", GOLD_TEXT, AgainButton);
                    button(p, "Menu", TEXT, ToMenuButton);
                });
            });
        });
}

pub(crate) fn result_input(
    again: Query<&Interaction, (Changed<Interaction>, With<AgainButton>)>,
    menu: Query<&Interaction, (Changed<Interaction>, With<ToMenuButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if pressed(&again) || keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Playing);
    } else if pressed(&menu) || keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Menu);
    }
}
