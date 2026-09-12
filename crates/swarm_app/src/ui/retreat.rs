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
    /// Cubes riding on the support ships, which is what is not in the bank
    /// YET: a full miner a long way from home is a hold a player should be
    /// able to see before the swarm gets to it.
    Cargo,
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
                (RetreatStat::Ore, "ore in the field"),
                (RetreatStat::Crystal, "crystal in the field"),
                (RetreatStat::Materials, "materials"),
                (RetreatStat::Volatiles, "volatiles"),
                (RetreatStat::Fuel, "jump fuel"),
                (RetreatStat::Data, "data"),
                (RetreatStat::Cargo, "cubes aboard"),
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
            RetreatStat::Cargo => {
                let (aboard, room): (u32, u32) = crews
                    .iter()
                    .fold((0, 0), |(a, r), (_, _, h, _)| (a + h.carrying, r + h.cap));
                format!("{aboard} of {room}")
            }
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
