//! Drive plumes: a stack of faceted frustums lit by the throttle a hull is
//! actually pulling.

use crate::*;

#[derive(Resource)]
pub(crate) struct FlameHandle(pub(crate) Handle<Mesh>);

/// One engine flame, as a faceted cone.
///
/// GEOMETRY, not particles, which is what makes it read as Homeworld rather
/// than as a campfire: the silhouette is a polygon with hard edges, it is the
/// same shape from every angle, and it is exactly as long as the throttle
/// says. A particle plume says "something hot is happening back there"; a cone
/// says "this ship is under power, by this much, in this direction".
///
/// Six sides on purpose. Enough that it is a cone and few enough that the
/// facets show, which is the whole look.
#[allow(clippy::too_many_arguments)]
/// The stations along a flame: how wide it is there, how bright, and how
/// much of it survives. The last closes the tip.
///
/// Four stations are three BANDS, and a band is what makes this read as a
/// shape. Homeworld draws a flame as geometry rather than as a smear, and
/// what carries that is a hard boundary the eye can find: each band is one
/// flat colour across its whole width and steps at the ring, so the flame has
/// three parts a person could point at instead of a gradient.
pub(crate) const FLAME_BANDS: [(f32, f32, f32); 4] = [
    (1.00, 1.00, 1.00),
    (0.74, 0.66, 0.72),
    (0.40, 0.30, 0.34),
    (0.00, 0.08, 0.00),
];

/// How many facets round the flame. Eight, because six reads as a wedge from
/// abeam and twelve is a smooth horn again.
pub(crate) const FLAME_SIDES: usize = 8;

/// One engine flame: a stack of faceted frustums about `base` along `dir`.
///
/// The vertices are NOT shared between facets. A ring of six shared vertices
/// fanning to a tip is what the first cut drew, and a shared vertex is a
/// colour the rasteriser interpolates ACROSS the edge between two facets, so
/// every hard line in the mesh came out as a smooth ramp and the cone read as
/// a horn of smoke. Three vertices per triangle, all of them the band's own
/// colour, is flat shading, and flat shading is the whole of the look.
pub(crate) fn add_flame(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    base: Vec3,
    dir: Vec3,
    radius: f32,
    len: f32,
    tint: Vec3,
) {
    if len <= 0.0 || radius <= 0.0 {
        return;
    }
    let f = dir.normalize_or(Vec3::NEG_Z);
    let up = if f.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    // (a, b, f) is right handed, so a ring wound in increasing angle gives a
    // triangle whose right hand normal points OUTWARD, which is what the back
    // face cull needs. Built rather than asserted: `a x b == f` by
    // construction, since `b = f x a`.
    let a = f.cross(up).normalize_or(Vec3::X);
    let b = f.cross(a);

    let ring = |n: usize, r: f32, along: f32| -> Vec3 {
        let t = n as f32 / FLAME_SIDES as f32 * std::f32::consts::TAU;
        base + f * along + (a * t.cos() + b * t.sin()) * r
    };
    let mut tri = |p: [Vec3; 3], c: [f32; 4]| {
        let n = pos.len() as u32;
        for v in p {
            pos.push(v.to_array());
            col.push(c);
        }
        idx.extend_from_slice(&[n, n + 1, n + 2]);
    };

    for band in 0..FLAME_BANDS.len() - 1 {
        let (r0, _, _) = FLAME_BANDS[band];
        let (r1, _, _) = FLAME_BANDS[band + 1];
        let (z0, z1) = (
            band as f32 / (FLAME_BANDS.len() - 1) as f32 * len,
            (band + 1) as f32 / (FLAME_BANDS.len() - 1) as f32 * len,
        );
        // The band takes the MEAN of the two stations it lies between, so the
        // steps fall between bands and not inside one.
        let bright = 0.5 * (FLAME_BANDS[band].1 + FLAME_BANDS[band + 1].1);
        let alpha = 0.5 * (FLAME_BANDS[band].2 + FLAME_BANDS[band + 1].2);
        let c = [tint.x * bright, tint.y * bright, tint.z * bright, alpha];
        for n in 0..FLAME_SIDES {
            let m = (n + 1) % FLAME_SIDES;
            let p00 = ring(n, r0 * radius, z0);
            let p01 = ring(m, r0 * radius, z0);
            if r1 <= 0.0 {
                // The last band closes on the axis, so it is one triangle
                // rather than a quad with a degenerate edge in it.
                tri([p00, p01, base + f * z1], c);
                continue;
            }
            let p10 = ring(n, r1 * radius, z1);
            let p11 = ring(m, r1 * radius, z1);
            tri([p00, p01, p10], c);
            tri([p01, p11, p10], c);
        }
    }
}

/// Every engine on every ship and carrier, as one mesh rebuilt each frame.
///
/// There are dozens of engines and nine thousand motes, so the motes get
/// their glow in the shader and everything countable gets geometry. That is
/// the same split the whole design keeps: the ECS holds what there are dozens
/// of, and the GPU holds the rest.
pub(crate) fn draw_flames(
    tick: Res<Tick>,
    handle: Option<Res<FlameHandle>>,
    hulls: Query<(&Hull, &Transform)>,
    hives: Query<(&Hive, &Transform, &Hull)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(handle) = handle else { return };
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Two flames per engine: a short white hot core inside a longer coloured
    // one. One alone is a flat cutout of colour; nested, the core reads as
    // the part that is actually burning and the body as what it throws.
    let mut cone = |at: Vec3, dir: Vec3, size: f32, throttle: f32, hot: Vec3, cool: Vec3, seed: u32| {
        // A flicker, hashed off the engine and the tick rather than rolled,
        // so both seats and a re-watch see the same flame.
        let flick = 0.88 + 0.24 * (swarm_core::rng::hash_cell(seed ^ (tick.tick / 3)) & 0xFF) as f32 / 255.0;
        // Long and narrow rather than short and wide: a jet, not a bell of
        // flame. The first cut was as broad as it was long and read as a
        // paper cone stuck on the back of the ship.
        let len = size * (0.9 + 6.5 * throttle) * flick;
        add_flame(&mut pos, &mut col, &mut idx, at, dir, size * 0.92, len, cool);
        add_flame(&mut pos, &mut col, &mut idx, at, dir, size * 0.52, len * 0.40, hot);
    };

    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let forward = xf.rotation * Vec3::Z;
        for e in &hull.engines {
            // What is LEFT of it. A drive whose cells have been chewed off
            // cannot burn, and one half eaten burns half: the flame is the
            // only thing on screen that says whether a ship still has its
            // legs, so it has to be read off the cells and not off a list
            // taken at spawn.
            let left = e.left(&hull.damage);
            let throttle = throttle_of(hull, forward, e.gun.out[2]) * left;
            if throttle <= 0.01 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(e.gun.at));
            let dir = (xf.rotation * Vec3::from(e.gun.out)).normalize_or(Vec3::NEG_Z);
            cone(
                at,
                dir,
                hull.model.cell * 2.4 * (0.55 + 0.45 * left),
                throttle,
                Vec3::new(5.4, 4.3, 3.0),
                Vec3::new(3.0, 0.86, 0.14),
                e.gun.cell ^ hull.seed,
            );
        }
    }
    for (hive, xf, hull) in &hives {
        if hull.dead_hull {
            continue;
        }
        // A carrier is always under way, and slowly: a fixed low throttle
        // rather than one read off its drift, which would be invisible.
        for e in &hive.engines {
            // The same rule on a carrier, which is a ship with a damage grid
            // like any other.
            let left = e.left(&hull.damage);
            if left <= 0.01 {
                continue;
            }
            // NOT `* hive.scale`. The carrier's own Transform already carries
            // that scale, and `transform_point` applies it, so multiplying it
            // in here squared it: an engine a fifth of the way out from the
            // centre was drawn at a fifth SQUARED of the scaled radius, which
            // put every carrier's flames in open space several lengths off its
            // hull. They looked unaligned because they were not on the ship.
            let at = xf.transform_point(Vec3::from(e.gun.at));
            let dir = (xf.rotation * Vec3::from(e.gun.out)).normalize_or(Vec3::NEG_Z);
            cone(
                at,
                dir,
                hull.model.cell * hive.scale * 2.2 * (0.55 + 0.45 * left),
                0.5 * left,
                Vec3::new(3.4, 5.4, 6.0),
                Vec3::new(0.45, 2.6, 3.8),
                e.gun.cell ^ hive.seed as u32,
            );
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
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

/// How hard one engine is burning, given which way it points.
///
/// A main engine burns to go FASTER and a retro burns to slow down, which is
/// what a retro is for. Both are propulsion and both wear the same surface, so
/// the only thing that tells them apart is which way their plume leaves: aft
/// for the drive, forward for the retro. Reading the throttle off the SPEED
/// rather than the acceleration lit both of them whenever the ship moved,
/// which is a ship fighting itself.
pub(crate) fn throttle_of(hull: &Hull, forward: Vec3, out_z: f32) -> f32 {
    if hull.dead_hull {
        return 0.0;
    }
    let cap = hull.model.radius() * HULL_ACCEL;
    let along = hull.accel.dot(forward) / cap.max(1e-4);
    if out_z < 0.0 {
        // The main drive. An idle that is not nought, because a ship with its
        // engines completely dark reads as one that has broken down.
        0.16 + 0.84 * along.clamp(0.0, 1.0)
    } else {
        // A retro, dark until something is being slowed.
        (-along).clamp(0.0, 1.0)
    }
}

/// The drive cells themselves brighten with the throttle, so the mouth of an
/// engine is hot before any flame comes out of it.
pub(crate) fn glow_engines(hulls: Query<(&Hull, &Transform)>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (hull, xf) in &hulls {
        // The main drive's own throttle: the surface is shared by every bell
        // on the ship and cannot be lit two ways at once.
        let t = throttle_of(hull, xf.rotation * Vec3::Z, -1.0);
        let Some(mat) = hull.surface_mats.get(SURF_DRIVE as usize) else { continue };
        if let Some(m) = materials.get_mut(mat) {
            m.emissive = LinearRgba::rgb(3.4 * t, 1.5 * t * t, 0.35 * t * t);
        }
    }
}
