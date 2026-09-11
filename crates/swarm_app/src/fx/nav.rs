//! The nav disc: the rim at the cursor, the lift, the pings and the lines
//! standing on a ship under way.

use crate::*;

#[derive(Resource)]
pub(crate) struct NavHandle(pub(crate) Handle<Mesh>);

/// One line as a quad turned edge on to the eye, which is the same trick the
/// beams use and for the same reason: it is the same width from anywhere.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_line(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    eye: Vec3,
    a: Vec3,
    b: Vec3,
    w: f32,
    c: [f32; 3],
    alpha: f32,
) {
    let dir = (b - a).normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let across = dir
        .cross(eye - (a + b) * 0.5)
        .normalize_or(Vec3::Y.cross(dir).normalize_or(Vec3::X));
    let base = pos.len() as u32;
    for s in [-1.0f32, 1.0] {
        pos.push((a + across * w * s).to_array());
        pos.push((b + across * w * s).to_array());
        col.push([c[0], c[1], c[2], alpha]);
        col.push([c[0], c[1], c[2], alpha]);
    }
    idx.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
}

/// The colours of the order picture, as vertex colours on an additive unlit
/// mesh: a little over one so the rim and the lines read against a lit hull
/// without going through the bloom threshold the flames sit over.
pub(crate) const CYAN: [f32; 3] = [0.35, 1.25, 1.30];

pub(crate) const GOLD: [f32; 3] = [1.30, 1.15, 0.40];

pub(crate) const ORANGE: [f32; 3] = [1.40, 0.80, 0.30];

pub(crate) const RED: [f32; 3] = [1.40, 0.45, 0.38];

/// A ring in the horizontal plane through `centre`: a flat ribbon, because
/// that is what it MEANS. Edge on from the side is correct and is exactly the
/// cue that tells a player the plane is a plane.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_ring(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    centre: Vec3,
    r: f32,
    w: f32,
    c: [f32; 3],
    alpha: f32,
    segments: usize,
) {
    for n in 0..segments {
        let a0 = n as f32 / segments as f32 * std::f32::consts::TAU;
        let a1 = (n + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        let base = pos.len() as u32;
        for a in [a0, a1] {
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            pos.push((centre + out * (r - w)).to_array());
            pos.push((centre + out * (r + w)).to_array());
            col.push([c[0], c[1], c[2], alpha]);
            col.push([c[0], c[1], c[2], alpha]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }
}

/// A filled disc in the horizontal plane, as a fan.
pub(crate) fn add_disc(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    centre: Vec3,
    r: f32,
    c: [f32; 3],
    alpha: f32,
    segments: usize,
) {
    let base = pos.len() as u32;
    pos.push(centre.to_array());
    col.push([c[0], c[1], c[2], alpha]);
    for n in 0..=segments {
        let a = n as f32 / segments as f32 * std::f32::consts::TAU;
        pos.push((centre + Vec3::new(a.cos(), 0.0, a.sin()) * r).to_array());
        col.push([c[0], c[1], c[2], alpha]);
    }
    for n in 0..segments as u32 {
        idx.extend_from_slice(&[base, base + 1 + n, base + 2 + n]);
    }
}

/// Draw the order picture, all of it in one mesh rebuilt every frame: the
/// selection rings, the standing orders, the move disc and the pings.
///
/// The disc is Homeworld's. A LARGE cyan disc on the plane through the
/// selection whose rim is at the cursor, with an X across it; a small gold
/// ring ON that rim where the order will land, with a gold line from the
/// selection out to it, which is the disc's radius; and when shift has lifted
/// the point, the right angle triangle: the vertical from the plane point up to the target, the direct
/// line from the selection to it, and a red ring at the raised point with the
/// distance beside it (`hud_orders` draws the label). A standing order is an
/// orange line and ring per ship until it arrives, which is the
/// acknowledgement: the order is visibly THERE after the click.
pub(crate) fn draw_nav(
    mode: Res<OrderMode>,
    order: Res<NavOrder>,
    pings: Res<Pings>,
    handle: Option<Res<NavHandle>>,
    hulls: Query<(&Hull, &Transform, Option<&Selected>), Without<Hive>>,
    fighters: Query<&Transform, (With<Fighter>, With<Selected>)>,
    cam: Query<&Transform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(handle) = handle else { return };
    let Ok(eye) = cam.single() else { return };
    let eye = eye.translation;

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // ---- what is selected, and where each ship has been sent ----
    for (hull, xf, sel) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let r = hull.model.radius();
        if sel.is_some() {
            add_ring(
                &mut pos,
                &mut col,
                &mut idx,
                xf.translation,
                r * 1.25,
                r * 0.03,
                CYAN,
                0.85,
                48,
            );
        }
        if let Some(t) = hull.order {
            add_line(
                &mut pos,
                &mut col,
                &mut idx,
                eye,
                xf.translation,
                t,
                r * 0.02,
                ORANGE,
                0.85,
            );
            add_ring(
                &mut pos,
                &mut col,
                &mut idx,
                t,
                r * 0.9,
                r * 0.03,
                ORANGE,
                0.9,
                48,
            );
        }
    }
    for xf in &fighters {
        let r = FIGHTER_RADIUS;
        add_ring(
            &mut pos,
            &mut col,
            &mut idx,
            xf.translation,
            r * 1.25,
            r * 0.06,
            CYAN,
            0.85,
            24,
        );
    }

    // ---- the move disc ----
    if *mode == OrderMode::Move {
        let r = order.radius;
        let at = order.anchor;
        // The disc's rim is AT THE CURSOR. Its radius is the order's own
        // distance on the plane, so the gold ring sits on the rim by
        // construction and shift raises the point straight off it: the
        // triangle's base is the disc's radius. This is Homeworld's disc,
        // which grows with the mouse, and not a range ring: the first port
        // drew a fixed rim at `MOVE_RANGE` while the cursor named a point a
        // third of the way out, and a disc that does not reach the cursor
        // says nothing about the order. It stops growing where the cursor is
        // clamped, which is `NavOrder::range`. Never smaller than the gold
        // ring, so the disc does not vanish under the ship at a zero order.
        let reach = (order.on_plane - at).length().max(r * 0.85);
        // The fill is ADDITIVE and in linear light, so the prototype's 0.13
        // of alpha blended sRGB is about 0.03 here: at 0.13 the whole field
        // went teal and the ships inside it read as under water.
        add_disc(&mut pos, &mut col, &mut idx, at, reach, CYAN, 0.03, 96);
        add_ring(
            &mut pos,
            &mut col,
            &mut idx,
            at,
            reach,
            r * 0.045,
            CYAN,
            0.95,
            96,
        );
        // The X: two diameters at forty five and a hundred and thirty five
        // degrees, so neither lies along the line to the target.
        for a in [
            std::f32::consts::FRAC_PI_4,
            3.0 * std::f32::consts::FRAC_PI_4,
        ] {
            let d = Vec3::new(a.cos(), 0.0, a.sin()) * reach;
            add_line(
                &mut pos,
                &mut col,
                &mut idx,
                eye,
                at - d,
                at + d,
                r * 0.03,
                CYAN,
                0.75,
            );
        }
        // Where it lands on the plane, and the line out to it.
        add_ring(
            &mut pos,
            &mut col,
            &mut idx,
            order.on_plane,
            r * 0.85,
            r * 0.035,
            GOLD,
            0.95,
            64,
        );
        add_line(
            &mut pos,
            &mut col,
            &mut idx,
            eye,
            at,
            order.on_plane,
            r * 0.03,
            GOLD,
            0.95,
        );
        if order.lifted() {
            let to = order.target();
            add_line(
                &mut pos,
                &mut col,
                &mut idx,
                eye,
                order.on_plane,
                to,
                r * 0.03,
                GOLD,
                0.95,
            );
            add_line(
                &mut pos,
                &mut col,
                &mut idx,
                eye,
                at,
                to,
                r * 0.03,
                GOLD,
                0.95,
            );
            add_ring(
                &mut pos,
                &mut col,
                &mut idx,
                to,
                r * 0.67,
                r * 0.035,
                RED,
                0.95,
                64,
            );
        }
    }

    // ---- the pings: open on a square root and fade ----
    for &(at, age, r) in &pings.0 {
        let t = (age / PING_LIFE).clamp(0.0, 1.0);
        add_ring(
            &mut pos,
            &mut col,
            &mut idx,
            at,
            r * (0.9 + 4.2 * t.sqrt()),
            r * 0.05,
            GOLD,
            1.0 - t,
            64,
        );
    }

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
