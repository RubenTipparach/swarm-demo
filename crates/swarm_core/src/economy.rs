//! What a cut is worth: the one place that says what a cell yields when it
//! is taken off something.
//!
//! Mining, salvage and the swarm's bite are the same verb, so the yield has
//! to be the thing that tells them apart, and it reads the same fact a wound
//! already draws: every cell knows what it is made of and what it was for.
//! Plating is materials wherever it came from; a drive bell is data because
//! what is worth taking off a drive bell is how it was built; an ore seam is
//! ore, the crystal grown against it is fuel, and the stone round both is
//! spoil.
//!
//! Three currencies, because three is what the ships imply and what a player
//! can hold in their head: materials repair and build, volatiles refine into
//! the jump fuel that is the only way out of a system, and data is research.
//!
//! And they are carried as CUBES. A cut takes cells off, a run of cells of
//! one kind packs into a cube, and the cube is a thing in the world that
//! comes off the rock and rides home on the ship that cut it. That is what
//! makes a hold a hold rather than a number going up: a miner that dies on
//! the way back is carrying something a player can see it lose.

use crate::voxel::{mat, purpose};

/// What one cut produced. Counted in cells, so a number here is a thing that
/// was physically taken off something and not a score.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Yield {
    pub materials: u32,
    pub volatiles: u32,
    pub data: u32,
}

impl Yield {
    pub const NOTHING: Yield = Yield {
        materials: 0,
        volatiles: 0,
        data: 0,
    };

    /// Everything this cut produced, of any kind.
    pub fn total(&self) -> u32 {
        self.materials + self.volatiles + self.data
    }

    pub fn is_nothing(&self) -> bool {
        self.total() == 0
    }
}

impl std::ops::AddAssign for Yield {
    fn add_assign(&mut self, o: Yield) {
        self.materials += o.materials;
        self.volatiles += o.volatiles;
        self.data += o.data;
    }
}

/// What is being cut. A rock's seams are worth what they are MADE of; a
/// hull's cells are worth what they were FOR.
///
/// The rock arm carries nothing, and that is the point of growing crystal
/// beside the ore rather than giving it rocks of its own: the cell says what
/// it is worth, so nothing upstream has to hand down a flavour and no cutter
/// can be told the wrong one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    Rock,
    /// Anything that was a ship: your own wrecks, a derelict, a carrier.
    Hull,
}

/// What one cell yields.
///
/// A rock gives up only its seams: the stone is spoil, which is what makes a
/// cut a SHAFT rather than a scrape, because the seams are buried under it.
/// A hull gives materials for its plating and structure and data for its
/// machinery, so a wreck is worth more for what it was than for what it
/// weighed, and a carrier is worth cutting at all.
pub fn yield_of(cut: Cut, material: u8, purp: u8) -> Yield {
    match cut {
        Cut::Rock => rock_cell(material),
        Cut::Hull => hull_cell(material, purp),
    }
}

/// Ore is metal and crystal is fuel, and a rock carries both because the
/// crystal grows on the ore. Everything else in there is stone.
fn rock_cell(material: u8) -> Yield {
    match material {
        mat::ACCENT => Yield {
            materials: 1,
            ..Yield::NOTHING
        },
        mat::GLOW => Yield {
            volatiles: 1,
            ..Yield::NOTHING
        },
        _ => Yield::NOTHING,
    }
}

fn hull_cell(material: u8, purp: u8) -> Yield {
    // Machinery first, because what a cell was FOR outranks what it is made
    // of: a gun's casing is worth cutting for the gun, not for the steel.
    let works = matches!(
        purp,
        purpose::PROPULSION | purpose::GUN | purpose::ORDNANCE | purpose::COMMAND
    );
    if works || matches!(material, mat::MACHINE | mat::GLOW) {
        return Yield {
            data: 1,
            ..Yield::NOTHING
        };
    }
    if mat::is_armour(material) || matches!(material, mat::FRAME | mat::CASE | mat::ACCENT) {
        return Yield {
            materials: 1,
            ..Yield::NOTHING
        };
    }
    Yield::NOTHING
}

/// How many cells of one seam pack into a CUBE.
///
/// The unit a harvester carries, and therefore the grain of the whole
/// economy: a rock holds about thirty cells of each seam, so it gives up
/// three or four cubes of each and a trip home is worth making. One cell a
/// cube would be a hundred entities a rock and a hold that filled on the
/// first bite.
pub const CUBE_CELLS: u32 = 8;

/// What a harvester carries: one cube of one thing.
///
/// A cube is not a share of a pile, it is a PIECE, so what it is worth is a
/// whole number written down once here. An ore cube is worth a lot and takes
/// a while to fill; a crystal cube is worth less and is what a jump is
/// counted in, so a fleet's cost reads as a number of cubes rather than as a
/// bar going down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cube {
    Ore,
    Crystal,
    /// Machinery off a dead hull, which is a salvager's own cargo.
    Data,
}

/// What each is worth when it is landed. The two the owner set are the ore
/// and the crystal; the data cube follows from what research costs, at a
/// couple of unlocks a cube.
pub const ORE_CUBE: u32 = 120;
pub const CRYSTAL_CUBE: u32 = 50;
pub const DATA_CUBE: u32 = 30;

impl Cube {
    pub fn worth(self) -> Yield {
        match self {
            Cube::Ore => Yield {
                materials: ORE_CUBE,
                ..Yield::NOTHING
            },
            Cube::Crystal => Yield {
                volatiles: CRYSTAL_CUBE,
                ..Yield::NOTHING
            },
            Cube::Data => Yield {
                data: DATA_CUBE,
                ..Yield::NOTHING
            },
        }
    }

    /// What it is drawn in: the ore's own gold, the crystal's own blue, and
    /// the green everything alive or machined in this game is lit with. The
    /// same colours the seam is, because a cube is a piece OF the seam and a
    /// player should not have to learn a second legend.
    pub fn colour(self) -> u32 {
        match self {
            Cube::Ore => 0xC8A24A,
            Cube::Crystal => 0x86E8FF,
            Cube::Data => 0x6FB835,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Cube::Ore => "ore",
            Cube::Crystal => "crystal",
            Cube::Data => "data",
        }
    }

    /// Take one cube's worth of cells out of what a cutter has loose, if it
    /// has that much of any one kind yet.
    ///
    /// The one place cells become cubes. It answers ONE cube per call rather
    /// than however many are in there, because every cube is a thing that
    /// comes off the rock and the caller is what spawns it: a function that
    /// returned three would be a caller that had to remember to spawn three.
    pub fn packed(loose: &mut Yield) -> Option<Cube> {
        if loose.materials >= CUBE_CELLS {
            loose.materials -= CUBE_CELLS;
            return Some(Cube::Ore);
        }
        if loose.volatiles >= CUBE_CELLS {
            loose.volatiles -= CUBE_CELLS;
            return Some(Cube::Crystal);
        }
        if loose.data >= CUBE_CELLS {
            loose.data -= CUBE_CELLS;
            return Some(Cube::Data);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rock::{self, Flavour};
    use crate::voxel::VoxelModel;

    /// Everything a model would give up if it were cut to nothing.
    fn whole(cut: Cut, m: &VoxelModel) -> Yield {
        let mut out = Yield::default();
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY {
                continue;
            }
            out += yield_of(cut, m.grid[n], m.purp[n]);
        }
        out
    }

    #[test]
    fn a_rock_gives_up_its_seams_and_nothing_else() {
        let m = rock::generate(22, 0.16, 700);
        let got = whole(Cut::Rock, &m);
        let ore = (0..m.len()).filter(|&n| m.grid[n] == mat::ACCENT).count() as u32;
        let crystal = (0..m.len()).filter(|&n| m.grid[n] == mat::GLOW).count() as u32;
        let solid = (0..m.len()).filter(|&n| m.grid[n] != mat::EMPTY).count() as u32;
        assert_eq!(got.materials, ore);
        assert_eq!(got.volatiles, crystal);
        assert_eq!(got.data, 0);
        assert!(ore > 0 && crystal > 0, "an ordinary rock carries both");
        // The stone is spoil: the yield is a small share of what was cut.
        assert!(got.total() < solid / 4, "{got:?} of {solid} is not a seam");
    }

    #[test]
    fn crystal_grows_on_the_ore_and_a_crystal_rock_carries_more_of_it() {
        let m = rock::generate_of(22, 0.16, 700, Flavour::Crystal);
        let plain = rock::generate_of(22, 0.16, 700, Flavour::Ore);
        let rich = whole(Cut::Rock, &m);
        let lean = whole(Cut::Rock, &plain);
        // The SAME rock: the lean moves the crystal and nothing else, so a
        // player who learns a system's asteroids does not have to learn them
        // again in the next one.
        assert_eq!(rich.materials, lean.materials, "the ore is where it was");
        assert!(
            rich.volatiles > lean.volatiles * 2,
            "{} against {}",
            rich.volatiles,
            lean.volatiles
        );
        // And every crystal touches ore, which is the rule the whole thing
        // is named for.
        for n in 0..m.len() {
            if m.grid[n] != mat::GLOW {
                continue;
            }
            let (i, j, k) = m.at(n);
            assert!(
                crate::voxel::NEIGHBOURS.iter().any(|(di, dj, dk)| {
                    let (a, b, c) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                    m.inside(a, b, c)
                        && m.grid[m.index(a as usize, b as usize, c as usize)] == mat::ACCENT
                }),
                "a crystal at {i},{j},{k} with no ore against it"
            );
        }
    }

    #[test]
    fn a_barren_rock_is_worth_nothing() {
        let m = rock::generate_of(22, 0.16, 703, Flavour::Barren);
        assert!(whole(Cut::Rock, &m).is_nothing());
        assert_eq!(
            (0..m.len())
                .filter(|&n| matches!(m.grid[n], mat::ACCENT | mat::GLOW))
                .count(),
            0,
            "a barren rock carries no seam to begin with, and so no crystal"
        );
    }

    #[test]
    fn cells_pack_into_cubes_one_at_a_time_and_the_remainder_is_kept() {
        let mut loose = Yield::NOTHING;
        assert_eq!(Cube::packed(&mut loose), None, "nothing packs nothing");
        // A cut's worth of ore, and a little crystal with it.
        loose += Yield {
            materials: CUBE_CELLS * 2 + 3,
            volatiles: CUBE_CELLS - 1,
            data: 0,
        };
        assert_eq!(Cube::packed(&mut loose), Some(Cube::Ore));
        assert_eq!(Cube::packed(&mut loose), Some(Cube::Ore));
        // The third is short, and what is left is KEPT: a cutter that lost
        // its remainder every bite would take twice as long for nothing a
        // player could see.
        assert_eq!(Cube::packed(&mut loose), None);
        assert_eq!(loose.materials, 3);
        assert_eq!(loose.volatiles, CUBE_CELLS - 1);
        loose.volatiles += 1;
        assert_eq!(Cube::packed(&mut loose), Some(Cube::Crystal));
        assert_eq!(loose.volatiles, 0);
    }

    #[test]
    fn a_rock_is_worth_a_few_cubes_of_each_and_that_is_the_grain_of_the_run() {
        let m = rock::generate(22, 0.16, 700);
        let got = whole(Cut::Rock, &m);
        let (ore, crystal) = (got.materials / CUBE_CELLS, got.volatiles / CUBE_CELLS);
        assert!(
            (3..=12).contains(&ore) && (3..=12).contains(&crystal),
            "a rock giving {ore} ore cubes and {crystal} crystal cubes is not a trip"
        );
        // And what that rock is worth landed, which is the number every price
        // in a run is set against.
        assert_eq!(ore * ORE_CUBE, Cube::Ore.worth().materials * ore);
        assert!(crystal * CRYSTAL_CUBE >= CRYSTAL_CUBE * 3);
    }

    #[test]
    fn a_hull_gives_materials_for_plate_and_data_for_machinery() {
        assert_eq!(
            yield_of(Cut::Hull, mat::PLATE, purpose::STRUCTURE).materials,
            1
        );
        assert_eq!(
            yield_of(Cut::Hull, mat::SKINNED, purpose::NONE).materials,
            1
        );
        assert_eq!(yield_of(Cut::Hull, mat::MACHINE, purpose::NONE).data, 1);
        // What it was FOR outranks what it is made of: a gun's plating is
        // worth cutting for the gun.
        assert_eq!(yield_of(Cut::Hull, mat::PLATE, purpose::GUN).data, 1);
        assert_eq!(yield_of(Cut::Hull, mat::PLATE, purpose::GUN).materials, 0);
        assert!(yield_of(Cut::Hull, mat::EMPTY, purpose::NONE).is_nothing());
    }

    #[test]
    fn a_wreck_is_worth_more_for_what_it_was_than_what_it_weighed() {
        // A frigate is mostly plating, so most of it is materials and the
        // data is the share that was machinery. Both have to be real.
        let mut m = VoxelModel::new(8, 8, 8, 0.1);
        for i in 0..8 {
            for j in 0..8 {
                for k in 0..8 {
                    m.set(i, j, k, mat::PLATE, 0x808080);
                }
            }
        }
        for k in 0..8 {
            let n = m.index(4, 4, k);
            m.grid[n] = mat::MACHINE;
            m.purp[n] = purpose::PROPULSION;
        }
        let got = whole(Cut::Hull, &m);
        assert_eq!(got.data, 8);
        assert_eq!(got.materials, 512 - 8);
        assert_eq!(got.volatiles, 0);
    }
}
