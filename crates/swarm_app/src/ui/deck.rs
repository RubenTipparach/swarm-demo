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
/// The deck is authored against a 1600 by 900 HUD and SCALED to the window.
///
/// That is the mockup's own arrangement (`#hud` is 1600 by 900 with a
/// transform on it) and it is what makes the two comparable: every number in
/// this file and in `yard.rs` can be read straight off
/// `docs/ui/rts-mockup.html` and checked against it. The first cut authored in
/// window pixels and drifted from the picture it was ported from the moment
/// anything moved, which is what "the proportions are not the same" was: the
/// fleet rail was 126 wide against the mockup's 38.

pub(crate) const HUD_H: f32 = 900.0;

pub(crate) const PANEL_W: f32 = 428.0;
pub(crate) const DECK_H: f32 = 92.0;

/// Everything on the deck that carries a number, named by what it says.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeckStat {
    /// A heading with no number beside it. Named rather than left off, so a
    /// section is built by one function whether or not it carries one.
    None,
    Clock,
    Strip,
    StripSub,
    Ship,
    Role,
    Speed,
    Guns,
    Armour,
    /// The build menu's own readouts.
    Modules,
    YardName,
    SlotsProd,
    SlotsMod,
    SlotsSens,
    CatName,
    CatCount,
    SubQueue,
    ShipQueue,
}

/// A bar's fill, which is a width rather than a string.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    Fuel,
    Unit,
    /// How far along the job at the front of the yard's queue is.
    Yard,
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
#[derive(Component, Clone, Copy)]
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

/// Anything that rides the bottom deck down, carrying the `top` it was
/// authored at.
///
/// The marker CARRIES the number because the slide has to put it back: every
/// node down here is positioned from the TOP, against the mockup's 1600 by 900
/// frame, so the slide adds an offset to that rather than setting the opposite
/// edge. `slide_deck` wrote `bottom` for one stage and the deck never moved,
/// because an absolutely positioned node with a definite `top` ignores it: two
/// systems disagreeing about which edge a node hangs from is the same defect
/// as a builder overwriting a field its caller set, and it fails silently the
/// same way.
#[derive(Component, Clone, Copy)]
pub(crate) struct RidesBottom(pub(crate) f32);

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
        // The build menu in a skirmish and a run, the range in a sandbox:
        // one panel, two contents, chosen by the mode.
        side_panel(p, &skin, &glyphs, &scene);
        bottom(p, &skin, &glyphs, &scene, page);
        if scene.retreat {
            run_controls(p, &skin, &glyphs);
        }
    });
}

/// Scale the whole HUD so a number authored against 1600 by 900 lands where
/// the mockup puts it, whatever the window is.
///
/// By HEIGHT, and that is the decision worth writing down. Scaling by width
/// leaves the bottom deck short of the bottom on anything taller than 16 by 9,
/// and scaling by the smaller of the two letterboxes a HUD that is supposed to
/// sit in the window's own corners. By height, every vertical number is exactly
/// the mockup's share of the screen, and a window wider than 16 by 9 simply has
/// more room between the left rail and the right panel, which is what a wider
/// screen SHOULD give a HUD.
///
/// `UiScale` rather than a transform on the root, because Bevy lays UI out in
/// logical pixels and this changes what one is: a child positioned 6 from the
/// right is still 6 scaled pixels from the window's own edge, so nothing has to
/// know it is being scaled.
/// A headless run has no `Window` at all, so it asks `Headless` for the size of
/// the target it is drawing to instead. Without that the HUD stayed at scale
/// one in exactly the runs that exist to photograph it, which is the trap
/// `--hud` fell into once already when Bevy handed UI to a primary window that
/// was not there.
pub(crate) fn scale_hud(
    windows: Query<&Window>,
    shot: Option<Res<Headless>>,
    mut scale: ResMut<UiScale>,
) {
    let h = windows
        .iter()
        .next()
        .map(|w| w.resolution.height())
        .or_else(|| shot.map(|s| s.height as f32));
    let Some(h) = h else {
        return;
    };
    let want = (h / HUD_H).clamp(0.25, 4.0);
    if (scale.0 - want).abs() > 1e-4 {
        scale.0 = want;
    }
}

/// The deck's own root, so a system can find everything under it.
#[derive(Component)]
pub(crate) struct DeckRoot;

/// The clock, top left, where it sits over nothing a player has to press.
fn clock(p: &mut ChildSpawnerCommands, skin: &Skin) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            top: Val::Px(10.0),
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
            right: Val::Px(6.0),
            top: Val::Px(2.0),
            width: Val::Px(PANEL_W),
            height: Val::Px(44.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
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
            left: Val::Px(8.0),
            top: Val::Px(50.0),
            width: Val::Px(38.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
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
                    width: Val::Px(38.0),
                    height: Val::Px(26.0),
                    padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(tok.row.col()),
                Chromed,
                RosterRow(n),
            ))
            .with_children(|row| {
                if let Some((_, small)) = fleet.of(class) {
                    row.spawn(schematic(small, 24.0, 20.0));
                }
                readout(row, "", 9.0, tok.cyan, RosterCount(n));
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
    // The mockup's `.brow3`: 6 in from each side, 802 down, 92 tall, and four
    // things across it. The command bar is 344, the unit panel 520, the
    // selection thumb takes what is left and the modules row is 428.
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(6.0),
            right: Val::Px(6.0),
            top: Val::Px(802.0),
            height: Val::Px(DECK_H),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Stretch,
            column_gap: Val::Px(6.0),
            ..default()
        },
        Pickable::IGNORE,
        BottomDeck,
        RidesBottom(802.0),
    ))
    .with_children(|b| {
        command_bar(b, skin, glyphs, scene, open);
        unit_panel(b, skin, glyphs);
        // The mockup's `.thumb`, which is what takes up the slack: without it
        // the three fixed blocks pack to the left and the modules row lands in
        // the middle of the screen rather than in the corner it belongs in.
        unit_thumb(b, skin);
        // The subsystem row, bottom right, which is the mockup's own MODULES.
        modules_row(b, skin, glyphs);
    });
    deck_tabs_row(p, skin, open);
    bottom_toggle(p, skin, glyphs);
}

/// The three tab strips, all on one line at 776, which is where the mockup
/// puts them: the pages on the left, the views in the middle and the panel's
/// own on the right.
fn deck_tabs_row(p: &mut ChildSpawnerCommands, skin: &Skin, open: Page) {
    // puts them: the pages on the left, the views in the middle and the
    // panel's own on the right.
    tab_strip(
        p,
        skin,
        Node {
            left: Val::Px(6.0),
            top: Val::Px(776.0),
            width: Val::Px(344.0),
            ..default()
        },
        &Page::ALL
            .iter()
            .map(|x| (x.name(), PageTab(*x), *x == open))
            .collect::<Vec<_>>(),
    );
    // The views, and this one is anchored to BOTH edges rather than given a
    // width. The mockup can say `left: 452; width: 700` because its `#hud` is
    // exactly 1600 wide; here the deck fills the WINDOW, so `right: 6` on the
    // panel's own strip is the window's right edge and not 1600's. At 1280 by
    // 800 the frame is 1441 authored units across, the panel strip starts at
    // 1013, and the middle strip's own 452 to 1152 ran straight under it: the
    // Menu tab was drawn and then covered, which is why two tabs showed where
    // the mockup draws three. 448 from the right is 1152 at exactly 1600, so
    // the picture is unchanged at 16 by 9 and the strip shrinks rather than
    // colliding at anything narrower.
    //
    // The rule: on a HUD anchored to two edges, anything BETWEEN two anchored
    // things is anchored to both, never positioned from one with a fixed width.
    tab_strip(
        p,
        skin,
        Node {
            left: Val::Px(452.0),
            right: Val::Px(448.0),
            top: Val::Px(776.0),
            ..default()
        },
        &ViewTab::ALL
            .iter()
            .map(|x| (x.label(), ViewTabButton(*x), false))
            .collect::<Vec<_>>(),
    );
    tab_strip(
        p,
        skin,
        Node {
            right: Val::Px(6.0),
            top: Val::Px(776.0),
            width: Val::Px(428.0),
            ..default()
        },
        &PanelTab::ALL
            .iter()
            .map(|x| (x.label(), PanelTabButton(*x), *x == PanelTab::Build))
            .collect::<Vec<_>>(),
    );
}

/// The toggle that drops the bottom deck, with the caret that says which way
/// it will go.
fn bottom_toggle(p: &mut ChildSpawnerCommands, skin: &Skin, glyphs: &Glyphs) {
    p.spawn((
        Button,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(6.0),
            top: Val::Px(752.0),
            width: Val::Px(142.0),
            height: Val::Px(22.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        frame(skin, Frame::Btn),
        Chromed,
        BottomToggle,
        RidesBottom(752.0),
    ))
    .with_children(|t| {
        // The caret says which way the deck is about to GO, which is the
        // mockup's own `.ca` rotating a half turn on `data-bottom`. A baked
        // mark flipped rather than two marks or a text character: `flip_y` is
        // one bool against a second bake, and the default font has no glyph
        // for a caret at all.
        t.spawn((
            Node {
                width: Val::Px(11.0),
                height: Val::Px(11.0),
                margin: UiRect::right(Val::Px(7.0)),
                ..default()
            },
            ImageNode {
                image: glyphs.of(Glyph::Caret),
                color: skin.tok().dim.col(),
                ..default()
            },
            Pickable::IGNORE,
            BottomCaret,
        ));
        label(t, "PANEL", 11.0, skin.tok().dim);
    });
}

/// The caret on the bottom deck's toggle, flipped by `light_deck`.
#[derive(Component)]
pub(crate) struct BottomCaret;
