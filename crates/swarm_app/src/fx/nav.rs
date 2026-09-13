//! The nav disc: the rim at the cursor, the lift, the pings and the lines
//! standing on a ship under way.

use crate::*;

#[derive(Resource)]
pub(crate) struct NavHandle(pub(crate) Handle<Mesh>);

/// One continuous ribbon through a polyline, every quad turned edge on to the
/// eye. This is the ONE place a stroke is built.
///
/// One place, because two was the defect: a LINE was turned to the eye and a
/// RING was a flat annulus in the horizontal plane, so at nought pitch every
/// ring in the game was seen exactly edge on and drew nothing at all. The
/// comment over the ring even argued for it ("edge on from the side is
/// correct, it is what tells a player the plane is a plane"), which is true of
/// the plane and false of the STROKE that marks it: a circle seen edge on is a
/// line segment, and a line segment is a thing with a width.
///
/// A STRIP and not a quad per segment, because this mesh is ADDITIVE: two
/// quads meeting at a joint overlap by however far the ribbon bends, and
/// additive lays the colour down twice there, which is a bright pip at every
/// one of a ring's ninety six joints. Sharing the joint's two vertices makes
/// it one surface, and the across vector there comes off the AVERAGE of the
/// two segments either side so the ribbon does not kink.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_strip(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    eye: Vec3,
    points: &[Vec3],
    closed: bool,
    w: f32,
    c: [f32; 3],
    alpha: f32,
) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let base = pos.len() as u32;
    for (i, &p) in points.iter().enumerate() {
        let prev = (i > 0)
            .then(|| points[i - 1])
            .or(closed.then(|| points[n - 1]));
        let next = points.get(i + 1).copied().or(closed.then(|| points[0]));
        // The tangent at this vertex is the average of the segments either
        // side of it, so one across vector serves both and the joint closes.
        let tan = match (prev, next) {
            (Some(a), Some(b)) => b - a,
            (None, Some(b)) => b - p,
            (Some(a), None) => p - a,
            (None, None) => return,
        }
        .normalize_or_zero();
        let view = (eye - p).normalize_or_zero();
        if tan == Vec3::ZERO || view == Vec3::ZERO {
            return;
        }
        // Edge on: across the stroke AND across the eye's own line to it.
        //
        // The fallback is for a stroke pointing straight at the camera, where
        // that cross is nought. What is wanted there is any direction square
        // to the VIEW, because a stroke going into the screen still has to be
        // a width across it; taking it square to the stroke instead would lay
        // the ribbon flat and put the stroke out, which on a ring at nought
        // pitch is a pinch to nothing at each of its two tangent points.
        let across = tan
            .cross(view)
            .normalize_or(Vec3::Y.cross(view).normalize_or(Vec3::X));
        pos.push((p - across * w).to_array());
        pos.push((p + across * w).to_array());
        col.push([c[0], c[1], c[2], alpha]);
        col.push([c[0], c[1], c[2], alpha]);
    }
    for s in 0..if closed { n } else { n - 1 } {
        let a = base + s as u32 * 2;
        let b = base + ((s + 1) % n) as u32 * 2;
        idx.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
    }
}

/// One line, as a strip of two points.
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
    add_strip(pos, col, idx, eye, &[a, b], false, w, c, alpha);
}

/// The colours of the order picture, as vertex colours on an additive unlit
/// mesh: a little over one so the rim and the lines read against a lit hull
/// without going through the bloom threshold the flames sit over.
pub(crate) const CYAN: [f32; 3] = [0.35, 1.25, 1.30];

pub(crate) const GOLD: [f32; 3] = [1.30, 1.15, 0.40];

pub(crate) const ORANGE: [f32; 3] = [1.40, 0.80, 0.30];

pub(crate) const RED: [f32; 3] = [1.40, 0.45, 0.38];

/// A ring in the horizontal plane through `centre`, stroked toward the eye.
///
/// The CIRCLE is still in the plane, which is what it means and what says the
/// plane is a plane; what faces the camera is the WIDTH of the line drawn
/// round it. So a ring seen from above is a circle and a ring seen from dead
/// level is a bar the same width, rather than nothing at all.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_ring(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    eye: Vec3,
    centre: Vec3,
    r: f32,
    w: f32,
    c: [f32; 3],
    alpha: f32,
    segments: usize,
) {
    let ring: Vec<Vec3> = (0..segments)
        .map(|n| {
            let a = n as f32 / segments as f32 * std::f32::consts::TAU;
            centre + Vec3::new(a.cos(), 0.0, a.sin()) * r
        })
        .collect();
    add_strip(pos, col, idx, eye, &ring, true, w, c, alpha);
}

/// A filled disc in the horizontal plane, as a fan.
///
/// The one thing here that is NOT turned toward the eye, and on purpose: this
/// is a fill rather than a stroke, so it is the plane itself. A disc that
/// always faced the camera would be a bubble round the selection washing over
/// every ship inside it from every angle, and at nought pitch what a player
/// reads the order off is the rim, the X and the lines, which all have width
/// now.
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
                eye,
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
                eye,
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
            eye,
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
            eye,
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
            eye,
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
                eye,
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
            eye,
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
