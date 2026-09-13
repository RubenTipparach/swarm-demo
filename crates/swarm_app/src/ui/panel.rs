//! The side panel: what a MODE puts on screen over and above the fight.
//!
//! It is the DEBUG panel and is not part of the game's own design, which is
//! why a campaign does not get one: a run is played off the strip, the rail
//! and the deck. It carries the range's numbers in the sandbox and the wave
//! in a skirmish, and the run's own two controls live down here with it
//! because they are the same kind of thing.

use crate::*;

/// The right panel: the BUILD MENU, as the mockup draws it, or the range in a
/// sandbox.
///
/// It was a REINFORCE list, which is one button per class calling a free wave,
/// wearing the place the mockup puts the yard. The wave is a yard ORDER now
/// and R is its shortcut, so the list here is what a yard can make and the two
/// are one mechanic rather than a factory drawn over a mechanic that is not
/// one.
///
/// ONE panel with two contents chosen by the mode, which is the rule this
/// panel has always kept: a sandbox is a range rather than a fleet you build,
/// so it gets what it is shooting at and a skirmish and a run get the yard.
///
/// Its geometry is the mockup's: 428 wide, 698 tall, 6 in from the right and
/// 50 down, with the three tabs on the line below it.
pub(crate) fn side_panel(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    glyphs: &Glyphs,
    scene: &SceneSpec,
) {
    p.spawn((
        framed(
            skin,
            Frame::Panel,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(6.0),
                top: Val::Px(50.0),
                width: Val::Px(PANEL_W),
                height: Val::Px(698.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
        ),
        SidePanel,
    ))
    .with_children(|s| {
        if scene.sandbox {
            head(s, skin, "The Range", Some(PanelToggle));
            range_rows(s, skin);
        } else {
            head(s, skin, "Build Menu", Some(PanelToggle));
            build_body(s, skin, glyphs);
        }
    });
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
