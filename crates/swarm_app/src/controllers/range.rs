//! The sandbox's controls: the toggles, the debug blast, the target reset,
//! and the range, where a weapon armed puts a click on a cell of the dummy.
//!
//! One rule from the move order: a button press is read by exactly one
//! system per frame, and the mode decides which. Arming a weapon is a mode,
//! `OrderMode::Range`, so a left click is a shot and never a selection, and
//! Escape disarms before it can reach the pause menu.

use crate::*;

/// The sandbox's state. Read whether or not the scene is a sandbox, and
/// always off outside one.
#[derive(Resource, Default)]
pub(crate) struct Sandbox {
    pub(crate) frozen: bool,
    pub(crate) slow: bool,
    pub(crate) invulnerable: bool,
    pub(crate) weapon: Option<Weapon>,
    /// Shots landed on the range, for the readout.
    pub(crate) shots: u32,
    /// Actions asked for this frame that need more than a flag flipped.
    pub(crate) pending: Vec<SandboxAction>,
    /// `--fire WEAPON,TICK`: one shot at the dummy's centre, off centre by
    /// a little, from the camera, so a headless run can photograph a tumble.
    pub(crate) auto: Option<(Weapon, u32)>,
    /// `--blast TICK`: one blast down the camera's own line, for the same
    /// reason and because a blast is aimed with a cursor and a headless run
    /// has none.
    pub(crate) auto_blast: Option<u32>,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SandboxAction {
    Freeze,
    Slow,
    Invulnerable,
    Blast,
    Reset,
    Arm(Weapon),
    Disarm,
}

/// What a torpedo does when it gets there.
#[derive(Clone, Copy)]
pub(crate) struct Landing {
    pub(crate) at: Vec3,
    pub(crate) dir: Vec3,
    pub(crate) weapon: Weapon,
}

/// Payloads whose rounds arrived this frame, handed over by `fly_tracers`.
#[derive(Resource, Default)]
pub(crate) struct Landed(pub(crate) Vec<Landing>);

/// The keys and the panel's buttons, which are the same actions asked for
/// twice, and the flags copied to where they act.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sandbox_input(
    keys: Res<ButtonInput<KeyCode>>,
    presses: Query<(&Interaction, &SandboxAction), Changed<Interaction>>,
    mut sb: ResMut<Sandbox>,
    mut mode: ResMut<OrderMode>,
    mut cfg: ResMut<SwarmConfig>,
    mut scene: ResMut<SceneSpec>,
    mut flagship: Query<&mut Hull, With<Flagship>>,
) {
    if !scene.sandbox {
        return;
    }
    let mut actions: Vec<SandboxAction> = presses
        .iter()
        .filter(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, a)| *a)
        .collect();
    for (key, action) in [
        (KeyCode::KeyZ, SandboxAction::Freeze),
        (KeyCode::KeyX, SandboxAction::Slow),
        (KeyCode::KeyV, SandboxAction::Invulnerable),
        (KeyCode::KeyB, SandboxAction::Blast),
        (KeyCode::KeyN, SandboxAction::Reset),
        (KeyCode::Digit0, SandboxAction::Disarm),
    ] {
        if keys.just_pressed(key) {
            actions.push(action);
        }
    }
    for w in Weapon::ALL {
        if keys.just_pressed(w.key()) {
            actions.push(SandboxAction::Arm(w));
        }
    }
    if keys.just_pressed(KeyCode::Escape) && *mode == OrderMode::Range {
        actions.push(SandboxAction::Disarm);
    }
    for a in actions {
        match a {
            SandboxAction::Freeze => sb.frozen = !sb.frozen,
            SandboxAction::Slow => sb.slow = !sb.slow,
            SandboxAction::Invulnerable => sb.invulnerable = !sb.invulnerable,
            SandboxAction::Arm(w) if *mode == OrderMode::Idle || *mode == OrderMode::Range => {
                sb.weapon = Some(w);
                *mode = OrderMode::Range;
            }
            SandboxAction::Arm(_) => {}
            SandboxAction::Disarm => {
                sb.weapon = None;
                if *mode == OrderMode::Range {
                    *mode = OrderMode::Idle;
                }
            }
            SandboxAction::Blast | SandboxAction::Reset => sb.pending.push(a),
        }
    }
    cfg.frozen = sb.frozen;
    scene.time_scale = if sb.slow { 0.25 } else { 1.0 };
    cfg.time_scale = scene.time_scale;
    for mut h in &mut flagship {
        h.invulnerable = sb.invulnerable;
    }
}

/// The two actions that touch the world: a blast where the cursor points on
/// the plane through the flagship, and a fresh dummy.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sandbox_fire(
    mut sb: ResMut<Sandbox>,
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    cfg: Res<SwarmConfig>,
    tex: Res<Textures>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    dummies: Query<Entity, With<Dummy>>,
    lead: Query<&Transform, With<Flagship>>,
    // Every hull the blast can reach: the flagship, its wing, the dummy and
    // the carriers. A blast that only moved the swarm was a blast a player
    // could not aim at anything.
    mut hulls: Query<(&mut Hull, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
    mut commands: Commands,
    mut forge: Forge,
    mut cube: Local<Option<Handle<Mesh>>>,
) {
    let scripted = sb.auto_blast == Some(tick.tick);
    if scripted {
        info!("sandbox: scripted blast at tick {}", tick.tick);
        sb.pending.push(SandboxAction::Blast);
    }
    let pending = std::mem::take(&mut sb.pending);
    for a in pending {
        match a {
            SandboxAction::Reset => {
                for e in &dummies {
                    commands.entity(e).despawn();
                }
                spawn_dummy(
                    &mut commands,
                    &mut forge.meshes,
                    &mut forge.materials,
                    &tex,
                    &scene,
                    cfg.hull_radius,
                );
                sb.shots = 0;
            }
            SandboxAction::Blast => {
                let Some(at) = blast_point(&windows, &cams, &lead, &hulls, &cfg, scripted) else {
                    continue;
                };
                let radius = cfg.hull_radius * 0.9;
                let cube = cube
                    .get_or_insert_with(|| forge.meshes.add(Cuboid::from_length(0.06)))
                    .clone();
                let (took, hit) = blast_hulls(
                    &mut hulls,
                    at,
                    radius,
                    tick.tick,
                    cube,
                    &mut commands,
                    &mut forge,
                );

                // SAY what it did. The complaint that started this was that a
                // blast was all fireball and no consequence, and a picture of
                // an explosion cannot tell a shot that took three hundred
                // cells off a carrier from one that went off in empty space.
                info!(
                    "blast at {:.1},{:.1},{:.1} r {:.1}: {} cells off {} hulls",
                    at.x, at.y, at.z, radius, took, hit
                );
                fx.blasts.push(Blast {
                    at: at.to_array(),
                    radius,
                    born: tick.tick,
                });
                let mut list = Vec::new();
                blast_sparks(tick.tick ^ 0xB1A5, at.to_array(), radius, 220, &mut list);
                fx.sparked += list.len();
                sparks.extend(list);
                flash_light(&mut commands, at, radius, BLAST_LUMENS, tick.tick);
            }
            _ => {}
        }
    }
}

/// Where a sandbox blast goes off: whatever is in FRONT of the cursor, or its
/// own full range into open space when the line meets nothing.
///
/// It used to land on the horizontal plane through the flagship, which is the
/// nav disc's rule and wrong for a shot: aiming at a carrier standing above
/// that plane put the blast on the floor underneath it. It is the same ray the
/// range weapons take, against every hull rather than only the dummy, and a
/// shot that meets nothing carries on and goes off out there, which is what a
/// shell does.
///
/// A SCRIPTED blast is aimed from the camera at the FLAGSHIP, the way a
/// scripted shot is aimed at the dummy. Two reasons, and the second is the
/// point of the flag. The camera's own forward is the honest answer to "where
/// is the cursor" when there is no cursor, and in the scene the harness renders
/// it threads the gap between the flagship and the dummy, so the flag went off
/// in empty space every time and proved nothing. And the ship this has to land
/// on is the PLAYER's: the range weapons only ever reach a dummy, so a blast
/// taking cells off the flagship is exactly the thing no other flag can
/// photograph.
fn blast_point(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    lead: &Query<&Transform, With<Flagship>>,
    hulls: &Query<(&mut Hull, &Transform)>,
    cfg: &SwarmConfig,
    scripted: bool,
) -> Option<Vec3> {
    let ray = aim_ray(windows, cams)?;
    let ray = match scripted.then(|| lead.iter().next()).flatten() {
        Some(xf) => Ray3d::new(
            ray.origin,
            Dir3::new(xf.translation - ray.origin).unwrap_or(Dir3::Z),
        ),
        None => ray,
    };
    let dir = *ray.direction;
    let reach = cfg.hull_radius * BLAST_REACH;
    Some(nearest_hull_hit(hulls, ray.origin, dir, reach).unwrap_or(ray.origin + dir * reach))
}

/// What a blast does to every hull it reaches: a sphere of cells off each,
/// and the chunks those breaches throw. Returns how many cells came off and
/// how many hulls it landed on, which is what the log reports.
///
/// The swarm feels the blast as a capsule, which it always did. What is new is
/// that the HULLS do: a blast that only killed motes was an explosion a player
/// had to take on trust.
///
/// The hole is `HULL_HOLE` of what the swarm feels, which is the reactor's own
/// rule reused rather than a second number: a pressure wave goes further than
/// the wreck it makes. At the full radius a blast is nine tenths of a hull
/// radius and kills every cell inside it outright, so one shot took 4818 cells
/// off a 6486 cell frigate and the range's target simply vanished. At three
/// tenths of that it opens a crater a player can look at: 553 off a Terran
/// frigate, measured, and the ship flies on.
fn blast_hulls(
    hulls: &mut Query<(&mut Hull, &Transform)>,
    at: Vec3,
    radius: f32,
    tick: u32,
    cube: Handle<Mesh>,
    commands: &mut Commands,
    forge: &mut Forge,
) -> (usize, usize) {
    let mut took = 0usize;
    let mut hit = 0usize;
    for (mut hull, xf) in hulls.iter_mut() {
        if hull.dead_hull || hull.invulnerable {
            continue;
        }
        let hull = &mut *hull;
        let local = xf.to_matrix().inverse().transform_point3(at);
        // In the hull's own frame, and so is the radius: a carrier is drawn
        // scaled and `blast_cells` works in cells.
        let scale = xf.scale.x.max(1e-4);
        let hole = radius * HULL_HOLE / scale;
        if local.length() > hull.model.radius() + hole {
            continue;
        }
        let b = Blast {
            at: local.to_array(),
            radius: hole,
            born: tick,
        };
        let breaches = hull.damage.blast_cells(&hull.model, &b, tick);
        if breaches.is_empty() {
            continue;
        }
        hull.breaches += breaches.len();
        took += breaches.len();
        hit += 1;
        let chunks: Vec<Chunk> = breaches
            .iter()
            .map(|br| chunk_for(&hull.model, br))
            .collect();
        throw_chunks(
            chunks,
            xf,
            cube.clone(),
            commands,
            &mut forge.materials,
            &mut forge.chunks,
        );
    }
    (took, hit)
}

/// How far a blast flies before it goes off in open space, in hull radii.
/// Past the carriers, so a shot aimed at nothing still crosses the picture.
const BLAST_REACH: f32 = 40.0;

/// The nearest live cell of any hull along a ray, in the world.
///
/// The same walk `first_hit` does for the range, over every hull rather than
/// over the dummies: holes included, so a blast aimed into a crater goes off
/// on the crater's floor.
fn nearest_hull_hit(
    hulls: &Query<(&mut Hull, &Transform)>,
    origin: Vec3,
    dir: Vec3,
    reach: f32,
) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for (hull, xf) in hulls.iter() {
        let inv = xf.to_matrix().inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(dir);
        let live = |n: usize| !hull.damage.is_dead(n);
        let Some(h) = swarm_core::ray::march(&hull.model, live, o.to_array(), d.to_array(), reach)
        else {
            continue;
        };
        let world = xf.transform_point(Vec3::from(h.point));
        let t = world.distance(origin);
        if best.is_none_or(|(b, _)| t < b) {
            best = Some((t, world));
        }
    }
    best.map(|(_, p)| p)
}

/// Where a blast is aimed: the cursor's ray, or the camera's own line when
/// there is no cursor to take one from.
///
/// A headless run has no window and a window with the pointer outside it has
/// no cursor position, and in both the honest answer is the middle of the
/// screen: a blast goes where you are LOOKING. It is also the only way a
/// scripted blast can be aimed at all, which is what makes this provable.
pub(crate) fn aim_ray(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Option<Ray3d> {
    if let Some(r) = cursor_ray(windows, cams) {
        return Some(r);
    }
    let (_, cam_xf) = cams.iter().next()?;
    Some(Ray3d::new(
        cam_xf.translation(),
        Dir3::new(cam_xf.forward().as_vec3()).unwrap_or(Dir3::NEG_Z),
    ))
}

/// The camera's ray through the cursor, or nothing when there is no cursor.
pub(crate) fn cursor_ray(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Option<Ray3d> {
    let window = windows.iter().next()?;
    let cursor = window.cursor_position()?;
    let (cam, cam_xf) = cams.iter().next()?;
    cam.viewport_to_world(cam_xf, cursor).ok()
}

/// A shot on the range: the armed weapon, along the camera's ray through
/// the cursor, onto the first live cell of a dummy it meets.
#[allow(clippy::too_many_arguments)]
pub(crate) fn range_input(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    tick: Res<Tick>,
    mode: Res<OrderMode>,
    cfg: Res<SwarmConfig>,
    mut sb: ResMut<Sandbox>,
    mut dummies: Query<(&mut Hull, &mut Tumble, &Transform), With<Dummy>>,
    flagship: Query<&Transform, (With<Flagship>, Without<Dummy>)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    // The scripted shot first: from the camera, at the dummy's centre of
    // mass, a little above it, so the picture is of a ship turning.
    let auto = sb.auto.filter(|(_, at)| *at == tick.tick).map(|(w, _)| w);
    if auto.is_some() {
        info!("range: scripted shot at tick {}", tick.tick);
    }
    let (weapon, ray) = if let Some(w) = auto {
        let Some((_, t, xf)) = dummies.iter().next() else {
            return;
        };
        let Some((_, cam_xf)) = cams.iter().next() else {
            return;
        };
        // A little above the centre, so the shot lands off centre and turns
        // the ship: a sixth of a radius, which is still under the deck.
        let aim = t.pivot + xf.rotation * Vec3::Y * cfg.hull_radius * 0.15;
        let eye = cam_xf.translation();
        (w, Ray3d::new(eye, Dir3::new(aim - eye).unwrap_or(Dir3::Z)))
    } else {
        if *mode != OrderMode::Range {
            return;
        }
        let Some(w) = sb.weapon else { return };
        let fire = if w.held() {
            buttons.pressed(MouseButton::Left) && tick.tick.is_multiple_of(6)
        } else {
            buttons.just_pressed(MouseButton::Left)
        };
        if !fire {
            return;
        }
        let Some(ray) = cursor_ray(&windows, &cams) else {
            return;
        };
        (w, ray)
    };
    let dir = *ray.direction;
    if weapon.flies() {
        // A torpedo goes where the ray meets the dummy, from the flagship,
        // and does its work when it gets there.
        let Some(at) = first_hit(&mut dummies, ray.origin, dir) else {
            return;
        };
        let from = flagship.iter().next().map_or(ray.origin, |x| x.translation);
        fx.tracers.push(Tracer {
            from,
            to: at,
            t: 0.0,
            rate: 1.0 / 50.0,
            burst: cfg.hull_radius * 0.5,
            // Whiter and slower than a flak round, so it reads as a different
            // thing in the air.
            colour: [3.0, 3.2, 3.6],
            payload: Some(Landing { at, dir, weapon }),
        });
        return;
    }
    if strike(
        weapon,
        &mut dummies,
        ray.origin,
        dir,
        tick.tick,
        &mut sparks,
    )
    .is_some()
    {
        sb.shots += 1;
    }
}

/// Torpedoes that arrived: struck along their own line, a little short of
/// where they were aimed so a dummy that turned meanwhile is still met.
pub(crate) fn land_torpedoes(
    tick: Res<Tick>,
    cfg: Res<SwarmConfig>,
    mut landed: ResMut<Landed>,
    mut sb: ResMut<Sandbox>,
    mut dummies: Query<(&mut Hull, &mut Tumble, &Transform), With<Dummy>>,
    mut sparks: ResMut<SparkQueue>,
) {
    for l in std::mem::take(&mut landed.0) {
        let origin = l.at - l.dir * cfg.hull_radius * 2.0;
        if strike(
            l.weapon,
            &mut dummies,
            origin,
            l.dir,
            tick.tick,
            &mut sparks,
        )
        .is_some()
        {
            sb.shots += 1;
        }
    }
}

/// Where a ray first meets any dummy, in the world.
fn first_hit(
    dummies: &mut Query<(&mut Hull, &mut Tumble, &Transform), With<Dummy>>,
    origin: Vec3,
    dir: Vec3,
) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for (hull, _, xf) in dummies.iter() {
        let inv = xf.to_matrix().inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(dir);
        let live = |n: usize| !hull.damage.is_dead(n);
        if let Some(h) = swarm_core::ray::march(&hull.model, live, o.to_array(), d.to_array(), 1e4)
        {
            if best.is_none_or(|(t, _)| h.t < t) {
                best = Some((h.t, xf.transform_point(Vec3::from(h.point))));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Land a weapon on the nearest dummy along a ray: the cells, the sparks
/// and the kick. Returns where it landed, in the world.
fn strike(
    weapon: Weapon,
    dummies: &mut Query<(&mut Hull, &mut Tumble, &Transform), With<Dummy>>,
    origin: Vec3,
    dir: Vec3,
    tick: u32,
    sparks: &mut SparkQueue,
) -> Option<Vec3> {
    for (mut hull, mut t, xf) in dummies.iter_mut() {
        let hull = &mut *hull;
        let inv = xf.to_matrix().inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(dir);
        let hit = {
            let live = |n: usize| !hull.damage.is_dead(n);
            swarm_core::ray::march(&hull.model, live, o.to_array(), d.to_array(), 1e4)
        };
        let Some(hit) = hit else {
            info!("range: the ray from {o:?} along {d:?} met nothing");
            continue;
        };
        let dir_local = swarm_core::fx::normalise(d.to_array());
        let breaches = land(weapon, &hull.model, &mut hull.damage, &hit, dir_local, tick);
        for br in &breaches {
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
            sparks.extend(spray);
            hull.breaches += 1;
        }
        let point = xf.transform_point(Vec3::from(hit.point));
        if weapon.kick() > 0.0 {
            let j = dir.normalize_or_zero() * weapon.kick() * t.body.mass;
            t.kick(hull, xf, point, j);
        }
        info!(
            "{} landed: {} cells off, spin {:.1} deg/s",
            weapon.label(),
            breaches.len(),
            t.spin.length().to_degrees()
        );
        return Some(point);
    }
    None
}
