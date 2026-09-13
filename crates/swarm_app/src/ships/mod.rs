//! A hull and what it does. The behaviours: it flies, keeps station, is
//! chewed, aims its guns, burns its drives and dies as a wreck.

mod body;
mod damage;
mod flames;
mod flight;
mod formation;
mod hull;
mod production;
mod spec;
mod turrets;
mod wreck;

pub(crate) use body::*;
pub(crate) use damage::*;
pub(crate) use flames::*;
pub(crate) use flight::*;
pub(crate) use formation::*;
pub(crate) use hull::*;
pub(crate) use production::*;
pub(crate) use spec::*;
pub(crate) use turrets::*;
pub(crate) use wreck::*;
