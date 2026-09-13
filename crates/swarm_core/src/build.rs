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
//! where a finished ship appears. It answers what a class IS, what it costs and
//! how far along a job has got, and the app does the spawning.

use crate::economy::{Yield, DATA_CUBE, ORE_CUBE};

/// What rung of a ladder a class stands on, read off its own key.
///
/// The manifest's `rung` is the CELL SIZE (frigate, escort, cruiser) and not
/// the class tier: a corvette is a short profile at the frigate's cell, so a
/// hull that reports `frigate` there may be either. The key is what carries
/// the tier, which is why this parses the key exactly as the jump price has
/// always done rather than reading the manifest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Corvette,
    Frigate,
    Destroyer,
    Cruiser,
    /// A civil yard's trade. It does not stand on a ladder, and it is priced
    /// as a frigate because that is what the jump has always charged for one.
    Trade,
}

impl Tier {
    /// The tier a class key names.
    ///
    /// A key with no underscore (`freighter`) and a suffix that is not a rung
    /// (`civil_lighter`) are both trades, which is the same answer the jump
    /// price's own default has always given them.
    pub fn of(class: &str) -> Tier {
        match class.rsplit_once('_').map(|(_, rung)| rung) {
            Some("corvette") => Tier::Corvette,
            Some("frigate") => Tier::Frigate,
            Some("destroyer") => Tier::Destroyer,
            Some("cruiser") => Tier::Cruiser,
            _ => Tier::Trade,
        }
    }

    /// What it costs to carry one hull of this tier out of a system.
    ///
    /// This is the ladder the jump drive has always priced by, and it lives
    /// here now because it is a rule about a CLASS and this is the crate rules
    /// live in. `retreat::jump` asks for it rather than keeping a second copy:
    /// two implementations of one ladder is exactly the divergent path this
    /// project's rules warn about.
    pub fn jump_cost(self) -> u32 {
        match self {
            Tier::Corvette => 25,
            Tier::Frigate | Tier::Trade => 50,
            Tier::Destroyer => 100,
            Tier::Cruiser => 200,
        }
    }

    /// And what it costs to MAKE one, which is ten times that.
    ///
    /// One number rather than a second table, and the sentence a player can
    /// hold is that a ship costs ten times to build what it costs to take with
    /// you. It also lands a frigate at 500 exactly, which is the price the
    /// approved mockup draws against its own frigate row.
    pub fn build_cost(self) -> u32 {
        self.jump_cost() * BUILD_FACTOR
    }
}

/// How many times a hull's jump price it costs to build one. See
/// `Tier::build_cost`.
pub const BUILD_FACTOR: u32 = 10;

/// What a fighter costs, which is a tenth of a corvette.
///
/// A fighter is not a hull off the manifest: the squadron is a dozen entities
/// sharing one mesh, so what is bought is a place in the wing rather than a
/// ship. Priced against the smallest thing that IS a hull, so the two are
/// comparable.
pub const FIGHTER_COST: u32 = 25;

/// Materials a yard turns into progress each second, before any module.
///
/// Ten, so a frigate is fifty seconds and a system of six minutes buys about
/// six of them if nothing else is spent. That is what makes a build order a
/// decision against the drive rather than a thing you do because you can.
pub const BUILD_RATE: f32 = 10.0;

/// The six buttons the HUD offers, which are the six marks already baked.
///
/// Five fall straight out of the fleet and are read off a class key rather
/// than typed, so a class added tomorrow is in its category tomorrow. The
/// sixth has no hull behind it and `Category::of` never answers it: a platform
/// is spawned from a corvette hull that is denied its flight, so what makes it
/// a platform is how the app spawns it and not which file it came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Fighter,
    Corvette,
    Frigate,
    Capital,
    Utility,
    Platform,
}

impl Category {
    /// Which category a class key belongs to.
    ///
    /// Destroyer and cruiser share CAPITAL, because the mockup draws six
    /// buttons and the fleet has four warship rungs: the two heavy ones are
    /// one decision at the point of ordering and stay two ships afterwards.
    pub fn of(class: &str) -> Category {
        match Tier::of(class) {
            Tier::Corvette => Category::Corvette,
            Tier::Frigate => Category::Frigate,
            Tier::Destroyer | Tier::Cruiser => Category::Capital,
            Tier::Trade => Category::Utility,
        }
    }

    /// Its name on the button.
    pub fn label(self) -> &'static str {
        match self {
            Category::Fighter => "fighter",
            Category::Corvette => "corvette",
            Category::Frigate => "frigate",
            Category::Capital => "capital",
            Category::Utility => "utility",
            Category::Platform => "platform",
        }
    }

    /// Every category, in the order the panel lays them out.
    pub const ALL: [Category; 6] = [
        Category::Fighter,
        Category::Corvette,
        Category::Frigate,
        Category::Capital,
        Category::Utility,
        Category::Platform,
    ];
}

/// What a hull can carry, which is its rung and nothing else.
///
/// Three counters, exactly the three the mockup draws: how many jobs run at
/// once, how many modules are fitted, and the one slot that changes what you
/// can see. Derived from the tier, so no table anywhere lists twenty three
/// ships and a class added tomorrow has slots tomorrow.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Slots {
    pub production: u32,
    pub module: u32,
    pub sensors: u32,
}

impl Slots {
    /// The slots a class starts with, before anything is fitted.
    ///
    /// A civil trade gets one production slot and a frigate's module bay,
    /// because a hold is a module bay: it is the one thing a trade has more of
    /// than a warship of its own price.
    pub fn of(class: &str) -> Slots {
        match Tier::of(class) {
            Tier::Corvette => Slots {
                production: 1,
                module: 1,
                sensors: 0,
            },
            Tier::Frigate => Slots {
                production: 2,
                module: 2,
                sensors: 1,
            },
            Tier::Destroyer => Slots {
                production: 3,
                module: 3,
                sensors: 1,
            },
            Tier::Cruiser => Slots {
                production: 4,
                module: 4,
                sensors: 1,
            },
            Tier::Trade => Slots {
                production: 1,
                module: 2,
                sensors: 1,
            },
        }
    }
}

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
    fn a_tier_is_read_off_the_key_and_a_trade_is_the_default() {
        assert_eq!(Tier::of("terran_corvette"), Tier::Corvette);
        assert_eq!(Tier::of("karisen_frigate"), Tier::Frigate);
        assert_eq!(Tier::of("rogue_destroyer"), Tier::Destroyer);
        assert_eq!(Tier::of("benefactor_cruiser"), Tier::Cruiser);
        // The manifest's own two shapes of civil key, and neither has a rung
        // suffix: one has no underscore at all.
        assert_eq!(Tier::of("freighter"), Tier::Trade);
        assert_eq!(Tier::of("civil_lighter"), Tier::Trade);
        assert_eq!(Tier::of("civil_miner"), Tier::Trade);
    }

    /// The ladder the jump has always charged, held here so moving it into the
    /// core cannot have changed a price.
    #[test]
    fn the_jump_ladder_is_the_one_the_drive_already_charged() {
        assert_eq!(Tier::of("terran_corvette").jump_cost(), 25);
        assert_eq!(Tier::of("terran_frigate").jump_cost(), 50);
        assert_eq!(Tier::of("civil_miner").jump_cost(), 50);
        assert_eq!(Tier::of("terran_destroyer").jump_cost(), 100);
        assert_eq!(Tier::of("terran_cruiser").jump_cost(), 200);
    }

    #[test]
    fn building_costs_ten_times_carrying_out_and_a_frigate_lands_at_500() {
        for key in [
            "terran_corvette",
            "terran_frigate",
            "civil_miner",
            "terran_destroyer",
            "terran_cruiser",
        ] {
            let t = Tier::of(key);
            assert_eq!(t.build_cost(), t.jump_cost() * 10, "{key}");
        }
        // The approved mockup's own frigate row.
        assert_eq!(Tier::of("terran_frigate").build_cost(), 500);
    }

    #[test]
    fn a_category_is_read_off_the_key_and_the_heavies_share_one() {
        assert_eq!(Category::of("rogue_corvette"), Category::Corvette);
        assert_eq!(Category::of("rogue_frigate"), Category::Frigate);
        assert_eq!(Category::of("rogue_destroyer"), Category::Capital);
        assert_eq!(Category::of("rogue_cruiser"), Category::Capital);
        assert_eq!(Category::of("civil_tanker"), Category::Utility);
        assert_eq!(Category::of("freighter"), Category::Utility);
        // Six buttons, and every one has a name.
        assert_eq!(Category::ALL.len(), 6);
        for c in Category::ALL {
            assert!(!c.label().is_empty());
        }
    }

    /// A platform is how it is SPAWNED, not which file it came from, so no key
    /// ever categorises as one.
    #[test]
    fn no_hull_is_a_platform_by_its_key() {
        for key in [
            "terran_corvette",
            "terran_cruiser",
            "civil_boxship",
            "freighter",
        ] {
            assert_ne!(Category::of(key), Category::Platform);
            assert_ne!(Category::of(key), Category::Fighter);
        }
    }

    #[test]
    fn slots_climb_the_ladder_and_a_corvette_has_no_sensor_bay() {
        let corv = Slots::of("terran_corvette");
        let cru = Slots::of("terran_cruiser");
        assert_eq!(corv.sensors, 0, "a needle has nowhere to put one");
        assert_eq!(cru.production, 4, "the mockup's own 0 of 4");
        for (a, b) in [
            ("terran_corvette", "terran_frigate"),
            ("terran_frigate", "terran_destroyer"),
            ("terran_destroyer", "terran_cruiser"),
        ] {
            assert!(
                Slots::of(a).production < Slots::of(b).production,
                "{a} should carry less than {b}"
            );
        }
        // A hold is a module bay: a trade carries a frigate's.
        assert_eq!(
            Slots::of("civil_miner").module,
            Slots::of("terran_frigate").module
        );
    }

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
