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

/// The picture of whatever is selected, set on SELECTION rather than drawn,
/// so it is one assignment a frame rather than a bake.
#[derive(Component)]
pub(crate) struct UnitShot;

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
    Call,
    Pause,
    /// Fly the order AND shoot on the way, which is the move order with the
    /// stance put back to aggressive in the same press.
    Attack,
    /// Hold station on whatever is picked next.
    Guard,
    /// Cycle aggressive, defensive, hold fire.
    Stance,
    /// Cycle the wing's shape: wedge, line, sphere.
    Form,
    /// Recall the fighters to the ship they fly off.
    Dock,
    /// Where a ship the yard builds flies to.
    Rally,
    /// Put every support ship to work, which is the right click `--job`
    /// photographs.
    Salvage,
    /// Spin the jump drive up.
    Hyper,
    /// Take your own reactor.
    Scuttle,
}

impl DeckCmd {
    /// What it is called. No key is spelled beside it, and Stop deliberately
    /// has none: WASD pans the camera, so a command that took S would take a
    /// pan key away the moment a player reached for it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            DeckCmd::Move => "Move",
            DeckCmd::Stop => "Stop",
            DeckCmd::Call => "Call",
            DeckCmd::Pause => "Pause",
            DeckCmd::Attack => "Attack",
            DeckCmd::Guard => "Guard",
            DeckCmd::Stance => "Stance",
            DeckCmd::Form => "Form",
            DeckCmd::Dock => "Dock",
            DeckCmd::Rally => "Rally",
            DeckCmd::Salvage => "Salvage",
            DeckCmd::Hyper => "Jump out",
            DeckCmd::Scuttle => "Scuttle",
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
    glyphs: Res<Glyphs>,
    fleet: Res<Schematics>,
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
        strip(p, &skin, &glyphs, &scene);
        rail(p, &skin, &fleet);
        // The side panel is the DEBUG panel and is not part of the game's own
        // design: a campaign is played off the strip, the rail and the deck.
        // It carries the range's numbers in the sandbox and the wave in a
        // skirmish, and a run gets neither.
        if !scene.retreat {
            side_panel(p, &skin, &fleet, &scene);
        }
        bottom(p, &skin, &glyphs, &scene, page);
        if scene.retreat {
            run_controls(p, &skin, &glyphs);
        }
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
fn strip(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs, scene: &SceneSpec) {
    let tok = skin.tok();
    let (a, b, ma, mb) = if scene.retreat {
        (tok.ore, tok.cry, Glyph::Ore, Glyph::Crystal)
    } else {
        (tok.teal, tok.cyan, Glyph::Capital, Glyph::Fighter)
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
        s.spawn(mark(glyphs, ma, 20.0, a));
        readout(s, "0", 17.0, a, DeckStat::Strip);
        s.spawn(mark(glyphs, mb, 20.0, b));
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
            // How long this system has, which is the one number a run is
            // actually played against.
            if scene.retreat {
                readout(f, "", 14.0, tok.gold, RetreatStat::Tide);
            }
        });
    });
}

/// The fleet rail down the left: one row per class the fleet actually has.
///
/// A row for a class with none of it is a row that says nothing, so the rows
/// are spawned once and HIDDEN rather than respawned: a rail rebuilt every
/// frame is nine allocations a frame for a picture that changes twice a match.
fn rail(p: &mut ChildSpawnerCommands, skin: &Skin, fleet: &Schematics) {
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
                Button,
                Node {
                    display: Display::None,
                    width: Val::Px(126.0),
                    padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(tok.row.col()),
                Chromed,
                RosterRow(n),
            ))
            .with_children(|row| {
                if let Some((_, small)) = fleet.of(class) {
                    row.spawn(schematic(small, 84.0, 28.0));
                }
                readout(row, "", 12.0, tok.cyan, RosterCount(n));
            });
        }
    });
}

/// The bottom deck: the command bar on the left and the selection in the
/// middle, with the groups and the view tabs over them.
fn bottom(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    glyphs: &Glyphs,
    scene: &SceneSpec,
    open: Page,
) {
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
        command_bar(b, skin, glyphs, scene, open);
        unit_panel(b, skin, glyphs);
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
