//! Click and box selection, on the left button.

use crate::*;

/// Under this much drag, in pixels, a press is a click and not a box.
pub(crate) const CLICK_PX: f32 = 6.0;

/// How far from a ship's centre on screen, in pixels, a click still takes it.
pub(crate) const PICK_PX: f32 = 48.0;

/// A ship the player has picked. Only ever on the player's own.
#[derive(Component)]
pub(crate) struct Selected;

/// The band box, in screen pixels, while the mode is `Box`.
#[derive(Resource, Default)]
pub(crate) struct Marquee {
    pub(crate) from: Vec2,
    pub(crate) to: Vec2,
}

/// Left button picks: a click takes one ship, a drag takes a box of them.
///
/// This is the button an RTS gives to selection, so the camera had to give it
/// up: orbit is the MIDDLE button now, with alt and left as an alias for a
/// mouse that has no middle. Holding shift adds to the selection rather than
/// replacing it, which is the one convention every RTS shares.
///
/// The box OPENS on the press and CLOSES on the release, and the release is
/// read off the BUTTON, never off the cursor. The first cut returned early
/// whenever the cursor was off the window, which is exactly where a drag that
/// started near the edge ends, so the release was never seen and the box
/// stayed open with no button down. Here the cursor is tracked wherever it
/// reports from and kept where it was last seen when it does not, and a
/// window that loses focus drops the box outright: a lost window is a lost
/// drag, and nothing is selected by it.
///
/// It never opens a move order. The release closes the box and that is all it
/// does; the order is the next right click's job.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    over_ui: Query<&Interaction>,
    mut mode: ResMut<OrderMode>,
    mut marquee: ResMut<Marquee>,
    mut commands: Commands,
    ships: Query<
        (Entity, &Transform, Option<&Hull>, Option<&Fighter>),
        Or<(With<Hull>, With<Fighter>)>,
    >,
    hives: Query<(), With<Hive>>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((cam, cam_xf)) = cams.single() else {
        return;
    };
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    match *mode {
        // In `Move` the left button is the confirm and nothing else. It does
        // not start a selection, which is the bug that emptied the order.
        OrderMode::Move => return,
        OrderMode::Idle => {
            if over_ui.iter().any(|i| *i != Interaction::None) || alt {
                return;
            }
            if !buttons.just_pressed(MouseButton::Left) {
                return;
            }
            let Some(cursor) = window.cursor_position() else {
                return;
            };
            marquee.from = cursor;
            marquee.to = cursor;
            *mode = OrderMode::Box;
            // The release is another frame's.
            return;
        }
        OrderMode::Box => {}
    }

    if let Some(cursor) = window.cursor_position() {
        marquee.to = cursor;
    }
    if !window.focused || keys.just_pressed(KeyCode::Escape) {
        *mode = OrderMode::Idle;
        return;
    }
    if buttons.pressed(MouseButton::Left) {
        return;
    }
    *mode = OrderMode::Idle;

    let add = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if !add {
        for (e, _, _, _) in &ships {
            commands.entity(e).remove::<Selected>();
        }
    }
    let (from, to) = (marquee.from, marquee.to);
    let lo = from.min(to);
    let hi = from.max(to);
    // A click rather than a drag: take whatever is nearest the pointer instead
    // of whatever is inside a box a few pixels across. The TRUE corners, not
    // the drawn ones: a drag that left the window still selects what it
    // covered.
    let click = (hi - lo).length() < CLICK_PX;
    let mut best: Option<(f32, Entity)> = None;

    for (e, xf, hull, _) in &ships {
        // Carriers are hulls too. They are not yours and cannot be ordered.
        if hives.get(e).is_ok() {
            continue;
        }
        if hull.map(|h| h.dead_hull).unwrap_or(false) {
            continue;
        }
        let Ok(p) = cam.world_to_viewport(cam_xf, xf.translation) else {
            continue;
        };
        if click {
            let d = p.distance(to);
            if d < PICK_PX && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, e));
            }
        } else if p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y {
            commands.entity(e).insert(Selected);
        }
    }
    if let Some((_, e)) = best {
        commands.entity(e).insert(Selected);
    }
}
