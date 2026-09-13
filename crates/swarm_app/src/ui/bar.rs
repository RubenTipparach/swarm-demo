//! The bottom deck: the command bar on the left and the selection beside it.
//!
//! A mark rather than a word, six across, which is the mockup's own grid: a
//! command bar reads as a bar of commands at a glance and a column of labels
//! does not.

use crate::*;

/// The command bar: three tabs and the page under whichever is pressed.
pub(crate) fn command_bar(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    glyphs: &Glyphs,
    scene: &SceneSpec,
    open: Page,
) {
    let tok = skin.tok();
    // 344 wide, which is the mockup's `.cmd`, and no tabs inside it: the page
    // tabs are a strip of their own on the line above, beside the view tabs
    // and the panel's own. Three strips on one line is the mockup's layout and
    // it is also one height rather than three that can drift.
    p.spawn((
        Node {
            width: Val::Px(344.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        for page in Page::ALL {
            // The range and the sandbox act only in the sandbox, so their
            // pages are DIM there rather than absent: a control a mode does
            // not have still tells you the mode does not have it.
            let live = page == Page::Fleet || scene.sandbox;
            let ink = if live { tok.ink } else { tok.faint };
            c.spawn((
                Node {
                    display: if page == open {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(3.0),
                    row_gap: Val::Px(3.0),
                    align_content: AlignContent::Stretch,
                    ..default()
                },
                Pickable::IGNORE,
                PageBody(page),
            ))
            .with_children(|g| page_keys(g, skin, glyphs, page, ink));
        }
    });
}

/// One page's buttons.
///
/// A mark rather than a word, six across, which is the mockup's own grid: a
/// command bar reads as a bar of commands at a glance and a column of labels
/// does not. The name stays under the mark, small, because an icon with
/// nothing beside it is the feature nobody can find that this project already
/// wrote down once.
fn page_keys(g: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs, page: Page, ink: Ink) {
    match page {
        Page::Fleet => {
            // Twelve, which is the mockup's own grid, in its own order: the
            // three that aim, the one that stops, the four that say HOW to
            // fight, and the four that end something.
            //
            // Bars and Frames are not here and never were in the mockup: they
            // turn a READOUT on and off rather than order a ship, so they live
            // on the Menu view with Resume. Focus is gone too, because space
            // already snaps the camera to the flagship and a cell that is a
            // second way to press one key is a cell that can disagree with it.
            for (cmd, mk) in [
                (DeckCmd::Move, Glyph::Move),
                (DeckCmd::Attack, Glyph::Attack),
                (DeckCmd::Guard, Glyph::Guard),
                (DeckCmd::Stop, Glyph::Stop),
                (DeckCmd::Form, Glyph::Form),
                (DeckCmd::Stance, Glyph::Stance),
                (DeckCmd::Dock, Glyph::Dock),
                (DeckCmd::Rally, Glyph::Rally),
                (DeckCmd::Salvage, Glyph::Salv),
                (DeckCmd::Hyper, Glyph::Hyper),
                (DeckCmd::Scuttle, Glyph::Scut),
            ] {
                cell(g, skin, glyphs, cmd.label(), mk, ink, cmd);
            }
            // Launch carries `CallButton` as well, because
            // `call_reinforcements` already reads that marker for R's own
            // button and a second way to call a wave would be a second wave
            // rule. It is the mockup's `c-launch` and the game's Call.
            cell(
                g,
                skin,
                glyphs,
                "Launch",
                Glyph::Launch,
                ink,
                (DeckCmd::Call, CallButton),
            );
        }
        Page::Strike => {
            for (w, mk) in [
                (Weapon::Beam, Glyph::Beam),
                (Weapon::Flak, Glyph::Flak),
                (Weapon::Slug, Glyph::Slug),
                (Weapon::Torpedo, Glyph::Torpedo),
                (Weapon::Bite, Glyph::Bite),
            ] {
                arm(g, skin, glyphs, w.label(), mk, ink, SandboxAction::Arm(w));
            }
            arm(
                g,
                skin,
                glyphs,
                "none",
                Glyph::Stop,
                ink,
                SandboxAction::Disarm,
            );
        }
        Page::Tactics => {
            cell(g, skin, glyphs, "Pause", Glyph::Stop, ink, DeckCmd::Pause);
            for (name, mk, a) in [
                ("Freeze", Glyph::Scut, SandboxAction::Freeze),
                ("Slow", Glyph::Hyper, SandboxAction::Slow),
                ("Shield", Glyph::Guard, SandboxAction::Invulnerable),
                ("Blast", Glyph::Flak, SandboxAction::Blast),
                ("Dummy", Glyph::Launch, SandboxAction::Reset),
            ] {
                arm(g, skin, glyphs, name, mk, ink, a);
            }
        }
    }
}

/// The box every command cell is laid out in.
fn cell_node() -> Node {
    Node {
        // Six across 344 with three between, and two rows of 92 less the gap:
        // the mockup's `.cmd` grid worked out rather than guessed at.
        width: Val::Px(54.0),
        height: Val::Px(44.0),
        padding: UiRect::axes(Val::Px(2.0), Val::Px(3.0)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        row_gap: Val::Px(1.0),
        ..default()
    }
}

/// A sandbox cell, whose caption is its key AND what it is at.
///
/// A toggle whose label never changes is a control nobody can read the state
/// of, which is the rail rule redux-tribes keeps. The key and the state both
/// come off `SandboxAction`, so the table that says which key arms a weapon is
/// the same one the handler reads.
fn arm(
    g: &mut ChildSpawnerCommands,
    skin: &Skin,
    glyphs: &Glyphs,
    name: &str,
    glyph: Glyph,
    ink: Ink,
    action: SandboxAction,
) {
    g.spawn((
        Button,
        cell_node(),
        frame(skin, Frame::Btn),
        Chromed,
        action,
    ))
    .with_children(|b| {
        b.spawn(mark(glyphs, glyph, 20.0, ink));
        label(b, name, 9.0, ink);
        readout(b, &action.hint(), 8.0, skin.tok().faint, CmdState(action));
    });
}

/// The second line of a sandbox cell, rewritten as the state changes.
#[derive(Component, Clone, Copy)]
pub(crate) struct CmdState(pub(crate) SandboxAction);

/// One command cell: its mark over its name.
pub(crate) fn cell(
    g: &mut ChildSpawnerCommands,
    skin: &Skin,
    glyphs: &Glyphs,
    name: &str,
    glyph: Glyph,
    ink: Ink,
    marker: impl Bundle,
) {
    g.spawn((
        Button,
        cell_node(),
        frame(skin, Frame::Btn),
        Chromed,
        marker,
    ))
    .with_children(|b| {
        b.spawn(mark(glyphs, glyph, 20.0, ink));
        label(b, name, 9.0, ink);
    });
}

/// The selection panel in the middle of the deck: what is picked, what it is
/// for, and what it is still made of.
///
/// The bar reads the REACTOR, which is the only thing that kills a ship, so it
/// is the only honest thing to put on a bar. Plating comes off and the ship
/// keeps flying.
pub(crate) fn unit_panel(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    let tok = skin.tok();
    p.spawn((
        framed(
            skin,
            Frame::Panel,
            Node {
                // The mockup's `.unit`: 520 across the bottom row, and the
                // row is 92 tall, so this OVERFLOWS unless it is told the
                // height it has. Without it the stat wells were drawn under
                // the bottom of the screen.
                width: Val::Px(520.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                overflow: Overflow::clip(),
                ..default()
            },
        ),
        Pickable::IGNORE,
    ))
    .with_children(|u| {
        kv(u, skin, "Unit", "nothing selected", DeckStat::Ship);
        kv(u, skin, "Role", "", DeckStat::Role);
        u.spawn((
            Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|s| {
            for (mk, stat, ink) in [
                (Glyph::Speed, DeckStat::Speed, tok.teal),
                (Glyph::Gun, DeckStat::Guns, tok.gold),
                (Glyph::Armour, DeckStat::Armour, tok.cyan),
            ] {
                s.spawn((
                    framed(
                        skin,
                        Frame::Well,
                        Node {
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(5.0),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                            ..default()
                        },
                    ),
                    Pickable::IGNORE,
                ))
                .with_children(|c| {
                    c.spawn(mark(glyphs, mk, 14.0, tok.faint));
                    readout(c, "0", 13.0, ink, stat);
                });
            }
        });
        bar(u, skin, tok.green, 6.0, Fill::Unit);
    });
}

/// The selection's picture and its hull bar, which the mockup keeps in a block
/// of its OWN between the unit panel and the modules row.
///
/// It was inside the unit panel and that is why the stat wells ran off the
/// bottom of the screen: a 92 tall row cannot hold a schematic, two rows of
/// text and three wells. Out here it also does the job the mockup gives it,
/// which is taking up whatever width is left so the modules row lands in the
/// corner rather than in the middle.
pub(crate) fn unit_thumb(p: &mut ChildSpawnerCommands, skin: &Skin) {
    p.spawn((
        Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|t| {
        t.spawn((
            framed(
                skin,
                Frame::Well,
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ),
            Pickable::IGNORE,
        ))
        .with_children(|w| {
            w.spawn((
                Node {
                    width: Val::Px(140.0),
                    height: Val::Px(46.0),
                    ..default()
                },
                ImageNode::default(),
                Pickable::IGNORE,
                UnitShot,
            ));
        });
        bar(t, skin, skin.tok().green, 6.0, Fill::Unit);
    });
}
