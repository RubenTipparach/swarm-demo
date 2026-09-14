//! What the build menu SAYS, and what pressing it does.
//!
//! `yard.rs` owns the layout and this owns the two systems that make it live,
//! which is the deck's own split a second time: a file that draws and a file
//! that reads are two responsibilities and a panel this size is where that
//! starts to matter.

use crate::*;

/// What the build list is showing this frame.
///
/// A resource rather than a component per row, because the list is rebuilt
/// whenever the category moves and a press has to name the SAME order the row
/// was drawn from: one list read by both is one answer, and a component
/// written by the fill and read by the press a frame later is two.
#[derive(Resource, Default)]
pub(crate) struct Offers(pub(crate) Vec<Order>);

/// Fill the build list from the manifest, and say what can be afforded.
///
/// A row the bank cannot cover is DIM rather than absent, which is the yard
/// between systems' own rule: what you could have had if you had mined one
/// more rock is the information the panel exists to give.
pub(crate) fn fill_build_list(
    views: Res<Views>,
    bank: Res<Bank>,
    baked: Baked,
    mut offers: ResMut<Offers>,
    mut rows: Query<(&BuildSlot, &mut Node)>,
    mut shots: Query<(&BuildShot, &mut ImageNode)>,
    mut says: Query<(&Says, &mut Text, &mut TextColor)>,
) {
    let tok = baked.skin.tok();
    // SHOW ALL is every category at once, in the order the buttons are laid
    // out, so a player who does not want to hunt through six lists does not
    // have to. It is the same `offers` per category and not a second rule.
    offers.0 = if views.all {
        Category::ALL
            .iter()
            .flat_map(|c| build::offers(buildable(), *c))
            .collect()
    } else {
        build::offers(buildable(), views.cat)
    };
    for (slot, mut node) in &mut rows {
        node.display = if slot.0 < offers.0.len() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (shot, mut img) in &mut shots {
        let class = offers.0.get(shot.0).and_then(order_hull);
        img.image = class
            .and_then(|c| baked.fleet.of(c))
            .map(|(_, small)| small)
            .unwrap_or_default();
    }
    // ONE query over what a piece of text SAYS rather than three over three
    // markers. Three would need `Without` on each to prove them disjoint,
    // which is a filter written to satisfy the borrow checker rather than to
    // say anything, and this project holds that a query's filters ARE its
    // interface.
    for (says, mut text, mut ink) in &mut says {
        let (want, lit) = match says {
            Says::Name(n) => (
                offers.0.get(*n).map(Order::label).unwrap_or_default(),
                tok.ink,
            ),
            Says::Cost(n) => (
                offers
                    .0
                    .get(*n)
                    .map(|o| o.cost().to_string())
                    .unwrap_or_default(),
                tok.gold,
            ),
            // What each button holds, and how much of it the bank covers. A
            // count alone would say a category has six rows and nothing about
            // whether any of them can be had, which is the question a player
            // is actually asking.
            Says::Have(cat) => {
                let list = build::offers(buildable(), *cat);
                let can = list.iter().filter(|o| bank.afford(o.price())).count();
                (format!("{} / {}", can, list.len()), tok.faint)
            }
        };
        let ink_now = match says {
            Says::Have(_) => lit.col(),
            Says::Name(n) | Says::Cost(n) => paid(offers.0.get(*n), &bank, lit, tok.faint),
        };
        if text.0 != want {
            text.0 = want;
        }
        if ink.0 != ink_now {
            ink.0 = ink_now;
        }
    }
}

/// Every baked picture the build menu draws with, as one parameter.
#[derive(SystemParam)]
pub(crate) struct Baked<'w> {
    pub(crate) skin: Res<'w, Skin>,
    pub(crate) fleet: Res<'w, Schematics>,
}

/// Which hull's schematic stands for an order.
///
/// A fighter has no class of its own, so it wears the hull the squadron
/// actually flies: the same answer `launch_fighters` uses, read off the same
/// constant, because a picture that disagreed with what takes off would be a
/// picture of another ship.
fn order_hull(order: &Order) -> Option<&str> {
    match order {
        Order::Hull(c) | Order::Platform(c) => Some(c),
        Order::Fighter => Some(FIGHTERS_HULL),
        Order::Fit(_) => None,
    }
}

/// A row's ink: what it is worth if the bank covers it, and dim if not.
fn paid(order: Option<&Order>, bank: &Bank, lit: Ink, dim: Ink) -> Color {
    match order {
        Some(o) if bank.afford(o.price()) => lit.col(),
        _ => dim.col(),
    }
}

/// Every class a yard can make, which is the MANIFEST and not the dropdown's
/// picked eight.
///
/// Read once and kept, because `Fleet::load` reads a directory and this is
/// asked once a frame: the fleet cannot change while the game is running, and
/// twenty three directory entries a frame is a read nobody asked for.
fn buildable() -> &'static [&'static str] {
    static KEYS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        Fleet::load()
            .0
            .into_iter()
            // Leaked on purpose and exactly once: the list lives as long as
            // the process and a `&'static str` is what the core's `offers`
            // takes, so the alternative is a `Vec<String>` rebuilt per call.
            .map(|h| &*Box::leak(h.key.into_boxed_str()))
            .collect()
    })
}

/// Every press the build menu takes: the tabs, the categories, SHOW ALL, a
/// build row and a module.
///
/// One system, because every one of them writes `Views` or the yard and two
/// systems writing one state is how a tab ends up saying something other than
/// what is open.
pub(crate) fn build_input(
    mut views: ResMut<Views>,
    mut bank: ResMut<Bank>,
    offers: Res<Offers>,
    scene: Res<SceneSpec>,
    presses: BuildPress,
    mut yards: Query<(&mut Shipyard, &Hull), With<Flagship>>,
    escorts: Query<(), With<Escort>>,
) {
    for (i, tab) in &presses.panel {
        if *i == Interaction::Pressed && tab.0.ready() {
            views.panel = tab.0;
        }
    }
    for (i, tab) in &presses.view {
        if *i == Interaction::Pressed {
            // A tab already open CLOSES, because a view is somewhere you go
            // into and come back out of and a player who opened Mission needs
            // a way back to the field that is the button they pressed.
            views.open = (views.open != Some(tab.0)).then_some(tab.0);
        }
    }
    for (i, cat) in &presses.cat {
        if *i == Interaction::Pressed {
            views.cat = cat.0;
            views.all = false;
        }
    }
    if presses.all.iter().any(|i| *i == Interaction::Pressed) {
        views.all = !views.all;
    }
    let Ok((mut yard, hull)) = yards.single_mut() else {
        return;
    };
    let class = hull.class.clone().unwrap_or_else(|| scene.hull.clone());
    let out = escorts.iter().count() as u32;
    for (i, slot) in &presses.row {
        if *i != Interaction::Pressed {
            continue;
        }
        let Some(order) = offers.0.get(slot.0).cloned() else {
            continue;
        };
        order_one(&mut yard.0, &mut bank, &class, order, out);
    }
    for (i, m) in &presses.module {
        if *i == Interaction::Pressed {
            order_one(&mut yard.0, &mut bank, &class, Order::Fit(m.0), out);
        }
    }
}

/// Every button the build menu carries, as ONE parameter.
///
/// Six queries in an argument list is the smell this project names, and they
/// are one thing: what was pressed on this panel.
#[derive(SystemParam)]
pub(crate) struct BuildPress<'w, 's> {
    pub(crate) panel:
        Query<'w, 's, (&'static Interaction, &'static PanelTabButton), Changed<Interaction>>,
    pub(crate) view:
        Query<'w, 's, (&'static Interaction, &'static ViewTabButton), Changed<Interaction>>,
    pub(crate) cat: Query<'w, 's, (&'static Interaction, &'static CatButton), Changed<Interaction>>,
    pub(crate) row: Query<'w, 's, (&'static Interaction, &'static BuildSlot), Changed<Interaction>>,
    pub(crate) module:
        Query<'w, 's, (&'static Interaction, &'static ModButton), Changed<Interaction>>,
    pub(crate) all: Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<ShowAll>)>,
}
