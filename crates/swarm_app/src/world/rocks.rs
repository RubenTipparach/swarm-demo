//! The asteroid field: what a rock is built on and how far everything holds
//! off one.

use crate::*;

/// An asteroid: a voxel model with a damage grid, which is to say a `Hull`
/// that nobody flies.
///
/// It was drawn and navigated round and nothing else until a miner needed to
/// cut one. Going through the ship builder is what gives it bricks, so a
/// shaft re-meshes eight cells on a side rather than the whole rock, and a
/// cut face that glows and cools on the same ramp a wound does. It is born
/// `inert`, so everything that skips a dead hull skips it: the swarm's
/// targets, the guns, the bars, the orders and the reactor rule.
#[derive(Component)]
pub(crate) struct Rock {
    pub(crate) flavour: Flavour,
    /// Seam cells STILL in it, counted down as a cutter takes them. The rock
    /// is what knows this: a readout that walked every cell of every rock to
    /// answer "is there anything left in this system" would be asking a
    /// hundred and fifty thousand cells a frame for a number that changes
    /// once every ten ticks.
    pub(crate) seam: u32,
}

/// Which flavour the nth rock of a field is.
///
/// The field LEANS and never commits: two in three go the way the node says
/// and the rest go the other way, and the second rock always goes against
/// the lean, so even a field of two carries both. A system with no ice is a
/// system that strands a run, and that is a map rule this keeps on the
/// field's side as well.
pub(crate) fn flavour_at(n: usize, lean: Flavour) -> Flavour {
    let other = match lean {
        Flavour::Ice => Flavour::Ore,
        _ => Flavour::Ice,
    };
    if n == 1 || n % 3 == 2 {
        other
    } else {
        lean
    }
}

/// The asteroid field: how many cells a rock is built on, what one of those
/// cells is worth against the ship's own radius, where the belt sits in hull
/// radii, and how much of a rock's own radius the navigation sphere fills.
pub(crate) const ROCK_LATTICE: usize = 22;

pub(crate) const ROCK_CELL: f32 = 0.16;

pub(crate) const ROCK_NEAR: f32 = 5.0;

pub(crate) const ROCK_FAR: f32 = 13.0;

/// Against the VOLUME radius, which is already the rock's mean size, so this
/// is a small trim rather than the deep inset a bounding sphere needed. A
/// sphere that CONTAINED a lumpy rock stands the swarm off well clear of the
/// thin axes, and a cloud swerving round empty space is worse than one
/// clipping a corner.
pub(crate) const ROCK_HULL: f32 = 0.95;

/// How far off a rock a SHIP holds, in its own radii, on top of the rock's.
pub(crate) const ROCK_CLEAR: f32 = 1.6;
