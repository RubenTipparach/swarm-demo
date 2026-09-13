//! The side panel: what a MODE puts on screen over and above the fight.
//!
//! It is the DEBUG panel and is not part of the game's own design, which is
//! why a campaign does not get one: a run is played off the strip, the rail
//! and the deck. It carries the range's numbers in the sandbox and the wave
//! in a skirmish, and the run's own two controls live down here with it
//! because they are the same kind of thing.

use crate::*;

/// The side panel: the flagship, what it is made of, and the wing.
///
/// This is the mockup's build menu with the mechanic swarm-demo actually has
/// under it. There is no ship production here, so the list is not a factory:
/// it is the reinforcement call (`R`) asked for once per class, which is the
/// same `call_one` the key already runs, parameterised by the class instead
/// of taking the flagship's. A queue with nothing to queue would have been a
/// picture of a mechanic.
pub(crate) fn side_panel(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    fleet: &Schematics,
    scene: &SceneSpec,
) {
    let tok = skin.tok();
    p.spawn((
        framed(
            skin,
            Frame::Panel,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(72.0),
                width: Val::Px(PANEL_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ),
        SidePanel,
    ))
    .with_children(|s| {
        head(s, skin, "Fleet", Some(PanelToggle));
        // The class picker, which swaps the flagship outright. It lives here
        // rather than in its old corner because a panel about the ship is
        // where you look for the ship, and the corner it was in is the clock's.
        s.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(9.0), Val::Px(4.0)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            frame(skin, Frame::Btn),
            Chromed,
            HullButton,
        ))
        .with_children(|b| {
            readout(b, "", 15.0, tok.cyan, DeckStat::Flag);
            // ASCII: the default font has no caret glyph and a missing one
            // draws as a hollow box, which reads as a bug in the button.
            label(b, "v", 12.0, tok.dim);
        });
        s.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            HullMenu,
        ))
        .with_children(|list| {
            for (n, name) in PICKABLE.iter().enumerate() {
                list.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(tok.row.col()),
                    Chromed,
                    HullPick(n),
                ))
                .with_children(|t| {
                    label(t, &hull_label(name), 12.0, tok.ink);
                });
            }
        });
        bar(s, skin, tok.green, 7.0, Fill::Flag);
        s.spawn((
            framed(
                skin,
                Frame::Well,
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
            ),
            Pickable::IGNORE,
        ))
        .with_children(|w| {
            kv(w, skin, "Cells", "", DeckStat::Cells);
            kv(w, skin, "Guns", "", DeckStat::FlagGuns);
            kv(w, skin, "Drives", "", DeckStat::Drives);
            kv(w, skin, "Reactor", "", DeckStat::Reactor);
        });
        if scene.sandbox {
            range_rows(s, skin);
        } else {
            reinforce_rows(s, skin, fleet);
        }
    });
}

/// The wave, one row per class.
///
/// This is the skirmish's own mechanic and not a factory: the row runs the
/// same `call_one` R runs, with the class as its argument instead of the
/// flagship's. A queue with nothing to queue would have been a picture of a
/// mechanic rather than one.
fn reinforce_rows(s: &mut ChildSpawnerCommands, skin: &Skin, fleet: &Schematics) {
    let tok = skin.tok();
    section(s, skin, "Reinforce", DeckStat::Wing);
    for (n, class) in PICKABLE.iter().enumerate() {
        s.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(tok.row.col()),
            Chromed,
            CallClass(n),
        ))
        .with_children(|row| {
            if let Some((_, small)) = fleet.of(class) {
                row.spawn(schematic(small, 72.0, 24.0));
            }
            label(row, &hull_label(class), 11.0, tok.ink);
            label(row, "+", 13.0, tok.gold);
        });
    }
}

/// What the range is shooting at, and what the last shot did to it.
///
/// The toggles and the weapons are NOT here: they are the deck's own Strike
/// and Tactics pages, and this is only the part those pages have no room for.
fn range_rows(s: &mut ChildSpawnerCommands, skin: &Skin) {
    section(s, skin, "The range", DeckStat::None);
    for (stat, name) in [
        (Stat::Target, "target"),
        (Stat::Mass, "mass"),
        (Stat::Centre, "centre of mass"),
        (Stat::Spin, "spin"),
        (Stat::Cells, "cells lost"),
        (Stat::Shots, "shots landed"),
    ] {
        kv(s, skin, name, "", stat);
    }
}

/// A section heading inside a panel: a gold word on the left and one live
/// number on the right.
fn section(s: &mut ChildSpawnerCommands, skin: &Skin, name: &str, stat: DeckStat) {
    let tok = skin.tok();
    s.spawn((
        Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|h| {
        label(h, &name.to_uppercase(), 12.0, tok.gold);
        readout(h, "", 12.0, tok.dim, stat);
    });
}

/// What a RUN is played on, which is two buttons and a clock.
///
/// The debug panel used to carry them in a list of eighteen numbers, and the
/// owner is right that a campaign is not played off a debug panel: what a
/// player needs in a system is how long they have, which is on the strip, and
/// the one button that leaves. It is bottom right, where the mockup puts its
/// green control, and it is green for the same reason.
pub(crate) fn run_controls(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    let tok = skin.tok();
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(14.0),
            bottom: Val::Px(14.0),
            column_gap: Val::Px(6.0),
            align_items: AlignItems::FlexEnd,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|r| {
        cell(
            r,
            skin,
            glyphs,
            "Rebuild",
            Glyph::Launch,
            tok.ink,
            RebuildButton,
        );
        r.spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            },
            frame(skin, Frame::Hot),
            Chromed,
            JumpButton,
        ))
        .with_children(|b| {
            b.spawn(mark(glyphs, Glyph::Hyper, 22.0, tok.green));
            label(b, "JUMP OUT", 15.0, tok.green);
        });
    });
}
