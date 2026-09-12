//! What the bank buys between systems, and what a run carries into the next
//! one.
//!
//! Two currencies and they do different jobs. MATERIALS buy hulls and hull
//! plating: another support ship, another escort, the scars welded shut.
//! DATA buys the RIGHT to buy: a role the yard has never built, or the next
//! rung of the flagship's own navy. That is the design's "equipment that
//! unlocks more research" said the one way that costs nothing to implement,
//! and it is what stops a run being a single pile of one number.

use crate::*;

/// One thing the refit screen offers.
///
/// A row rather than a branch, so a new offer is a row and not a new screen:
/// this project's own open for extension rule, and the reason the screen
/// below is a loop over `offers` with no knowledge of what any of them are.
#[derive(Component, Clone, PartialEq, Debug)]
pub(crate) enum Buy {
    /// Weld the flagship's scars shut. The one thing a run cannot do without
    /// if it means to see act three.
    Repair,
    /// One more of the flagship's class, flying as an escort.
    Escort,
    /// A support ship of a role the yard has unlocked.
    Ship(Role),
    /// A role the yard has never built.
    Research(Role),
    /// The next rung of the flagship's own navy: a frigate becomes a
    /// destroyer, a destroyer a heavy cruiser. Read off the fleet manifest
    /// rather than typed, so a class added tomorrow is on the ladder
    /// tomorrow.
    Refit(String),
    /// Put a dead ship back together out of what a salvager recovered of
    /// it. The number is which ship, by the seed it flew under.
    Rebuild(u32),
}

/// What one costs, as (materials, data).
///
/// Written in CUBES, because a cube is what a player actually carries home
/// and "two cubes" is a thing they can count in the field: a price in bare
/// materials would be a number nothing in the world corresponds to. Tuning
/// numbers, so they live here beside the one system that reads them.
///
/// A rock gives up three or four ore cubes, so an escort is most of a rock
/// and a support ship is a third of one.
pub(crate) const ESCORT_COST: u32 = 2 * ORE_CUBE;
pub(crate) const SHIP_COST: u32 = ORE_CUBE;
pub(crate) const RESEARCH_COST: u32 = DATA_CUBE;
pub(crate) const REFIT_COST: (u32, u32) = (4 * ORE_CUBE, 2 * DATA_CUBE);

/// A repair is priced by the CELL rather than by the cube, because that is
/// what it is: a hole is so many cells and welding one shut costs what it
/// costs. A ship half eaten therefore costs about a cube.
pub(crate) const REPAIR_PER_CELL: u32 = 1;

/// What each tender takes off the yard's repair bill, and the most they can
/// take between them. Half, because a repair nobody pays for is a repair
/// nobody thinks about.
pub(crate) const TENDER_OFF: f32 = 0.25;
pub(crate) const TENDER_OFF_MAX: f32 = 0.5;

impl Buy {
    pub(crate) fn price(&self, run: &RunState) -> (u32, u32) {
        match self {
            // By the cell, so a ship shot to pieces costs what it costs, and
            // a tender in the fleet is a discount on it: the crews that weld
            // in the field are the crews that weld in the yard.
            Buy::Repair => {
                let tenders = run.fleet.iter().filter(|&&r| r == Role::Tender).count() as u32;
                let off = 1.0 - (TENDER_OFF * tenders as f32).min(TENDER_OFF_MAX);
                let cells = run.scars.len() as u32 * REPAIR_PER_CELL;
                ((cells as f32 * off) as u32, 0)
            }
            Buy::Escort => (ESCORT_COST, 0),
            Buy::Ship(_) => (SHIP_COST, 0),
            Buy::Research(_) => (0, RESEARCH_COST),
            Buy::Refit(_) => REFIT_COST,
            // What a hulk costs is what is MISSING from it: the materials
            // are a hull's own price either way, and the research is what
            // tells a ship reassembled from what was found apart from one
            // worked out from what was not.
            Buy::Rebuild(ship) => hulk_of(run, *ship)
                .and_then(|h| h.rebuild_price())
                .unwrap_or((u32::MAX, u32::MAX)),
        }
    }

    pub(crate) fn label(&self, run: &RunState) -> String {
        match self {
            Buy::Repair => format!("Weld {} cells", run.scars.len()),
            // A ship there is no berth for says so on the button, because
            // "not enough in the bank" would be a lie a player would spend
            // a system acting on.
            Buy::Escort | Buy::Ship(_) if berthed(run) >= berths(run) => {
                "No berth: build a freighter".into()
            }
            Buy::Escort => format!("Another {}", hull_label(&run.flagship)),
            Buy::Ship(r) => format!("Build a {}", r.label()),
            Buy::Research(r) => format!("Research the {}", r.label()),
            Buy::Refit(k) => format!("Refit to a {}", hull_label(k)),
            Buy::Rebuild(ship) => match hulk_of(run, *ship) {
                Some(h) => format!(
                    "Rebuild the {} ({:.0}% recovered)",
                    hull_label(&h.class),
                    h.share() * 100.0
                ),
                None => "Rebuild".into(),
            },
        }
    }

    /// Whether a berth is needed for this, which is what a freighter is FOR.
    /// Research and a repair take none: they add no hull.
    pub(crate) fn berths(&self) -> bool {
        matches!(self, Buy::Escort | Buy::Ship(_) | Buy::Rebuild(_))
    }

    /// Spend it. Answers whether it happened, so the screen can say no
    /// without knowing what any of these cost.
    pub(crate) fn take(&self, run: &mut RunState) -> bool {
        let (m, d) = self.price(run);
        if run.bank.materials < m || run.bank.data < d {
            return false;
        }
        // A hull needs somewhere to put it. This is the freighter's second
        // job and the reason a fleet cannot simply grow: the yard will build
        // what you can afford and not what you cannot carry.
        if self.berths() && berthed(run) >= berths(run) {
            return false;
        }
        run.bank.materials -= m;
        run.bank.data -= d;
        match self {
            Buy::Repair => run.scars.clear(),
            Buy::Escort => run.escorts += 1,
            Buy::Ship(r) => run.fleet.push(*r),
            Buy::Research(r) => run.unlocked.push(*r),
            Buy::Refit(k) => run.flagship = k.clone(),
            // It flies again, and the wreck is spent: a hulk put back
            // together is not a hulk any more, and a rebuild that could be
            // bought twice would be a fleet made of one dead frigate.
            Buy::Rebuild(ship) => {
                run.escorts += 1;
                run.hulks.retain(|h| h.ship != *ship);
            }
        }
        true
    }
}

/// Everything on offer where the run stands, in the order a player reads
/// them: what is wrong first, then what can be added, then what can be
/// learned.
pub(crate) fn offers(run: &RunState, fleet: &Fleet) -> Vec<Buy> {
    let mut out = Vec::new();
    if !run.scars.is_empty() {
        out.push(Buy::Repair);
    }
    out.push(Buy::Escort);
    for &role in &run.unlocked {
        out.push(Buy::Ship(role));
    }
    if let Some(next) = next_rung(&run.flagship, fleet) {
        out.push(Buy::Refit(next));
    }
    // Every wreck there is enough of. A yard can put back what the field
    // cannot, which is the whole of the difference between the two tiers:
    // under half of it and nothing is offered at all.
    for hulk in &run.hulks {
        if hulk.rebuild_price().is_some() {
            out.push(Buy::Rebuild(hulk.ship));
        }
    }
    for role in ALL_ROLES {
        if !run.unlocked.contains(&role) {
            out.push(Buy::Research(role));
        }
    }
    out
}

/// Every role there is, which is the list research works through. Written
/// once here rather than at each caller, because a seventh role tomorrow is
/// a row on the screen tomorrow and nothing else.
pub(crate) const ALL_ROLES: [Role; 6] = [
    Role::Miner,
    Role::Tanker,
    Role::Salvager,
    Role::Survey,
    Role::Freighter,
    Role::Tender,
];

/// The next class up the same navy's ladder, or nothing at the top of it.
///
/// `Fleet` is already sorted navy by navy up the rungs and is read off the
/// hull files rather than typed, so this is the manifest's own order and not
/// a second table that could disagree with it.
pub(crate) fn next_rung(from: &str, fleet: &Fleet) -> Option<String> {
    let navy = from.split_once('_').map(|(n, _)| n)?;
    let at = fleet.0.iter().position(|h| h.key == from)?;
    fleet
        .0
        .iter()
        .skip(at + 1)
        .find(|h| h.key.starts_with(navy))
        .map(|h| h.key.clone())
}

/// One of the run's dead ships by the seed it flew under.
pub(crate) fn hulk_of(run: &RunState, ship: u32) -> Option<&Hulk> {
    run.hulks.iter().find(|h| h.ship == ship)
}

/// How many hulls the fleet may field beyond the command ship, and how many
/// it is fielding.
///
/// A freighter is what raises the first number, which with its cut of
/// everything landed is the whole of what a freighter does: it never cuts
/// anything and it is what a growing fleet is built ON.
pub(crate) fn berths(run: &RunState) -> u32 {
    let freighters = run.fleet.iter().filter(|&&r| r == Role::Freighter).count() as u32;
    BASE_BERTHS + FREIGHT_BERTHS * freighters
}

pub(crate) fn berthed(run: &RunState) -> u32 {
    run.escorts + run.fleet.len() as u32
}

/// A class key as a person reads it.
pub(crate) fn hull_label(key: &str) -> String {
    key.replace('_', " ")
}

/// The scars the run arrived with, put on the hull it arrived in.
///
/// The frame AFTER the field is spawned, because the flagship arrives
/// through commands, which is the same reason `size_holds` runs there. They
/// go on as `scar` rather than `kill`, so they are dead and COLD: a ship
/// that limped out of the last system should not arrive with its holes white
/// hot, and there is no tick before nought to kill them at.
pub(crate) fn apply_scars(
    run: Res<RunState>,
    mut q: Query<&mut Hull, (With<Flagship>, Added<Hull>)>,
) {
    let Ok(mut hull) = q.single_mut() else { return };
    if run.scars.is_empty() {
        return;
    }
    let mut took = 0;
    for &n in &run.scars {
        if (n as usize) < hull.model.len() && hull.damage.scar(n as usize).is_some() {
            took += 1;
        }
    }
    info!("the flagship arrives with {took} cells still open");
}

/// What the flagship is carrying when it jumps, for the next system to
/// arrive with. Read off the damage grid rather than counted as a number,
/// because a share of a hull is a hit point bar with extra steps and this
/// game's whole damage model is WHERE the holes are.
pub(crate) fn scars_of(hull: &Hull) -> Vec<u32> {
    (0..hull.model.len())
        .filter(|&n| hull.damage.is_dead(n))
        .map(|n| n as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand in for the manifest: a navy with three rungs and another
    /// navy's frigate after it, which is what the ladder must not step onto.
    fn shelf() -> Fleet {
        Fleet(
            [
                "terran_frigate",
                "terran_destroyer",
                "terran_cruiser",
                "karisen_frigate",
            ]
            .into_iter()
            .map(|key| HullEntry {
                key: key.into(),
                label: key.into(),
            })
            .collect(),
        )
    }

    #[test]
    fn the_ladder_is_the_navys_own_and_stops_at_the_top() {
        let f = shelf();
        assert_eq!(
            next_rung("terran_frigate", &f).as_deref(),
            Some("terran_destroyer")
        );
        assert_eq!(
            next_rung("terran_destroyer", &f).as_deref(),
            Some("terran_cruiser")
        );
        // Not the next hull on the shelf, which is another navy's.
        assert_eq!(next_rung("terran_cruiser", &f), None);
        assert_eq!(next_rung("karisen_frigate", &f), None);
        assert_eq!(next_rung("nothing_at_all", &f), None);
    }

    #[test]
    fn a_purchase_costs_what_it_says_and_is_refused_when_it_cannot_be_paid() {
        let mut run = RunState::new(1, "terran_frigate");
        run.bank = Bank::default();
        // Nothing in the bank buys nothing.
        assert!(!Buy::Escort.take(&mut run));
        assert!(!Buy::Research(Role::Salvager).take(&mut run));
        assert_eq!(run.escorts, 1);
        // And the two currencies do not stand in for one another.
        run.bank.materials = 9000;
        assert!(!Buy::Research(Role::Salvager).take(&mut run));
        run.bank.data = RESEARCH_COST;
        assert!(Buy::Research(Role::Salvager).take(&mut run));
        assert_eq!(run.bank.data, 0);
        assert_eq!(run.bank.materials, 9000, "research spends no materials");
        assert!(run.unlocked.contains(&Role::Salvager));
        // Researched, paid for, and still refused: a run opens with its
        // berths full, which is the freighter's own rule and has its own
        // test. Make room, and the same purchase goes through.
        assert!(!Buy::Ship(Role::Salvager).take(&mut run));
        run.escorts -= 1;
        assert!(Buy::Ship(Role::Salvager).take(&mut run));
        assert_eq!(run.bank.materials, 9000 - SHIP_COST);
        assert_eq!(
            run.fleet.iter().filter(|&&r| r == Role::Salvager).count(),
            1
        );
    }

    #[test]
    fn a_repair_is_priced_by_the_cells_it_welds_and_closes_all_of_them() {
        let mut run = RunState::new(2, "terran_frigate");
        run.scars = (0..40).collect();
        assert_eq!(Buy::Repair.price(&run).0, 40 * REPAIR_PER_CELL);
        run.bank.materials = 40 * REPAIR_PER_CELL - 1;
        assert!(!Buy::Repair.take(&mut run), "a cell short is a cell short");
        run.bank.materials += 1;
        assert!(Buy::Repair.take(&mut run));
        assert!(run.scars.is_empty());
        assert_eq!(run.bank.materials, 0);
    }

    #[test]
    fn the_yard_offers_what_the_run_can_actually_use() {
        let f = shelf();
        let mut run = RunState::new(3, "terran_frigate");
        let first = offers(&run, &f);
        assert!(
            !first.contains(&Buy::Repair),
            "a sound hull is not offered a weld"
        );
        assert!(
            first.contains(&Buy::Ship(Role::Miner)),
            "what it has, it can build"
        );
        assert!(
            !first.contains(&Buy::Ship(Role::Salvager)),
            "and what it has not researched, it cannot"
        );
        assert!(first.contains(&Buy::Research(Role::Salvager)));
        assert!(first.contains(&Buy::Refit("terran_destroyer".into())));
        // Researching a role moves it from one list to the other, which is
        // the whole of the tech tree: data buys the right to spend materials.
        run.bank.data = RESEARCH_COST;
        assert!(Buy::Research(Role::Salvager).take(&mut run));
        run.scars.push(7);
        let then = offers(&run, &f);
        assert!(then.contains(&Buy::Ship(Role::Salvager)));
        assert!(!then.contains(&Buy::Research(Role::Salvager)));
        assert_eq!(
            then.first(),
            Some(&Buy::Repair),
            "what is wrong is read first"
        );
    }

    #[test]
    fn a_freighter_is_what_a_fleet_grows_on() {
        let f = shelf();
        let mut run = RunState::new(4, "terran_frigate");
        run.bank.materials = 9000;
        // It opens with three berths and three hulls in them: an escort and
        // the two ships a run starts with.
        assert_eq!(berths(&run), BASE_BERTHS);
        assert_eq!(berthed(&run), 3);
        assert!(
            !Buy::Ship(Role::Miner).take(&mut run),
            "a full fleet builds nothing, however rich it is"
        );
        assert!(!Buy::Escort.take(&mut run));
        assert_eq!(run.bank.materials, 9000, "and is not charged for it");
        // Research and a repair need no berth, because they add no hull.
        run.bank.data = RESEARCH_COST;
        assert!(Buy::Research(Role::Freighter).take(&mut run));
        // A freighter is the way out, and it takes the last berth to do it.
        run.fleet.pop();
        assert!(Buy::Ship(Role::Freighter).take(&mut run));
        assert_eq!(berths(&run), BASE_BERTHS + FREIGHT_BERTHS);
        assert!(
            Buy::Escort.take(&mut run),
            "two more berths, so two more hulls"
        );
        assert!(Buy::Ship(Role::Miner).take(&mut run));
        assert!(!Buy::Ship(Role::Miner).take(&mut run), "and then no more");
        // The button says so rather than blaming the bank.
        assert!(Buy::Ship(Role::Miner).label(&run).contains("freighter"));
        let _ = offers(&run, &f);
    }

    #[test]
    fn a_tender_is_a_discount_on_the_yards_own_welding() {
        let mut run = RunState::new(5, "terran_frigate");
        run.scars = (0..200).collect();
        let full = Buy::Repair.price(&run).0;
        assert_eq!(full, 200 * REPAIR_PER_CELL);
        run.fleet.push(Role::Tender);
        let one = Buy::Repair.price(&run).0;
        assert!(one < full && one as f32 >= full as f32 * 0.7);
        run.fleet.push(Role::Tender);
        run.fleet.push(Role::Tender);
        // Three of them, and the discount stops at half: a repair nobody
        // pays for is a repair nobody thinks about.
        assert_eq!(Buy::Repair.price(&run).0, full / 2);
    }

    /// A hulk recovered to a given share of what the ship was.
    fn hulk(share: f32) -> Hulk {
        Hulk {
            ship: 77,
            class: "terran_frigate".into(),
            whole: 1000,
            got: (1000.0 * share) as u32,
        }
    }

    #[test]
    fn a_wreck_is_rebuilt_in_the_field_at_seven_tenths_and_never_under_half() {
        // Under half is scrap: there is no price, and the yard does not
        // offer it at all. That is what stops a run treating every loss as
        // a delay.
        let scrap = hulk(0.49);
        assert_eq!(scrap.rebuild_price(), None);
        assert!(!scrap.in_field());
        // Half to seven tenths is a ship a YARD can put back, and what is
        // missing has to be worked out rather than found: the materials are
        // a hull's own price and the research is three times over.
        let yard = hulk(0.6);
        assert_eq!(
            yard.rebuild_price(),
            Some((REBUILD_COST, REBUILD_DATA * REBUILD_PENALTY))
        );
        assert!(!yard.in_field(), "not enough of it came back to reassemble");
        // And seven tenths is reassembly, in the field, at the plain price.
        let field = hulk(0.7);
        assert_eq!(field.rebuild_price(), Some((REBUILD_COST, REBUILD_DATA)));
        assert!(field.in_field());
        // A share is a share: more than the whole ship cannot come back.
        let over = Hulk {
            got: 4000,
            ..hulk(1.0)
        };
        assert_eq!(over.share(), 1.0);
        // And a ship nobody ever measured is not a ship: no whole, no share,
        // rather than a division by nought.
        let none = Hulk {
            whole: 0,
            ..hulk(1.0)
        };
        assert_eq!(none.share(), 0.0);
        assert_eq!(none.rebuild_price(), None);
    }

    #[test]
    fn the_yard_rebuilds_a_hulk_once_and_charges_a_berth_for_it() {
        let f = shelf();
        let mut run = RunState::new(6, "terran_frigate");
        run.hulks.push(hulk(0.8));
        run.hulks.push(hulk(0.2));
        run.hulks[1].ship = 78;
        // Only the one there is enough of.
        let rows = offers(&run, &f);
        assert!(rows.contains(&Buy::Rebuild(77)));
        assert!(!rows.contains(&Buy::Rebuild(78)), "under half is scrap");
        assert!(Buy::Rebuild(77).label(&run).contains("80%"));
        // A hull needs a berth like any other, and a run opens with none
        // spare: the freighter rule reaches a rebuild too.
        run.bank.materials = REBUILD_COST;
        run.bank.data = REBUILD_DATA;
        assert!(!Buy::Rebuild(77).take(&mut run));
        run.escorts -= 1;
        assert!(Buy::Rebuild(77).take(&mut run));
        assert_eq!(run.escorts, 1, "it flies again");
        assert_eq!(run.bank.materials, 0);
        assert_eq!(run.bank.data, 0);
        // And the wreck is spent: a rebuild that could be bought twice would
        // be a fleet made out of one dead frigate.
        assert!(hulk_of(&run, 77).is_none());
        assert!(!Buy::Rebuild(77).take(&mut run));
        assert!(!offers(&run, &f).contains(&Buy::Rebuild(77)));
    }

    #[test]
    fn a_run_only_moves_along_a_branch_it_is_standing_on() {
        let mut run = RunState::new(0xBEEF, "terran_frigate");
        let was = run.at;
        let branches = run.node().next.clone();
        assert!(!branches.is_empty(), "a start node leads somewhere");
        // Somewhere it cannot reach from here leaves it where it is.
        let far = (0..u16::MAX).find(|n| !branches.contains(n) && *n != was);
        run.go(far.expect("a node it cannot reach"));
        assert_eq!(run.at, was);
        run.go(branches[0]);
        assert_eq!(run.at, branches[0]);
    }
}
