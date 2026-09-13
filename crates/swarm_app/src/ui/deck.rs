//! The command deck: what is on screen during a fight.
//!
//! This is the RTS mockup built for real (`docs/ui/rts-mockup.html`), and the
//! division it draws is the one it is built on: the CHROME is the skin's, the
//! LAYOUT is here, and every number on it is read off the game rather than
//! authored beside it. Nothing on this deck is a control for a mechanic that
//! does not exist, which is why there is no research row and no build queue:
//! a button that does nothing is worse than a missing one, because a player
//! spends a fight wondering what it was for.

use crate::*;

/// Which page of the command bar is up.
///
/// Fleet is what a ship does, Strike is what the range fires and Tactics is
/// what the sandbox holds still. The last two act only in the sandbox, so
/// they are DIM elsewhere rather than absent: what you could have had is the
/// information a greyed control carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Fleet,
    Strike,
    Tactics,
}

impl Page {
    pub(crate) const ALL: [Page; 3] = [Page::Fleet, Page::Strike, Page::Tactics];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Page::Fleet => "Fleet",
            Page::Strike => "Strike",
            Page::Tactics => "Tactics",
        }
    }
}

/// What the deck is showing, and where its two sliding halves have got to.
///
/// One resource rather than a flag per widget, because "is the side panel
/// open" is a fact about the SESSION and three things read it: the toggle,
/// the slide and the tab that reopens it.
#[derive(Resource)]
pub(crate) struct Deck {
    pub(crate) page: Page,
    pub(crate) panel: bool,
    pub(crate) bottom: bool,
    /// How far each half has actually travelled, nought shut to one open.
    /// Eased rather than snapped, because a panel that appears is a panel
    /// that startles and one that slides is a panel you watched arrive.
    pub(crate) panel_at: f32,
    pub(crate) bottom_at: f32,
}

impl Default for Deck {
    fn default() -> Self {
        Deck {
            page: Page::Fleet,
            panel: true,
            bottom: true,
            panel_at: 1.0,
            bottom_at: 1.0,
        }
    }
}

/// How long a half takes to slide, as a time constant in seconds.
pub(crate) const SLIDE: f32 = 0.11;

/// How wide the side panel is, and how tall the bottom deck stands.
pub(crate) const PANEL_W: f32 = 322.0;
pub(crate) const DECK_H: f32 = 150.0;

/// Everything on the deck that carries a number, named by what it says.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeckStat {
    /// A heading with no number beside it. Named rather than left off, so a
    /// section is built by one function whether or not it carries one.
    None,
    Clock,
    Strip,
    StripSub,
    Wing,
    Ship,
    Role,
    Speed,
    Guns,
    Armour,
    Flag,
    Cells,
    FlagGuns,
    Drives,
    Reactor,
}

/// A bar's fill, which is a width rather than a string.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    Fuel,
    Unit,
    Flag,
}

/// One row of the fleet rail, and the class it stands for.
#[derive(Component)]
pub(crate) struct RosterRow(pub(crate) usize);

/// The count on that row.
#[derive(Component)]
pub(crate) struct RosterCount(pub(crate) usize);

/// A row of the reinforcement list: press it and one of that class flies in.
#[derive(Component)]
pub(crate) struct CallClass(pub(crate) usize);

/// The buttons on the Fleet page, which are keys the game already has asked
/// for a second time.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeckCmd {
    Move,
    Stop,
    Focus,
    Call,
    Bars,
    Fps,
    Pause,
}

impl DeckCmd {
    fn label(self) -> (&'static str, &'static str) {
        match self {
            DeckCmd::Move => ("Move", "RMB"),
            DeckCmd::Stop => ("Stop", "S"),
            DeckCmd::Focus => ("Focus", "F"),
            DeckCmd::Call => ("Call", "R"),
            DeckCmd::Bars => ("Bars", ""),
            DeckCmd::Fps => ("Frames", ""),
            DeckCmd::Pause => ("Pause", "Spc"),
        }
    }
}

/// A tab of the command bar, and the page it shows.
#[derive(Component)]
pub(crate) struct PageTab(pub(crate) Page);

/// The body of one page, shown when its tab is pressed.
#[derive(Component)]
pub(crate) struct PageBody(pub(crate) Page);

/// The side panel, which slides off the right edge.
#[derive(Component)]
pub(crate) struct SidePanel;

/// The bottom deck, which slides off the bottom.
#[derive(Component)]
pub(crate) struct BottomDeck;

/// The two toggles, and the caret on the second one.
#[derive(Component)]
pub(crate) struct PanelToggle;

#[derive(Component)]
pub(crate) struct BottomToggle;

/// Build the whole deck.
///
/// Each region is its own function for the reason this project keeps
/// everywhere: a function that needs a section comment inside it is two
/// functions, and `build_hud` was five hundred and eighty three lines once.
pub(crate) fn build_deck(
    mut commands: Commands,
    skin: Res<Skin>,
    scene: Res<SceneSpec>,
    mut deck: ResMut<Deck>,
) {
    // Open on the page the MODE is about. The sandbox is a range, so a player
    // who asked for one should not have to find the weapons first.
    let page = if scene.sandbox {
        Page::Strike
    } else {
        Page::Fleet
    };
    deck.page = page;
    let root = commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Pickable::IGNORE,
            DeckRoot,
        ))
        .id();
    commands.entity(root).with_children(|p| {
        clock(p, &skin);
        strip(p, &skin, &scene);
        rail(p, &skin);
        side_panel(p, &skin, &scene);
        bottom(p, &skin, &scene, page);
    });
}

/// The deck's own root, so a system can find everything under it.
#[derive(Component)]
pub(crate) struct DeckRoot;

/// The clock, top left, where it sits over nothing a player has to press.
fn clock(p: &mut ChildSpawnerCommands, skin: &Skin) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(9.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        readout(
            c,
            "Game Time  0:00",
            14.0,
            skin.tok().sky_ink,
            DeckStat::Clock,
        );
        // The frame counter beside it rather than in the far corner, which is
        // the resource strip's now. They are the same kind of readout: a fact
        // about the session rather than about the fleet.
        readout(c, "", 12.0, skin.tok().faint, FpsText);
    });
}

/// The resource strip, top right.
///
/// What it carries is the MODE's: a run is materials and the fuel it needs to
/// jump, and a skirmish is the carriers left and the wing that is flying. Two
/// contents, one strip, because "what is this fight about" belongs in the same
/// corner whatever the fight is.
fn strip(p: &mut ChildSpawnerCommands, skin: &Skin, scene: &SceneSpec) {
    let tok = skin.tok();
    let (a, b) = if scene.retreat {
        (tok.ore, tok.cry)
    } else {
        (tok.teal, tok.cyan)
    };
    p.spawn(framed(
        skin,
        Frame::Panel,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(14.0),
            top: Val::Px(8.0),
            width: Val::Px(PANEL_W),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(14.0),
            ..default()
        },
    ))
    .insert(Pickable::IGNORE)
    .with_children(|s| {
        readout(s, "0", 17.0, a, DeckStat::Strip);
        s.spawn((
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|f| {
            readout(f, "", 12.0, b, DeckStat::StripSub);
            bar(f, skin, b, 5.0, Fill::Fuel);
        });
    });
}

/// The fleet rail down the left: one row per class the fleet actually has.
///
/// A row for a class with none of it is a row that says nothing, so the rows
/// are spawned once and HIDDEN rather than respawned: a rail rebuilt every
/// frame is nine allocations a frame for a picture that changes twice a match.
fn rail(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            top: Val::Px(56.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|r| {
        for (n, class) in PICKABLE.iter().enumerate() {
            r.spawn((
                Node {
                    display: Display::None,
                    width: Val::Px(148.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(tok.row.col()),
                Pickable::IGNORE,
                RosterRow(n),
            ))
            .with_children(|row| {
                label(row, &hull_label(class), 11.0, tok.dim);
                readout(row, "", 12.0, tok.cyan, RosterCount(n));
            });
        }
    });
}

/// The side panel: the flagship, what it is made of, and the wing.
///
/// This is the mockup's build menu with the mechanic swarm-demo actually has
/// under it. There is no ship production here, so the list is not a factory:
/// it is the reinforcement call (`R`) asked for once per class, which is the
/// same `call_one` the key already runs, parameterised by the class instead
/// of taking the flagship's. A queue with nothing to queue would have been a
/// picture of a mechanic.
fn side_panel(p: &mut ChildSpawnerCommands, skin: &Skin, scene: &SceneSpec) {
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
        if scene.retreat {
            run_rows(s, skin);
        } else if scene.sandbox {
            range_rows(s, skin);
        } else {
            reinforce_rows(s, skin);
        }
    });
}

/// The wave, one row per class.
///
/// This is the skirmish's own mechanic and not a factory: the row runs the
/// same `call_one` R runs, with the class as its argument instead of the
/// flagship's. A queue with nothing to queue would have been a picture of a
/// mechanic rather than one.
fn reinforce_rows(s: &mut ChildSpawnerCommands, skin: &Skin) {
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
            label(row, &hull_label(class), 12.0, tok.ink);
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

/// The run's own numbers, and the two buttons that end a system.
///
/// The same panel as a skirmish's, with what a RUN is about in it: there is no
/// wave to call in a retreat, because reinforcements are bought at the yard
/// between systems. One panel, two contents, chosen by the mode, which is the
/// resource strip's own rule a second time.
fn run_rows(s: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    readout(s, "", 17.0, tok.gold, RetreatStat::Tide);
    section(s, skin, "The field", DeckStat::None);
    for (stat, name) in [
        (RetreatStat::Ore, "ore"),
        (RetreatStat::Crystal, "crystal"),
        (RetreatStat::Materials, "materials"),
        (RetreatStat::Volatiles, "volatiles"),
        (RetreatStat::Fuel, "jump fuel"),
        (RetreatStat::Data, "data"),
        (RetreatStat::Cargo, "cubes aboard"),
        (RetreatStat::Salvage, "best wreck"),
    ] {
        kv(s, skin, name, "0", stat);
    }
    btn(
        s,
        skin,
        "Rebuild a wreck  (B)",
        13.0,
        Node {
            width: Val::Percent(100.0),
            ..default()
        },
        RebuildButton,
    );
    btn(
        s,
        skin,
        "Jump out  (J)",
        13.0,
        Node {
            width: Val::Percent(100.0),
            ..default()
        },
        JumpButton,
    );
    readout(s, "", 11.0, tok.dim, RetreatStat::Crews);
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

/// The bottom deck: the command bar on the left and the selection in the
/// middle, with the groups and the view tabs over them.
fn bottom(p: &mut ChildSpawnerCommands, skin: &Skin, scene: &SceneSpec, open: Page) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            height: Val::Px(DECK_H),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(12.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
            ..default()
        },
        Pickable::IGNORE,
        BottomDeck,
    ))
    .with_children(|b| {
        command_bar(b, skin, scene, open);
        unit_panel(b, skin);
    });
    p.spawn((
        Button,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(DECK_H + 4.0),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
            ..default()
        },
        frame(skin, Frame::Btn),
        Chromed,
        BottomToggle,
    ))
    .with_children(|t| {
        label(t, "PANEL", 11.0, skin.tok().dim);
    });
}

/// The command bar: three tabs and the page under whichever is pressed.
fn command_bar(p: &mut ChildSpawnerCommands, skin: &Skin, scene: &SceneSpec, open: Page) {
    let tok = skin.tok();
    p.spawn((
        Node {
            width: Val::Px(300.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        c.spawn((
            Node {
                column_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|t| {
            for page in Page::ALL {
                btn(
                    t,
                    skin,
                    &page.name().to_uppercase(),
                    11.0,
                    Node {
                        flex_grow: 1.0,
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                        ..default()
                    },
                    PageTab(page),
                );
            }
        });
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
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                Pickable::IGNORE,
                PageBody(page),
            ))
            .with_children(|g| page_keys(g, skin, page, ink));
        }
    });
}

/// One page's buttons. Every one of them is a key the game already has, which
/// is why the label carries the key: a button that is only a key is a feature
/// nobody can find, and a key that is only a button is one nobody can reach in
/// a hurry.
fn page_keys(g: &mut ChildSpawnerCommands, skin: &Skin, page: Page, ink: Ink) {
    let node = || Node {
        width: Val::Px(94.0),
        padding: UiRect::axes(Val::Px(4.0), Val::Px(5.0)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: Val::Px(1.0),
        ..default()
    };
    match page {
        Page::Fleet => {
            for cmd in [DeckCmd::Move, DeckCmd::Stop, DeckCmd::Focus] {
                let (name, hint) = cmd.label();
                cell(g, skin, name, hint, ink, node(), cmd);
            }
            // Call carries `CallButton` as well, because `call_reinforcements`
            // already reads that marker for R's own button and a second way to
            // call a wave would be a second wave rule.
            let (name, hint) = DeckCmd::Call.label();
            cell(
                g,
                skin,
                name,
                hint,
                ink,
                node(),
                (DeckCmd::Call, CallButton),
            );
            for cmd in [DeckCmd::Bars, DeckCmd::Fps] {
                let (name, hint) = cmd.label();
                cell(g, skin, name, hint, ink, node(), cmd);
            }
        }
        Page::Strike => {
            for w in Weapon::ALL {
                arm(g, skin, w.label(), ink, node(), SandboxAction::Arm(w));
            }
            arm(g, skin, "none", ink, node(), SandboxAction::Disarm);
        }
        Page::Tactics => {
            cell(g, skin, "Pause", "Spc", ink, node(), DeckCmd::Pause);
            for (name, a) in [
                ("Freeze", SandboxAction::Freeze),
                ("Slow", SandboxAction::Slow),
                ("Shield", SandboxAction::Invulnerable),
                ("Blast", SandboxAction::Blast),
                ("Dummy", SandboxAction::Reset),
            ] {
                arm(g, skin, name, ink, node(), a);
            }
        }
    }
}

/// A sandbox cell, whose second line is its key AND what it is at.
///
/// A toggle whose label never changes is a control nobody can read the state
/// of, which is the rail rule redux-tribes keeps. The key and the state both
/// come off `SandboxAction`, so the table that says which key arms a weapon is
/// the same one the handler reads.
fn arm(
    g: &mut ChildSpawnerCommands,
    skin: &Skin,
    name: &str,
    ink: Ink,
    node: Node,
    action: SandboxAction,
) {
    g.spawn((Button, node, frame(skin, Frame::Btn), Chromed, action))
        .with_children(|b| {
            label(b, name, 12.0, ink);
            readout(b, &action.hint(), 9.0, skin.tok().faint, CmdState(action));
        });
}

/// The second line of a sandbox cell, rewritten as the state changes.
#[derive(Component, Clone, Copy)]
pub(crate) struct CmdState(pub(crate) SandboxAction);

/// One command cell: its name over the key that does the same thing.
fn cell(
    g: &mut ChildSpawnerCommands,
    skin: &Skin,
    name: &str,
    hint: &str,
    ink: Ink,
    node: Node,
    marker: impl Bundle,
) {
    g.spawn((Button, node, frame(skin, Frame::Btn), Chromed, marker))
        .with_children(|b| {
            label(b, name, 12.0, ink);
            label(b, hint, 9.0, skin.tok().faint);
        });
}

/// The selection panel in the middle of the deck: what is picked, what it is
/// for, and what it is still made of.
///
/// The bar reads the REACTOR, which is the only thing that kills a ship, so it
/// is the only honest thing to put on a bar. Plating comes off and the ship
/// keeps flying.
fn unit_panel(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    p.spawn((
        framed(
            skin,
            Frame::Panel,
            Node {
                width: Val::Px(300.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
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
            for (tag, stat, ink) in [
                ("SPD", DeckStat::Speed, tok.teal),
                ("GUN", DeckStat::Guns, tok.gold),
                ("ARM", DeckStat::Armour, tok.cyan),
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
                    label(c, tag, 9.0, tok.faint);
                    readout(c, "0", 13.0, ink, stat);
                });
            }
        });
        bar(u, skin, tok.green, 6.0, Fill::Unit);
    });
}
