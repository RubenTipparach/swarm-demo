//! Who gives orders, and never what a ship does with them. The mode decides
//! which system reads a button press this frame.

mod camera;
mod orders;
mod selection;

pub(crate) use camera::*;
pub(crate) use orders::*;
pub(crate) use selection::*;

use crate::*;

/// Which system owns the mouse this frame.
///
/// This is the whole of the fix for the first cut, which read every button in
/// every system. The left button confirmed an order in one system and started
/// a selection in another on the same press, so confirming a move CLEARED the
/// selection it was for; a release off the window never reached the box, so
/// the box stayed open with no button down. A button press is read by exactly
/// one system per frame, and the mode is what decides which: `select_input`
/// acts only in `Idle` and `Box`, `nav_input` opens only from `Idle` and acts
/// only in `Move`, and `toggle_pause` reaches the menu only from `Idle`.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OrderMode {
    #[default]
    Idle,
    /// The left button is down and a band box is open.
    Box,
    /// A move order is being aimed: the disc is up.
    Move,
}
