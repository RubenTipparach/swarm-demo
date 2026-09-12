//! The Long Retreat: what a system is worth, who gathers it, and the drive
//! that takes you out before the swarm's fleet arrives.
//!
//! The rules are the core's (`swarm_core::economy`, `tide`, `map`). This is
//! the ships that act on them: what a support ship is, what it is carrying,
//! what job it has been given, and the bank that everything it cuts ends up
//! in.

mod cargo;
mod jump;
mod refit;
mod run;
mod salvage;
mod tender;
mod tide;
mod work;

pub(crate) use cargo::*;
pub(crate) use jump::*;
pub(crate) use refit::*;
pub(crate) use run::*;
pub(crate) use salvage::*;
pub(crate) use tender::*;
pub(crate) use tide::*;
pub(crate) use work::*;

use crate::*;

/// What the fleet has gathered, and the only numbers a refit screen spends.
///
/// Volatiles and fuel are two things on purpose: what a miner cuts out of an
/// ice rock is not fuel yet, and the tanker turning one into the other over
/// time is what makes sending the miner to the ice EARLY worth doing. A pile
/// of volatiles with no tanker alive is a pile of rock.
#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct Bank {
    pub(crate) materials: u32,
    pub(crate) volatiles: u32,
    pub(crate) fuel: f32,
    pub(crate) data: u32,
}

impl Bank {
    pub(crate) fn take(&mut self, got: Yield) {
        self.materials += got.materials;
        self.volatiles += got.volatiles;
        self.data += got.data;
    }
}

/// What a support ship is for. One role is one job it knows how to do, and
/// the hull that draws it is the civil trade that would actually do it.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Role {
    Miner,
    Tanker,
    Salvager,
    Survey,
    Freighter,
    Tender,
}

impl Role {
    /// The hull a role flies, which is a trade already in the fleet.
    pub(crate) fn hull(self) -> &'static str {
        match self {
            Role::Miner => "civil_miner",
            Role::Tanker => "civil_tanker",
            Role::Salvager => "civil_hauler",
            Role::Survey => "civil_lighter",
            Role::Freighter => "freighter",
            Role::Tender => "civil_liner",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Role::Miner => "miner",
            Role::Tanker => "tanker",
            Role::Salvager => "salvager",
            Role::Survey => "survey ship",
            Role::Freighter => "freighter",
            Role::Tender => "tender",
        }
    }

    /// Whether this role can work the thing under the cursor. A miner cuts
    /// rocks, a salvager cuts hulls, a survey ship SCANS either and takes
    /// nothing off it, and none of them does another's job, which is what
    /// makes losing one of them hurt.
    pub(crate) fn cuts_rock(self) -> bool {
        matches!(self, Role::Miner | Role::Survey)
    }

    pub(crate) fn cuts_wreck(self) -> bool {
        matches!(self, Role::Salvager | Role::Survey)
    }

    /// A survey ship reads a body rather than cutting it: the same job, the
    /// same standoff and the same trip home, and what it fills its hold with
    /// is DATA. A body can only be learned once, which is what stops a run
    /// parking one survey ship on one rock for ever.
    pub(crate) fn scans(self) -> bool {
        matches!(self, Role::Survey)
    }

    /// What a tender does, which is the only thing in this game that mends
    /// rather than breaks.
    pub(crate) fn mends(self) -> bool {
        matches!(self, Role::Tender)
    }

    pub(crate) fn parse(s: &str) -> Option<Role> {
        [
            Role::Miner,
            Role::Tanker,
            Role::Salvager,
            Role::Survey,
            Role::Freighter,
            Role::Tender,
        ]
        .into_iter()
        .find(|r| r.label().split(' ').next() == Some(s) || r.hull() == s)
    }
}

/// A support ship, and nothing else about it: the role is what it does and
/// the `Hull` beneath it is every other thing a ship is.
#[derive(Component)]
pub(crate) struct Support {
    pub(crate) role: Role,
    /// Where it flies when it has nothing to do, in the flagship's frame.
    /// Kept here rather than worked out again when a job ends, because two
    /// places computing one station is two stations the day either moves.
    pub(crate) station: Vec3,
}

/// What a hold has in it, in CUBES.
///
/// `carrying` is what is aboard and `loose` is the cells cut since the last
/// cube popped, which is the remainder rather than cargo: a cutter that
/// dropped its part cube every bite would take twice as long for nothing
/// anybody could see. The cubes themselves are entities and are not in here,
/// because a cube is a thing in the world and this is only the count that
/// says when a hold is full.
#[derive(Component, Default)]
pub(crate) struct Hold {
    pub(crate) loose: Yield,
    pub(crate) carrying: u32,
    pub(crate) cap: u32,
}

impl Hold {
    pub(crate) fn full(&self) -> bool {
        self.carrying >= self.cap
    }

    pub(crate) fn share(&self) -> f32 {
        if self.cap == 0 {
            0.0
        } else {
            self.carrying as f32 / self.cap as f32
        }
    }
}

/// What a support ship has been told to do.
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Job {
    #[default]
    Idle,
    /// Cutting the thing it was pointed at.
    Work(Entity),
    /// Hold full: back to the command ship, then to the thing it was on.
    Unload(Option<Entity>),
}

/// How many cells of hull a support ship needs per CUBE it can carry. A
/// miner comes out at four and a freighter at more, which is what makes the
/// freighter worth building: the trip home is the cost, so the ship that
/// makes fewer of them is the ship that gathers faster.
pub(crate) const CELLS_PER_SLOT: u32 = 2200;

/// How often a cutter takes a bite, in ticks, and how deep it bores, in
/// cells. A cut is a SHAFT: the ore is buried under ninety five percent of a
/// rock's own stone, so this is the rate the shaft advances at.
pub(crate) const CUT_TICKS: u32 = 10;
pub(crate) const CUT_DEPTH: f32 = 3.0;

/// How wide a SALVAGE cut is, in cells of radius.
///
/// A salvager is not drilling for a seam, it is lifting a hulk apart, so
/// what it takes is a section rather than a column. Two cells of radius is
/// about thirty cells a cut, which at `CUT_TICKS` is two hundred a second
/// and puts a whole frigate's wreckage inside one system's clock. A bore
/// took three, which is fifty times slower and would have made every tier of
/// a rebuild unreachable while looking exactly like a salvager at work.
pub(crate) const SALVAGE_CUT: f32 = 2.0;

/// Where a cutter stands and how far it cuts, both as multiples of the
/// nearest the SHIP's own avoidance will let it get (`work_jobs` computes
/// that from the same two numbers `fly_hull` uses). Just outside, so it
/// settles rather than fighting the rock's own push, and a third again as
/// slack on the reach, because an order is arrived at approximately.
pub(crate) const WORK_STANDOFF: f32 = 1.05;
pub(crate) const WORK_REACH: f32 = 1.35;

/// What a survey ship learns per pass, in cells of data, and how much of a
/// body there is to learn: a quarter of what is IN it, which on an ordinary
/// rock is a couple of data cubes and two unlocks. A body is scanned once
/// and then known, so a survey ship's work is going and looking rather than
/// standing still.
pub(crate) const SCAN_RATE: u32 = 4;
pub(crate) const SCAN_SHARE: u32 = 4;

/// What each freighter adds to everything landed, as a share of it. The
/// freighter cuts nothing: what it does is make every other ship's trip
/// worth more, so the first one is worth building the moment two cutters are
/// working.
pub(crate) const FREIGHT_SHARE: f32 = 0.25;

/// How many ships a fleet may field beyond the command ship, and what a
/// freighter adds to that. Berths are the freighter's other job and the
/// reason a fleet cannot simply grow: a run opens able to field the three it
/// starts with, and every hull after that needs somewhere to put it.
pub(crate) const BASE_BERTHS: u32 = 3;
pub(crate) const FREIGHT_BERTHS: u32 = 2;

/// How far round a body a cutter steps when a shaft breaks through, in
/// radians. A fifth of a turn, so it reaches a fresh face in one move and
/// still works its way round rather than jumping to the far side.
pub(crate) const WORK_STEP: f32 = 1.25;

/// How near the command ship a hold has to come to be emptied, and where a
/// ship unloading is told to fly, both in the command ship's own radii.
pub(crate) const UNLOAD_REACH: f32 = 3.0;
pub(crate) const UNLOAD_STATION: Vec3 = Vec3::new(0.0, -0.6, -2.2);

/// Spawn the support ships a system starts with, in a line off the
/// flagship's quarter so a player can see what they have.
pub(crate) fn spawn_support(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    radius: f32,
) {
    for (n, &role) in scene.support.iter().enumerate() {
        let side = if n % 2 == 0 { -1.0 } else { 1.0 };
        let rank = (n / 2 + 1) as f32;
        let station = Vec3::new(side * radius * 3.0, -radius * 0.6, -rank * radius * 3.4);
        let (e, _) = spawn_hull(
            commands,
            meshes,
            materials,
            tex,
            role.hull(),
            // A third of the flagship's chewers, as an escort carries: a
            // support ship the swarm could not hurt would make escorting it
            // a decision with nothing on the other side of it.
            ShipSpec::at(Transform::from_translation(station))
                .chewers((scene.chewers / 3) as u32)
                .seed(0x5A17 ^ (n as u32).wrapping_mul(0x9E37))
                .station(station),
        );
        commands
            .entity(e)
            .insert((Support { role, station }, Job::default(), Hold::default()));
    }
}

/// A hold's capacity comes off the hull it is in, so a bigger trade carries
/// more without a table saying so. Done the frame after the spawn, because
/// the `Hull` arrives through commands and cannot be read at spawn time.
pub(crate) fn size_holds(mut q: Query<(&Hull, &mut Hold), Added<Hull>>) {
    for (hull, mut hold) in &mut q {
        if hold.cap == 0 {
            hold.cap = (hull.cells as u32 / CELLS_PER_SLOT).max(1);
        }
    }
}
