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
    if rocks.get(target).is_ok_and(|r| !r.worth_cutting()) {
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

/// What a support ship can be sent to: a rock, or what is left of a ship.
///
/// `With<Hull>` is not decoration. A turret cut loose by an explosion is
/// given `Wreck` and has no `Hull` of its own, so a nearest-first scan that
/// asked only for `Wreck` would send a salvager to a gun, and `work_jobs`,
/// which needs the cells, would find no body and quietly set the job back to
/// idle. The salvager then flew home having cut nothing and nothing in the
/// log said why: the run report is what caught it, because a picture of a
/// wreck cannot tell one nobody reached from one that was cut over. The
/// filter IS the interface, and a job target is a body with cells in it.
pub(crate) type WorkableBody = Or<(With<Rock>, With<Wreck>)>;

pub(crate) fn script_jobs(
    tick: Res<Tick>,
    auto: Res<Script>,
    targets: Query<(Entity, &Transform, Option<&Rock>), (With<Hull>, WorkableBody)>,
    mut crews: Query<(Entity, &Support, &mut Job, &Transform)>,
    mut fired: Local<bool>,
    mut commands: Commands,
) {
    if !tick.cue(auto.job, &mut fired) {
        return;
    }
    for (entity, support, mut job, xf) in &mut crews {
        // The nearest thing this role can work, which is the rock a player
        // would have picked and is the only choice a script can defend.
        let mut best: Option<(f32, Entity)> = None;
        for (e, at, rock) in &targets {
            let can = match rock {
                Some(r) => support.role.cuts_rock() && r.worth_cutting(),
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
    mut targets: Query<BodyRow, Without<Support>>,
    cubes: Query<(Entity, &Cargo)>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    mut bank: ResMut<Bank>,
    mut sparks: ResMut<SparkQueue>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
    mut cube: Local<Option<Handle<Mesh>>>,
    mut cargo_mesh: Local<Option<Handle<Mesh>>>,
) {
    // Counted once for the whole pass rather than per ship: a freighter is
    // a fact about the fleet, not about the miner that happens to be home.
    let freighters = crews
        .iter()
        .filter(|(_, s, _, h, _, _)| s.role == Role::Freighter && !h.dead_hull)
        .count() as u32;
    for (entity, support, mut job, mut hull, mut hold, xf) in &mut crews {
        if hull.dead_hull {
            continue;
        }
        match *job {
            Job::Idle => {}
            Job::Work(target) => {
                let Ok((rock, rock_xf, seam, scan, part)) = targets.get_mut(target) else {
                    // What it was working is gone: a rock mined out is still
                    // there, but a wreck that aged out is not.
                    *job = Job::Idle;
                    hull.order = None;
                    continue;
                };
                let crew = (entity, &mut *job, &mut *hull, &mut *hold, xf);
                let world = (
                    &mut commands,
                    &mut *meshes,
                    &mut *materials,
                    &mut *chunk_mats,
                    &mut *cargo_mesh,
                );
                // A survey ship READS a body and a cutter takes it apart:
                // the same standoff, the same hold and the same trip home,
                // and two jobs so neither is an `if` inside the other.
                if support.role.scans() {
                    survey_body(crew, (rock, rock_xf, scan), target, tick.tick, world);
                } else {
                    work_body(
                        crew,
                        (rock, rock_xf, seam, part),
                        target,
                        tick.tick,
                        &mut sparks,
                        world,
                        &mut cube,
                    );
                }
            }
            Job::Unload(back) => unload_hold(
                (entity, support, &mut job, &mut hull, &mut hold, xf),
                back,
                &lead,
                &cfg,
                (&mut bank, &cubes, freighters),
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
    into: (&mut Bank, &Query<(Entity, &Cargo)>, u32),
    commands: &mut Commands,
) {
    let (entity, support, job, hull, hold, xf) = crew;
    // The bank, the cubes and the freighters travel together because they
    // are three halves of one question: what this load is worth landed.
    let (bank, cubes, freighters) = into;
    // In the flagship's RADII, and in its frame, like every other station in
    // this game: a distance in bare units means one thing on a corvette and
    // another on a heavy cruiser.
    hull.order = Some(lead.pos + lead.rot * (UNLOAD_STATION * cfg.hull_radius));
    if xf.translation.distance(lead.pos) > cfg.hull_radius * UNLOAD_REACH {
        return;
    }
    // What is landed is the CUBES it is actually carrying, not a number it
    // has been keeping: the cubes are the cargo, so they are what is asked.
    let load = cubes_of(cubes, entity);
    if !load.is_empty() {
        let mut got = Yield::NOTHING;
        for (e, kind) in &load {
            got += kind.worth();
            commands.entity(*e).despawn();
        }
        // And the freighters take their cut, which is the freighter's whole
        // job: it cuts nothing and it makes every other ship's trip worth
        // more, so the first one pays for itself the moment two cutters are
        // working. On what is LANDED, so a freighter is worth nothing to a
        // fleet that is not gathering.
        let lift = 1.0 + FREIGHT_SHARE * freighters as f32;
        if freighters > 0 {
            got.materials = (got.materials as f32 * lift) as u32;
            got.volatiles = (got.volatiles as f32 * lift) as u32;
            got.data = (got.data as f32 * lift) as u32;
        }
        info!(
            "{} landed {} cubes: {:?} ({freighters} freighters)",
            support.role.label(),
            load.len(),
            got
        );
        bank.take(got);
        hold.carrying = 0;
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

/// A body a CUTTER holds while it works it: what it is made of, where it
/// is, what seam is left in it, and what ship it used to be.
///
/// Named rather than written out, which is what keeps `work_body` readable
/// and is what clippy's complex type rule is actually asking for.
pub(crate) type CutRow<'a> = (
    Mut<'a, Hull>,
    &'a Transform,
    Option<Mut<'a, Rock>>,
    Option<Mut<'a, Remains>>,
);

/// One cutter, one body, one tick of work: stand off it, take a bite when
/// the clock says, count what came off the rock, and pop a cube whenever the
/// loose pile makes one.
///
/// Its own function because `work_jobs` is a LOOP and a dispatch and this is
/// what the Work arm of it does, which is two things and therefore two
/// functions. The crew's row and the target's row are passed whole, the way
/// `unload_hold` takes its crew: they are each one thing.
#[allow(clippy::too_many_arguments)]
fn work_body(
    crew: (Entity, &mut Job, &mut Hull, &mut Hold, &Transform),
    body: CutRow,
    target: Entity,
    tick: u32,
    sparks: &mut SparkQueue,
    world: Spawner,
    cube: &mut Option<Handle<Mesh>>,
) {
    let (entity, job, hull, hold, xf) = crew;
    let (mut rock, rock_xf, mut seam, mut part) = body;
    let scale = rock_xf.scale.x.max(1e-3);
    let (out, stand) = cutter_stand(&rock, rock_xf, hull, xf);
    hull.order = Some(rock_xf.translation + out * stand * WORK_STANDOFF);
    let gap = xf.translation.distance(rock_xf.translation);
    if gap > stand * WORK_REACH || !tick.is_multiple_of(CUT_TICKS) {
        return;
    }
    // A rock with nothing left in it is stone, and stone is worth no yield
    // at all: what is left to cut is not a reason to go on cutting it.
    let cut = match seam.as_deref() {
        Some(r) if !r.worth_cutting() => {
            *job = Job::Unload(None);
            return;
        }
        Some(_) => Cut::Rock,
        None => Cut::Hull,
    };
    let (got, cells, chunks) = cut_once(&mut rock, rock_xf, xf.translation, cut, tick, sparks);
    // What came off a rock is counted off the rock, so the one number a
    // player reads ("is there anything left here") is the number the cutter
    // is actually working against.
    if let Some(rock) = seam.as_deref_mut() {
        rock.ore = rock.ore.saturating_sub(got.materials);
        rock.crystal = rock.crystal.saturating_sub(got.volatiles);
    }
    // And a piece of a dead ship remembers what came off it, which is what
    // decides later whether there is enough of that ship left to rebuild.
    // The cubes are the same cubes: recovering parts pays either way, and
    // the choice a player makes is whether to spend the bank putting them
    // back together.
    if let Some(part) = part.as_deref_mut() {
        part.got += cells;
    }
    let (commands, meshes, materials, mats, mesh) = world;
    throw_chunks(
        chunks,
        rock_xf,
        chunk_cube(meshes, cube, rock.model.cell * scale),
        commands,
        materials,
        mats,
    );
    // What the cut freed goes into the loose pile, and every time that makes
    // a CUBE one comes off the rock.
    hold.loose += got;
    pop_cubes(
        hold,
        cut.pack(),
        entity,
        rock_xf.translation + out * stand * 0.72,
        out,
        CUBE_SIZE * hull.model.radius(),
        (commands, meshes, materials, mats, mesh),
    );
    if hold.full() {
        *job = Job::Unload(Some(target));
    }
    if cells == 0 {
        // The shaft came out the far side: nothing is left along that line,
        // and the answer is to WALK ROUND rather than to go home, because a
        // rock is done when it has nothing left in it (the arm above) and
        // not when one face of it is used up. The first cut of this gave up
        // here and left three quarters of the field's crystal in the ground.
        let round = Quat::from_axis_angle(out.any_orthonormal_vector(), WORK_STEP);
        hull.order = Some(rock_xf.translation + round * out * stand * WORK_STANDOFF);
    }
}

/// A body a support ship can work: what it is made of, where it is, what is
/// still in it, and what is still to be learned about it.
pub(crate) type BodyRow<'a> = (
    &'a mut Hull,
    &'a Transform,
    Option<&'a mut Rock>,
    Option<&'a mut Scanned>,
    Option<&'a mut Remains>,
);

/// One survey ship, one body, one pass: stand off it and read it.
///
/// It takes nothing off, which is the whole difference from a cut: no bore,
/// no chunks, no sparks and no hole. What fills its hold is DATA, and the
/// body is what remembers how much of it is left, so a rock somebody has
/// already surveyed is a rock nobody profits from surveying again.
fn survey_body(
    crew: (Entity, &mut Job, &mut Hull, &mut Hold, &Transform),
    body: (Mut<Hull>, &Transform, Option<Mut<Scanned>>),
    target: Entity,
    tick: u32,
    world: Spawner,
) {
    let (entity, job, hull, hold, xf) = crew;
    let (rock, rock_xf, scan) = body;
    let (out, stand) = cutter_stand(&rock, rock_xf, hull, xf);
    hull.order = Some(rock_xf.translation + out * stand * WORK_STANDOFF);
    let gap = xf.translation.distance(rock_xf.translation);
    if gap > stand * WORK_REACH || !tick.is_multiple_of(CUT_TICKS) {
        return;
    }
    let (commands, meshes, materials, mats, mesh) = world;
    let got = match scan {
        Some(mut known) => {
            let take = SCAN_RATE.min(known.left);
            known.left -= take;
            take
        }
        // Never read before: what there is to learn is a share of what is
        // in it, and the body keeps the rest.
        None => {
            let worth = scan_worth(&rock.model);
            let take = SCAN_RATE.min(worth);
            commands
                .entity(target)
                .insert(Scanned { left: worth - take });
            take
        }
    };
    if got == 0 {
        // Nothing left to learn here. Home, and then somewhere else.
        *job = Job::Unload(None);
        return;
    }
    hold.loose.data += got;
    // A SEAM whatever the body is, and that is the survey ship's whole
    // argument rather than an oversight: it learns more about a ship by
    // reading it than anybody learns by cutting it up, so its data packs at
    // eight cells a cube where a salvager's packs at sixty four.
    pop_cubes(
        hold,
        Pack::Seam,
        entity,
        rock_xf.translation + out * stand * 0.72,
        out,
        CUBE_SIZE * hull.model.radius(),
        (commands, meshes, materials, mats, mesh),
    );
    if hold.full() {
        *job = Job::Unload(Some(target));
    }
}

/// What is left to learn about a body, in cells of data.
///
/// Put on the body the first time a survey ship reads it and counted down
/// from there, so a rock that has been surveyed is a rock nobody profits
/// from surveying again. On the BODY rather than on the ship, because it is
/// a fact about the rock: a second survey ship arriving learns what is left
/// rather than starting over.
#[derive(Component)]
pub(crate) struct Scanned {
    pub(crate) left: u32,
}

/// How much there is to learn about a body: a share of what is IN it, which
/// is its seams on a rock and its machinery on a hull. A survey of a barren
/// rock is worth what a barren rock is worth.
pub(crate) fn scan_worth(m: &VoxelModel) -> u32 {
    let worth = (0..m.len())
        .filter(|&n| matches!(m.grid[n], mat::ACCENT | mat::GLOW | mat::MACHINE))
        .count() as u32;
    worth / SCAN_SHARE
}

/// Everything a cube needs to be built with, passed whole: the world to
/// spawn into, the meshes and materials to make one out of, and the caches
/// that stop a hundred cubes being a hundred of each.
type Spawner<'a, 'w, 's> = (
    &'a mut Commands<'w, 's>,
    &'a mut Assets<Mesh>,
    &'a mut Assets<StandardMaterial>,
    &'a mut ChunkMaterials,
    &'a mut Option<Handle<Mesh>>,
);

/// Turn what a cutter has loose into cubes, while it still has room.
///
/// Out of the SHAFT, at the face, where the cutter is working: a cube that
/// appeared beside the ship would be a number going up with a mesh on it.
/// The hold is what says when to stop, so a full ship stops making them and
/// the rest of the seam stays in the rock for the next trip.
fn pop_cubes(
    hold: &mut Hold,
    pack: Pack,
    to: Entity,
    at: Vec3,
    out: Vec3,
    size: f32,
    world: Spawner,
) {
    let (commands, meshes, materials, mats, mesh) = world;
    while !hold.full() {
        let Some(kind) = Cube::packed(&mut hold.loose, pack) else {
            break;
        };
        spawn_cube(
            commands, meshes, materials, mats, mesh, kind, at, out, size, to,
        );
        hold.carrying += 1;
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

/// One bite of a cutter: cut at the surface toward what is worth having, and
/// answer what came off.
///
/// The SHAPE of the cut is the one thing the kind decides. A miner drives a
/// shaft, because what it is after is a seam a few cells wide and the shaft
/// is how it reaches one. A salvager is taking a hulk APART, so it lifts a
/// section at a time: same standoff, same hold, same trip home, and one
/// parameter rather than two cutters. The rate is what makes the rebuild
/// tiers reachable at all, and it is measured rather than guessed, because a
/// bored column at three cells a cut needs half an hour to recover half a
/// frigate and there is no system that long.
fn cut_once(
    target: &mut Hull,
    xf: &Transform,
    from: Vec3,
    cut: Cut,
    tick: u32,
    sparks: &mut SparkQueue,
) -> (Yield, u32, Vec<Chunk>) {
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
        return (Yield::NOTHING, 0, Vec::new());
    };
    let breaches = match cut {
        Cut::Rock => {
            let depth = target.model.cell * CUT_DEPTH;
            target
                .damage
                .bore(&target.model, hit.point, dir.to_array(), depth, tick)
        }
        Cut::Hull => {
            let section = Blast {
                at: hit.point,
                radius: target.model.cell * SALVAGE_CUT,
                born: tick,
            };
            target.damage.blast_cells(&target.model, &section, tick)
        }
    };
    let mut got = Yield::NOTHING;
    let mut chunks = Vec::new();
    for b in &breaches {
        let n = b.cell as usize;
        got += yield_of(cut, target.model.grid[n], target.model.purp[n]);
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
    (got, breaches.len() as u32, chunks)
}

/// The nearest live seam cell to a point in the model's own frame, of EITHER
/// kind: the crystal grows on the ore, so a shaft driven at whichever is
/// nearer arrives at both, and a miner never has to be told which it is
/// after.
fn nearest_seam(target: &Hull, from: Vec3) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for n in 0..target.model.len() {
        let seam = matches!(
            target.model.grid[n],
            swarm_core::mat::ACCENT | swarm_core::mat::GLOW
        );
        if !seam || target.damage.is_dead(n) {
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
