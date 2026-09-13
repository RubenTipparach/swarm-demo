//! The numbers on the sensors manager: what bearing you are looking along,
//! and how far out each ring stands.
//!
//! They are UI nodes rather than mesh, and the whole rest of that view is a
//! mesh, so this is the one place the two halves meet: there is no text in a
//! `TriangleList`, and a map with no numbers on it is a picture of a fleet
//! rather than something a player can give an order off. Each mark is placed
//! by projecting its own point on the plane, which is what keeps it ON the
//! ring it labels at every angle the camera can be turned to.

use crate::*;

/// What one mark says.
#[derive(Component, Clone, Copy)]
pub(crate) enum Mark {
    /// A bearing round the horizon, in degrees, drawn as the mockup draws it:
    /// three digits, so 000 and 090 are the same width as 315 and the ring
    /// does not appear to breathe as it turns.
    Bearing(u16),
    /// A range ring, by its index out from the pivot. What it SAYS is the
    /// radius in world units, which is the unit every other distance in this
    /// game is already in.
    Range(usize),
}

/// How many rings the horizon carries, which is `fx::sensors`' own count.
const RINGS: usize = 4;

/// Which bearing the range labels are stacked along, in degrees.
///
/// Due WEST, which is the one side of the map the side panel never covers.
/// The panel is four hundred and twenty eight wide on the right, so a column
/// of ranges laid out to the east is a column with its far half behind the
/// build menu: the first cut put them on the forty five diagonal and lost the
/// two outer rings. The bearing label that shares this line is pushed further
/// out than the rings are, so the two never land on each other.
const RANGE_BEARING: f32 = 270.0;

/// Spawn every mark once, hidden. There are twelve and the set never changes,
/// so a pool that grew would be a pool that never shrank.
pub(crate) fn sensor_marks(p: &mut ChildSpawnerCommands, skin: &Skin) {
    let tok = skin.tok();
    for n in 0..8 {
        one(p, tok.faint, Mark::Bearing(n * 45));
    }
    for n in 0..RINGS {
        one(p, tok.cyan, Mark::Range(n));
    }
}

fn one(p: &mut ChildSpawnerCommands, ink: Ink, mark: Mark) {
    p.spawn((
        Text::new(""),
        TextFont {
            font_size: 11.0,
            ..default()
        },
        TextColor(ink.col()),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
        Pickable::IGNORE,
        mark,
    ));
}

/// Put every mark where its own point on the plane lands on screen.
///
/// **Divided by `UiScale`, and that is the whole of the arithmetic worth
/// writing down.** The deck is authored at 1600 by 900 and scaled by the
/// window's height, so a `Val::Px` is an AUTHORED pixel and what
/// `world_to_viewport` hands back is a window one. A label placed straight
/// from the projection lands at that fraction of the way across the screen,
/// which is a number sitting well inside the ring it is supposed to name.
pub(crate) fn place_sensor_marks(
    views: Res<Views>,
    scale: Res<UiScale>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    orbit: Query<&Orbit, With<Camera3d>>,
    mut marks: Query<(&Mark, &mut Node, &mut Text)>,
) {
    let open = views.open == Some(ViewTab::Sensors);
    let (Ok((cam, cam_xf)), Ok(orbit)) = (cams.single(), orbit.single()) else {
        return;
    };
    let hub = Vec3::new(orbit.target.x, orbit.target.y, orbit.target.z);
    let k = scale.0.max(0.01);
    for (mark, mut node, mut text) in &mut marks {
        let (at, says) = match *mark {
            // Just inside the horizon, so the number sits on the rim rather
            // than outside a ring that may already be at the edge of the
            // frame.
            Mark::Bearing(deg) => {
                let a = (deg as f32).to_radians();
                (
                    hub + Vec3::new(a.sin(), 0.0, -a.cos()) * SENSORS_R * 1.13,
                    format!("{deg:03}"),
                )
            }
            Mark::Range(n) => {
                let r = SENSORS_R * (n + 1) as f32 / RINGS as f32;
                let a = RANGE_BEARING.to_radians();
                (
                    hub + Vec3::new(a.sin(), 0.0, -a.cos()) * r,
                    format!("{r:.0}u"),
                )
            }
        };
        let on = open
            .then(|| cam.world_to_viewport(cam_xf, at).ok())
            .flatten();
        match on {
            Some(p) => {
                node.display = Display::Flex;
                node.left = Val::Px(p.x / k - 12.0);
                node.top = Val::Px(p.y / k - 6.0);
                if text.0 != says {
                    text.0 = says;
                }
            }
            None => node.display = Display::None,
        }
    }
}
