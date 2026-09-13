//! The sensors manager's picture: the ground, what the fleet can SEE of it,
//! the horizon and its rings, and a stalk under every contact.
//!
//! It is a camera MODE and not a screen, which is the mockup's own rule and
//! the whole reason it is worth having: it does not open over the field, it
//! pulls the eye back off it. So the ships on it are the real ships at their
//! real size, a press is the same raycast the field uses, and the map cannot
//! be out of register with the world because there is only one of them.
//!
//! **What it says is COVERAGE.** The operational area is washed dark and each
//! ship lights the ground it can see, so the picture answers the one question
//! this view exists for: where is the fleet blind. An enemy standing in the
//! dark is not drawn at all, which is what makes moving a ship worth doing
//! here rather than only on the field.

use crate::*;

/// The furniture: the horizon, its rings, the bearings and the stalks. Lines,
/// so it is ADDITIVE.
#[derive(Resource)]
pub(crate) struct SensorHandle(pub(crate) Handle<Mesh>);

/// The ground and what the fleet sees of it. Fills, so it is ALPHA BLENDED,
/// and that is the whole reason it cannot share the furniture's mesh: a wash
/// that DARKENS is a thing additive blending cannot express at all.
#[derive(Resource)]
pub(crate) struct GroundHandle(pub(crate) Handle<Mesh>);

/// How many rings inside the horizon.
///
/// Four, at a quarter of the horizon each, which is the mockup's own step and
/// what lets a range label be read off a ring rather than estimated between
/// two of them.
const RINGS: usize = 4;

/// The ink the furniture is drawn in: the HUD's own cyan, so the map reads as
/// part of the deck rather than as a second thing over the field.
const SENSOR_INK: [f32; 3] = [0.29, 0.72, 0.85];
const STALK_INK: [f32; 3] = [0.42, 0.88, 0.98];

/// The unseen ground: a dark blue wash over the whole operational area.
///
/// Alpha rather than additive, because the point of it is that unwatched
/// space is DARKER: additive can only ever add light, so a wash built that
/// way would make the blind half of the map the bright half.
const DARK_INK: [f32; 3] = [0.02, 0.05, 0.17];
const DARK_ALPHA: f32 = 0.44;

/// What a ship lights up, and it goes to nothing at its own range.
///
/// The mockup's own three stops (0.30 at the middle, 0.16 at about two
/// thirds, nothing at the rim) as the two a fan can carry. Laid over the wash
/// rather than added to it, so two ships watching the same ground are a
/// little brighter there and never a blown out white: this is a map, and the
/// information in it is the EDGE of what is covered.
const SEEN_INK: [f32; 3] = [0.22, 0.58, 1.0];
const SEEN_ALPHA: (f32, f32) = (0.40, 0.0);

/// A little under the plane, so the back to front sort of the transparent
/// pass puts the ground behind the furniture standing on it.
const GROUND_DROP: f32 = 0.6;

/// Draw the ground, the coverage, the horizon and the contacts.
pub(crate) fn draw_sensors(
    views: Res<Views>,
    handle: Option<Res<SensorHandle>>,
    ground: Option<Res<GroundHandle>>,
    cam: Query<(&Transform, &Orbit), With<Camera3d>>,
    hulls: Contacts,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(handle), Some(ground)) = (handle, ground) else {
        return;
    };
    let Ok((xf, orbit)) = cam.single() else {
        return;
    };
    // Off, and both meshes are EMPTIED rather than the entities hidden: the
    // view is rebuilt from state every frame, so there is no stale picture to
    // hide.
    if views.open != Some(ViewTab::Sensors) {
        let _ = meshes.insert(handle.0.id(), empty_mesh());
        let _ = meshes.insert(ground.0.id(), empty_mesh());
        return;
    }
    let eye = xf.translation;
    // The plane is the PIVOT's, which is the one the player drives and the one
    // a move order is already given on: two planes would be two answers to
    // "where is that ship really".
    let floor = orbit.target.y;
    let hub = Vec3::new(orbit.target.x, floor, orbit.target.z);
    // The line widths ride the eye's own distance, or a line a thousand units
    // out is a line under a pixel wide.
    let w = orbit.eye * 0.0016;

    let mut furniture = Draw::default();
    horizon(&mut furniture, eye, hub, w);
    contacts(&mut furniture, (eye, floor, orbit.eye, w), &hulls);
    let _ = meshes.insert(handle.0.id(), furniture.mesh());

    let mut lit = Draw::default();
    cover(&mut lit, hub, &hulls);
    let _ = meshes.insert(ground.0.id(), lit.mesh());
}

/// Every contact the manager draws, and what it can see with.
///
/// A type alias because the tuple is four optionals over two filters, which
/// is exactly the complexity clippy is right to name: the QUERY is the
/// interface here and it reads better with a name on it.
type Contacts<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        &'static Hull,
        Option<&'static Hive>,
        Option<&'static Selected>,
    ),
    (With<Hull>, Without<Camera3d>),
>;

/// How far one hull sees, in world units.
///
/// The CATEGORY's, which is the core's answer, so a class added tomorrow sees
/// tomorrow. A hull with no class is a carrier or a rock: neither is yours and
/// neither lights anything, so nothing is watching from it.
fn eyes(hull: &Hull) -> f32 {
    if hull.dead_hull {
        return 0.0;
    }
    match hull.class.as_deref() {
        Some(class) => Category::of(class).sensor_range(),
        None => 0.0,
    }
}

/// The dark ground, and the patches the fleet lights on it.
fn cover(d: &mut Draw, hub: Vec3, hulls: &Contacts) {
    let floor = hub - Vec3::Y * GROUND_DROP;
    add_disc(
        &mut d.pos,
        &mut d.col,
        &mut d.idx,
        floor,
        SENSORS_R,
        DARK_INK,
        (DARK_ALPHA, DARK_ALPHA),
        96,
    );
    // Then what is watched, laid OVER the wash in the same mesh so the order
    // is the submission order and needs no second material to arrange.
    for (ship, hull, hive, _) in hulls {
        if hive.is_some() {
            continue;
        }
        let r = eyes(hull);
        if r <= 0.0 {
            continue;
        }
        let at = Vec3::new(ship.translation.x, floor.y, ship.translation.z);
        add_disc(
            &mut d.pos, &mut d.col, &mut d.idx, at, r, SEEN_INK, SEEN_ALPHA, 64,
        );
    }
}

/// Whether anything of the fleet's can see a point on the plane.
fn watched(at: Vec3, hulls: &Contacts) -> bool {
    hulls.iter().any(|(ship, hull, hive, _)| {
        hive.is_none() && {
            let r = eyes(hull);
            r > 0.0 && Vec2::new(at.x - ship.translation.x, at.z - ship.translation.z).length() < r
        }
    })
}

/// The horizon, the rings inside it, the bearings and the pivot.
fn horizon(d: &mut Draw, eye: Vec3, hub: Vec3, w: f32) {
    for n in 1..=RINGS {
        let t = n as f32 / RINGS as f32;
        add_ring(
            &mut d.pos,
            &mut d.col,
            &mut d.idx,
            eye,
            hub,
            SENSORS_R * t,
            w * if n == RINGS { 1.6 } else { 0.7 },
            SENSOR_INK,
            if n == RINGS { 0.5 } else { 0.22 },
            96,
        );
    }
    // A tick every fifteen degrees round the horizon, longer every forty
    // five, because a contact at nine o'clock is only useful if the screen
    // agrees with the order about to be given. The NUMBERS beside them are
    // `place_sensor_marks`: text is a UI node here and the rest of this is a
    // mesh, and a label is worth the crossing.
    for n in 0..24 {
        let a = n as f32 / 24.0 * std::f32::consts::TAU;
        let big = n % 3 == 0;
        let out = Vec3::new(a.sin(), 0.0, -a.cos());
        add_line(
            &mut d.pos,
            &mut d.col,
            &mut d.idx,
            eye,
            hub + out * SENSORS_R * if big { 0.94 } else { 0.97 },
            hub + out * SENSORS_R,
            w * if big { 0.8 } else { 0.5 },
            SENSOR_INK,
            if big { 0.5 } else { 0.22 },
        );
    }
    // The pivot itself, because the whole map is drawn about it and a map
    // whose own centre is invisible cannot say where it is looking.
    let arm = SENSORS_R * 0.02;
    for out in [Vec3::X, Vec3::Z] {
        add_line(
            &mut d.pos,
            &mut d.col,
            &mut d.idx,
            eye,
            hub - out * arm,
            hub + out * arm,
            w * 0.7,
            GOLD,
            0.8,
        );
    }
}

/// A stalk and a foot ring under every contact the fleet can SEE.
fn contacts(d: &mut Draw, fit: (Vec3, f32, f32, f32), hulls: &Contacts) {
    let (eye, floor, out, w) = fit;
    for (ship, hull, hive, picked) in hulls {
        let at = ship.translation;
        let foot = Vec3::new(at.x, floor, at.z);
        // Yours is always on the map, because it is the thing doing the
        // looking. An ENEMY is on it only where something can see that
        // ground, which is the whole claim this screen makes: a carrier in
        // the dark is a carrier you have not found yet.
        if hive.is_some() && !watched(foot, hulls) {
            continue;
        }
        if hull.dead_hull && hive.is_none() && hull.class.is_none() {
            continue;
        }
        let ink = if picked.is_some() {
            STALK_INK
        } else if hive.is_some() {
            [0.86, 0.45, 0.92]
        } else {
            SENSOR_INK
        };
        let lit = if picked.is_some() { 0.95 } else { 0.5 };
        add_line(
            &mut d.pos,
            &mut d.col,
            &mut d.idx,
            eye,
            foot,
            at,
            w * 0.6,
            ink,
            lit,
        );
        add_ring(
            &mut d.pos,
            &mut d.col,
            &mut d.idx,
            eye,
            foot,
            out * 0.012,
            w * 0.8,
            ink,
            lit * 0.8,
            24,
        );
    }
}
