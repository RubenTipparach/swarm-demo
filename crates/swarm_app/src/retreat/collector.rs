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

/// One arm's shoulder, which is what the grip swings. The number is which
/// side it is on, so one pose serves both arms mirrored.
#[derive(Component)]
pub(crate) struct Claw(pub(crate) f32);

/// One arm's ELBOW, and the whole reason an arm reads as an arm.
///
/// The first cut had none: the shoulder was the only joint and every segment
/// of the arm was laid out along one axis from it, so the upper arm, the
/// forearm and the claw were a rigid stick that could only ever swing. A
/// limb with one joint is a spar with a hand on the end, and the owner said
/// so off the picture. It is a child of the shoulder, so the forearm and the
/// jaws ride the bend with nothing recomputing where they are, which is the
/// same reason the arm itself is a child of the craft.
#[derive(Component)]
pub(crate) struct Elbow;

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

/// Where the arm is jointed and how long each bone is, in cells. The claws
/// are measured to the middle of the jaws rather than to the wrist, because
/// what these numbers exist to answer is where a cube is HELD.
const SHOULDER_AT: Vec3 = Vec3::new(1.6, -0.2, 1.6);
const UPPER: f32 = 2.6;
const FORE_GRIP: f32 = 2.9;

/// The arm's two poses, in radians, and the whole of what the grip eases
/// between: how far out the shoulder is swung, how far the upper arm is
/// raised, and how far the elbow is bent back.
///
/// **The elbow is never nought at either end, and that is the rule rather
/// than a taste.** A limb straightened out to a line is the one shape an
/// animal never holds, which is what the owner was looking at: even reaching
/// for something a crane keeps an angle in it. So the smaller of the two
/// bends is a quarter turn's worth and the arm is an ARC in every frame this
/// craft is ever drawn in.
///
/// Idle is the one that had to be designed rather than derived. The claws
/// point FORWARD, which means the yaw stays small (a wide splay turns the
/// claws outboard and the craft reads as a thing warding something off), the
/// upper arm is raised well up, and the elbow brings the forearm back down
/// level: shoulders up, elbows out, hands in front, which is how anything
/// that picks things up waits to pick something up.
///
/// Carrying is the opposite of what the first cut assumed. It SPREADS: a
/// cube is several times the craft's own cell, so arms folded in would be
/// two claws buried inside it. Held wide, the jaws sit on the cube's flanks
/// and the craft reads as carrying rather than as impaling.
const IDLE_YAW: f32 = 0.30;
const IDLE_UPPER: f32 = -0.72;
const IDLE_ELBOW: f32 = 1.02;
const HOLD_YAW: f32 = 0.62;
const HOLD_UPPER: f32 = -0.40;
const HOLD_ELBOW: f32 = 0.55;

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
        spawn_arm(commands, kit, c, s, body);
    }
}

/// One ARM: a shoulder, an upper arm, an elbow, a forearm and two jaws.
///
/// Its own function because `spawn_collector` is the BODY and this is a
/// limb, which is two things and therefore two functions, and because the
/// craft went over this project's own hundred lines the moment the arm
/// gained a joint. The limit is the work, never a reason to raise it.
fn spawn_arm(commands: &mut Commands, kit: &CraftKit, c: f32, s: f32, body: Entity) {
    // The arm is a CHAIN of three pivots, and each bone hangs off the
    // joint above it rather than off the craft: shoulder, then elbow at
    // the far end of the upper arm, then a jaw either side of the wrist.
    // So the forearm and the claws ride the bend for nothing, exactly as
    // the whole arm rides the craft.
    let shoulder = commands
        .spawn((
            Transform::from_translation(
                Vec3::new(s * SHOULDER_AT.x, SHOULDER_AT.y, SHOULDER_AT.z) * c,
            ),
            Visibility::default(),
            Claw(s),
            ChildOf(body),
        ))
        .id();
    // The shoulder ball, then the upper arm out to the elbow.
    box_at(
        commands, kit, c, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, &kit.arm, shoulder,
    );
    box_at(
        commands,
        kit,
        c,
        0.7,
        0.7,
        UPPER,
        0.0,
        0.0,
        UPPER * 0.5,
        &kit.arm,
        shoulder,
    );
    let elbow = commands
        .spawn((
            Transform::from_translation(Vec3::Z * UPPER * c),
            Visibility::default(),
            Elbow,
            ChildOf(shoulder),
        ))
        .id();
    box_at(
        commands, kit, c, 0.9, 0.9, 0.9, 0.0, 0.0, 0.0, &kit.arm, elbow,
    );
    box_at(
        commands, kit, c, 0.6, 0.6, 2.2, 0.0, 0.0, 1.1, &kit.arm, elbow,
    );
    box_at(
        commands, kit, c, 0.3, 0.3, 0.3, 0.0, 0.0, 2.4, &kit.lamp, elbow,
    );
    for j in [-1.0f32, 1.0] {
        let jaw = commands
            .spawn((
                Transform::from_translation(Vec3::new(0.0, j * 0.35, 2.2) * c),
                Visibility::default(),
                Jaw(j),
                ChildOf(elbow),
            ))
            .id();
        box_at(
            commands, kit, c, 0.5, 0.4, 1.4, 0.0, 0.0, 0.7, &kit.arm, jaw,
        );
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
        cube_xf.translation = xf.translation + xf.rotation * (claw_point(col.grip) * size);
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

/// Where the shoulder stands at this grip, and which way it is swung.
fn shoulder_pose(grip: f32, side: f32) -> Quat {
    let g = grip.clamp(0.0, 1.0);
    let yaw = IDLE_YAW + (HOLD_YAW - IDLE_YAW) * g;
    let up = IDLE_UPPER + (HOLD_UPPER - IDLE_UPPER) * g;
    // Yaw first and then pitch, so the swing is about the CRAFT's up and the
    // raise is about the arm's own beam: the other order swings the arm
    // about an axis that has already been tipped, and the two arms then come
    // out at different heights.
    Quat::from_rotation_y(side * yaw) * Quat::from_rotation_x(up)
}

/// How far the elbow is bent back at this grip. It takes no side, because a
/// bend is the same bend on both arms: only the swing is mirrored.
fn elbow_pose(grip: f32) -> Quat {
    let g = grip.clamp(0.0, 1.0);
    Quat::from_rotation_x(IDLE_ELBOW + (HOLD_ELBOW - IDLE_ELBOW) * g)
}

/// Where the claws are holding something, in the craft's own cells and its
/// own frame.
///
/// It is WALKED down the same chain the draw is posed from rather than
/// written down as a number beside it, which is this project's rule about
/// one fact drawn twice: a carry point authored on its own is a cube riding
/// in mid air the first time anybody tunes an angle, and the tuning is the
/// whole of what this section is. It follows the grip as well, so a cube
/// comes in with the arms as they close rather than snapping to the pose
/// they end at.
fn claw_point(grip: f32) -> Vec3 {
    let sh = shoulder_pose(grip, 1.0);
    let hand =
        SHOULDER_AT + sh * (Vec3::Z * UPPER) + (sh * elbow_pose(grip)) * (Vec3::Z * FORE_GRIP);
    // On the centreline, because the arms are mirrored: one claw's x is the
    // other's, so what a PAIR of them holds is how far forward and how high
    // they hold it, and the sides cancel.
    Vec3::new(0.0, hand.y, hand.z)
}

/// Swing the arms from the one number the flight publishes.
///
/// The mockup's own pose, and what the two ends of it say is what the craft
/// is DOING at a glance: reaching, with the claws held out in front, or
/// carrying, with them spread round what it has.
/// One joint of an arm: what it is, what it hangs off, and the pose that is
/// written to it. Named rather than written out, which is what keeps
/// `pose_claws` readable and is what clippy's complex type rule is actually
/// asking for, exactly as `CutRow` and `HaulShip` are.
///
/// Each filter says the row is NOT the other two joints, and that is not
/// decoration: all three write a `Transform`, and Bevy proves two queries
/// disjoint from their FILTERS rather than from anything anybody knows about
/// the data, so without them the app refuses the system at startup.
pub(crate) type ArmRow<'a> = (&'a Claw, &'a ChildOf, &'a mut Transform);
pub(crate) type ArmOnly = (Without<Elbow>, Without<Jaw>);
pub(crate) type ElbowRow<'a> = (&'a ChildOf, &'a mut Transform);
pub(crate) type ElbowOnly = (With<Elbow>, Without<Claw>, Without<Jaw>);
pub(crate) type JawRow<'a> = (&'a Jaw, &'a ChildOf, &'a mut Transform);
pub(crate) type JawOnly = (Without<Claw>, Without<Elbow>);

pub(crate) fn pose_claws(
    craft: Query<&Collector>,
    mut arms: Query<ArmRow, ArmOnly>,
    mut elbows: Query<ElbowRow, ElbowOnly>,
    mut jaws: Query<JawRow, JawOnly>,
) {
    for (claw, of, mut xf) in &mut arms {
        let Ok(col) = craft.get(of.parent()) else {
            continue;
        };
        xf.rotation = shoulder_pose(col.grip, claw.0);
    }
    // A joint's parent is the joint ABOVE it and the grip is on the craft,
    // so an elbow walks one more link and a jaw walks two. Three queries
    // rather than one with `Option`s, because Bevy proves them disjoint from
    // their FILTERS and not from anything anybody knows about the data.
    for (of, mut xf) in &mut elbows {
        let Ok((_, arm_of, _)) = arms.get(of.parent()) else {
            continue;
        };
        let Ok(col) = craft.get(arm_of.parent()) else {
            continue;
        };
        xf.rotation = elbow_pose(col.grip);
    }
    for (jaw, of, mut xf) in &mut jaws {
        let Ok((elbow_of, _)) = elbows.get(of.parent()) else {
            continue;
        };
        let Ok((_, arm_of, _)) = arms.get(elbow_of.parent()) else {
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
