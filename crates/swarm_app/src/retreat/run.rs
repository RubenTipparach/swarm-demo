//! A run: where you are on the map, what you are carrying, and which ships
//! are still with you.
//!
//! Everything in here survives a system, which is what makes it a run rather
//! than a series of skirmishes. What does not survive is anything that was
//! left outside the jump field.

use crate::*;

#[derive(Resource, Clone)]
pub(crate) struct RunState {
    pub(crate) seed: u64,
    pub(crate) map: Map,
    /// Which node the fleet is in.
    pub(crate) at: u16,
    /// What the last jump banked, which is what the next system starts with.
    pub(crate) bank: Bank,
    /// The support ships that made it through the field.
    pub(crate) fleet: Vec<Role>,
    /// How many systems have been left behind.
    pub(crate) systems: u32,
    pub(crate) flagship: String,
    /// How many escorts are flying with the flagship.
    pub(crate) escorts: u32,
}

impl Default for RunState {
    fn default() -> Self {
        RunState::new(0xB0A7, "terran_cruiser")
    }
}

impl RunState {
    pub(crate) fn new(seed: u64, flagship: &str) -> RunState {
        RunState {
            seed,
            map: map::generate(seed, ACTS, DEPTH),
            at: 0,
            // The command ship arrives with its own tank part full, which is
            // what stops one bad minute ending a good run: enough for one
            // hop, and never enough for two.
            bank: Bank {
                fuel: RESERVE,
                ..Bank::default()
            },
            // What a run opens with: one of each of the two ships that make
            // the loop work at all. Everything else is bought.
            fleet: vec![Role::Miner, Role::Tanker],
            systems: 0,
            flagship: flagship.into(),
            escorts: 1,
        }
    }

    pub(crate) fn node(&self) -> &MapNode {
        self.map.node(self.at)
    }

    /// The gate is the last node of the map, so a run standing on one has
    /// nowhere left to jump to and is over.
    pub(crate) fn done(&self) -> bool {
        self.node().next.is_empty()
    }

    /// Move on after a jump.
    ///
    /// The FIRST branch, until there is a map screen to pick one on. That
    /// keeps the run playable end to end now and is exactly the line the map
    /// screen replaces: the choice is which of `next` this takes, and every
    /// path crosses every act whichever it is.
    pub(crate) fn advance(&mut self) {
        if let Some(&to) = self.node().next.first() {
            self.at = to;
        }
    }

    /// What this run is, for the log. A run is a function of its seed, so
    /// this is what somebody reproducing a picture needs.
    pub(crate) fn brief(&self) -> String {
        let node = self.node();
        format!(
            "run {:#x}: system {} of {}, act {} node {}, {}",
            self.seed,
            self.systems + 1,
            self.map.nodes.len(),
            node.act + 1,
            self.at,
            node.tag.label()
        )
    }

    /// Write the system this node is into the scene the field is built from.
    /// Everything a node means is here: what is in the field, how long the
    /// tide gives you, and which ships you arrive with.
    pub(crate) fn write(&self, scene: &mut SceneSpec) {
        let node = self.node();
        scene.retreat = true;
        scene.seed = node.seed;
        scene.hull = self.flagship.clone();
        scene.reinforce = self.escorts;
        scene.support = self.fleet.clone();
        scene.lean = node.tag.leans();
        scene.tide = Tide {
            carriers: node.tag.carriers(BASE_CARRIERS),
            ..Tide::default()
        };
        scene.hives = scene.tide.carriers;
        // A start system is thin on purpose and a nest is thick: the rocks
        // are what a player is there for, so the tag moves them too.
        scene.rocks = match node.tag {
            Tag::Start => 8,
            Tag::Quiet => 8,
            Tag::Nest | Tag::Gate => 10,
            _ => 14,
        };
        scene.fighters = 8;
        scene.chewers = 40;
        scene.launch_delay = 4.0;
    }
}

/// Three acts of three systems, which is the run the design proposes.
pub(crate) const ACTS: u8 = 3;
pub(crate) const DEPTH: u8 = 3;

/// How many carriers an ordinary system's tide ends with.
pub(crate) const BASE_CARRIERS: usize = 5;
