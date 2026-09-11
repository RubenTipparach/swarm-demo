//! The part of the game that is not the engine.
//!
//! Everything here is a pure function of its inputs and depends on nothing but
//! `std`. That is the boundary redux-tribes keeps between `sim_core` and `web`,
//! kept for the same reason: a rule with one implementation cannot be changed in
//! one of two places, and a crate with no engine in it can be tested without one
//! and carried to the next engine without a fork.
//!
//! - [`voxel`]: the lattice and a model on it, ported from `design.ts`.
//! - [`mesh`]: faces where a solid cell meets one that is not, greedily merged,
//!   ported from `hull.ts`, with a region form for re-meshing one brick.
//! - [`damage`]: hit points per cell, the dead set, which bricks are dirty, and
//!   the heat ramp a fresh hole cools along, ported from `wound.ts`.
//! - [`alien`]: seeded, symmetric, connected voxel motes on a small lattice.
//! - [`rng`]: a deterministic generator and the hash a chunk's drift comes from.
//! - [`rock`]: asteroids, seeded, in one piece, with ore in them.
//! - [`sky`]: the nebula baked to a cubemap and the stars as points, from
//!   `sky.ts`.
//! - [`fx`]: what a shot is, what a blast kills, and what comes off a thing
//!   that dies.
//! - [`body`]: a hull as a rigid body, off its live cells: mass, centre,
//!   inertia, and what a hit at a point does.
//! - [`ray`]: which live cell a ray meets first, so a click lands on a hull.

pub mod alien;
pub mod body;
pub mod damage;
pub mod fx;
pub mod mesh;
pub mod ray;
pub mod rng;
pub mod rock;
pub mod sky;
pub mod voxel;

pub use damage::DamageGrid;
pub use fx::{Beam, Blast, Spark, SparkKind};
pub use mesh::{greedy_mesh, mesh_region, MeshData};
pub use voxel::{mat, Surface, VoxelModel, Window, HULL_NX, HULL_NY, HULL_NZ, SURF_COUNT};
