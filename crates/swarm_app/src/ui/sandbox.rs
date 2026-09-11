//! The sandbox panel: the toggles with their state in the label, the
//! weapons, and the target's live numbers.

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

/// The label inside a toggle, which says on or off.
#[derive(Component)]
pub(crate) struct SandboxLabel(pub(crate) SandboxAction);

pub(crate) fn build_sandbox_panel(
    mut commands: Commands,
    scene: Res<SceneSpec>,
    fleet: Res<Fleet>,
) {
    let target = fleet.0[fleet.index_of(&scene.target_hull)].label.clone();
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            panel(Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(16.0),
                width: Val::Px(300.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            text(p, "SANDBOX", 12.0, MUTED);
            for (action, label) in [
                (SandboxAction::Freeze, "Freeze swarm  (Z)  off"),
                (SandboxAction::Slow, "Slow motion  (X)  off"),
                (SandboxAction::Invulnerable, "Invulnerable  (V)  off"),
                (SandboxAction::Blast, "Blast at cursor  (B)"),
                (SandboxAction::Reset, "New target  (N)"),
            ] {
                let b = button(p, label, TEXT, action);
                relabel(p, b, action);
            }
            p.spawn(Node {
                height: Val::Px(6.0),
                ..default()
            });
            text(p, "THE RANGE", 12.0, MUTED);
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|r| {
                for w in Weapon::ALL {
                    let b = button(
                        r,
                        &format!("{} {}", w.number(), w.label()),
                        TEXT,
                        SandboxAction::Arm(w),
                    );
                    relabel(r, b, SandboxAction::Arm(w));
                }
                let b = button(r, "0 none", TEXT, SandboxAction::Disarm);
                relabel(r, b, SandboxAction::Disarm);
            });
            p.spawn(Node {
                height: Val::Px(6.0),
                ..default()
            });
            for (stat, label) in [
                (Stat::Target, "target"),
                (Stat::Mass, "mass"),
                (Stat::Centre, "centre of mass"),
                (Stat::Spin, "spin"),
                (Stat::Cells, "cells lost"),
                (Stat::Shots, "shots landed"),
            ] {
                let first = if matches!(stat, Stat::Target) {
                    target.clone()
                } else {
                    String::new()
                };
                row(p, label, move |r| {
                    r.spawn((
                        Text::new(first),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(TEXT),
                        stat,
                        Pickable::IGNORE,
                    ));
                });
            }
        });
}

/// Tag a button's label so `sandbox_readouts` can rewrite it.
fn relabel(p: &mut ChildSpawnerCommands, b: Entity, action: SandboxAction) {
    p.commands().entity(b).insert(SandboxLabel(action));
}

/// The labels say what the toggles are at and which weapon is armed, and
/// the readouts say what the target weighs and how fast it turns.
pub(crate) fn sandbox_readouts(
    sb: Res<Sandbox>,
    scene: Res<SceneSpec>,
    fleet: Res<Fleet>,
    labelled: Query<(&SandboxLabel, &Children, &mut BorderColor)>,
    mut texts: Query<&mut Text, Without<Stat>>,
    mut stats: Query<(&Stat, &mut Text)>,
    dummies: Query<(&Hull, &Tumble), With<Dummy>>,
) {
    let mut labelled = labelled;
    for (l, children, mut border) in &mut labelled {
        let (want, lit) = match l.0 {
            SandboxAction::Freeze => (format!("Freeze swarm  (Z)  {}", on(sb.frozen)), sb.frozen),
            SandboxAction::Slow => (format!("Slow motion  (X)  {}", on(sb.slow)), sb.slow),
            SandboxAction::Invulnerable => (
                format!("Invulnerable  (V)  {}", on(sb.invulnerable)),
                sb.invulnerable,
            ),
            SandboxAction::Blast => ("Blast at cursor  (B)".into(), false),
            SandboxAction::Reset => ("New target  (N)".into(), false),
            SandboxAction::Arm(w) => (
                format!("{} {}", w.number(), w.label()),
                sb.weapon == Some(w),
            ),
            SandboxAction::Disarm => ("0 none".into(), sb.weapon.is_none()),
        };
        *border = BorderColor::all(if lit { GOLD_TEXT } else { LINE });
        for c in children.iter() {
            if let Ok(mut t) = texts.get_mut(c) {
                if t.0 != want {
                    t.0 = want.clone();
                }
            }
        }
    }
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

fn on(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}
