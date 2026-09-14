//! The collector: a tiny craft with two arms that carries one cube home.
//!
//! A cutter cuts and a COLLECTOR carries, which is the split the owner asked
//! for and the mockup drew (`docs/ui/collector-mockup.html`). One civil hull
//! used to do both: it bored a shaft, towed its cubes in a line behind it,
//! flew home, landed the load and flew all the way back out, and most of a
//! system went on the transit. The cutter never leaves the rock now, its
//! cubes pile at the shaft mouth, and these ferry them.
//!
//! It is a CRAFT and not a hull: no voxel model, no damage grid, no cells,
//! which is what a fighter already is and for the same reason. Two of them
//! per cutter is a couple of dozen transforms at most, so they are entities
//! under this project's own rule about what the ECS holds.

use crate::*;

/// One collector craft, and what it is doing.
///
/// `cube` is what it has CLAIMED, which is what stops two of them flying at
/// the same one: the claim is written on the cube (`Cargo.to`) and read back
/// here, so there is one answer to "who is coming for this" and it lives on
/// the thing being come for.
#[derive(Component)]
pub(crate) struct Collector {
    pub(crate) vel: Vec3,
    pub(crate) cube: Option<Entity>,
    /// Nought folded, one closed on a cube. ONE number poses both arms,
    /// which is the mockup's own rule: everything else about the grab is
    /// geometry, so the sim publishes a float and the draw does the rest.
    pub(crate) grip: f32,
}

/// One arm's shoulder, which is what the grip swings.
#[derive(Component)]
pub(crate) struct Claw(pub(crate) f32);

/// One jaw of a claw. Two per arm, and the grip closes them.
#[derive(Component)]
pub(crate) struct Jaw(pub(crate) f32);

/// How many collectors a cutter keeps, and how often a lost one is replaced.
///
/// Two, because one is a queue: a cutter making a cube every few seconds
/// with one ferry would pile them faster than they could be taken away, and
/// three is a crowd round a shaft nobody can read.
pub(crate) const PER_CUTTER: usize = 2;

pub(crate) const COLLECTOR_REPLACE: u32 = 90;

/// A cell of the craft, in FLAGSHIP radii.
///
/// The mockup draws the craft at 0.96 units against a Terran frigate of 6.34,
/// so it is 0.15 of a frigate's length and 0.30 of its radius over six cells.
/// That is what makes it a fighter rather than a ship, which is the whole of
/// what the owner approved, and reading it off the flagship is what the
/// squadron already does (`FIGHTER_SCALE`).
///
/// It is NOT read off the cube, and the first cut was: the mockup's cube is
/// 0.36 UNITS and this game's is 0.36 of the CUTTER'S RADIUS, which is several
/// units, so a craft held to the mockup's 2.67 cubes came out five units long
/// and stood beside the flagship as another warship. A ratio ported between
/// two pictures has to be ported with the thing it was a ratio OF.
pub(crate) const CELL: f32 = 0.0503;

/// How far ahead of its middle the claws are, in cells: the shoulder at 1.6
/// plus an upper arm of 2.6 and a forearm of 2.2, which is the mockup's arm.
pub(crate) const REACH: f32 = 6.4;

/// How fast one flies and how near it has to be to take a cube or land one,
/// in its own CELLS a second and in cells. Sixty cells a second is about ten
/// of its own lengths, which is a small craft rather than a capital ship: the
/// transit out to a rock is seconds and not the minute a cutter's cruise took.
pub(crate) const SPEED: f32 = 60.0;

/// Generous, because what it has to reach is a cube several times its own
/// cell and a cube half in the claws reads exactly as one taken.
pub(crate) const GRAB: f32 = 9.0;

/// How fast the arms open and close, in grip a second.
pub(crate) const GRIP_RATE: f32 = 2.2;

/// What it takes to put a craft in the world: the two asset stores and the
/// queue that spawns it, which always travel together.
///
/// A `SystemParam` rather than three arguments, which is this project's own
/// rule about an argument list long enough for clippy to count, and the same
/// answer `Forge` and `Baked` already are.
#[derive(SystemParam)]
pub(crate) struct Yardarm<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) meshes: ResMut<'w, Assets<Mesh>>,
    pub(crate) materials: ResMut<'w, Assets<StandardMaterial>>,
}

/// Keep two collectors per cutter, out of the flagship.
///
/// It REPLACES rather than launching once, exactly as the squadron does: a
/// craft lost with the ship it was ferrying to would otherwise leave a cutter
/// piling cubes nobody comes for, and a mechanic that can only ever be spent
/// is a fuse rather than a loop.
pub(crate) fn launch_collectors(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    // Where the flagship is and how big it is, which `publish_hull` already
    // answers for the swarm: two queries here would be a second source for
    // one fact, and a craft launched off a different flagship from the one
    // the cloud is chasing is a craft launched off nothing.
    cfg: Res<SwarmConfig>,
    cutters: Query<(&Support, &Hull), Without<Flagship>>,
    have: Query<(), With<Collector>>,
    mut yard: Yardarm,
    mut next: Local<u32>,
) {
    if !scene.sieged() {
        return;
    }
    // How many cutters there are to ferry for. A survey ship makes no cubes,
    // so it gets none: what a collector carries is cargo off a cut.
    let cubes = cutters
        .iter()
        .filter(|(s, h)| !s.role.scans() && !h.dead_hull)
        .count();
    let radius = cfg.hull_radius;
    if radius <= 0.0 {
        return;
    }
    let size = radius * CELL;
    let want = cubes * PER_CUTTER;
    let out = have.iter().count();
    if out >= want || tick.tick < *next {
        return;
    }
    *next = tick.tick + COLLECTOR_REPLACE;
    // The whole set the first time and one at a time after that, which is
    // the squadron's own rule: pacing the first launch would mean a minute
    // of cubes piling before anything came for them.
    let n = if out == 0 { want - out } else { 1 };
    let kit = CraftKit::new(&mut yard.meshes, &mut yard.materials);
    for i in out..out + n {
        let a = i as f32 * 2.399_963_2;
        let at = cfg.hull_centre + Vec3::new(a.cos(), 0.35, a.sin()) * radius * 0.9;
        spawn_collector(&mut yard.commands, &kit, at, size);
    }
    if out == 0 {
        info!("{n} collectors up, {PER_CUTTER} per cutter over {cubes} cutters");
    }
}

/// Every mesh and material a collector is made of, built once per launch.
///
/// A struct rather than eight arguments, which is this project's own rule
/// about an argument list that long, and it is what lets every craft in a
/// wave share one upload: a collector is boxes, so the whole fleet of them
/// is a handful of handles cloned.
pub(crate) struct CraftKit {
    pub(crate) mesh: Handle<Mesh>,
    pub(crate) plate: Handle<StandardMaterial>,
    pub(crate) dark: Handle<StandardMaterial>,
    pub(crate) cab: Handle<StandardMaterial>,
    pub(crate) arm: Handle<StandardMaterial>,
    pub(crate) hazard: Handle<StandardMaterial>,
    pub(crate) drive: Handle<StandardMaterial>,
    pub(crate) lamp: Handle<StandardMaterial>,
}

impl CraftKit {
    /// The mockup's own palette, and the yellow is a decision about what a
    /// collector IS rather than a taste: every warship here wears a navy and
    /// a collector wears none of them. It is plant, like the machine that
    /// digs a road up, painted the one colour nothing else on the field is
    /// wearing. The cab, the arms and the lamp stay cool so the craft is a
    /// shape with parts rather than one yellow lump.
    pub(crate) fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> CraftKit {
        let mut paint = |rgb: u32, glow: f32| {
            let [r, g, b] = [
                ((rgb >> 16) & 0xFF) as f32 / 255.0,
                ((rgb >> 8) & 0xFF) as f32 / 255.0,
                (rgb & 0xFF) as f32 / 255.0,
            ];
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                emissive: LinearRgba::rgb(r * glow, g * glow, b * glow),
                perceptual_roughness: 0.62,
                metallic: 0.22,
                ..default()
            })
        };
        CraftKit {
            // A unit cube, scaled per box: one mesh for the whole craft.
            mesh: meshes.add(Cuboid::from_length(1.0)),
            plate: paint(0xE3A02E, 0.0),
            dark: paint(0x8A4A14, 0.0),
            cab: paint(0x2B3946, 0.0),
            arm: paint(0x9B9288, 0.0),
            hazard: paint(0x22190E, 0.0),
            // The drive and the lamp are the two lit things, so they read at
            // the range a cutter is actually watched from.
            drive: paint(0xFF7A18, 2.4),
            lamp: paint(0x4FD8E8, 2.0),
        }
    }
}

/// One craft: a hull, a cab, a tank, a hazard band, two drives, a lamp and
/// two arms, every one of them a box.
///
/// The arms are CHILDREN, so one rotation on a shoulder swings the whole arm
/// with nothing recomputing where its parts are. That is the same reason a
/// turret is a child of its hull.
fn spawn_collector(commands: &mut Commands, kit: &CraftKit, at: Vec3, cell: f32) {
    // `cell` is ONE cell of the craft in world units, already off the
    // flagship, which is the same number `fly_collectors` flies and grabs
    // in. Multiplying by `CELL` again here is what the first cut did, and it
    // drew every craft at a twentieth of its own size: a speck a few pixels
    // across that flew and grabbed correctly, so only a picture could say
    // so. A scale that is applied where it is READ cannot be applied again
    // where it is USED.
    let c = cell;
    let body = commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            Collector {
                vel: Vec3::ZERO,
                cube: None,
                grip: 0.0,
            },
        ))
        .id();
    box_at(
        commands, kit, c, 3.0, 1.6, 6.0, 0.0, 0.0, 0.0, &kit.plate, body,
    );
    box_at(
        commands, kit, c, 2.0, 1.2, 1.8, 0.0, 1.3, 1.6, &kit.cab, body,
    );
    box_at(
        commands, kit, c, 2.4, 2.0, 2.2, 0.0, 0.2, -2.0, &kit.dark, body,
    );
    box_at(
        commands,
        kit,
        c,
        3.1,
        0.5,
        0.9,
        0.0,
        0.2,
        0.4,
        &kit.hazard,
        body,
    );
    box_at(
        commands, kit, c, 0.3, 0.3, 0.3, 0.0, 1.95, 1.6, &kit.lamp, body,
    );
    for s in [-1.0f32, 1.0] {
        box_at(
            commands,
            kit,
            c,
            0.9,
            0.9,
            0.5,
            s * 0.8,
            0.2,
            -3.2,
            &kit.drive,
            body,
        );
        // The shoulder is the pivot the grip swings, so it is placed and
        // everything on the arm is drawn out along its own +Z from there.
        let arm = commands
            .spawn((
                Transform::from_translation(Vec3::new(s * 1.6, -0.2, 1.6) * c),
                Visibility::default(),
                Claw(s),
                ChildOf(body),
            ))
            .id();
        box_at(
            commands, kit, c, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, &kit.arm, arm,
        );
        box_at(
            commands, kit, c, 0.7, 0.7, 2.6, 0.0, 0.0, 1.3, &kit.arm, arm,
        );
        box_at(
            commands, kit, c, 0.9, 0.9, 0.9, 0.0, 0.0, 2.6, &kit.arm, arm,
        );
        box_at(
            commands, kit, c, 0.6, 0.6, 2.2, 0.0, 0.0, 3.7, &kit.arm, arm,
        );
        box_at(
            commands, kit, c, 0.3, 0.3, 0.3, 0.0, 0.0, 5.0, &kit.lamp, arm,
        );
        for j in [-1.0f32, 1.0] {
            let jaw = commands
                .spawn((
                    Transform::from_translation(Vec3::new(0.0, j * 0.35, 4.8) * c),
                    Visibility::default(),
                    Jaw(j),
                    ChildOf(arm),
                ))
                .id();
            box_at(
                commands, kit, c, 0.5, 0.4, 1.4, 0.0, 0.0, 0.7, &kit.arm, jaw,
            );
        }
    }
}

/// One box of the craft, in CELLS of its own, under a parent.
///
/// A free function rather than a closure, because a closure over `Commands`
/// holds it for its whole life and nothing else in the spawn could then use
/// it: that is Bevy's own borrow rule rather than a style choice.
#[allow(clippy::too_many_arguments)]
fn box_at(
    commands: &mut Commands,
    kit: &CraftKit,
    c: f32,
    w: f32,
    h: f32,
    d: f32,
    x: f32,
    y: f32,
    z: f32,
    mat: &Handle<StandardMaterial>,
    parent: Entity,
) -> Entity {
    commands
        .spawn((
            Mesh3d(kit.mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(Vec3::new(x, y, z) * c).with_scale(Vec3::new(w, h, d) * c),
            ChildOf(parent),
        ))
        .id()
}

/// Where a collector is flying to and what it lands into: the flagship's own
/// pose, how big it is and the bank a cube is worth something in.
///
/// One `SystemParam`, because these four are read together on every craft and
/// a system with nine arguments is the smell this project names.
#[derive(SystemParam)]
pub(crate) struct Home<'w> {
    pub(crate) lead: Res<'w, Lead>,
    pub(crate) cfg: Res<'w, SwarmConfig>,
    pub(crate) bank: ResMut<'w, Bank>,
}

/// Fly every collector: claim a cube, go and get it, bring it home.
///
/// One system for the whole loop because it is one story and the legs are
/// decided by what the craft is holding rather than by a state anybody has
/// to keep in step: no claim is "go and look", a claim not yet in the claws
/// is "go and get it", and a cube aboard is "take it home".
pub(crate) fn fly_collectors(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    mut home: Home,
    freighters: Query<(&Support, &Hull)>,
    mut cubes: Query<(Entity, &mut Cargo, &mut Transform), Without<Collector>>,
    mut craft: Query<(Entity, &mut Collector, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = scene.step(&time);
    if dt <= 0.0 {
        return;
    }
    let lift = freight_lift(&freighters);
    for (me, mut col, mut xf) in &mut craft {
        // In the craft's OWN cells, so a collector flies and grabs at the
        // same scale it is drawn at whatever the flagship is.
        let size = home.cfg.hull_radius.max(0.1) * CELL;
        let (speed, grab) = (size * SPEED, size * GRAB);
        // What it is holding decides where it is going, and nothing else in
        // here reads a mode: the cube IS the state.
        let (goal, want_grip, landing) = match col.cube.and_then(|c| cubes.get(c).ok()) {
            Some((_, cargo, _)) if cargo.held => (home.lead.pos, 1.0, true),
            Some((_, _, cube_xf)) => (cube_xf.translation, 0.0, false),
            _ => {
                col.cube = None;
                (claim(me, &mut col, &mut cubes, xf.translation), 0.0, false)
            }
        };
        col.grip += (want_grip - col.grip).clamp(-GRIP_RATE * dt, GRIP_RATE * dt);
        let to = goal - xf.translation;
        let d = to.length();
        // Slow into it, so a craft settles onto a cube rather than batting it
        // away: the same envelope a capital ship flies, at a craft's scale.
        let aim = if d > 1e-4 {
            to / d * speed * (d / (grab * 3.0)).min(1.0)
        } else {
            Vec3::ZERO
        };
        col.vel = col.vel.lerp(aim, (dt * 4.0).min(1.0));
        xf.translation += col.vel * dt;
        if col.vel.length() > speed * 0.02 {
            let face = Transform::from_translation(xf.translation)
                .looking_to(-col.vel.normalize(), Vec3::Y)
                .rotation;
            xf.rotation = xf.rotation.slerp(face, (dt * 3.0).min(1.0));
        }
        if !xf.translation.is_finite() {
            xf.translation = home.lead.pos;
            col.vel = Vec3::ZERO;
        }
        // In the claws, or landed. A cube is placed at the claws every frame
        // rather than parented, because a cube is spawned by the cutter and
        // re-parenting it would mean two places deciding what it rides.
        let Some(cube) = col.cube else { continue };
        let Ok((_, mut cargo, mut cube_xf)) = cubes.get_mut(cube) else {
            col.cube = None;
            continue;
        };
        if !cargo.held {
            if d < grab {
                cargo.held = true;
            }
            continue;
        }
        cube_xf.translation = xf.translation + xf.rotation * (Vec3::Z * REACH * size * 1.1);
        cube_xf.rotation = xf.rotation;
        if landing && d < home.cfg.hull_radius * UNLOAD_REACH {
            let mut got = cargo.kind.worth();
            got.materials = (got.materials as f32 * lift) as u32;
            got.volatiles = (got.volatiles as f32 * lift) as u32;
            got.data = (got.data as f32 * lift) as u32;
            info!("a collector landed {:?}: {got:?}", cargo.kind);
            home.bank.take(got);
            commands.entity(cube).despawn();
            col.cube = None;
        }
    }
}

/// What a fleet's freighters add to everything landed.
///
/// One function, because the cutter's own unload asks the same question and
/// two places computing one share is two shares the day either moves.
pub(crate) fn freight_lift(crews: &Query<(&Support, &Hull)>) -> f32 {
    let n = crews
        .iter()
        .filter(|(s, h)| s.role == Role::Freighter && !h.dead_hull)
        .count() as u32;
    1.0 + FREIGHT_SHARE * n as f32
}

/// Take the nearest cube nobody is coming for, and write the claim on it.
fn claim(
    me: Entity,
    col: &mut Collector,
    cubes: &mut Query<(Entity, &mut Cargo, &mut Transform), Without<Collector>>,
    from: Vec3,
) -> Vec3 {
    let mut best: Option<(f32, Entity, Vec3)> = None;
    for (e, cargo, xf) in cubes.iter() {
        if cargo.to.is_some() {
            continue;
        }
        let d = xf.translation.distance_squared(from);
        if best.is_none_or(|(b, _, _)| d < b) {
            best = Some((d, e, xf.translation));
        }
    }
    let Some((_, e, at)) = best else { return from };
    if let Ok((_, mut cargo, _)) = cubes.get_mut(e) {
        cargo.to = Some(me);
        col.cube = Some(e);
    }
    at
}

/// Swing the arms from the one number the flight publishes.
///
/// The mockup's own pose, and the two halves of it are what say what the
/// craft is DOING at a glance: the shoulders are splayed wide while it is
/// reaching and folded in once it has something, and the jaws are open on
/// the way and shut on the cube.
pub(crate) fn pose_claws(
    craft: Query<&Collector>,
    mut arms: Query<(&Claw, &ChildOf, &mut Transform), Without<Jaw>>,
    mut jaws: Query<(&Jaw, &ChildOf, &mut Transform), Without<Claw>>,
) {
    for (claw, of, mut xf) in &mut arms {
        let Ok(col) = craft.get(of.parent()) else {
            continue;
        };
        xf.rotation = Quat::from_rotation_y(claw.0 * (1.0 - col.grip * 1.25))
            * Quat::from_rotation_x(-0.5 + col.grip * 0.42);
    }
    for (jaw, of, mut xf) in &mut jaws {
        // A jaw's parent is the ARM and the grip is on the craft, so this
        // walks one more link. Two queries rather than one with an `Option`,
        // because Bevy proves them disjoint from their filters.
        let Ok((_, arm_of, _)) = arms.get(of.parent()) else {
            continue;
        };
        let Ok(col) = craft.get(arm_of.parent()) else {
            continue;
        };
        xf.rotation = Quat::from_rotation_x(jaw.0 * (0.55 - col.grip * 0.42));
    }
}

/// How far off a watched craft the camera stands and how far above it, in
/// the craft's own cells: close enough that a six cell craft fills a third
/// of the frame, and behind its shoulder so the arms and whatever is in
/// them are between the eye and the field.
const WATCH_BACK: f32 = 26.0;
const WATCH_UP: f32 = 9.0;

/// Ride the first collector with the camera.
///
/// `--target` names a PLACE, which is enough for everything else in this
/// game: a hull, a rock, a wreck and the fleet all stand still enough to be
/// photographed from a coordinate. A collector does not, and the pictures
/// said so: a wide shot of the field puts the craft at a few pixels and a
/// close one is a camera inside whatever the craft has left, so the mechanic
/// could be proved from the log and not from a picture. A harness flag that
/// FOLLOWS is the answer, and it is the same answer `--fixed-dt` is: hold
/// the thing still against the camera so a still can be taken of it.
///
/// It is one craft and not a choice of craft, because the point is to see
/// what a collector does rather than which collector does it.
pub(crate) fn watch_craft(
    scene: Res<SceneSpec>,
    cfg: Res<SwarmConfig>,
    craft: Query<&Transform, With<Collector>>,
    mut cams: Query<&mut Transform, (With<Camera3d>, Without<Collector>)>,
) {
    if !scene.watch_craft {
        return;
    }
    let Some(at) = craft.iter().next() else {
        return;
    };
    let size = cfg.hull_radius.max(0.1) * CELL;
    // Behind and above in the CRAFT's own frame, so the camera swings with
    // it and what is in the claws stays in the same corner of the picture.
    let eye = at.translation + at.rotation * Vec3::new(0.0, WATCH_UP, -WATCH_BACK) * size;
    for mut xf in &mut cams {
        *xf = Transform::from_translation(eye).looking_at(at.translation, Vec3::Y);
    }
}
