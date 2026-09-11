//! The frame: what a run is (its scene, its clocks, its exit) and the
//! headless harness that photographs one.

mod headless;
mod scene;

pub(crate) use headless::*;
pub(crate) use scene::*;
