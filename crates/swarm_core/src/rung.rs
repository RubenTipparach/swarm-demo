//! What a class's own KEY gives it: which rung it stands on, which list it is
//! offered under, and what it can carry.
//!
//! Split off `build` the day that file crossed this project's own nine hundred
//! lines, along the line its module doc already drew: this answers what a class
//! IS and `build` answers what a yard DOES with one. `build` re-exports every
//! name here, so nothing outside the crate learned a new path, which is the
//! same answer `damage` gave when `heat` and `wound` came out of it.

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
}
