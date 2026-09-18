//! A ship that BUILDS ships: the yard a hull carries, and where what it makes
//! appears.
//!
//! The queue's rules are `swarm_core::build` and nothing here repeats them.
//! What this file owns is the two things that crate cannot know: which entity
//! carries a yard, and where a finished hull stands when it arrives.

use crate::*;

/// The bank a skirmish opens with, in materials.
///
/// A run's bank is what it mined and carries between systems; a skirmish has
/// no economy at all, so without a figure here its build menu would be six
/// categories of rows nobody could ever press. The mockup's own strip reads
/// three thousand, which is six frigates, so that is the number.
pub(crate) const SKIRMISH_BANK: u32 = 3000;

/// A hull's own yard, which is the core's `Yard` on a component.
///
/// A newtype rather than the core type deriving `Component`, because that
/// derive in the core would be the engine reaching back across the boundary
/// this project keeps: the core depends on nothing but `std`.
#[derive(Component, Default)]
pub(crate) struct Shipyard(pub(crate) Yard);

/// A gun that never moves: a hull denied its flight.
///
/// A marker and a filter rather than an `if` in the flight rule, which is this
/// project's own interface segregation: a rule a system must not see is what
/// its query says it does not take.
#[derive(Component)]
pub(crate) struct Platform;

/// The flagship carries the yard.
///
/// The frame AFTER the field is spawned, because the flagship arrives through
/// commands, which is the same reason `apply_scars` runs there. It is written
/// as "any flagship without one" rather than as a line in the spawn, so a
/// field torn down and built again opens a fresh yard with no rule about it
/// anywhere.
pub(crate) fn open_yards(
    mut commands: Commands,
    ships: Query<Entity, (With<Flagship>, Without<Shipyard>)>,
) {
    for e in &ships {
        commands.entity(e).insert(Shipyard::default());
    }
}

/// Everything a finished job needs to become a ship, as ONE parameter.
///
/// The smell this project names is an argument list, and a system that spawns
/// a hull carries the same five every other spawner does. `Forge` is the same
/// answer for the meshes and the materials; this is the one for a berth.
#[derive(SystemParam)]
pub(crate) struct Berth<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) meshes: ResMut<'w, Assets<Mesh>>,
    pub(crate) materials: ResMut<'w, Assets<StandardMaterial>>,
    pub(crate) tex: Res<'w, Textures>,
    pub(crate) lead: Res<'w, Lead>,
    pub(crate) wing: Res<'w, Wing>,
}

/// Advance every yard, and put what finished in the world.
pub(crate) fn work_yards(
    time: Res<Time>,
    mut scene: ResMut<SceneSpec>,
    mut berth: Berth,
    mut yards: Query<(&mut Shipyard, &Hull), With<Flagship>>,
    escorts: Query<(), With<Escort>>,
) {
    let dt = scene.step(&time);
    let mut out = escorts.iter().count() as u32;
    for (mut yard, hull) in &mut yards {
        let Some(class) = hull.class.clone() else {
            continue;
        };
        // A hangar fits ITSELF inside the tick, so the places it opens are
        // read either side of the tick rather than off a finished job: the
        // core hands back only what the app has to put somewhere, and a module
        // is not one of those.
        let before = yard.0.fit().hangar;
        let done = yard.0.tick(&class, dt);
        scene.fighters += yard.0.fit().hangar - before;
        for order in done {
            match &order {
                // A fighter is a PLACE in the squadron rather than an entity
                // spawned here: `launch_fighters` already replaces losses up
                // to the ceiling, so raising the ceiling is the whole of it
                // and there is no second launch path to keep in step.
                Order::Fighter => scene.fighters += 1,
                Order::Hull(class) => {
                    launch(&mut berth, hull, class, out);
                    out += 1;
                }
                Order::Platform(class) => emplace(&mut berth, hull, class, out),
                Order::Fit(_) => continue,
            }
            info!("{} built", order.label());
        }
    }
}

/// A hull arrives the way a reinforcement does, because it IS one: `call_one`
/// is the one implementation and a second arrival written here would be the
/// divergent path this project calls a defect.
fn launch(berth: &mut Berth, hull: &Hull, class: &str, n: u32) {
    call_one(
        &mut berth.commands,
        &mut berth.meshes,
        &mut berth.materials,
        &berth.tex,
        class,
        Wave {
            radius: hull.model.radius(),
            lead_pos: berth.lead.pos,
            lead_rot: berth.lead.rot,
            chewers: 0,
            n,
            shape: berth.wing.0,
        },
    );
}

/// A platform is put where it will STAND rather than flown in, which is the
/// one thing that makes it a platform: the same spawn, at the station itself
/// instead of out past it, with the marker that keeps `fly_hull` off it.
///
/// It goes on the WING's books all the same, because `spawn_hull` reads a
/// class that is not on them as the flagship, and a second ship wearing that
/// marker is a camera, a nav disc and a swarm target that all do nothing.
fn emplace(berth: &mut Berth, hull: &Hull, class: &str, n: u32) {
    let radius = hull.model.radius();
    let station = Vec3::from(formation::station(berth.wing.0, n as usize)) * radius;
    let at = Transform::from_translation(berth.lead.pos + berth.lead.rot * station)
        .looking_to(berth.lead.rot * Vec3::NEG_Z, Vec3::Y);
    let (e, _) = spawn_hull(
        &mut berth.commands,
        &mut berth.meshes,
        &mut berth.materials,
        &berth.tex,
        class,
        ShipSpec::at(at).seed(0x50A7 + n * 0x4F1B).wing(),
    );
    berth.commands.entity(e).insert(Platform);
}

/// Put one order on a yard's queue, and take what it costs.
///
/// The ONE place a job is accepted, so the three gates are asked once rather
/// than once per button: the bank covers the whole price, the wing has a
/// station left, and a module has a bay to sit in. Two callers press it, the
/// build row and the R key, and a gate written at each of them is a gate one
/// of them would have differently.
///
/// Nothing is taken from a bank that cannot cover the price, because
/// `Bank::spend` is all or nothing, and nothing is queued when it cannot.
pub(crate) fn order_one(
    yard: &mut Yard,
    bank: &mut Bank,
    class: &str,
    order: Order,
    out: u32,
) -> bool {
    match &order {
        // A hull and a platform both take a station of the wing, which is
        // what the formation is laid out for: the queue counts too, or a
        // player could order ten and watch six arrive.
        Order::Hull(_) | Order::Platform(_) => {
            let coming = yard
                .queue
                .iter()
                .filter(|t| !t.order.is_fit() && t.order != Order::Fighter)
                .count() as u32;
            if out + coming >= WING_MAX {
                info!("the wing is full at {WING_MAX}");
                return false;
            }
        }
        Order::Fit(m) if !yard.room_for(class, *m) => {
            info!("no bay left for a {}", m.label());
            return false;
        }
        _ => {}
    }
    if !bank.spend(order.price()) {
        info!("not enough in the bank for a {}", order.label());
        return false;
    }
    info!(
        "{} ordered, {} in the queue",
        order.label(),
        yard.queue.len() + 1
    );
    yard.order(order);
    true
}

/// `--build TICK`: the build row, pressed by the harness.
///
/// Through `order_one` like every other caller, on `Tick::cue` like every
/// other scripted cue, because both of those are rules this project has
/// already learned once each: a second ordering path would be a gate written
/// twice, and an exact tick comparison fires only under `--fixed-dt`.
pub(crate) fn script_build(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    mut bank: ResMut<Bank>,
    mut yards: Query<&mut Shipyard, With<Flagship>>,
    escorts: Query<(), With<Escort>>,
    mut fired: Local<bool>,
) {
    if scene.build == 0 || !tick.cue(Some(scene.build), &mut fired) {
        return;
    }
    let Ok(mut yard) = yards.single_mut() else {
        return;
    };
    // A class key is a hull and the one word that is not is a fighter, which
    // is exactly the two kinds of row the panel offers: what it presses is
    // read off the argument rather than a second table saying which is which.
    let order = if scene.build_what == "fighter" {
        Order::Fighter
    } else {
        Order::Hull(scene.build_what.clone())
    };
    order_one(
        &mut yard.0,
        &mut bank,
        &scene.hull,
        order,
        escorts.iter().count() as u32,
    );
}
