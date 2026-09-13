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
/// Every hull that is still flying, which is what a group picks from.
type LiveHulls<'w, 's> = Query<'w, 's, (Entity, &'static Hull), (Without<Hive>, Without<Wreck>)>;

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
    mut commands: Commands,
    tick: Res<Tick>,
    mut hud: ResMut<Hud>,
    mut out: Orders,
    picked: Query<Entity, (With<Selected>, With<Hull>, Without<Hive>)>,
    mut hulls: Query<&mut Hull, (With<Selected>, Without<Hive>)>,
) {
    for (i, cmd) in &presses {
        if *i != Interaction::Pressed {
            continue;
        }
        let mine: Vec<Entity> = picked.iter().collect();
        if !arm_a_mode(*cmd, &mut out, &mine, &mut commands) {
            act_at_once(
                *cmd,
                &mut out,
                &mut hud,
                &mine,
                &mut commands,
                &mut hulls,
                &tick,
            );
        }
    }
}

/// The cells that ARM something and wait for a second press, and whether this
/// was one of them.
///
/// Four of the twelve work this way and they all work the same way: a mode
/// costs one more click and lets the player take as long as they like over the
/// part that is actually hard, which is the lesson the move order was rebuilt
/// as a prototype to learn. Nothing here resolves anything.
fn arm_a_mode(cmd: DeckCmd, out: &mut Orders, mine: &[Entity], commands: &mut Commands) -> bool {
    let said = match cmd {
        DeckCmd::Move => {
            out.ask.open = true;
            return true;
        }
        // Attack IS move, with the stance put back to aggressive in the same
        // press. It is not a second order path: `nav_input` stays the one
        // implementation of what a move order is, and what makes this attack
        // move rather than move is which markers are on the ship when it
        // arrives.
        DeckCmd::Attack => {
            for &e in mine {
                Stance::Aggressive.apply(e, commands);
            }
            out.fleet.0 = Stance::Aggressive;
            out.ask.open = true;
            "attack move: pick a point"
        }
        DeckCmd::Guard => {
            *out.mode = OrderMode::Guard;
            "guard: pick a ship"
        }
        // A rally point is a PLACE, so it is aimed with the disc the move
        // order already aims with: one disc, and what it commits to is the
        // difference. A second way to name a point in three dimensions would
        // be that whole elevation flow written twice.
        DeckCmd::Rally => {
            out.ask.open = true;
            out.ask.rally = true;
            "rally: aim the disc"
        }
        _ => return false,
    };
    out.ack.text = said.into();
    out.ack.left = ACK_LIFE;
    true
}

/// The cells that do their whole job on the press.
///
/// Two of them do nothing here on purpose. Launch carries `CallButton` and
/// Jump out carries `JumpButton`, which are the markers the wave and the drive
/// already read off their own controls: a second way to call a wave would be a
/// second wave rule, and a second way to spin a drive up would be two drives.
fn act_at_once(
    cmd: DeckCmd,
    out: &mut Orders,
    hud: &mut Hud,
    mine: &[Entity],
    commands: &mut Commands,
    hulls: &mut Query<&mut Hull, (With<Selected>, Without<Hive>)>,
    tick: &Tick,
) {
    let said = match cmd {
        DeckCmd::Stop => {
            let mut n = 0;
            for mut h in hulls.iter_mut() {
                if h.order.take().is_some() {
                    n += 1;
                }
            }
            format!("{n} held")
        }
        DeckCmd::Stance => set_stance(commands, &mut out.fleet, mine).label().into(),
        DeckCmd::Form => {
            out.wing.0 = out.wing.0.next();
            format!("the wing keeps a {}", out.wing.0.label())
        }
        DeckCmd::Dock => {
            out.docked.0 = !out.docked.0;
            if out.docked.0 {
                "fighters recalled".into()
            } else {
                "fighters away".into()
            }
        }
        // The right click that already puts every support ship to work.
        DeckCmd::Salvage => {
            out.work.0 = true;
            "support ships to work".into()
        }
        DeckCmd::Scuttle => format!("{} scuttled", scuttle(hulls, mine, tick.tick)),
        DeckCmd::Pause => {
            hud.paused = !hud.paused;
            return;
        }
        DeckCmd::Call | DeckCmd::Hyper => return,
        // Every arming cell was handled before this was reached.
        DeckCmd::Move | DeckCmd::Attack | DeckCmd::Guard | DeckCmd::Rally => return,
    };
    out.ack.text = said;
    out.ack.left = ACK_LIFE;
}

/// Everything a command cell WRITES, as one parameter.
///
/// The bar went from six cells to twelve and `deck_commands` went to thirteen
/// arguments with it, which is this project's own smell for a missing struct.
/// `DeckView` below is the same answer for what the deck READS; this is the
/// other half, and the two together are why the handler is one page rather
/// than a parameter list nobody can scan.
#[derive(SystemParam)]
pub(crate) struct Orders<'w> {
    pub(crate) ask: ResMut<'w, NavAsk>,
    pub(crate) work: ResMut<'w, WorkAsk>,
    pub(crate) mode: ResMut<'w, OrderMode>,
    pub(crate) fleet: ResMut<'w, FleetStance>,
    pub(crate) wing: ResMut<'w, Wing>,
    pub(crate) docked: ResMut<'w, Docked>,
    pub(crate) ack: ResMut<'w, Ack>,
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
/// What the build menu reads off the ship that carries the yard.
///
/// A struct built once before the loop rather than a query inside it: a
/// readout system that asked the fleet a question per row would ask it nine
/// times for nine rows of one panel.
struct YardFacts {
    name: String,
    slots: Slots,
    /// Production jobs running, module bays filled, sensors fitted.
    at: Slots,
    /// What is waiting, split the way the two queue lines are.
    fits: usize,
    hulls: usize,
}

impl Default for YardFacts {
    fn default() -> Self {
        YardFacts {
            name: "no yard".into(),
            slots: Slots::default(),
            at: Slots::default(),
            fits: 0,
            hulls: 0,
        }
    }
}

impl YardFacts {
    /// What one ship's yard reads as.
    fn of(hull: &Hull, yard: &Yard) -> YardFacts {
        let Some(class) = hull.class.clone() else {
            return YardFacts::default();
        };
        let fitted = |sensor: bool| {
            yard.modules
                .iter()
                .filter(|m| m.is_sensor() == sensor)
                .count() as u32
        };
        let fits = yard.queue.iter().filter(|t| t.order.is_fit()).count();
        YardFacts {
            slots: Slots::of(&class),
            at: Slots {
                // A production slot is BUSY when a job is on it, which is the
                // front of the queue up to however many run at once: that is
                // what a production slot IS rather than a second counter.
                production: (yard.queue.len() as u32).min(yard.slots(&class)),
                module: fitted(false),
                sensors: fitted(true),
            },
            fits,
            hulls: yard.queue.len() - fits,
            name: hull_label(&class),
        }
    }
}

/// What a queue line says, which is a COUNT rather than a word: "(Empty)" is
/// what the mockup draws for nothing waiting and a number is what it draws for
/// anything else, and one function is why the two lines cannot disagree.
fn queue_line(what: &str, n: usize) -> String {
    match n {
        0 => format!("{what} Queue (Empty)"),
        1 => format!("{what} Queue: 1 waiting"),
        _ => format!("{what} Queue: {n} waiting"),
    }
}

pub(crate) fn deck_readouts(
    view: DeckView,
    views: Res<Views>,
    hives: Query<(), (With<Hive>, Without<Wreck>)>,
    escorts: Query<(), With<Escort>>,
    picked: PickedShip,
    yards: Query<(&Hull, &Shipyard), With<Flagship>>,
    mut stats: Query<(&DeckStat, &mut Text)>,
) {
    let secs = view.tick.tick / 60;
    // What the build menu says, off the ship that carries the yard. The
    // flagship today: `Slots::of` reads a hull's rung and nothing else, so
    // these are the real numbers for the real class rather than a placeholder
    // waiting for the queue to land.
    let yard = yards
        .single()
        .map(|(h, y)| YardFacts::of(h, &y.0))
        .unwrap_or_default();
    let unit = picked.iter().next().map(|(h, flag, sup)| {
        let role = match (flag.is_some(), sup) {
            (true, _) => "Command".into(),
            (_, Some(s)) => format!("{:?}", s.role),
            _ => "Escort".into(),
        };
        card(h, &role)
    });
    for (stat, mut t) in &mut stats {
        let want = match stat {
            DeckStat::None => continue,
            // The build menu's readouts, which stage three fills from a live
            // `Yard`. Until that lands they say what a hull's rung ALREADY
            // gives it, which is a true number rather than a placeholder: the
            // slots are `Slots::of` on the flagship's own class.
            DeckStat::YardName => yard.name.clone(),
            DeckStat::SlotsProd => format!("{} / {}", yard.at.production, yard.slots.production),
            DeckStat::SlotsMod => format!("{} / {}", yard.at.module, yard.slots.module),
            DeckStat::SlotsSens => format!("{} / {}", yard.at.sensors, yard.slots.sensors),
            DeckStat::Modules => format!("{} / {}", yard.at.module, yard.slots.module),
            DeckStat::CatName => views.cat.label().into(),
            DeckStat::CatCount => format!("{} materials", view.bank.materials),
            DeckStat::SubQueue => queue_line("Subsystem", yard.fits),
            DeckStat::ShipQueue => queue_line("Ship", yard.hulls),
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
    yards: Query<&Shipyard, With<Flagship>>,
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
    let job = yards
        .single()
        .ok()
        .and_then(|y| y.0.queue.first().map(Task::progress))
        .unwrap_or(0.0);
    let tok = skin.tok();
    for (fill, mut n, mut c) in &mut fills {
        let share = match fill {
            Fill::Fuel => fuel,
            Fill::Unit => unit,
            // What the yard is ACTUALLY doing, which is how far along the
            // job at the front of its queue is. The flagship's reactor was
            // there before the queue existed and said nothing about the
            // panel it sits under.
            Fill::Yard => job,
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

/// The picture of whatever is selected.
///
/// Its own system rather than a branch of the one below, because putting the
/// selection on screen and CHANGING the selection are two jobs and a function
/// that did both needed eight arguments to say so.
pub(crate) fn show_unit_shot(
    fleet: Res<Schematics>,
    sel: Query<&Hull, With<Selected>>,
    flagship: Query<&Hull, With<Flagship>>,
    mut shot: Query<&mut ImageNode, (With<UnitShot>, Without<YardShot>)>,
    mut yard: Query<&mut ImageNode, With<YardShot>>,
) {
    let pic = |h: Option<&Hull>| {
        h.and_then(|h| h.class.as_deref())
            .and_then(|c| fleet.of(c))
            .map(|(big, _)| big)
            .unwrap_or_default()
    };
    if let Ok(mut node) = shot.single_mut() {
        let want = pic(sel.iter().next());
        if node.image != want {
            node.image = want;
        }
    }
    // The build menu's own picture, which is the ship that carries the yard
    // rather than whatever is selected: the panel is about what is BUILDING,
    // and a stepper whose picture changed with the selection would be two
    // controls fighting over one image.
    if let Ok(mut node) = yard.single_mut() {
        let want = pic(flagship.iter().next());
        if node.image != want {
            node.image = want;
        }
    }
}

/// A rail row picks a whole class.
///
/// A row of the rail IS the group display: pressing one takes every live ship
/// of that class, which is what an RTS control group is, arrived at from the
/// fleet you actually have rather than from a number somebody had to assign.
/// Shift adds, which is the one convention every RTS shares and the same rule
/// the band box already keeps. No digit is bound to it, because the range
/// already owns one to five and a key that means two things in two modes is a
/// key nobody can learn.
pub(crate) fn pick_group(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    rows: Query<(&Interaction, &RosterRow), Changed<Interaction>>,
    fleet: LiveHulls,
    picked: Query<Entity, With<Selected>>,
) {
    let Some((_, row)) = rows.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let Some(&class) = PICKABLE.get(row.0) else {
        return;
    };
    let add = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if !add {
        for e in &picked {
            commands.entity(e).remove::<Selected>();
        }
    }
    for (e, h) in &fleet {
        if !h.dead_hull && h.class.as_deref() == Some(class) {
            commands.entity(e).insert(Selected);
        }
    }
}
