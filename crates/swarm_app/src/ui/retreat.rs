//! The retreat's panel: the clock you are playing against, what the fleet
//! has gathered, and the button that leaves.

use crate::*;

/// One live number on the panel.
#[derive(Component, Clone, Copy)]
pub(crate) enum RetreatStat {
    Tide,
    /// Seam cells still in the field's rocks, which is the one number that
    /// answers "is there anything left here worth staying for".
    Seam,
    Materials,
    Volatiles,
    Fuel,
    Data,
    Crews,
}

/// The button that says the same thing the J key does.
#[derive(Component)]
pub(crate) struct JumpButton;

pub(crate) fn build_retreat_panel(mut commands: Commands, run: Res<RunState>) {
    let node = run.node();
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            panel(Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(16.0),
                width: Val::Px(320.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            text(
                p,
                &format!(
                    "SYSTEM {}  act {}  {}",
                    run.systems + 1,
                    node.act + 1,
                    node.tag.label()
                ),
                12.0,
                MUTED,
            );
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(GOLD_TEXT),
                RetreatStat::Tide,
                Pickable::IGNORE,
            ));
            p.spawn(Node {
                height: Val::Px(6.0),
                ..default()
            });
            for (stat, label) in [
                (RetreatStat::Seam, "seam in the field"),
                (RetreatStat::Materials, "materials"),
                (RetreatStat::Volatiles, "volatiles"),
                (RetreatStat::Fuel, "jump fuel"),
                (RetreatStat::Data, "data"),
            ] {
                row(p, label, move |r| {
                    r.spawn((
                        Text::new("0"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(TEXT),
                        stat,
                        Pickable::IGNORE,
                    ));
                });
            }
            p.spawn(Node {
                height: Val::Px(6.0),
                ..default()
            });
            button(p, "Jump out  (J)", GOLD_TEXT, JumpButton);
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(MUTED),
                RetreatStat::Crews,
                Pickable::IGNORE,
            ));
        });
}

/// The panel's numbers, and the one line that says what the fleet is doing.
pub(crate) fn retreat_readouts(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    bank: Res<Bank>,
    drive: Res<JumpDrive>,
    crews: Query<(&Support, &Job, &Hold, &Hull)>,
    rocks: Query<&Rock>,
    mut stats: Query<(&RetreatStat, &mut Text, &mut TextColor)>,
) {
    let seam: u32 = rocks.iter().map(|r| r.seam).sum();
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
            RetreatStat::Seam => seam.to_string(),
            RetreatStat::Materials => bank.materials.to_string(),
            RetreatStat::Volatiles => bank.volatiles.to_string(),
            RetreatStat::Fuel => format!("{:.0} of {JUMP_FUEL:.0}", bank.fuel),
            RetreatStat::Data => bank.data.to_string(),
            RetreatStat::Crews => crews
                .iter()
                .map(|(s, j, h, hull)| {
                    let doing = if hull.dead_hull {
                        "lost".to_string()
                    } else {
                        match j {
                            Job::Idle => "idle".into(),
                            Job::Work(_) => format!("cutting {:.0}%", h.share() * 100.0),
                            Job::Unload(_) => "unloading".into(),
                        }
                    };
                    format!("{}: {doing}", s.role.label())
                })
                .collect::<Vec<_>>()
                .join("\n"),
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
            let want = if bank.fuel >= JUMP_FUEL { GREEN } else { TEXT };
            if colour.0 != want {
                colour.0 = want;
            }
        }
    }
}
