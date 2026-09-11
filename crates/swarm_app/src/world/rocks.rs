//! The asteroid field: what a rock is built on and how far everything holds
//! off one.

use crate::*;

/// An asteroid. Drawn and navigated round, and that is all it does: it has no
/// damage grid, because nothing in the game can hurt a rock yet and a grid
/// per rock is eight thousand cells of book keeping for a fact nobody asks.
#[derive(Component)]
pub(crate) struct Rock;

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
