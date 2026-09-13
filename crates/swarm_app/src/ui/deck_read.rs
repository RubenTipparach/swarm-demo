//! What the command deck SAYS, every frame, and what its buttons do.
//!
//! The layout is built once in `deck.rs` and never respawned. Everything here
//! writes into it: a text, a bar's width, a row's display. That split is the
//! one the health bars already keep and it is why a deck costs nothing when
//! nothing on it has changed.

use crate::*;

/// The panel's own node, told apart from the two things that slide with the
/// bottom deck. A name rather than three filters written out at the call site,
/// because Bevy proves two queries disjoint from their FILTERS and a filter
/// list is therefore an interface rather than an implementation detail.
type PanelNode<'w, 's> =
    Query<'w, 's, &'static mut Node, (With<SidePanel>, Without<BottomDeck>, Without<BottomToggle>)>;

/// What the unit panel reads: the hull, and the two markers that say what it
/// is FOR.
type PickedShip<'w, 's> = Query<
    'w,
    's,
    (
        &'static Hull,
        Option<&'static Flagship>,
        Option<&'static Support>,
    ),
    (With<Selected>, Without<Hive>),
>;

/// Anything on the deck that lights: the frame it wears, or the ground behind
/// it, and whether it is a tab that stays lit while its page is up.
type LitControls<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        Option<&'static PageTab>,
        Option<&'static mut ImageNode>,
        Option<&'static mut BackgroundColor>,
    ),
    With<Chromed>,
>;

/// Slide the two halves, and place what rides on them.
///
/// Eased on a time constant rather than stepped per frame, so the slide takes
/// the same wall time at twenty frames a second as at a hundred and twenty:
/// an animation measured in frames is an animation that is a different length
/// on every machine.
pub(crate) fn slide_deck(
    real: Res<Time<Real>>,
    mut deck: ResMut<Deck>,
    mut panel: PanelNode,
    mut bottom: Query<&mut Node, (With<BottomDeck>, Without<BottomToggle>)>,
    mut toggle: Query<&mut Node, With<BottomToggle>>,
) {
    let dt = real.delta_secs().min(0.25);
    let k = 1.0 - (-dt / SLIDE).exp();
    let (pw, pb) = (
        if deck.panel { 1.0 } else { 0.0 },
        if deck.bottom { 1.0 } else { 0.0 },
    );
    deck.panel_at += (pw - deck.panel_at) * k;
    deck.bottom_at += (pb - deck.bottom_at) * k;
    // Off the right edge by its own width plus the margin, so a shut panel is
    // fully gone rather than a sliver of frame down the side of the screen.
    let right = 14.0 - (1.0 - deck.panel_at) * (PANEL_W + 20.0);
    for mut n in &mut panel {
        n.right = Val::Px(right);
    }
    let down = -(1.0 - deck.bottom_at) * DECK_H;
    for mut n in &mut bottom {
        n.bottom = Val::Px(down);
    }
    for mut n in &mut toggle {
        n.bottom = Val::Px(DECK_H + 4.0 + down);
    }
}

/// The tabs, the two toggles, and which page body is shown.
pub(crate) fn deck_tabs(
    mut deck: ResMut<Deck>,
    tabs: Query<(&Interaction, &PageTab), Changed<Interaction>>,
    panel: Query<&Interaction, (Changed<Interaction>, With<PanelToggle>)>,
    bottom: Query<&Interaction, (Changed<Interaction>, With<BottomToggle>)>,
    mut bodies: Query<(&mut Node, &PageBody)>,
) {
    for (i, tab) in &tabs {
        if *i == Interaction::Pressed {
            deck.page = tab.0;
        }
    }
    if panel.iter().any(|i| *i == Interaction::Pressed) {
        deck.panel = !deck.panel;
    }
    if bottom.iter().any(|i| *i == Interaction::Pressed) {
        deck.bottom = !deck.bottom;
    }
    for (mut n, body) in &mut bodies {
        let want = if body.0 == deck.page {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
}

/// The Fleet page's buttons, which are the keys the game already has.
///
/// A move order OPENS here rather than being resolved here: `nav_input` is the
/// one implementation of what a move order is, and this sets the same flag the
/// right button does. Two places that opened a disc would be two discs the day
/// either learned something.
pub(crate) fn deck_commands(
    presses: Query<(&Interaction, &DeckCmd), Changed<Interaction>>,
    mut hud: ResMut<Hud>,
    mut ask: ResMut<NavAsk>,
    mut orbit: Query<&mut Orbit>,
    mut ack: ResMut<Ack>,
    mut hulls: Query<&mut Hull, (With<Selected>, Without<Hive>)>,
) {
    for (i, cmd) in &presses {
        if *i != Interaction::Pressed {
            continue;
        }
        match cmd {
            DeckCmd::Move => ask.0 = true,
            DeckCmd::Stop => {
                let mut n = 0;
                for mut h in &mut hulls {
                    if h.order.take().is_some() {
                        n += 1;
                    }
                }
                ack.text = format!("{n} held");
                ack.left = ACK_LIFE;
            }
            DeckCmd::Focus => {
                for mut o in &mut orbit {
                    o.follow = true;
                }
            }
            // The wave is `call_reinforcements`, which reads this button the
            // way it reads R. Nothing to do here but let it see the press.
            DeckCmd::Call => {}
            DeckCmd::Bars => hud.show_bars = !hud.show_bars,
            DeckCmd::Fps => hud.show_fps = !hud.show_fps,
            DeckCmd::Pause => hud.paused = !hud.paused,
        }
    }
}

/// A press on a reinforcement row: one more of that class, on the next free
/// station of the wing.
///
/// `call_one` is the same function R runs; what the row changes is the class
/// argument, which is what makes the list a list rather than six copies of a
/// button.
#[allow(clippy::too_many_arguments)]
pub(crate) fn call_class(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    scene: Res<SceneSpec>,
    lead: Res<Lead>,
    presses: Query<(&Interaction, &CallClass), Changed<Interaction>>,
    flagship: Query<&Hull, With<Flagship>>,
    escorts: Query<(), With<Escort>>,
) {
    let Some((_, pick)) = presses.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let Some(&class) = PICKABLE.get(pick.0) else {
        return;
    };
    let Ok(hull) = flagship.single() else { return };
    let out = escorts.iter().count() as u32;
    if out >= WING_MAX {
        info!("the wing is full at {WING_MAX}");
        return;
    }
    call_one(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        class,
        hull.model.radius(),
        lead.pos,
        lead.rot,
        (scene.chewers / 3) as u32,
        out,
    );
    info!("{class} inbound, {} in the wing", out + 1);
}

/// Everything the deck reads that is not a query, as ONE parameter.
///
/// Five resources in an argument list is the smell this project names; the
/// missing struct is "what the deck is looking at this frame", and Bevy's own
/// `SystemParam` is what lets it be one without any of them being copied.
#[derive(SystemParam)]
pub(crate) struct DeckView<'w> {
    pub(crate) tick: Res<'w, Tick>,
    pub(crate) scene: Res<'w, SceneSpec>,
    pub(crate) bank: Res<'w, Bank>,
    pub(crate) drive: Res<'w, JumpDrive>,
}

/// What one hull says on the unit panel.
struct Card {
    name: String,
    role: String,
    speed: f32,
    guns: usize,
    cells: usize,
    left: f32,
}

/// Read a card off a hull. One function, because the unit panel and the fleet
/// panel ask the same four questions of two different ships.
fn card(hull: &Hull, role: &str) -> Card {
    let live = (0..hull.model.grid.len())
        .filter(|&c| hull.model.grid[c] != 0 && !hull.damage.is_dead(c))
        .count();
    let guns = hull
        .guns
        .iter()
        .filter(|g| !hull.damage.is_dead(g.cell as usize))
        .count();
    Card {
        name: hull
            .class
            .as_deref()
            .map(hull_label)
            .unwrap_or_else(|| "unknown hull".into()),
        role: role.into(),
        speed: HULL_SPEED * hull.model.radius() * hull.thrust(),
        guns,
        cells: live,
        left: reactor_left(hull),
    }
}

/// What share of a hull's reactor is still there, which is the only thing that
/// kills it and therefore the only honest thing to put on a bar.
pub(crate) fn reactor_left(hull: &Hull) -> f32 {
    if hull.reactor.is_empty() {
        return 1.0;
    }
    let live = hull
        .reactor
        .iter()
        .filter(|&&c| !hull.damage.is_dead(c))
        .count();
    live as f32 / hull.reactor.len() as f32
}

/// Every number on the deck, once a frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn deck_readouts(
    view: DeckView,
    hives: Query<(), (With<Hive>, Without<Wreck>)>,
    escorts: Query<(), With<Escort>>,
    picked: PickedShip,
    flagship: Query<&Hull, With<Flagship>>,
    mut stats: Query<(&DeckStat, &mut Text)>,
) {
    let secs = view.tick.tick / 60;
    let unit = picked.iter().next().map(|(h, flag, sup)| {
        let role = match (flag.is_some(), sup) {
            (true, _) => "Command".into(),
            (_, Some(s)) => format!("{:?}", s.role),
            _ => "Escort".into(),
        };
        card(h, &role)
    });
    let flag = flagship.single().ok().map(|h| card(h, "Command"));
    for (stat, mut t) in &mut stats {
        let want = match stat {
            DeckStat::None => continue,
            DeckStat::Clock => format!(
                "Game Time  {}:{:02}:{:02}",
                secs / 3600,
                secs / 60 % 60,
                secs % 60
            ),
            DeckStat::Strip if view.scene.retreat => view.bank.materials.to_string(),
            DeckStat::Strip => format!("{} carriers", hives.iter().count()),
            DeckStat::StripSub if view.scene.retreat => {
                format!("{:.0} / {} to jump", view.bank.fuel, view.drive.cost)
            }
            DeckStat::StripSub => format!("wing {} of {WING_MAX}", escorts.iter().count()),
            DeckStat::Wing => format!("{} of {WING_MAX}", escorts.iter().count()),
            DeckStat::Ship => unit
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "nothing selected".into()),
            DeckStat::Role => unit.as_ref().map(|c| c.role.clone()).unwrap_or_default(),
            DeckStat::Speed => unit
                .as_ref()
                .map(|c| format!("{:.1}", c.speed))
                .unwrap_or_else(|| "0".into()),
            DeckStat::Guns => unit
                .as_ref()
                .map(|c| c.guns.to_string())
                .unwrap_or_else(|| "0".into()),
            DeckStat::Armour => unit
                .as_ref()
                .map(|c| c.cells.to_string())
                .unwrap_or_else(|| "0".into()),
            DeckStat::Flag => flag
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "no flagship".into()),
            DeckStat::Cells => flag
                .as_ref()
                .map(|c| c.cells.to_string())
                .unwrap_or_default(),
            DeckStat::FlagGuns => flag
                .as_ref()
                .map(|c| c.guns.to_string())
                .unwrap_or_default(),
            DeckStat::Drives => flag
                .as_ref()
                .map(|_| {
                    format!(
                        "{:.0}%",
                        flagship.single().map(|h| h.thrust() * 100.0).unwrap_or(0.0)
                    )
                })
                .unwrap_or_default(),
            DeckStat::Reactor => flag
                .as_ref()
                .map(|c| format!("{:.0}%", c.left * 100.0))
                .unwrap_or_default(),
        };
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// The three bars: the fuel toward a jump, the selection's reactor and the
/// flagship's. A width rather than a string, which is why they are not in the
/// readout above.
pub(crate) fn deck_bars(
    view: DeckView,
    skin: Res<Skin>,
    hives: Query<(), (With<Hive>, Without<Wreck>)>,
    picked: Query<&Hull, (With<Selected>, Without<Hive>)>,
    flagship: Query<&Hull, With<Flagship>>,
    mut fills: Query<(&Fill, &mut Node, &mut BackgroundColor)>,
) {
    // The strip's bar is whatever the MODE is counting down to: a run is
    // filling a tank and a skirmish is emptying the carriers. One bar, two
    // meanings, and both of them are the thing the fight ends on.
    let fuel = if view.scene.retreat {
        if view.drive.cost > 0 {
            (view.bank.fuel / view.drive.cost as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    } else {
        let all = view.scene.hives.max(1) as f32;
        ((all - hives.iter().count() as f32) / all).clamp(0.0, 1.0)
    };
    let unit = picked.iter().next().map(reactor_left).unwrap_or(0.0);
    let flag = flagship.single().ok().map(reactor_left).unwrap_or(0.0);
    let tok = skin.tok();
    for (fill, mut n, mut c) in &mut fills {
        let share = match fill {
            Fill::Fuel => fuel,
            Fill::Unit => unit,
            Fill::Flag => flag,
        };
        let want = Val::Percent(share * 100.0);
        if n.width != want {
            n.width = want;
        }
        // Three colours a player can name rather than a ramp nobody can read,
        // and the fuel bar is not one of them: filling a tank is not damage.
        if matches!(fill, Fill::Fuel) {
            continue;
        }
        let ink = if share > 0.6 {
            tok.green
        } else if share > 0.3 {
            tok.gold
        } else {
            tok.red
        };
        if c.0 != ink.col() {
            c.0 = ink.col();
        }
    }
}

/// The fleet rail: a row per class you actually have, and nothing for a class
/// you have none of.
pub(crate) fn deck_roster(
    hulls: Query<&Hull, (Without<Hive>, Without<Wreck>)>,
    mut rows: Query<(&RosterRow, &mut Node)>,
    mut counts: Query<(&RosterCount, &mut Text)>,
) {
    let mut have = [0usize; PICKABLE.len()];
    for h in &hulls {
        if h.dead_hull {
            continue;
        }
        if let Some(class) = h.class.as_deref() {
            if let Some(n) = PICKABLE.iter().position(|p| *p == class) {
                have[n] += 1;
            }
        }
    }
    for (row, mut n) in &mut rows {
        let want = if have[row.0] > 0 {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
    for (c, mut t) in &mut counts {
        let want = format!("x{}", have[c.0]);
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// What a control on the deck looks like under the pointer, and what a tab
/// looks like when its page is the one showing.
///
/// One system, because a press and a pressed STATE are the same picture: the
/// armed frame. A tab that lit on hover and a tab that lit because its page is
/// open would be two writers for one image, which is the defect that put the
/// wrong label on the wrong tab in the mockup.
pub(crate) fn light_deck(deck: Res<Deck>, skin: Res<Skin>, mut lit: LitControls) {
    let tok = skin.tok();
    for (i, tab, image, bg) in &mut lit {
        let hot = *i != Interaction::None || tab.is_some_and(|t| t.0 == deck.page);
        if let Some(mut img) = image {
            let want = skin.tex(if hot { Frame::Hot } else { Frame::Btn });
            if img.image != want {
                img.image = want;
            }
        }
        if let Some(mut c) = bg {
            let want = if hot { tok.row_hot } else { tok.row }.col();
            if c.0 != want {
                c.0 = want;
            }
        }
    }
}

/// What the sandbox cells say they are at.
///
/// The key and the state both come off the action itself, so the deck renders
/// one answer short and nothing else renders it long: the panel that used to
/// spell "Freeze swarm  (Z)  off" is gone, and with it the second copy of the
/// key table it carried.
pub(crate) fn deck_state(
    sb: Res<Sandbox>,
    skin: Res<Skin>,
    mut cells: Query<(&CmdState, &mut Text, &mut TextColor)>,
) {
    let tok = skin.tok();
    for (c, mut t, mut colour) in &mut cells {
        let lit = c.0.lit(&sb);
        let want = match lit {
            Some(true) => format!("{}  on", c.0.hint()),
            Some(false) => format!("{}  off", c.0.hint()),
            None => c.0.hint(),
        };
        if t.0 != want {
            t.0 = want;
        }
        let ink = if lit == Some(true) {
            tok.gold
        } else {
            tok.faint
        };
        if colour.0 != ink.col() {
            colour.0 = ink.col();
        }
    }
}
