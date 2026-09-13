//! The command bar's orders: what each of the twelve cells actually does.
//!
//! Six of them were already in the game wearing other labels. The six that are
//! new are MARKERS and FILTERS rather than branches, which is this project's
//! own rule: a thing a system must not see is a marker and a filter, and never
//! an `if` in twelve systems. That is why a stance costs the gun systems one
//! word each and not a parameter.

use bevy::ecs::query::QueryFilter;

use crate::*;

/// Point defence only: the beams stand down and the flak keeps firing.
///
/// A marker rather than a field, so `fire_guns` says `Without<Defensive>` and
/// `fire_flak` says nothing at all. The whole of the rule is in those two
/// filters, which is also what lets Bevy prove the two systems disjoint.
#[derive(Component)]
pub(crate) struct Defensive;

/// Nothing fires. Both gun systems filter it out.
#[derive(Component)]
pub(crate) struct HoldFire;

/// Which of the three a ship is on, for the panel to read back.
///
/// The markers ARE the rule and this is the readout: one enum the cell cycles
/// and the label reads, so a button can never say something other than what
/// the filters are doing. Derived from the markers rather than stored beside
/// them, or the two would drift the first time one was set without the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Stance {
    #[default]
    Aggressive,
    Defensive,
    Hold,
}

impl Stance {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Stance::Aggressive => "Aggressive",
            Stance::Defensive => "Defensive",
            Stance::Hold => "Hold fire",
        }
    }

    /// The next one round, because the cell is one button rather than three.
    pub(crate) fn next(self) -> Stance {
        match self {
            Stance::Aggressive => Stance::Defensive,
            Stance::Defensive => Stance::Hold,
            Stance::Hold => Stance::Aggressive,
        }
    }

    /// Put a hull on this stance, which is the one place the two markers are
    /// written. Anything else setting one of them is a second writer.
    pub(crate) fn apply(self, e: Entity, commands: &mut Commands) {
        let mut ship = commands.entity(e);
        ship.remove::<Defensive>();
        ship.remove::<HoldFire>();
        match self {
            Stance::Aggressive => {}
            Stance::Defensive => {
                ship.insert(Defensive);
            }
            Stance::Hold => {
                ship.insert(HoldFire);
            }
        }
    }
}

/// What the fleet is on, for the cell's own label.
#[derive(Resource, Default)]
pub(crate) struct FleetStance(pub(crate) Stance);

/// Hold station on another ship rather than on the flagship.
///
/// An escort's station is the flagship's by construction; a GUARD is the same
/// behaviour aimed somewhere else, so it reuses the flight envelope entirely:
/// the system writes `Hull.order` every frame and `fly_hull` does the rest.
/// Nothing new flies.
#[derive(Component)]
pub(crate) struct Guard {
    pub(crate) of: Entity,
    /// Where round the guarded ship this one sits, in its own radii.
    pub(crate) station: Vec3,
}

/// Where a ship the yard builds flies to once it is out.
///
/// `None` is the flagship itself, which is where a reinforcement has always
/// gone. A rally point is the one thing that makes building a ship different
/// from calling one: a wave arrives on the flagship and a built ship arrives
/// where you put it.
#[derive(Resource, Default)]
pub(crate) struct Rally(pub(crate) Option<Vec3>);

/// The shape the wing keeps, which the Form cell cycles.
#[derive(Resource, Default)]
pub(crate) struct Wing(pub(crate) Shape);

/// Fighters recalled to the ship they fly off.
///
/// The squadron patrols the band the swarm holds, which is what wears it down;
/// docked, it flies the flagship's own station instead and stops taking
/// attrition. A flag rather than a second flight rule, because where a fighter
/// wants to be is already one number the patrol reads.
#[derive(Resource, Default)]
pub(crate) struct Docked(pub(crate) bool);

/// Fly every guarding ship to its station on whatever it is guarding.
///
/// It writes an ORDER rather than a position, so the guard accelerates, tops
/// out, slows into the arrival and turns to face the way it is going exactly
/// as a ship under a player's own order does. A guard that teleported to an
/// offset would be a formation with no weight to it.
///
/// A guard whose target is gone stops guarding rather than flying at the last
/// place it stood: `Query::get` fails on a despawned entity, which is the
/// check, and the marker comes off so the ship keeps whatever order it had.
pub(crate) fn hold_guard(
    mut commands: Commands,
    guards: Query<(Entity, &Guard)>,
    targets: Query<(&Transform, &Hull), Without<Guard>>,
    mut hulls: Query<&mut Hull, With<Guard>>,
) {
    for (e, guard) in &guards {
        let Ok((xf, target)) = targets.get(guard.of) else {
            commands.entity(e).remove::<Guard>();
            continue;
        };
        if target.dead_hull {
            commands.entity(e).remove::<Guard>();
            continue;
        }
        let Ok(mut hull) = hulls.get_mut(e) else {
            continue;
        };
        let radius = target.model.radius().max(0.1);
        hull.order = Some(xf.translation + xf.rotation * (guard.station * radius));
    }
}

/// Pick the ship a selection will guard, on the next left press.
///
/// It acts only in `OrderMode::Guard`, which is the rule the whole enum
/// exists for: the same press is a band box in `Idle` and a shot in `Range`,
/// and exactly one system reads it per frame. Escape leaves the mode without
/// guarding anything, which is the move order's own rule kept.
///
/// A ship may not guard ITSELF, and the nearest hull to a click is very often
/// the one already selected: without that check the first press of this would
/// hand a ship an order to fly to where it already is, for ever.
pub(crate) fn guard_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut mode: ResMut<OrderMode>,
    mut commands: Commands,
    mut ack: ResMut<Ack>,
    picked: Query<Entity, (With<Selected>, With<Hull>)>,
    hulls: Query<(Entity, &Transform, &Hull), Without<Hive>>,
) {
    if *mode != OrderMode::Guard {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        *mode = OrderMode::Idle;
        ack.text = "guard cancelled".into();
        ack.left = ACK_LIFE;
        return;
    }
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    *mode = OrderMode::Idle;
    let Some(ray) = cursor_ray(&windows, &cams) else {
        return;
    };
    let mine: Vec<Entity> = picked.iter().collect();
    // The hull whose centre is nearest the ray, which is the same answer a
    // click on a ship gives everywhere else in this game.
    let mut best: Option<(f32, Entity)> = None;
    for (e, xf, hull) in &hulls {
        if hull.dead_hull || mine.contains(&e) {
            continue;
        }
        let to = xf.translation - ray.origin;
        let along = to.dot(*ray.direction);
        if along <= 0.0 {
            continue;
        }
        let miss = (to - *ray.direction * along).length();
        if miss < hull.model.radius() * 2.0 && best.is_none_or(|(b, _)| miss < b) {
            best = Some((miss, e));
        }
    }
    let Some((_, of)) = best else {
        ack.text = "nothing there to guard".into();
        ack.left = ACK_LIFE;
        return;
    };
    for (n, e) in mine.iter().enumerate() {
        commands.entity(*e).insert(Guard {
            of,
            station: Vec3::from(formation::station(Shape::Sphere, n)),
        });
    }
    ack.text = format!("{} guarding", mine.len());
    ack.left = ACK_LIFE;
}

/// Put every selected ship on a stance, and say what it is now.
pub(crate) fn set_stance(
    commands: &mut Commands,
    fleet: &mut FleetStance,
    picked: &[Entity],
) -> Stance {
    let want = fleet.0.next();
    fleet.0 = want;
    for &e in picked {
        want.apply(e, commands);
    }
    want
}

/// Take the reactor of every selected ship.
///
/// It goes through the reactor rule rather than despawning anything, so a
/// scuttled ship leaves the same wreck the swarm would have made of it: one
/// death path and not two, which is the rule `--wreck` exists under.
pub(crate) fn scuttle<F: QueryFilter>(
    hulls: &mut Query<&mut Hull, F>,
    picked: &[Entity],
    tick: u32,
) -> usize {
    let mut n = 0;
    for &e in picked {
        let Ok(mut hull) = hulls.get_mut(e) else {
            continue;
        };
        if hull.dead_hull {
            continue;
        }
        // Every cell of the reactor, which is what `go_critical` is waiting to
        // see. Exactly what `script_wreck` does for the harness, because there
        // is one way a ship dies and this is a second caller of it rather than
        // a second path to it.
        let hull = &mut *hull;
        for c in hull.reactor.clone() {
            hull.damage.kill(c, tick);
        }
        n += 1;
    }
    n
}
