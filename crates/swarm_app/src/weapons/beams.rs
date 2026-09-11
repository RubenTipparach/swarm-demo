//! Beams: fired at carriers, resolved against them, drawn edge on.

use crate::*;

/// A beam is fired at its full range and CUT at what it reached.
///
/// redux-tribes' own rule, and the reason for it: the full range endpoint is
/// what a MISS looks like, so a beam that ran out into space is not a defect
/// and a beam that carried on through what it hit is. Cutting it also means a
/// carrier is cover: the capsule handed to the swarm is the short one, so the
/// fighters behind it live.
pub(crate) fn resolve_beams(
    tick: Res<Tick>,
    mut fx: ResMut<LiveFx>,
    mut hives: Query<(Entity, &Hive, &Transform, &mut Hull)>,
    mut sparks: ResMut<SparkQueue>,
) {
    for b in fx.beams.iter_mut() {
        // Only the ones fired this tick: a beam is resolved once, when it
        // goes off, and lives out its nine ticks as whatever it became.
        if b.born != tick.tick {
            continue;
        }
        // The nearest carrier along it, by fraction of the beam rather than
        // by distance from the muzzle, which is the same order and is the
        // number the cut wants anyway.
        let mut hit: Option<(f32, Entity)> = None;
        for (e, h, xf, hull) in &hives {
            if hull.dead_hull {
                continue;
            }
            let Some(t) = b.reaches(xf.translation.to_array(), h.radius) else {
                continue;
            };
            if hit.map_or(true, |(bt, _)| t < bt) {
                hit = Some((t, e));
            }
        }
        let Some((t, target)) = hit else { continue };
        *b = b.cut(t);

        let end = Vec3::from(b.to);
        // The splash where it landed, thrown back along the beam.
        let back = (Vec3::from(b.from) - end).normalize_or(Vec3::Y);
        let mut list = Vec::new();
        breach_sparks(
            tick.tick.wrapping_mul(2_654_435_761),
            tick.tick,
            end.to_array(),
            back.to_array(),
            0.5,
            &mut list,
        );
        for sp in &mut list {
            // PURPLE. What a beam splashes off a carrier is the animal, and
            // the animal is chitin over violet: it used to throw the green its
            // own lamps are lit with, so a hit read as a light rather than as
            // a thing being opened up.
            sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
            sp.size *= 1.6;
        }
        sparks.extend(list);

        // And it takes CELLS off, the way it does on a ship. The hit point is
        // taken into the carrier's own frame first, because `bite` works in
        // model coordinates and a carrier is drawn scaled, turned and a long
        // way from the origin.
        if let Ok((_, _, xf, mut hull)) = hives.get_mut(target) {
            let local = xf.to_matrix().inverse().transform_point3(end);
            let hull = &mut *hull;
            // Several bites in a ring round the impact rather than one, so a
            // beam opens a crater the size of a beam instead of taking a
            // single cell out of a mothership.
            for k in 0..BEAM_BITES {
                let h = |q: u32| {
                    swarm_core::rng::hash_cell(tick.tick.wrapping_mul(2654435761) ^ (k * 977) ^ q)
                        as f32
                        / u32::MAX as f32
                        - 0.5
                };
                let jit = Vec3::new(h(1), h(7), h(19)) * hull.model.cell * 2.4;
                if let Some(br) = hull.damage.bite(
                    &hull.model,
                    (local + jit).to_array(),
                    BEAM_DAMAGE,
                    tick.tick,
                ) {
                    let at = xf.transform_point(Vec3::from(hull.model.centre_of(br.cell as usize)));
                    let out = xf.rotation * Vec3::from(br.outward);
                    let mut spray = Vec::new();
                    breach_sparks(
                        br.cell,
                        br.tick,
                        at.to_array(),
                        out.to_array(),
                        hull.model.cell * hull.model.cell.max(0.02),
                        &mut spray,
                    );
                    for sp in &mut spray {
                        sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
                    }
                    sparks.extend(spray);
                    hull.breaches += 1;
                }
            }
        }
    }
}

/// Guns go off on their own cadence, staggered so a broadside is a rattle
/// rather than one bang, and sweep so the beams rake the cloud instead of
/// drilling the same hole in it forever.
pub(crate) fn fire_guns(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    // Not the carriers, which are hulls now: a mothership does not
    // carry the fleet's guns and would otherwise open fire on its own side.
    hulls: Query<(&Hull, &Transform), Without<Hive>>,
    hives: Query<(&Hive, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let reach = hull.model.radius() * BEAM_RANGE;
        for (n, g) in hull.guns.iter().enumerate() {
            // Staggered by the gun's own cell, so two hulls of one class do
            // not fire in lockstep and the pattern does not read as a clock.
            let phase = (swarm_core::rng::hash_cell(g.cell ^ hull.seed) % scene.cadence) as u32;
            if (tick.tick + phase) % scene.cadence != 0 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(g.at));
            let out = (xf.rotation * Vec3::from(g.out)).normalize_or_zero();
            // A gun looks for a CARRIER, because the CPU can see one: a hive
            // is an entity and a mote is not. What it cannot do is pick a
            // mote, which is the whole reason a shot is a volume the shader
            // resolves rather than a target the CPU chose.
            //
            // Nearest one it can actually bear on, so a mount on the far
            // flank does not swing through its own ship to reach something.
            let mut target: Option<(f32, Vec3)> = None;
            for (_, hxf) in &hives {
                let to = hxf.translation - at;
                if to.normalize_or_zero().dot(out) < 0.15 {
                    continue;
                }
                let d = to.length_squared();
                if target.map_or(true, |(bd, _)| d < bd) {
                    target = Some((d, hxf.translation));
                }
            }
            // A slow rake when there is nothing to shoot at, so the guns are
            // not simply silent between carriers. Trigonometry is fine here
            // and nowhere near the core: it decides where a light is drawn.
            let t = tick.tick as f32 * 0.03 + n as f32 * 1.7;
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(out);
            let dir = match target {
                // Aimed, with a little spread, so some shots miss and run out
                // into space. A gun that never missed would make the carriers
                // a countdown rather than a fight.
                Some((_, to)) => {
                    let aim = (to - at).normalize_or(out);
                    (aim + side * (t.sin() * 0.06) + up * ((t * 0.7).cos() * 0.05)).normalize()
                }
                None => (out + side * (t.sin() * 0.45) + up * ((t * 0.7).cos() * 0.30)).normalize(),
            };
            fx.beams.push(Beam {
                from: at.to_array(),
                to: (at + dir * reach).to_array(),
                // Wide. A beam a couple of cells across cut a thread through
                // the cloud and killed almost nothing you could see; the point
                // of firing into a swarm is the swath.
                radius: hull.model.cell * BEAM_WIDTH,
                born: tick.tick,
            });
            fx.fired += 1;
            let mut list = Vec::new();
            muzzle_sparks(
                g.cell.wrapping_add(tick.tick),
                at.to_array(),
                dir.to_array(),
                hull.model.cell,
                &mut list,
            );
            sparks.extend(list);
        }
    }
}

/// Rebuild the beam mesh: one strip of three quads per beam, turned edge on to
/// the eye.
///
/// Three quads rather than one so the beam has a soft edge: the outer columns
/// carry no alpha and the inner two carry all of it, which is a bright core
/// with a falloff either side. One quad could only be a flat slab.
pub(crate) fn draw_beams(
    tick: Res<Tick>,
    fx: Res<LiveFx>,
    handle: Option<Res<BeamHandle>>,
    cam: Query<&Transform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quads: ResMut<BeamQuads>,
) {
    let Some(handle) = handle else { return };
    let Ok(eye) = cam.single() else { return };
    let eye = eye.translation;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    const OFFSETS: [f32; 4] = [-1.0, -0.34, 0.34, 1.0];
    const ALPHAS: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
    for b in &fx.beams {
        let (from, to) = (Vec3::from(b.from), Vec3::from(b.to));
        let dir = (to - from).normalize_or_zero();
        let mid = (from + to) * 0.5;
        // Edge on to the eye, so the beam is the same width from anywhere.
        let across = dir
            .cross(eye - mid)
            .normalize_or(Vec3::Y.cross(dir).normalize_or(Vec3::X));
        // It fires bright and goes out; the tail thins as it does.
        let fade = 1.0 - b.age(tick.tick);
        let w = b.radius * (0.35 + 0.65 * fade);
        // Well over white: the camera is HDR and bloom thresholds just under
        // one after tone mapping, so a beam has to CLEAR that to glow rather
        // than merely to be a pale blue line.
        let hot = Vec3::new(3.0, 5.0, 9.0) * fade;
        let base = pos.len() as u32;
        for c in 0..4 {
            let off = across * OFFSETS[c] * w;
            pos.push((from + off).to_array());
            pos.push((to + off).to_array());
            // The far end of a beam is dimmer than the muzzle, which is what
            // makes it read as travelling rather than as a painted line.
            col.push([hot.x, hot.y, hot.z, ALPHAS[c]]);
            col.push([hot.x * 0.5, hot.y * 0.5, hot.z * 0.5, ALPHAS[c] * 0.55]);
        }
        for c in 0..3u32 {
            let a = base + c * 2;
            idx.extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
        }
    }
    // The rounds in the air, as short bright bolts. Drawn here rather than in
    // their own pass because a tracer and a beam are the same picture problem:
    // a line in space that has to be the same width from anywhere, which is a
    // quad turned edge on to the eye.
    for t in &fx.tracers {
        let along = t.to - t.from;
        let at = t.from + along * t.t;
        let len = along.length() * TRACER_LEN;
        let back = along.normalize_or(Vec3::Z) * len;
        // Clipped at the muzzle, so a bolt grows out of the gun instead of
        // starting in front of it.
        let tail = t.from + along * (t.t - TRACER_LEN).max(0.0);
        let tail = if (at - tail).length() < len {
            tail
        } else {
            at - back
        };
        add_line(
            &mut pos,
            &mut col,
            &mut idx,
            eye,
            tail,
            at,
            TRACER_WIDTH,
            t.colour,
            1.0,
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    quads.0 = idx.len() / 6;
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
