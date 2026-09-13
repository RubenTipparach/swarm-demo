//! The sensors manager's furniture: the horizon, its rings and the stalks.
//!
//! It is a camera MODE and not a screen, which is the mockup's own rule and
//! the whole reason it is worth having: it does not open over the field, it
//! pulls the eye back off it. So the ships on it are the real ships at their
//! real size, a press is the same raycast the field uses, and the map cannot
//! be out of register with the world because there is only one of them.
//!
//! What this file draws is laid ON TOP of the live view and paints no ground
//! of its own, because the ground IS the battle.

use crate::*;

/// The mesh the furniture is rebuilt into every frame.
#[derive(Resource)]
pub(crate) struct SensorHandle(pub(crate) Handle<Mesh>);

/// How many rings inside the horizon, and how tall a contact's stalk may
/// stand before it is clamped.
///
/// Three rings, because the horizon and the pivot are already two edges and a
/// reader counts three gaps without having to: a ring every quarter is a
/// dartboard and a ring at the halfway is a target.
const RINGS: usize = 3;

/// The ink the furniture is drawn in: the HUD's own cyan, so the map reads as
/// part of the deck rather than as a second thing over the field.
const SENSOR_INK: [f32; 3] = [0.29, 0.72, 0.85];
const STALK_INK: [f32; 3] = [0.42, 0.88, 0.98];

/// Draw the horizon, the rings and a stalk under every contact.
///
/// A STALK is what makes a position in three dimensions readable at all: a
/// contact drawn on its own is a dot whose height nobody can judge, and a line
/// down to the pivot's plane with a ring at its foot says both where it is and
/// how far above the plane it stands. It is Homeworld's own answer and it is
/// the reason this view can be worked in rather than only looked at.
pub(crate) fn draw_sensors(
    views: Res<Views>,
    handle: Option<Res<SensorHandle>>,
    cam: Query<(&Transform, &Orbit), With<Camera3d>>,
    hulls: Contacts,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(handle) = handle else { return };
    let Ok((xf, orbit)) = cam.single() else {
        return;
    };
    // Off, and the mesh is EMPTIED rather than the entity hidden: the view is
    // rebuilt from state every frame, so there is no stale picture to hide.
    if views.open != Some(ViewTab::Sensors) {
        let _ = meshes.insert(handle.0.id(), empty_mesh());
        return;
    }
    let eye = xf.translation;
    // The plane is the PIVOT's, which is the one the player drives and the one
    // a move order is already given on: two planes would be two answers to
    // "where is that ship really".
    let floor = orbit.target.y;
    let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    // The line widths ride the eye's own distance, or a line a thousand units
    // out is a line under a pixel wide.
    let w = orbit.eye * 0.0016;
    let hub = Vec3::new(orbit.target.x, floor, orbit.target.z);

    horizon(&mut pos, &mut col, &mut idx, eye, hub, w);
    contacts(
        &mut pos,
        &mut col,
        &mut idx,
        (eye, floor, orbit.eye, w),
        &hulls,
    );
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    if pos.is_empty() {
        mesh = empty_mesh();
    } else {
        let n = pos.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        mesh.insert_indices(Indices::U32(idx));
    }
    let _ = meshes.insert(handle.0.id(), mesh);
}

/// Every contact the manager draws a stalk for.
///
/// A type alias because the tuple is three optionals over two filters, which
/// is exactly the complexity clippy is right to name: the QUERY is the
/// interface here and it reads better with a name on it.
type Contacts<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        Option<&'static Hive>,
        Option<&'static Selected>,
    ),
    (With<Hull>, Without<Camera3d>),
>;

/// The horizon, the rings inside it and the four bearings.
fn horizon(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    eye: Vec3,
    hub: Vec3,
    w: f32,
) {
    for n in 1..=RINGS {
        let t = n as f32 / RINGS as f32;
        add_ring(
            pos,
            col,
            idx,
            hub,
            SENSORS_R * t,
            w * if n == RINGS { 1.6 } else { 0.7 },
            SENSOR_INK,
            if n == RINGS { 0.5 } else { 0.22 },
            96,
        );
    }
    // Four bearings out to the horizon, so the ring has an orientation and a
    // player can say which way a contact lies rather than only how far.
    for n in 0..4 {
        let a = n as f32 * std::f32::consts::FRAC_PI_2;
        add_line(
            pos,
            col,
            idx,
            eye,
            hub,
            hub + Vec3::new(a.sin(), 0.0, a.cos()) * SENSORS_R,
            w * 0.5,
            SENSOR_INK,
            0.16,
        );
    }
}

/// A stalk and a foot ring under every contact.
fn contacts(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    fit: (Vec3, f32, f32, f32),
    hulls: &Contacts,
) {
    let (eye, floor, out, w) = fit;
    for (ship, hive, picked) in hulls {
        let at = ship.translation;
        let foot = Vec3::new(at.x, floor, at.z);
        // A carrier is a contact too, and it is the one a player most wants to
        // find: this view exists to say where everything ELSE is.
        let ink = if picked.is_some() {
            STALK_INK
        } else if hive.is_some() {
            [0.86, 0.45, 0.92]
        } else {
            SENSOR_INK
        };
        let lit = if picked.is_some() { 0.95 } else { 0.5 };
        add_line(pos, col, idx, eye, foot, at, w * 0.6, ink, lit);
        add_ring(
            pos,
            col,
            idx,
            foot,
            out * 0.012,
            w * 0.8,
            ink,
            lit * 0.8,
            24,
        );
    }
}
