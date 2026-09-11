//! The frame: what a run is (its scene, its clocks, its exit) and the
//! headless harness that photographs one.

mod headless;
mod scene;
mod state;

pub(crate) use headless::*;
pub(crate) use scene::*;
pub(crate) use state::*;
