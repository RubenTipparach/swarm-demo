//! The flagship and what keeps station on it, and what the fleet tells the
//! swarm about where it is.

use crate::*;

/// Where the flagship is, was heading and how fast.
///
/// A resource rather than a second query, because a system that holds every
/// hull's `Transform` mutably cannot also read the flagship's: Bevy refuses a
/// read of the same component in the same system. Published a frame behind,
/// which is a centimetre and nothing anybody can see.
///
/// Nothing FLIES on it any more. It is where a cutter is told to bring its
/// load and where a wave is aimed when it is called, both of which are one
/// shot answers rather than a goal chased every frame.
#[derive(Resource, Default)]
pub(crate) struct Lead {
    pub(crate) pos: Vec3,
    pub(crate) rot: Quat,
    pub(crate) vel: Vec3,
}

/// An order handed to a hull that does not exist yet.
///
/// `spawn_hull` inserts the `Hull` through `Commands`, so nothing in the same
/// system can reach into it; this is the order waiting for it, applied the
/// moment it is really there.
#[derive(Component)]
pub(crate) struct NavTo(pub(crate) Vec3);

/// The ship the player commands. The camera follows it, the nav disc gives it
/// orders and the swarm chases it.
///
/// Exactly one hull carries this, and the three systems that want THE ship
/// rather than A ship ask for it: before reinforcements there was one hull and
/// `single()` said so, which silently became "do nothing at all" the moment
/// there were two.
#[derive(Component)]
pub(crate) struct Flagship;

/// A ship on the wing's books: what the yard counts against `WING_MAX`, and
/// what the deck calls a unit that is neither the command ship nor a trade.
///
/// A MARKER and nothing more. It used to carry the station it was flying and
/// `fly_hull` chased that every frame, which is the follow the leader rule
/// that is gone: a ship flies its order and holds where it stops, and a wing
/// that is to move with the flagship is told to with `Guard`. So it is never
/// taken off either, because it says which books a ship is on rather than
/// what it is doing this second: an escort sent somewhere is still an escort,
/// and removing the marker to mean "it has an order now" was quietly freeing
/// a berth every time one was given one.
#[derive(Component)]
pub(crate) struct Escort;

/// How many escorts a wing holds, and how many one press of R brings.
pub(crate) const WING_MAX: u32 = 6;

pub(crate) const WING_WAVE: u32 = 2;

/// Everything about ONE reinforcement that is not which class it is.
///
/// A struct rather than six more arguments, because `call_one` was already at
/// ten under the rule this file's own project keeps: an argument list that long
/// is the smell that says a struct is missing, and the day the wing gained a
/// SHAPE was the day that debt came due. Four callers hand this over and none
/// of them spells out a station.
#[derive(Clone, Copy)]
pub(crate) struct Wave {
    /// The leader's hull radius, which every offset here is measured in.
    pub(crate) radius: f32,
    pub(crate) lead_pos: Vec3,
    pub(crate) lead_rot: Quat,
    pub(crate) chewers: u32,
    /// Which station of the wing, from nought.
    pub(crate) n: u32,
    pub(crate) shape: Shape,
}

/// Call one reinforcement in, off the map, on the `n`th station of a wing.
///
/// WHERE that station is belongs to `swarm_core::formation`, because the shape
/// is the player's now: the Form cell cycles wedge, line and sphere, and a
/// wedge written out here would be one of the three living somewhere the other
/// two do not. What the wedge IS has not moved, and the core's suite holds it
/// to the expression this function used to carry.
///
/// An escort carries chewers of its own, fewer than the flagship. The GPU
/// swarm knows one hull centre and chases the flagship alone, so without them
/// a wing would be four ships that add guns and can never be hurt: calling
/// reinforcements would be free, which is the one thing a reinforcement must
/// not be.
///
/// It ARRIVES on one order and then holds. The station used to be a goal it
/// chased for the rest of its life, which is what made a wing move whenever
/// the flagship did; it is a place it is SENT to now, through the same
/// `Hull.order` a player's own right click writes, so the flight envelope,
/// the rock avoidance and the heading are the one implementation they always
/// were and a ship that has arrived is a ship standing still.
pub(crate) fn call_one(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    which: &str,
    wave: Wave,
) {
    let Wave {
        radius,
        lead_pos,
        lead_rot,
        chewers,
        n,
        shape,
    } = wave;
    let side = if n % 2 == 0 { 1.0 } else { -1.0 };
    let station = Vec3::from(formation::station(shape, n as usize)) * radius;
    // It ARRIVES: dropped well outside the formation, on the far side of its
    // own station, so a wave flies in past the camera rather than appearing
    // in the middle of the fight.
    let at = lead_pos
        + lead_rot * (station + Vec3::new(side * radius * 4.0, radius * 1.5, -radius * 9.0));
    let xf = Transform::from_translation(at).looking_to(lead_rot * Vec3::NEG_Z, Vec3::Y);
    let (e, _) = spawn_hull(
        commands,
        meshes,
        materials,
        tex,
        which,
        ShipSpec::at(xf)
            .chewers(chewers)
            .seed(0x9E37 + n * 0x4F1B)
            .wing(),
    );
    // Through `NavTo` rather than by writing the order here, because the
    // `Hull` arrives through `Commands` and nothing in this system can reach
    // into it yet. That is what the component is for and the flagship's own
    // `--move` already goes the same way.
    commands
        .entity(e)
        .insert(NavTo(lead_pos + lead_rot * station));
}

/// Where the density field stands this frame.
///
/// It rides with the fight rather than standing still at the origin, and it is
/// sized to hold everything the swarm actually flies to: the flagship, the
/// ships it attacks, the carriers the motes launch from and the rocks the
/// veins wind round. A cube, so a cell is a cube and a shadow is the same
/// length whichever way the sun happens to be pointing.
///
/// Outside it the field's answer is "nothing in the way", which is the right
/// answer rather than a fallback: a mote out beyond the carriers is on its own
/// in open space and there is nothing out there to shadow it.
pub(crate) fn publish_field(mut cfg: ResMut<SwarmConfig>, mut said: Local<bool>) {
    let centre = cfg.hull_centre;
    // Never smaller than the ship and the traffic round it, or a swarm that
    // has killed every carrier would shrink its own field to a point.
    let mut half = cfg.hull_radius * 8.0;
    for x in cfg
        .hives
        .iter()
        .chain(cfg.rocks.iter())
        .chain(cfg.targets.iter())
    {
        half = half.max((x.truncate() - centre).abs().max_element() + x.w);
    }
    half *= 1.15;
    let cell = 2.0 * half / GRID as f32;
    cfg.field = (centre - Vec3::splat(half)).extend(cell);
    if !*said {
        *said = true;
        info!(
            "density field: {GRID}^3 cells of {cell:.2} units over a box {:.0} across",
            2.0 * half
        );
    }
}

/// The swarm wants the ship, so it has to be told where the ship IS.
///
/// Which is what makes moving it worth doing: the cloud is pulled along
/// behind, and a player who runs can watch the swarm string out.
pub(crate) fn publish_hull(
    lead_q: Query<(Entity, &Hull, &Transform), With<Flagship>>,
    // Every player hull, which is the flagship and the wing. NOT the carriers:
    // they are hulls too now, and a swarm that attacked its own motherships
    // would be a fight with one side in it.
    ships: Query<(&Hull, &Transform), Without<Hive>>,
    mut cfg: ResMut<SwarmConfig>,
    mut lead: ResMut<Lead>,
    mut was: Local<Option<Entity>>,
    mut last_n: Local<usize>,
) {
    // ---- what the swarm may attack ----
    //
    // A LIST, not a centre. The cloud used to chase one published position, so
    // it was always one animal on one ship: calling in reinforcements put five
    // frigates on the map and the swarm still sat on exactly one of them,
    // which is not what a swarm does and makes "divide it by where you put
    // your ships" impossible to express. Every live hull is a target now and a
    // mote picks one from its own seed.
    cfg.targets.clear();
    for (hull, xf) in &ships {
        if hull.dead_hull || cfg.targets.len() >= swarm::MAX_TARGETS {
            continue;
        }
        cfg.targets.push(xf.translation.extend(hull.model.radius()));
    }
    if *last_n != cfg.targets.len() {
        info!(
            "the swarm has {} ships to divide between",
            cfg.targets.len()
        );
        *last_n = cfg.targets.len();
    }

    // The FLAGSHIP, not whichever hull the query happened to yield last. With
    // one ship those were the same thing and this iterated; with a wing out,
    // the cloud would have chased whichever escort was stored last and the
    // formation would have tried to keep station on itself.
    // The flagship, for the camera, the nav disc and the formation. It is one
    // of the targets above and has no special standing to the swarm.
    let n = lead_q.iter().count();
    if n != 1 {
        if was.is_some() {
            warn!("the swarm has {n} flagships to chase, keeping the last target");
            *was = None;
        }
        return;
    }
    let Ok((e, hull, xf)) = lead_q.single() else {
        return;
    };
    if *was != Some(e) {
        info!("the flagship is {e}");
        *was = Some(e);
    }
    cfg.hull_centre = xf.translation;
    cfg.hull_radius = hull.model.radius();
    lead.pos = xf.translation;
    lead.rot = xf.rotation;
    lead.vel = hull.vel;
}

/// Hand a freshly spawned hull the order that was waiting for it.
pub(crate) fn apply_nav_to(mut commands: Commands, mut q: Query<(Entity, &mut Hull, &NavTo)>) {
    for (e, mut hull, to) in &mut q {
        hull.order = Some(to.0);
        commands.entity(e).remove::<NavTo>();
    }
}

/// R ORDERS a wave: two more of the flagship's class onto the yard's queue.
///
/// It used to spawn them outright and for nothing, which is a second way of
/// getting a ship beside the build menu's: two paths to one thing is this
/// project's own divergent path defect, and the one that stays is the one a
/// player pays for. So the key and the Call button are a shortcut to the row
/// the panel already offers, and `order_one` is the single gate both go
/// through.
pub(crate) fn call_reinforcements(
    keys: Res<ButtonInput<KeyCode>>,
    scene: Res<SceneSpec>,
    mut bank: ResMut<Bank>,
    mut yards: Query<&mut Shipyard, With<Flagship>>,
    escorts: Query<(), With<Escort>>,
    button: Query<&Interaction, (Changed<Interaction>, With<CallButton>)>,
) {
    let clicked = button.iter().any(|i| *i == Interaction::Pressed);
    if !keys.just_pressed(KeyCode::KeyR) && !clicked {
        return;
    }
    let Ok(mut yard) = yards.single_mut() else {
        return;
    };
    let out = escorts.iter().count() as u32;
    for _ in 0..WING_WAVE {
        if !order_one(
            &mut yard.0,
            &mut bank,
            &scene.hull,
            Order::Hull(scene.wing_class().to_string()),
            out,
        ) {
            break;
        }
    }
}
