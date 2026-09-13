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
    // The ORDERS panel: 344 wide, which is the mockup's `.cmd`, and a frame of
    // its own now. The cells used to sit straight on the field with nothing
    // behind them, so a row of buttons over a bright hull had no ground to
    // read against and the deck came out as three unrelated things and a spray
    // of controls. Its caption says what the orders will apply to.
    deck_panel(
        p,
        skin,
        Node {
            width: Val::Px(344.0),
            ..default()
        },
        ("ORDERS", DeckStat::Picked),
        |c| {
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
        },
    );
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
        // Six across the panel's own inner width: 344 less two 5 pixel
        // margins is 334, and five 3 pixel gaps leave 53 each. Worked out
        // against the panel rather than against the row, which is what the
        // first cut measured and why twelve commands wrapped to three rows
        // and the last one fell out of the bottom of the deck.
        width: Val::Px(53.0),
        // No HEIGHT: the wrap container stretches its rows, so twelve cells
        // make two rows of 32 and the sandbox's five make one row of 68. A
        // fixed height would have to be right for both and can only be right
        // for one.
        padding: UiRect::axes(Val::Px(2.0), Val::Px(2.0)),
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
        b.spawn(mark(glyphs, glyph, 17.0, ink));
        label(b, name, 8.0, ink);
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
        b.spawn(mark(glyphs, glyph, 17.0, ink));
        label(b, name, 8.0, ink);
    });
}

/// The UNIT panel: what is selected, what it is made of, and its plan.
///
/// ONE panel now. The stats and the schematic were two boxes side by side,
/// each with its own frame, which made four blocks across a row that is about
/// three things: what you are ordering, what it is, and what it carries. A
/// picture OF the selected ship belongs inside the panel about that ship.
///
/// It GROWS rather than taking a width, so the two fixed panels either side
/// keep their own and this one is whatever is between them. That is the same
/// rule the middle tab strip landed on: anything between two anchored things
/// is measured from both.
pub(crate) fn unit_panel(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    deck_panel(
        p,
        skin,
        Node {
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            ..default()
        },
        ("UNIT", DeckStat::Ship),
        |u| {
            u.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    column_gap: Val::Px(PAD),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                unit_stats(row, skin, glyphs);
                unit_plan(row, skin);
            });
        },
    );
}

/// The numbers half of the unit panel: what it is for, what it carries, and
/// how much reactor it has left.
fn unit_stats(row: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    let tok = skin.tok();
    row.spawn((
        Node {
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|l| {
        kv(l, skin, "Role", "", DeckStat::Role);
        l.spawn((
            Node {
                width: Val::Percent(100.0),
                column_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|st| {
            for (mk, stat, ink) in [
                (Glyph::Speed, DeckStat::Speed, tok.teal),
                (Glyph::Gun, DeckStat::Guns, tok.gold),
                (Glyph::Armour, DeckStat::Armour, tok.cyan),
            ] {
                st.spawn((
                    framed(
                        skin,
                        Frame::Well,
                        Node {
                            flex_grow: 1.0,
                            flex_basis: Val::Px(0.0),
                            min_width: Val::Px(0.0),
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(5.0),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                            ..default()
                        },
                    ),
                    Pickable::IGNORE,
                ))
                .with_children(|c| {
                    c.spawn(mark(glyphs, mk, 13.0, tok.faint));
                    readout(c, "0", 13.0, ink, stat);
                });
            }
        });
        bar(l, skin, tok.green, 5.0, Fill::Unit);
    });
}

/// The plan half: the selected ship's own schematic, in a well of its own.
fn unit_plan(row: &mut ChildSpawnerCommands, skin: &Skin) {
    row.spawn((
        framed(
            skin,
            Frame::Well,
            Node {
                width: Val::Px(166.0),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(2.0)),
                ..default()
            },
        ),
        Pickable::IGNORE,
    ))
    .with_children(|w| {
        w.spawn((
            fit_node(SCHEM, 150.0, 46.0),
            ImageNode::default(),
            Pickable::IGNORE,
            UnitShot,
        ));
    });
}
