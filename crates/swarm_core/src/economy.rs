//! What a cut is worth: the one place that says what a cell yields when it
//! is taken off something.
//!
//! Mining, salvage and the swarm's bite are the same verb, so the yield has
//! to be the thing that tells them apart, and it reads the same fact a wound
//! already draws: every cell knows what it is made of and what it was for.
//! Plating is materials wherever it came from; a drive bell is data because
//! what is worth taking off a drive bell is how it was built; an ore seam is
//! ore and the stone round it is spoil.
//!
//! Three currencies, because three is what the ships imply and what a player
//! can hold in their head: materials repair and build, volatiles refine into
//! the jump fuel that is the only way out of a system, and data is research.

use crate::rock::Flavour;
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

/// What is being cut. A rock's seam is worth what the rock is made of; a
/// hull's cells are worth what they were for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    Rock(Flavour),
    /// Anything that was a ship: your own wrecks, a derelict, a carrier.
    Hull,
}

/// What one cell yields.
///
/// A rock gives up only its seam: the stone is spoil, which is what makes a
/// cut a SHAFT rather than a scrape, because the seam is buried under it.
/// A hull gives materials for its plating and structure and data for its
/// machinery, so a wreck is worth more for what it was than for what it
/// weighed, and a carrier is worth cutting at all.
pub fn yield_of(cut: Cut, material: u8, purp: u8) -> Yield {
    match cut {
        Cut::Rock(f) => rock_cell(f, material),
        Cut::Hull => hull_cell(material, purp),
    }
}

fn rock_cell(flavour: Flavour, material: u8) -> Yield {
    if material != mat::ACCENT {
        return Yield::NOTHING;
    }
    match flavour {
        Flavour::Ore => Yield {
            materials: 1,
            ..Yield::NOTHING
        },
        Flavour::Ice => Yield {
            volatiles: 1,
            ..Yield::NOTHING
        },
        Flavour::Barren => Yield::NOTHING,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rock;
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
    fn a_rock_gives_up_its_seam_and_nothing_else() {
        let m = rock::generate(22, 0.16, 700);
        let got = whole(Cut::Rock(Flavour::Ore), &m);
        let seam = (0..m.len()).filter(|&n| m.grid[n] == mat::ACCENT).count() as u32;
        let solid = (0..m.len()).filter(|&n| m.grid[n] != mat::EMPTY).count() as u32;
        assert_eq!(got.materials, seam);
        assert_eq!(got.volatiles, 0);
        assert!(
            seam > 0 && seam < solid / 4,
            "{seam} of {solid} is not a seam"
        );
        // The stone is spoil: the yield is a small share of what was cut.
        assert!(got.total() < solid / 4);
    }

    #[test]
    fn an_ice_rock_gives_volatiles_from_the_same_cells() {
        let m = rock::generate_of(22, 0.16, 700, Flavour::Ice);
        let ore = whole(Cut::Rock(Flavour::Ore), &m);
        let ice = whole(Cut::Rock(Flavour::Ice), &m);
        assert_eq!(ice.volatiles, ore.materials);
        assert_eq!(ice.materials, 0);
        assert!(ice.volatiles > 0);
    }

    #[test]
    fn a_barren_rock_is_worth_nothing() {
        let m = rock::generate_of(22, 0.16, 703, Flavour::Barren);
        assert!(whole(Cut::Rock(Flavour::Barren), &m).is_nothing());
        assert_eq!(
            (0..m.len()).filter(|&n| m.grid[n] == mat::ACCENT).count(),
            0,
            "a barren rock carries no seam to begin with"
        );
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
