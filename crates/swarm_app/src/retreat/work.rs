//! Cutting: the one verb every support ship has, pointed at different
//! things.
//!
//! A miner's shaft, a salvager's cut and the swarm's own bite take cells off
//! a voxel model through the same damage grid, which is why a rock a miner
//! has worked looks like a hull the swarm has chewed. What tells them apart
//! is `economy::yield_of`, which reads the cell that came off.

use crate::*;

/// Give the selection a job, when the thing under the cursor is one and the
/// selection can do it.
///
/// This CONSUMES the right button when it takes it, so the rule that a press
/// is read by exactly one system a frame still holds: this runs before
/// `nav_input`, and a press it used is not there to be read as a move order.
/// A right click on a rock with a miner selected is a mine order; the same
/// click with a frigate selected is the move order it always was.
pub(crate) fn assign_work(
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mode: Res<OrderMode>,
    targets: Query<(Entity, &Hull, &Transform), Or<(With<Rock>, With<Wreck>)>>,
    rocks: Query<&Rock>,
    mut crews: Query<(Entity, &Support, &mut Job), With<Selected>>,
    mut commands: Commands,
    mut ack: ResMut<Ack>,
) {
    if *mode != OrderMode::Idle || !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    if crews.is_empty() {
        return;
    }
    let Some(ray) = cursor_ray(&windows, &cams) else {
        return;
    };
    // The nearest workable thing the ray passes through. A sphere is enough
    // to pick with: what the cut lands on is decided cell by cell later, and
    // a player aiming at a rock is aiming at a rock.
    let mut best: Option<(f32, Entity, bool)> = None;
    for (e, hull, xf) in &targets {
        let r = hull.model.radius() * xf.scale.x.max(1e-3);
        let to = xf.translation - ray.origin;
        let along = to.dot(*ray.direction);
        if along <= 0.0 {
            continue;
        }
        if (to - *ray.direction * along).length() > r {
            continue;
        }
        if best.is_none_or(|(t, _, _)| along < t) {
            best = Some((along, e, rocks.contains(e)));
        }
    }
    let Some((_, target, is_rock)) = best else {
        return;
    };
    // A rock with no seam left is stone, and a miner sent to it would fly
    // out, cut for a minute and come back with nothing. Say so rather than
    // taking the order: the button is still consumed, because the click was
    // about the rock either way.
    if rocks.get(target).is_ok_and(|r| r.seam == 0) {
        buttons.clear_just_pressed(MouseButton::Right);
        ack.text = "that rock is mined out".into();
        ack.left = ACK_LIFE;
        return;
    }
    let mut given = 0;
    for (_, support, mut job) in &mut crews {
        let can = if is_rock {
            support.role.cuts_rock()
        } else {
            support.role.cuts_wreck()
        };
        if !can {
            continue;
        }
        *job = Job::Work(target);
        given += 1;
    }
    if given == 0 {
        return;
    }
    // An ordered ship stops keeping station, exactly as one given a move
    // order does: a ship that flew to a rock and straight back to its slot
    // would be a job that did nothing.
    for (e, _, job) in &crews {
        if matches!(*job, Job::Work(_)) {
            commands.entity(e).remove::<Escort>();
        }
    }
    buttons.clear_just_pressed(MouseButton::Right);
    ack.text = format!(
        "{given} {} to work",
        if given == 1 { "ship" } else { "ships" }
    );
    ack.left = ACK_LIFE;
}

/// What the harness asked this system to do, and when.
///
/// The headless equivalent of the two things a player presses: the right
/// click that puts a ship to work, and the button that jumps. Same terms the
/// range's `--fire` keeps: a picture of a miner cutting a shaft cannot be
/// taken by a harness that has no pointer, and one taken by a harness that
/// called `work_jobs` itself would be a picture of the harness.
#[derive(Resource, Default)]
pub(crate) struct Script {
    /// The tick every support ship goes to work at.
    pub(crate) job: Option<u32>,
    /// The tick the drive finishes spooling at, which is the moment the
    /// picture is of. A scripted jump is not paid for: half an hour of
    /// mining is not a thing a headless run can afford to render.
    pub(crate) jump: Option<u32>,
    /// Take the first branch off the map without waiting to be told, so a
    /// headless run can photograph the system AFTER a jump: the scars, the
    /// roster and the bank a run carries only show in the next one.
    pub(crate) onward: bool,
}

pub(crate) fn script_jobs(
    tick: Res<Tick>,
    auto: Res<Script>,
    targets: Query<(Entity, &Transform, Option<&Rock>), Or<(With<Rock>, With<Wreck>)>>,
    mut crews: Query<(Entity, &Support, &mut Job, &Transform)>,
    mut commands: Commands,
) {
    if auto.job != Some(tick.tick) {
        return;
    }
    for (entity, support, mut job, xf) in &mut crews {
        // The nearest thing this role can work, which is the rock a player
        // would have picked and is the only choice a script can defend.
        let mut best: Option<(f32, Entity)> = None;
        for (e, at, rock) in &targets {
            let can = match rock {
                Some(r) => support.role.cuts_rock() && r.seam > 0,
                None => support.role.cuts_wreck(),
            };
            if !can {
                continue;
            }
            let d = at.translation.distance_squared(xf.translation);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, e));
            }
        }
        let Some((_, target)) = best else { continue };
        *job = Job::Work(target);
        commands.entity(entity).remove::<Escort>();
        if let Ok((_, at, _)) = targets.get(target) {
            info!(
                "{} to work on the body at {:.1},{:.1},{:.1}",
                support.role.label(),
                at.translation.x,
                at.translation.y,
                at.translation.z
            );
        }
    }
}

/// Fly the job, cut what it is on, and bring the hold home when it is full.
#[allow(clippy::too_many_arguments)]
pub(crate) fn work_jobs(
    tick: Res<Tick>,
    mut crews: Query<(Entity, &Support, &mut Job, &mut Hull, &mut Hold, &Transform)>,
    mut targets: Query<(&mut Hull, &Transform, Option<&mut Rock>), Without<Support>>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    mut bank: ResMut<Bank>,
    mut sparks: ResMut<SparkQueue>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
    mut cube: Local<Option<Handle<Mesh>>>,
) {
    for (entity, support, mut job, mut hull, mut hold, xf) in &mut crews {
        if hull.dead_hull {
            continue;
        }
        match *job {
            Job::Idle => {}
            Job::Work(target) => {
                let Ok((mut rock, rock_xf, mut seam)) = targets.get_mut(target) else {
                    // What it was cutting is gone: a rock mined out is still
                    // there, but a wreck that aged out is not.
                    *job = Job::Idle;
                    hull.order = None;
                    continue;
                };
                let scale = rock_xf.scale.x.max(1e-3);
                let (out, stand) = cutter_stand(&rock, rock_xf, &hull, xf);
                hull.order = Some(rock_xf.translation + out * stand * WORK_STANDOFF);
                let gap = xf.translation.distance(rock_xf.translation);
                if gap > stand * WORK_REACH || !tick.tick.is_multiple_of(CUT_TICKS) {
                    continue;
                }
                // A rock with nothing left in it is stone, and stone is
                // worth no yield at all: what is left to cut is not a
                // reason to go on cutting it.
                let cut = match seam.as_deref() {
                    Some(r) if r.seam == 0 => {
                        *job = Job::Unload(None);
                        continue;
                    }
                    Some(r) => Cut::Rock(r.flavour),
                    None => Cut::Hull,
                };
                let (got, cells, chunks, seams) = cut_once(
                    &mut rock,
                    rock_xf,
                    xf.translation,
                    cut,
                    tick.tick,
                    &mut sparks,
                );
                if let Some(rock) = seam.as_deref_mut() {
                    rock.seam = rock.seam.saturating_sub(seams);
                }
                throw_chunks(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &mut chunk_mats,
                    &mut cube,
                    rock.model.cell * scale,
                    rock_xf,
                    &chunks,
                );
                hold.got += got;
                hold.cells += cells;
                if hold.full() {
                    *job = Job::Unload(Some(target));
                }
                if cells == 0 {
                    // The shaft came out the far side: nothing is left along
                    // that line, and the answer is to WALK ROUND rather than
                    // to go home, because a rock is done when it has nothing
                    // left in it (the arm above) and not when one face of it
                    // is used up. The first cut of this ever written gave up
                    // here and left three quarters of the field's ice in the
                    // ground.
                    let round = Quat::from_axis_angle(out.any_orthonormal_vector(), WORK_STEP);
                    hull.order = Some(rock_xf.translation + round * out * stand * WORK_STANDOFF);
                }
            }
            Job::Unload(back) => unload_hold(
                (entity, support, &mut job, &mut hull, &mut hold, xf),
                back,
                &lead,
                &cfg,
                &mut bank,
                &mut commands,
            ),
        }
    }
}

/// A full hold, home to the command ship, emptied into the bank, and then
/// back to what it was cutting.
///
/// The crew's own row of the query is passed whole rather than as six
/// arguments: it is one thing, a ship with a job and a hold, and taking it
/// apart at this boundary would only put it back together on the other side.
fn unload_hold(
    crew: (Entity, &Support, &mut Job, &mut Hull, &mut Hold, &Transform),
    back: Option<Entity>,
    lead: &Lead,
    cfg: &SwarmConfig,
    bank: &mut Bank,
    commands: &mut Commands,
) {
    let (entity, support, job, hull, hold, xf) = crew;
    // In the flagship's RADII, and in its frame, like every other station in
    // this game: a distance in bare units means one thing on a corvette and
    // another on a heavy cruiser.
    hull.order = Some(lead.pos + lead.rot * (UNLOAD_STATION * cfg.hull_radius));
    if xf.translation.distance(lead.pos) > cfg.hull_radius * UNLOAD_REACH {
        return;
    }
    if hold.cells > 0 {
        info!(
            "{} unloaded {} cells: {:?}",
            support.role.label(),
            hold.cells,
            hold.got
        );
        bank.take(hold.got);
        hold.got = Yield::NOTHING;
        hold.cells = 0;
    }
    match back {
        Some(t) => *job = Job::Work(t),
        None => {
            *job = Job::Idle;
            hull.order = None;
            // Back into the formation, on the station it was given when it
            // was built, since a ship with nothing to do should be somewhere
            // a player can find it.
            commands.entity(entity).insert(Escort {
                station: support.station,
            });
        }
    }
}

/// Where a cutter stands to work a body: the way out from it, and how far.
///
/// How near a cutter can get is not the job's to decide. `fly_hull` holds
/// every ship off a rock by the rock's own navigation sphere plus its own
/// radii, so a standoff written against the ROCK alone is a standoff the
/// flight rule can refuse, and then a miner hovers just outside its own
/// reach for ever with nothing in the log to say so. It is written in the
/// avoidance's own terms instead, and the reach is slack on top of where the
/// ship will actually end up standing.
fn cutter_stand(rock: &Hull, rock_xf: &Transform, ship: &Hull, ship_xf: &Transform) -> (Vec3, f32) {
    let scale = rock_xf.scale.x.max(1e-3);
    let stand = rock.model.volume_radius() * scale * ROCK_HULL + ship.model.radius() * ROCK_CLEAR;
    // On the side it came from, so two ships working one rock do not fly
    // through each other to reach it.
    let out = (ship_xf.translation - rock_xf.translation).normalize_or(Vec3::Y);
    (out, stand)
}

/// One bite of a cutter: bore from the surface toward what is worth having,
/// and answer what came off.
fn cut_once(
    target: &mut Hull,
    xf: &Transform,
    from: Vec3,
    cut: Cut,
    tick: u32,
    sparks: &mut SparkQueue,
) -> (Yield, u32, Vec<Chunk>, u32) {
    let inv = xf.to_matrix().inverse();
    let origin = inv.transform_point3(from);
    // Aim at the nearest seam rather than at the middle: a miner cuts TOWARD
    // the ore, so the shaft it leaves is a shaft somebody drove on purpose.
    // With no seam left (or a hull, which is worth cutting anywhere) the
    // middle is the honest answer.
    let aim = nearest_seam(target, origin).unwrap_or(Vec3::ZERO);
    let dir = (aim - origin).normalize_or(-origin.normalize_or(Vec3::Z));
    let live = |n: usize| !target.damage.is_dead(n);
    let Some(hit) =
        swarm_core::ray::march(&target.model, live, origin.to_array(), dir.to_array(), 1e4)
    else {
        return (Yield::NOTHING, 0, Vec::new(), 0);
    };
    let depth = target.model.cell * CUT_DEPTH;
    let breaches = target
        .damage
        .bore(&target.model, hit.point, dir.to_array(), depth, tick);
    let mut got = Yield::NOTHING;
    let mut chunks = Vec::new();
    let mut seams = 0;
    for b in &breaches {
        let n = b.cell as usize;
        got += yield_of(cut, target.model.grid[n], target.model.purp[n]);
        if target.model.grid[n] == swarm_core::mat::ACCENT {
            seams += 1;
        }
        let at = xf.transform_point(Vec3::from(target.model.centre_of(n)));
        let out = xf.rotation * Vec3::from(b.outward);
        let mut list = Vec::new();
        breach_sparks(
            b.cell,
            b.tick,
            at.to_array(),
            out.to_array(),
            target.model.cell,
            &mut list,
        );
        sparks.extend(list);
        chunks.push(chunk_for(&target.model, b));
    }
    target.breaches += breaches.len();
    (got, breaches.len() as u32, chunks, seams)
}

/// The nearest live seam cell to a point in the model's own frame.
fn nearest_seam(target: &Hull, from: Vec3) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for n in 0..target.model.len() {
        if target.model.grid[n] != swarm_core::mat::ACCENT || target.damage.is_dead(n) {
            continue;
        }
        let p = Vec3::from(target.model.centre_of(n));
        let d = p.distance_squared(from);
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, p));
        }
    }
    best.map(|(_, p)| p)
}
