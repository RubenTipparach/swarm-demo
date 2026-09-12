//! The clock every system is played against: the swarm arrives in three
//! phases, and the last one is the one you are supposed to have left before.
//!
//! Nothing here spawns anything. It answers one question, how much of the
//! swarm is out at tick N, and the app builds that many carriers. Keeping it
//! a pure function of the tick is what makes a system a function of its
//! frame count under `--fixed-dt`, the same property every headless picture
//! in this project rests on.

/// Which part of the tide a system is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// A trickle, one carrier, far off. Mine freely.
    Probes,
    /// Carriers launching properly, the cloud building. Support ships need
    /// escort now.
    Swarm,
    /// The rest of the fleet, all at once. Staying is a decision.
    Fleet,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Probes => "probes",
            Phase::Swarm => "swarm",
            Phase::Fleet => "FLEET",
        }
    }
}

/// How long each phase lasts, in ticks at sixty a second.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tide {
    pub probes: u32,
    pub swarm: u32,
    /// How many carriers the system holds once the fleet is in.
    pub carriers: usize,
}

impl Default for Tide {
    fn default() -> Self {
        // Two minutes and four, which is the shape the design proposes and
        // the number every price is tuned against.
        Tide {
            probes: 2 * 60 * 60,
            swarm: 4 * 60 * 60,
            carriers: 6,
        }
    }
}

impl Tide {
    pub fn phase_at(&self, tick: u32) -> Phase {
        if tick < self.probes {
            Phase::Probes
        } else if tick < self.probes + self.swarm {
            Phase::Swarm
        } else {
            Phase::Fleet
        }
    }

    /// Ticks until the fleet arrives, and nought once it has.
    pub fn until_fleet(&self, tick: u32) -> u32 {
        (self.probes + self.swarm).saturating_sub(tick)
    }

    /// How many carriers should be in the system at this tick.
    ///
    /// One through the probes, a ramp through the swarm, everything once the
    /// fleet is in. It never goes DOWN, because a carrier that arrived is in
    /// the system whatever the clock says next: the app spawns the
    /// difference and the ones it spawned stay until something kills them.
    pub fn carriers_at(&self, tick: u32) -> usize {
        let all = self.carriers.max(1);
        match self.phase_at(tick) {
            Phase::Probes => 1,
            Phase::Swarm => {
                let t = (tick - self.probes) as f32 / self.swarm.max(1) as f32;
                // Up to half the fleet over the middle phase, so the last
                // half arriving at once is what the word FLEET means.
                let half = (all as f32 / 2.0).ceil();
                1 + (t * (half - 1.0).max(0.0)).floor() as usize
            }
            // And it does not stop. A system a fleet could hold for ever is
            // a system worth farming, so the swarm keeps sending: one more
            // carrier every `SIEGE` after the fleet is in, with no ceiling.
            // Staying is always possible and always gets worse, which is
            // what makes leaving a judgement rather than a rule.
            Phase::Fleet => {
                let over = tick - self.probes - self.swarm;
                all + (over / SIEGE) as usize
            }
        }
    }
}

/// How long the swarm takes to send one more carrier once its fleet is in,
/// in ticks. A minute, which is long enough that holding the ground you have
/// is worth doing and short enough that nobody holds it twice over.
pub const SIEGE: u32 = 60 * 60;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_swarm_keeps_coming_once_its_fleet_is_in() {
        let t = Tide::default();
        let fleet = t.probes + t.swarm;
        assert_eq!(t.carriers_at(fleet), t.carriers, "the fleet, and then more");
        assert_eq!(t.carriers_at(fleet + SIEGE - 1), t.carriers);
        assert_eq!(t.carriers_at(fleet + SIEGE), t.carriers + 1);
        assert_eq!(t.carriers_at(fleet + SIEGE * 9), t.carriers + 9);
        // Which is the point: there is no number of them a player can hold
        // for ever, so staying is a judgement rather than a rule.
        assert!(t.carriers_at(fleet + SIEGE * 40) > t.carriers * 4);
    }

    #[test]
    fn the_three_phases_come_in_order_and_the_last_one_lasts() {
        let t = Tide::default();
        assert_eq!(t.phase_at(0), Phase::Probes);
        assert_eq!(t.phase_at(t.probes - 1), Phase::Probes);
        assert_eq!(t.phase_at(t.probes), Phase::Swarm);
        assert_eq!(t.phase_at(t.probes + t.swarm - 1), Phase::Swarm);
        assert_eq!(t.phase_at(t.probes + t.swarm), Phase::Fleet);
        assert_eq!(t.phase_at(u32::MAX), Phase::Fleet);
    }

    #[test]
    fn the_swarm_only_ever_grows_and_ends_whole() {
        let t = Tide::default();
        let mut last = 0;
        for tick in (0..t.probes + t.swarm + 600).step_by(7) {
            let n = t.carriers_at(tick);
            assert!(n >= last, "carriers went down at {tick}: {last} then {n}");
            assert!(n <= t.carriers);
            last = n;
        }
        assert_eq!(t.carriers_at(0), 1);
        assert_eq!(t.carriers_at(t.probes + t.swarm), t.carriers);
        // Half the fleet is still outside when the middle phase ends, which
        // is what makes the fleet's arrival an event rather than a slope.
        let before = t.carriers_at(t.probes + t.swarm - 1);
        assert!(before <= t.carriers / 2 + 1, "{before} of {}", t.carriers);
    }

    #[test]
    fn the_countdown_reaches_nought_exactly_when_the_fleet_does() {
        let t = Tide::default();
        assert_eq!(t.until_fleet(0), t.probes + t.swarm);
        assert_eq!(t.until_fleet(t.probes + t.swarm), 0);
        assert_eq!(t.until_fleet(u32::MAX), 0);
    }

    #[test]
    fn one_carrier_is_a_legal_tide() {
        let t = Tide {
            carriers: 1,
            ..Tide::default()
        };
        // One all the way to the fleet: the ramp has nothing to ramp.
        assert_eq!(t.carriers_at(0), 1);
        assert_eq!(t.carriers_at(t.probes + t.swarm), 1);
        // And then the siege, which is every system's rule whatever it
        // started with.
        assert_eq!(t.carriers_at(t.probes + t.swarm + SIEGE), 2);
    }
}
