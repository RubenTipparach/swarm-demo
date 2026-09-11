//! The pause menu, and the escape key's order of business.

use crate::*;

/// The whole pause overlay, shown and hidden by its `display`.
#[derive(Component)]
pub(crate) struct PauseMenu;

/// Escape opens the menu and Escape closes it.
///
/// It sets `SwarmConfig.paused` as well as its own flag, because the swarm
/// lives in the render world on the other side of an extract and does not see
/// this resource: `advance_clock` already reads that one and hands the tick a
/// dt of nought, so the cloud freezes where it is instead of being stepped by
/// a frame that was not simulated.
pub(crate) fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    mut hud: ResMut<Hud>,
    mut cfg: ResMut<SwarmConfig>,
    mut menu: Query<&mut Node, With<PauseMenu>>,
    resume: Query<&Interaction, (Changed<Interaction>, With<ResumeButton>)>,
    mode: Res<OrderMode>,
    mut started: Local<bool>,
) {
    // The menu is built hidden, so a session that was asked to START paused
    // has to be shown once. Done here rather than in `build_hud` because the
    // display is this system's to own: two places writing it is two places to
    // keep in step.
    if !*started {
        *started = true;
        if hud.paused {
            cfg.paused = true;
            for mut n in &mut menu {
                n.display = Display::Flex;
            }
            return;
        }
    }
    let clicked = resume.iter().any(|i| *i == Interaction::Pressed);
    // Escape belongs to the ORDER first. One escape cancels an open move, or
    // an open box, and does nothing else; the next one, with nothing open,
    // reaches the menu. This runs BEFORE `select_input` and `nav_input` so it
    // sees the mode the key was pressed in and not the `Idle` they leave
    // behind, or one escape would cancel the order AND open the menu.
    // Space is the plain pause, which is the key an RTS puts it on.
    if keys.just_pressed(KeyCode::Space)
        || (keys.just_pressed(KeyCode::Escape) && *mode == OrderMode::Idle)
    {
        hud.paused = !hud.paused;
    } else if clicked {
        hud.paused = false;
    } else {
        return;
    }
    cfg.paused = hud.paused;
    for mut n in &mut menu {
        n.display = if hud.paused {
            Display::Flex
        } else {
            Display::None
        };
    }
}
