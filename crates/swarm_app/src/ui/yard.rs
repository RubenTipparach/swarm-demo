//! The build menu, the modules row and the two rows of view tabs.
//!
//! Every number in this file is the mockup's own, against its 1600 by 900
//! layout: the panel is 428 wide at 6 from the right and 50 from the top, the
//! bottom row is 92 tall at 802, a category button is 42 and a queue header is
//! 19. `scale_hud` is what makes those hold at any window size, so a figure
//! here can be read straight off `docs/ui/rts-mockup.html` and checked.

use crate::*;

/// Which tab of the right panel is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Component)]
pub(crate) enum PanelTab {
    #[default]
    Build,
    Research,
    Launch,
}

impl PanelTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PanelTab::Build => "Build",
            PanelTab::Research => "Research",
            PanelTab::Launch => "Launch",
        }
    }

    pub(crate) const ALL: [PanelTab; 3] = [PanelTab::Build, PanelTab::Research, PanelTab::Launch];
}

/// Which of the three middle views is open, if any.
///
/// `None` is the field, which is the state the game is played in. A view is a
/// thing you go INTO and come back out of, so one enum and not three flags:
/// three flags is how two views end up open at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Component)]
pub(crate) enum ViewTab {
    Mission,
    Sensors,
    Menu,
}

impl ViewTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ViewTab::Mission => "Mission",
            ViewTab::Sensors => "Sensors",
            ViewTab::Menu => "Menu",
        }
    }

    pub(crate) const ALL: [ViewTab; 3] = [ViewTab::Mission, ViewTab::Sensors, ViewTab::Menu];
}

/// What the deck is showing on its two tab rows, and what the build list is
/// filtered to.
///
/// One resource and not four, because they are one question: what a player is
/// looking at. Four flags in four places is how a tab ends up saying something
/// other than what is open, which the mockup shipped once already.
#[derive(Resource)]
pub(crate) struct Views {
    pub(crate) panel: PanelTab,
    /// The open view, or the field.
    pub(crate) open: Option<ViewTab>,
    /// Which category the build list is filtered to.
    pub(crate) cat: Category,
    /// Every category at once, which is what SHOW ALL means.
    pub(crate) all: bool,
}

impl Default for Views {
    fn default() -> Views {
        Views {
            panel: PanelTab::Build,
            open: None,
            // The rung the game opens on, so the first thing the panel offers
            // is the class the player is already flying.
            cat: Category::Frigate,
            all: false,
        }
    }
}

/// Which category of the build list is showing.
#[derive(Component, Clone, Copy)]
pub(crate) struct CatButton(pub(crate) Category);

/// The schematic on one row of the build list.
#[derive(Component, Clone, Copy)]
pub(crate) struct BuildShot(pub(crate) usize);

/// A cell of the modules row.
#[derive(Component, Clone, Copy)]
pub(crate) struct ModButton(pub(crate) Module);

/// A tab of the right panel.
#[derive(Component, Clone, Copy)]
pub(crate) struct PanelTabButton(pub(crate) PanelTab);

/// A tab of the middle row.
#[derive(Component, Clone, Copy)]
pub(crate) struct ViewTabButton(pub(crate) ViewTab);

/// One row of a tab strip, which is the mockup's own `.tabs`: buttons 24 tall
/// with two pixels between them.
///
/// Three strips use it (the pages, the views and the panel's own), so it is one
/// function: three copies of a strip is three places for a height to drift.
pub(crate) fn tab_strip<T: Component + Copy>(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    node: Node,
    tabs: &[(&str, T, bool)],
) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            column_gap: Val::Px(2.0),
            ..node
        },
        Pickable::IGNORE,
    ))
    .with_children(|s| {
        for (name, marker, hot) in tabs {
            let ink = if *hot {
                skin.tok().gold
            } else {
                skin.tok().dim
            };
            s.spawn((
                Button,
                Node {
                    height: Val::Px(24.0),
                    flex_grow: 1.0,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                frame(skin, if *hot { Frame::Hot } else { Frame::Btn }),
                Chromed,
                *marker,
            ))
            .with_children(|b| {
                label(b, name, 13.0, ink);
            });
        }
    });
}

/// The MODULES row, bottom right: what is fitted to the ship that has the
/// yard, and what could be.
///
/// Six cells, which are the six the mockup draws and the six
/// `swarm_core::build::Module::ALL` carries. A module a hull has no room for
/// is DIM rather than absent, which is the map screen's own rule: what you
/// could have had is the information the row exists to give.
pub(crate) fn modules_row(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    let tok = skin.tok();
    p.spawn((
        Node {
            width: Val::Px(428.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|m| {
        m.spawn((
            Node {
                height: Val::Px(12.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|h| {
            label(h, "MODULES", 10.0, tok.dim);
            readout(h, "0 / 0", 10.0, tok.ink, DeckStat::Modules);
        });
        m.spawn((
            Node {
                flex_grow: 1.0,
                column_gap: Val::Px(3.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|g| {
            for kind in Module::ALL {
                g.spawn((
                    Button,
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                    frame(skin, Frame::Btn),
                    Chromed,
                    ModButton(kind),
                ))
                .with_children(|b| {
                    b.spawn(mark(glyphs, module_mark(kind), 18.0, tok.dim));
                    label(b, kind.label(), 9.0, tok.dim);
                });
            }
        });
    });
}

/// Which baked mark stands for a module.
///
/// Read off the module rather than authored beside the row, so a module added
/// tomorrow needs a mark and nothing else. Four of the six wear a mark that
/// already meant the same thing elsewhere on the deck, which is the point of
/// a mark being DATA: a drive is a drive whether it is a module or a stat.
fn module_mark(m: Module) -> Glyph {
    match m {
        Module::Production => Glyph::Utility,
        Module::Hangar => Glyph::Dock,
        Module::Armour => Glyph::Armour,
        Module::Drives => Glyph::Speed,
        Module::Sensors => Glyph::Attack,
        Module::Refinery => Glyph::Crystal,
    }
}

/// The right panel's body when the Build tab is showing.
///
/// Laid out top to bottom exactly as the mockup draws it: the producing ship
/// and its stepper, its hull bar, the three slot counters, SHOW ALL, the six
/// categories, the category heading, the list, the two queue headers and the
/// queue well. Every height here is the mockup's.
pub(crate) fn build_body(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    yard_stage(p, skin);
    cat_grid(p, skin, glyphs);
    build_list(p, skin);
    queue_lines(p, skin);
}

/// The producing ship: its name, its picture, the bar of the job it is on and
/// the three counters its rung gives it.
fn yard_stage(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    // The producing ship's name and the caret that would open a list of the
    // ships with a yard. One has one today.
    p.spawn((
        Node {
            height: Val::Px(21.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::horizontal(Val::Px(6.0)),
            ..default()
        },
        frame(skin, Frame::Btn),
        Pickable::IGNORE,
    ))
    .with_children(|s| {
        readout(s, "", 13.5, tok.ink, DeckStat::YardName);
        label(s, "v", 11.0, tok.dim);
    });
    p.spawn((
        Node {
            column_gap: Val::Px(4.0),
            align_items: AlignItems::Stretch,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|st| {
        // The mockup's `.pstage`, and WITHOUT the two arrows it draws either
        // side. They step between the ships that carry a yard and exactly one
        // does, so they are a control for a mechanic that does not exist,
        // which this project holds is worse than a missing one: a player
        // spends a fight wondering what they were for. They come back the day
        // a module puts a yard on an escort.
        st.spawn((
            framed(
                skin,
                Frame::Well,
                Node {
                    flex_grow: 1.0,
                    height: Val::Px(128.0),
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
                    width: Val::Px(300.0),
                    height: Val::Px(100.0),
                    ..default()
                },
                ImageNode::default(),
                Pickable::IGNORE,
                YardShot,
            ));
        });
    });
    bar(p, skin, tok.green, 6.0, Fill::Yard);
    // The three counters, which are the hull's rung and nothing else.
    for (name, stat) in [
        ("Production Slots", DeckStat::SlotsProd),
        ("Module Slots", DeckStat::SlotsMod),
        ("Sensors Slot", DeckStat::SlotsSens),
    ] {
        p.spawn((
            Node {
                height: Val::Px(15.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|r| {
            label(r, name, 12.0, tok.dim);
            readout(r, "0 / 0", 12.0, tok.ink, stat);
        });
    }
    p.spawn((
        Button,
        Node {
            height: Val::Px(21.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        frame(skin, Frame::Btn),
        Chromed,
        ShowAll,
    ))
    .with_children(|b| {
        label(b, "SHOW ALL", 12.5, tok.ink);
    });
}

/// SHOW ALL and the six category buttons, three across, with the header line
/// under them that names what the list below is filtered to.
fn cat_grid(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    let tok = skin.tok();
    // Six categories, three across, which is the mockup's own grid.
    p.spawn((
        Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(3.0),
            row_gap: Val::Px(3.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        for cat in Category::ALL {
            c.spawn((
                Button,
                Node {
                    // Three across: a third of the panel less the two gaps.
                    width: Val::Px(134.0),
                    height: Val::Px(42.0),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(5.0),
                    padding: UiRect::left(Val::Px(2.0)),
                    ..default()
                },
                frame(skin, Frame::Btn),
                Chromed,
                CatButton(cat),
            ))
            .with_children(|b| {
                b.spawn(mark(glyphs, cat_mark(cat), 22.0, tok.dim));
                b.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|t| {
                    label(t, cat.label(), 10.5, tok.ink);
                    readout(t, "0 / 0", 9.5, tok.faint, Says::Have(cat));
                });
            });
        }
    });
    p.spawn((
        Node {
            height: Val::Px(16.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|h| {
        readout(h, "", 12.5, tok.gold, DeckStat::CatName);
        readout(h, "", 12.5, tok.gold, DeckStat::CatCount);
    });
}

/// The rows themselves, laid out empty and filled by `fill_build_list`.
fn build_list(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    // The list itself, which is filled from the manifest rather than typed.
    // A WELL rather than a bare node, or the rows a category has none of are a
    // hole in the panel with the fight showing through it.
    p.spawn((
        framed(
            skin,
            Frame::Well,
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                overflow: Overflow::clip(),
                ..default()
            },
        ),
        Pickable::IGNORE,
        BuildList,
    ))
    .with_children(|l| {
        // Rows enough for the longest category, hidden until they are used,
        // because a row spawned per frame is a row the picker has to rebuild.
        for n in 0..BUILD_ROWS {
            l.spawn((
                Button,
                Node {
                    display: Display::None,
                    height: Val::Px(34.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(tok.row.col()),
                Chromed,
                BuildSlot(n),
            ))
            .with_children(|r| {
                label(r, ">", 10.0, tok.faint);
                // The picture is set by the fill, because which hull a row
                // offers moves with the category: a schematic baked into the
                // row would be the same ship under every button.
                r.spawn((
                    Node {
                        width: Val::Px(46.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    ImageNode::default(),
                    Pickable::IGNORE,
                    BuildShot(n),
                ));
                readout(r, "", 12.5, tok.ink, Says::Name(n));
                readout(r, "", 12.0, tok.gold, Says::Cost(n));
            });
        }
    });
}

/// The two queue lines and the well under them.
fn queue_lines(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    // Two queues, because a module and a hull are not the same wait.
    for (stat, tag) in [
        (DeckStat::SubQueue, "Subsystem Queue (Empty)"),
        (DeckStat::ShipQueue, "Ship Queue (Empty)"),
    ] {
        p.spawn((
            Node {
                height: Val::Px(19.0),
                align_items: AlignItems::Center,
                padding: UiRect::left(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(tok.row.col()),
            Pickable::IGNORE,
        ))
        .with_children(|q| {
            readout(q, tag, 12.5, tok.dim, stat);
        });
    }
    p.spawn((
        framed(
            skin,
            Frame::Well,
            Node {
                height: Val::Px(104.0),
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::FlexStart,
                column_gap: Val::Px(3.0),
                row_gap: Val::Px(3.0),
                ..default()
            },
        ),
        QueueWell,
    ))
    .insert(Pickable::IGNORE);
}

/// How many build rows are laid out. The longest category is the four warship
/// rungs across four navies, so sixteen with room to spare.
pub(crate) const BUILD_ROWS: usize = 18;

/// Which baked mark stands for a category. The six were drawn for exactly
/// this, which is why five of them are the class marks themselves.
fn cat_mark(c: Category) -> Glyph {
    match c {
        Category::Fighter => Glyph::Fighter,
        Category::Corvette => Glyph::Corvette,
        Category::Frigate => Glyph::Frigate,
        Category::Capital => Glyph::Capital,
        Category::Utility => Glyph::Utility,
        Category::Platform => Glyph::Platform,
    }
}

/// The SHOW ALL button.
#[derive(Component)]
pub(crate) struct ShowAll;

/// The list every build row lives in.
#[derive(Component)]
pub(crate) struct BuildList;

/// One row of the build list, by index.
#[derive(Component)]
pub(crate) struct BuildSlot(pub(crate) usize);

/// What one live piece of text on the build menu says.
///
/// One component for three things rather than three, because they are one
/// question and Bevy proves two queries disjoint from their FILTERS: three
/// markers would need a `Without` for each of the other two on every query,
/// which is a filter written to satisfy the borrow checker rather than to say
/// anything about what a system reads.
#[derive(Component, Clone, Copy)]
pub(crate) enum Says {
    /// What a build row makes.
    Name(usize),
    /// What it costs.
    Cost(usize),
    /// How much of a category the bank can cover, of how much it holds.
    Have(Category),
}

/// The well the queue is drawn in.
#[derive(Component)]
pub(crate) struct QueueWell;

/// The picture of whatever ship the yard is on.
#[derive(Component)]
pub(crate) struct YardShot;
