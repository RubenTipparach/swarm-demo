//! The field yard: what a ship can make, what it costs, and how long it takes.
//!
//! Every number here is a multiple of one the game already ships. The jump
//! already prices a hull by its rung and the economy already values a cube, so
//! a build price is the first times ten and a module price is a count of the
//! second. That is the whole of the design's defence: a number that cannot be
//! written as a multiple of one already shipped is a number somebody has to
//! tune for ever and nobody can argue about.
//!
//! It knows nothing about Bevy, nothing about a hull's cells and nothing about
//! where a finished ship appears. It answers what a job COSTS and how far along
//! it has got, and the app does the spawning. What a CLASS is, which is its
//! rung, its category and its slots, is `rung`, re-exported here so nothing
//! outside the crate has two paths to one answer.

use crate::economy::{Yield, DATA_CUBE, ORE_CUBE};
pub use crate::rung::*;

/// A thing bolted to a ship that moves one number some system already reads.
///
/// That is the rule the whole list is under and it is what keeps this feature
/// cheap: a module must not add a resolution path, it must change a constant
/// something is already dividing by. All six do, and the doc on each says
/// which system.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Module {
    /// One more production slot, and the queue runs half again as fast.
    Production,
    /// More room in the wing, and losses replaced faster.
    Hangar,
    /// The hull's own armour multiplier, which `DamageGrid::with_armour` takes
    /// per ship already.
    Armour,
    /// Top speed and acceleration, which is `fly_hull`'s envelope.
    Drives,
    /// How far a gun will bear, and how far the sensors view pulls back.
    Sensors,
    /// Refines volatiles into fuel without a tanker, at a third of one's rate.
    Refinery,
}

impl Module {
    /// What it costs, in the cubes a player counts in the field.
    pub fn price(self) -> Yield {
        let (ore, crystal, data) = match self {
            Module::Production => (2, 0, 0),
            Module::Hangar => (2, 0, 0),
            Module::Armour => (3, 0, 0),
            Module::Drives => (2, 0, 0),
            Module::Sensors => (1, 0, 1),
            Module::Refinery => (2, 1, 0),
        };
        Yield {
            materials: ore * ORE_CUBE,
            volatiles: crystal * crate::economy::CRYSTAL_CUBE,
            data: data * DATA_CUBE,
        }
    }

    /// Its name on the button.
    pub fn label(self) -> &'static str {
        match self {
            Module::Production => "production",
            Module::Hangar => "hangar",
            Module::Armour => "armour",
            Module::Drives => "drives",
            Module::Sensors => "sensors",
            Module::Refinery => "refinery",
        }
    }

    /// Whether it takes the SENSORS slot rather than a module slot.
    ///
    /// One rule in one place, because the panel draws two counters off it and
    /// the fit checks against two different caps.
    pub fn is_sensor(self) -> bool {
        matches!(self, Module::Sensors)
    }

    /// Every module, in the order the row lays them out.
    pub const ALL: [Module; 6] = [
        Module::Production,
        Module::Hangar,
        Module::Armour,
        Module::Drives,
        Module::Sensors,
        Module::Refinery,
    ];
}

/// What the modules on one ship add up to.
///
/// A struct of NUMBERS rather than a list of modules, because every system
/// that cares wants one figure and none of them should have to know what a
/// module is. `fly_hull` wants a speed multiplier; it does not want to walk a
/// vector asking whether any of them is a drive.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fit {
    /// Production slots on top of the hull's own.
    pub production: u32,
    /// A multiplier on `BUILD_RATE`.
    pub rate: f32,
    /// Places in the wing on top of the squadron's own cap.
    pub hangar: u32,
    /// A multiplier on the hull's armour.
    pub armour: f32,
    /// A multiplier on top speed and acceleration.
    pub drives: f32,
    /// A multiplier on how far a gun bears and how far the eye pulls back.
    pub sensors: f32,
    /// Volatiles turned into fuel a second, with no tanker in the fleet.
    pub refine: f32,
}

impl Fit {
    /// A ship with nothing fitted: every multiplier one, every bonus nought.
    pub const BARE: Fit = Fit {
        production: 0,
        rate: 1.0,
        hangar: 0,
        armour: 1.0,
        drives: 1.0,
        sensors: 1.0,
        refine: 0.0,
    };

    /// Add up what a list of modules does.
    ///
    /// Multipliers COMPOUND and bonuses add, so two drive modules are 1.25
    /// squared rather than 1.5: a second copy of a thing is worth a little
    /// less than the first, which is what stops a cruiser's four slots being
    /// four times a frigate's two.
    pub fn of(modules: &[Module]) -> Fit {
        let mut f = Fit::BARE;
        for &m in modules {
            match m {
                Module::Production => {
                    f.production += 1;
                    f.rate *= 1.5;
                }
                Module::Hangar => f.hangar += HANGAR_PLACES,
                Module::Armour => f.armour *= 1.6,
                Module::Drives => f.drives *= 1.25,
                Module::Sensors => f.sensors *= 1.4,
                Module::Refinery => f.refine += REFINE_RATE,
            }
        }
        f
    }
}

/// How many more fighters a hangar module makes room for.
pub const HANGAR_PLACES: u32 = 6;

/// Volatiles a refinery module turns into fuel each second, which is a third
/// of what a tanker manages.
pub const REFINE_RATE: f32 = 10.0 / 3.0;

/// What a job is making.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Order {
    /// A hull off the manifest, by its class key.
    Hull(String),
    /// A place in the wing.
    Fighter,
    /// A gun mount that never moves, built from a corvette hull.
    Platform(String),
    /// A module, which fits to the ship that built it.
    Fit(Module),
}

impl Order {
    /// What it costs in materials.
    ///
    /// A module's price is not only materials, and this is the materials part
    /// of it: the queue advances on materials alone, so volatiles and data are
    /// taken at the moment the job is ORDERED and never refunded by the bar.
    pub fn cost(&self) -> u32 {
        match self {
            Order::Hull(class) | Order::Platform(class) => Tier::of(class).build_cost(),
            Order::Fighter => FIGHTER_COST,
            Order::Fit(m) => m.price().materials,
        }
    }

    /// Everything it costs, including what is taken up front.
    pub fn price(&self) -> Yield {
        match self {
            Order::Fit(m) => m.price(),
            _ => Yield {
                materials: self.cost(),
                volatiles: 0,
                data: 0,
            },
        }
    }

    /// Whether it lands in a module slot rather than in the world.
    pub fn is_fit(&self) -> bool {
        matches!(self, Order::Fit(_))
    }

    /// What the build row calls it.
    ///
    /// The class key with its underscores opened out, which is the same
    /// answer the setup form and the rail already give: a name written twice
    /// is a name two screens can disagree about.
    pub fn label(&self) -> String {
        match self {
            Order::Hull(class) => class.replace('_', " "),
            Order::Platform(class) => format!("{} platform", class.replace('_', " ")),
            Order::Fighter => "fighter".into(),
            Order::Fit(m) => m.label().into(),
        }
    }

    /// Which category's list it belongs on.
    pub fn category(&self) -> Category {
        match self {
            Order::Hull(class) => Category::of(class),
            Order::Platform(_) => Category::Platform,
            Order::Fighter => Category::Fighter,
            // A module is fitted rather than ordered off the class list, so it
            // is never on one: the modules row is its own control.
            Order::Fit(_) => Category::Platform,
        }
    }
}

/// One thing being made.
///
/// A TASK rather than a job, because the app already has a `Job`: what a
/// support ship has been sent to do. Two types of one name in one codebase is
/// how `Scene` became `SceneSpec` here once already, and the newcomer is the
/// one that renames.
#[derive(Clone, PartialEq, Debug)]
pub struct Task {
    pub order: Order,
    /// Materials worth of work put in so far. A float because the rate is per
    /// second and the tick is a sixtieth: a rate in a resource counted in
    /// whole units needs somewhere to keep the remainder, and this is it.
    pub done: f32,
}

impl Task {
    /// A job just ordered, with nothing done.
    pub fn new(order: Order) -> Task {
        Task { order, done: 0.0 }
    }

    /// How far along it is, nought to one.
    pub fn progress(&self) -> f32 {
        let cost = self.order.cost().max(1) as f32;
        (self.done / cost).clamp(0.0, 1.0)
    }

    /// Whether it is finished.
    pub fn ready(&self) -> bool {
        self.done >= self.order.cost() as f32
    }

    /// What cancelling it hands back: the materials NOT yet turned into work.
    ///
    /// By the share of the bar, because that is the only honest answer: a job
    /// half done gives half back. Nothing else is refunded, so the data a
    /// module cost is spent the moment it is ordered.
    pub fn refund(&self) -> u32 {
        (self.order.cost() as f32 - self.done).max(0.0) as u32
    }
}

/// One ship's yard: what it has fitted, and what it is making.
///
/// The QUEUE is a plain `Vec` in order, and as many jobs as there are
/// production slots advance at once from the front. That is what a production
/// slot IS, rather than a second list to keep in step with the first.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Yard {
    pub queue: Vec<Task>,
    pub modules: Vec<Module>,
    pub paused: bool,
}

impl Yard {
    /// An empty yard.
    pub fn new() -> Yard {
        Yard::default()
    }

    /// What its modules add up to.
    pub fn fit(&self) -> Fit {
        Fit::of(&self.modules)
    }

    /// How many jobs it can advance at once, hull plus modules.
    pub fn slots(&self, class: &str) -> u32 {
        Slots::of(class).production + self.fit().production
    }

    /// Whether another module will fit, which is two caps rather than one.
    ///
    /// A sensor takes the sensors slot and everything else takes a module
    /// slot, so a ship with its module bay full can still take a sensor and a
    /// corvette with no sensors slot can never take one.
    pub fn room_for(&self, class: &str, m: Module) -> bool {
        let slots = Slots::of(class);
        let fitted = self
            .modules
            .iter()
            .filter(|x| x.is_sensor() == m.is_sensor());
        // The hull's own bay and nothing else: a production module opens a
        // production slot, not room for another module, so there is no fitted
        // term here. The first cut added one that was always nought, which is
        // this project's own "computed a zero two ways and added it" a second
        // time and clippy is what caught it.
        let cap = if m.is_sensor() {
            slots.sensors
        } else {
            slots.module
        };
        (fitted.count() as u32) < cap
    }

    /// Put a job on the end of the queue.
    pub fn order(&mut self, order: Order) {
        self.queue.push(Task::new(order));
    }

    /// Take a job off, and answer what it refunds.
    ///
    /// Out of range is `None` rather than a panic, because the caller is a
    /// button and a button can be pressed on a row that has just finished.
    pub fn cancel(&mut self, n: usize) -> Option<u32> {
        if n >= self.queue.len() {
            return None;
        }
        let job = self.queue.remove(n);
        Some(job.refund())
    }

    /// Advance the front of the queue by one step, and hand back whatever
    /// finished on it.
    ///
    /// Finished jobs come off in queue order and a module fits itself, because
    /// a module belongs to the ship that made it and there is nothing for the
    /// app to do with one. Everything else is handed back for the app to
    /// spawn, since where a ship appears is not a rule this crate knows.
    pub fn tick(&mut self, class: &str, dt: f32) -> Vec<Order> {
        if self.paused || dt <= 0.0 {
            return Vec::new();
        }
        let step = BUILD_RATE * self.fit().rate * dt;
        let slots = self.slots(class) as usize;
        for job in self.queue.iter_mut().take(slots) {
            job.done += step;
        }
        let mut out = Vec::new();
        let mut n = 0;
        while n < self.queue.len() {
            if self.queue[n].ready() {
                let job = self.queue.remove(n);
                match job.order {
                    Order::Fit(m) => self.modules.push(m),
                    other => out.push(other),
                }
            } else {
                n += 1;
            }
        }
        out
    }
}

/// What a yard offers under one category, off the classes it can build.
///
/// Read off the manifest rather than typed, which is this project's own open
/// for extension rule: a class added tomorrow is on the panel tomorrow, in
/// whichever category its own rung puts it, and no list anywhere names twenty
/// three ships. Two categories have no hull of their own and so are not a
/// filter over the fleet. A FIGHTER has no class behind it at all and is one
/// row whatever the fleet holds. A PLATFORM is a corvette denied its flight,
/// so it reads the corvettes' own list and says what it makes of them.
pub fn offers(classes: &[&str], cat: Category) -> Vec<Order> {
    match cat {
        Category::Fighter => vec![Order::Fighter],
        Category::Platform => classes
            .iter()
            .filter(|k| Tier::of(k) == Tier::Corvette)
            .map(|k| Order::Platform((*k).to_string()))
            .collect(),
        _ => classes
            .iter()
            .filter(|k| Category::of(k) == cat)
            .map(|k| Order::Hull((*k).to_string()))
            .collect(),
    }
}

/// What a data cube opens.
///
/// Data buys the RIGHT to buy, which is the rule the yard between systems
/// already keeps. Same resource, same sentence, so the two yards are one
/// economy rather than two.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unlock {
    /// Every rung of another navy's ladder.
    Navy(String),
    /// The next tier up for a navy already open.
    Tier(Tier),
    /// A module kind the fleet has never fitted.
    Module(Module),
    /// A civil trade, exactly as the yard between systems prices one.
    Role(String),
}

impl Unlock {
    /// What it costs in data.
    pub fn price(&self) -> u32 {
        match self {
            Unlock::Navy(_) => 4 * DATA_CUBE,
            Unlock::Tier(_) | Unlock::Module(_) => 2 * DATA_CUBE,
            Unlock::Role(_) => DATA_CUBE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_fit_changes_nothing_and_every_module_moves_one_number() {
        let bare = Fit::BARE;
        assert_eq!(Fit::of(&[]), bare);
        assert_eq!(Fit::of(&[Module::Production]).production, 1);
        assert!(Fit::of(&[Module::Production]).rate > bare.rate);
        assert_eq!(Fit::of(&[Module::Hangar]).hangar, HANGAR_PLACES);
        assert!(Fit::of(&[Module::Armour]).armour > bare.armour);
        assert!(Fit::of(&[Module::Drives]).drives > bare.drives);
        assert!(Fit::of(&[Module::Sensors]).sensors > bare.sensors);
        assert!(Fit::of(&[Module::Refinery]).refine > bare.refine);
    }

    /// Multipliers compound rather than adding, so the second copy of a thing
    /// is worth less than the first.
    #[test]
    fn two_of_a_module_are_worth_less_than_twice_one() {
        let one = Fit::of(&[Module::Drives]);
        let two = Fit::of(&[Module::Drives, Module::Drives]);
        assert!(two.drives > one.drives);
        assert!(two.drives < 2.0 * one.drives);
        // But a bonus counted in places does add, because half a fighter is
        // not a thing.
        assert_eq!(
            Fit::of(&[Module::Hangar, Module::Hangar]).hangar,
            2 * HANGAR_PLACES
        );
    }

    #[test]
    fn every_module_is_priced_in_cubes_and_the_sensor_takes_its_own_slot() {
        for m in Module::ALL {
            let p = m.price();
            assert!(!p.is_nothing(), "{} is free", m.label());
            assert_eq!(
                p.materials % ORE_CUBE,
                0,
                "{} is not whole cubes",
                m.label()
            );
            assert!(!m.label().is_empty());
        }
        assert!(Module::Sensors.is_sensor());
        for m in Module::ALL.iter().filter(|m| **m != Module::Sensors) {
            assert!(!m.is_sensor(), "{} is not a sensor", m.label());
        }
    }

    #[test]
    fn a_frigate_takes_fifty_seconds_at_the_bare_rate() {
        let mut y = Yard::new();
        y.order(Order::Hull("terran_frigate".into()));
        let mut secs = 0.0f32;
        let mut done = Vec::new();
        while done.is_empty() && secs < 600.0 {
            done = y.tick("terran_frigate", 1.0 / 60.0);
            secs += 1.0 / 60.0;
        }
        assert_eq!(done.len(), 1);
        assert!(
            (secs - 50.0).abs() < 1.0,
            "a frigate at 500 over a rate of 10 is fifty seconds, got {secs}"
        );
        assert!(y.queue.is_empty());
    }

    /// A production module is worth exactly what its doc says: one more slot
    /// and half again the rate.
    #[test]
    fn a_production_module_makes_the_queue_faster_and_wider() {
        let mut bare = Yard::new();
        let mut fitted = Yard::new();
        fitted.modules.push(Module::Production);
        for y in [&mut bare, &mut fitted] {
            for _ in 0..3 {
                y.order(Order::Hull("terran_corvette".into()));
            }
        }
        // A corvette hull has one production slot, so the bare yard advances
        // one job and the fitted one advances two.
        bare.tick("terran_corvette", 1.0);
        fitted.tick("terran_corvette", 1.0);
        assert!(
            bare.queue[1].done == 0.0,
            "a bare corvette builds one at a time"
        );
        assert!(
            fitted.queue[1].done > 0.0,
            "the module opened a second slot"
        );
        assert!(
            fitted.queue[0].done > bare.queue[0].done,
            "and made it faster"
        );
    }

    #[test]
    fn cancelling_hands_back_the_share_that_is_not_built_yet() {
        let mut y = Yard::new();
        y.order(Order::Hull("terran_frigate".into()));
        // Half of fifty seconds.
        y.tick("terran_frigate", 25.0);
        let back = y.cancel(0).expect("a job to cancel");
        assert!(
            (back as i64 - 250).abs() <= 1,
            "half a 500 job should refund about half, got {back}"
        );
        assert!(y.queue.is_empty());
        assert_eq!(y.cancel(0), None, "a row that is gone refunds nothing");
    }

    #[test]
    fn a_paused_yard_makes_nothing_and_loses_nothing() {
        let mut y = Yard::new();
        y.order(Order::Hull("terran_frigate".into()));
        y.paused = true;
        assert!(y.tick("terran_frigate", 10.0).is_empty());
        assert_eq!(y.queue[0].done, 0.0);
        y.paused = false;
        y.tick("terran_frigate", 1.0);
        assert!(y.queue[0].done > 0.0, "and it picks up where it stood");
    }

    /// A module belongs to the ship that made it, so it fits itself and the
    /// app is handed nothing.
    #[test]
    fn a_finished_module_fits_itself_and_a_finished_hull_is_handed_over() {
        let mut y = Yard::new();
        y.order(Order::Fit(Module::Drives));
        y.order(Order::Hull("terran_corvette".into()));
        // A cruiser has four production slots, so both advance together: the
        // module is 240 materials and the corvette 250, which is half a minute
        // at the bare rate. Ticked for a minute so neither is cut short.
        let mut out = Vec::new();
        for _ in 0..3600 {
            out.extend(y.tick("terran_cruiser", 1.0 / 60.0));
        }
        assert_eq!(y.modules, vec![Module::Drives], "the module fitted itself");
        assert_eq!(
            out,
            vec![Order::Hull("terran_corvette".into())],
            "and only the hull came back"
        );
        assert!(y.queue.is_empty());
    }

    #[test]
    fn a_module_is_refused_once_its_own_slots_are_full() {
        let mut y = Yard::new();
        // A corvette has one module slot and no sensors slot.
        assert!(y.room_for("terran_corvette", Module::Drives));
        assert!(
            !y.room_for("terran_corvette", Module::Sensors),
            "a needle has nowhere to put a sensor"
        );
        y.modules.push(Module::Drives);
        assert!(
            !y.room_for("terran_corvette", Module::Armour),
            "the bay is full"
        );
        // And a sensor still fits a frigate with a full module bay.
        y.modules.push(Module::Armour);
        assert!(y.room_for("terran_frigate", Module::Sensors));
    }

    #[test]
    fn a_job_reports_where_it_has_got_to() {
        let mut j = Task::new(Order::Hull("terran_frigate".into()));
        assert_eq!(j.progress(), 0.0);
        assert!(!j.ready());
        j.done = 250.0;
        assert!((j.progress() - 0.5).abs() < 1e-6);
        j.done = 9999.0;
        assert!(j.ready());
        assert_eq!(j.progress(), 1.0, "and never reports past finished");
        assert_eq!(j.refund(), 0, "nor refunds more than was put in");
    }

    #[test]
    fn what_an_order_costs_is_what_its_class_costs() {
        assert_eq!(
            Order::Hull("terran_cruiser".into()).cost(),
            Tier::Cruiser.build_cost()
        );
        assert_eq!(Order::Fighter.cost(), FIGHTER_COST);
        assert_eq!(
            Order::Platform("terran_corvette".into()).cost(),
            Tier::Corvette.build_cost()
        );
        assert_eq!(
            Order::Fit(Module::Sensors).cost(),
            Module::Sensors.price().materials
        );
        // A module's data is taken up front and the bar never refunds it.
        assert!(Order::Fit(Module::Sensors).price().data > 0);
        assert_eq!(Order::Hull("terran_frigate".into()).price().data, 0);
        assert!(Order::Fit(Module::Drives).is_fit());
        assert!(!Order::Fighter.is_fit());
    }

    #[test]
    fn a_category_offers_the_classes_its_own_rung_puts_in_it() {
        const FLEET: [&str; 6] = [
            "terran_corvette",
            "terran_frigate",
            "terran_destroyer",
            "terran_cruiser",
            "karisen_frigate",
            "civil_tanker",
        ];
        // Every class the fleet holds is offered exactly once, under the
        // category its own key answers, and nothing is offered twice.
        let mut seen = Vec::new();
        for cat in Category::ALL {
            for order in offers(&FLEET, cat) {
                if let Order::Hull(class) = &order {
                    assert_eq!(Category::of(class), cat);
                    seen.push(class.clone());
                }
            }
        }
        seen.sort();
        let mut want: Vec<String> = FLEET.iter().map(|k| (*k).to_string()).collect();
        want.sort();
        assert_eq!(seen, want);
        // A destroyer and a cruiser share one button, which is the one place
        // the count is not one class per rung.
        assert_eq!(offers(&FLEET, Category::Capital).len(), 2);
        // The two with no hull behind them. A fighter is one row whatever the
        // fleet holds; a platform is every corvette in it.
        assert_eq!(offers(&FLEET, Category::Fighter), vec![Order::Fighter]);
        assert_eq!(
            offers(&FLEET, Category::Platform),
            vec![Order::Platform("terran_corvette".into())]
        );
        // An empty fleet offers no hull and still offers a fighter, or a run
        // that has lost its yard would have nothing to press at all.
        assert!(offers(&[], Category::Frigate).is_empty());
        assert_eq!(offers(&[], Category::Fighter).len(), 1);
    }

    #[test]
    fn an_order_says_what_it_is_called_and_which_list_it_is_on() {
        let hull = Order::Hull("terran_frigate".into());
        assert_eq!(hull.label(), "terran frigate");
        assert_eq!(hull.category(), Category::Frigate);
        let pad = Order::Platform("rogue_corvette".into());
        assert_eq!(pad.label(), "rogue corvette platform");
        assert_eq!(pad.category(), Category::Platform);
        assert_eq!(Order::Fighter.label(), "fighter");
        assert_eq!(Order::Fighter.category(), Category::Fighter);
        assert_eq!(Order::Fit(Module::Hangar).label(), Module::Hangar.label());
    }

    #[test]
    fn research_is_priced_in_data_and_a_navy_costs_the_most() {
        let navy = Unlock::Navy("karisen".into());
        let tier = Unlock::Tier(Tier::Cruiser);
        let role = Unlock::Role("civil_miner".into());
        assert!(navy.price() > tier.price());
        assert!(tier.price() > role.price());
        assert_eq!(role.price(), DATA_CUBE);
        assert_eq!(Unlock::Module(Module::Hangar).price(), tier.price());
    }
}
