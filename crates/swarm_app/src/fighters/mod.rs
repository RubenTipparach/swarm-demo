//! The squadron: a dozen fighters that patrol the ring, wear down inside it
//! and fire into it.

use crate::*;

/// One of the ship's own fighters.
///
/// It has no damage grid and no hit points: the swarm cannot shoot back, so a
/// fighter is a gun the player owns that flies rather than a ship that can be
/// lost. When the swarm can kill one, this grows a hull like everything else.
#[derive(Component)]
pub(crate) struct Fighter {
    /// Which slot of the squadron it is, which is what spreads the patrol
    /// stations and staggers the firing.
    pub(crate) slot: u32,
    /// One down to nought. A fighter patrols inside the ring the swarm holds,
    /// which is the densest part of the cloud, so it is worn down by BEING
    /// there rather than by any particular mote: the CPU cannot see where a
    /// mote is, so attrition by depth into the swarm's own band is the honest
    /// approximation and it puts the cost where the risk is.
    pub(crate) hp: f32,
    pub(crate) vel: Vec3,
    /// Where it is heading right now, in the world. Re-picked when it gets
    /// there, so a fighter flies a circuit of the cloud instead of parking.
    pub(crate) goal: Vec3,
}

/// The ship's own strike craft: how many it puts up, how far out they work,
/// how fast they fly and how often each one fires.
///
/// A dozen, and they are ENTITIES rather than motes, for the reason the whole
/// design turns on: the ECS holds what there are dozens of. They share one
/// mesh and one material between them, so twelve fighters are twelve
/// transforms and one upload, which is why a dozen is free and a thousand
/// would not be.
pub(crate) const FIGHTERS_HULL: &str = "terran_corvette";

/// How big a fighter is drawn, against the flagship's own radius.
pub(crate) const FIGHTER_SCALE: f32 = 0.16;

/// Where they work, in flagship radii: out at the swarm's own standoff.
pub(crate) const FIGHTER_STATION: f32 = 3.4;

pub(crate) const FIGHTER_SPEED: f32 = 9.0;

pub(crate) const FIGHTER_TURN: f32 = 2.6;

/// Ticks between one fighter's bursts, and how big a burst is in flagship
/// radii. Short ranged: a fighter has to GO to the cloud.
pub(crate) const FIGHTER_CADENCE: u32 = 14;

/// How fast a fighter is worn down at the very centre of the swarm's band, in
/// share of its life per second, and how fast it patches up once clear of it.
/// How many ticks between replacements, so a wing rebuilds rather than
/// popping back into existence the frame it was lost.
pub(crate) const FIGHTER_REPLACE: u32 = 150;

pub(crate) const FIGHTER_WEAR: f32 = 0.22;

pub(crate) const FIGHTER_MEND: f32 = 0.10;

pub(crate) const FIGHTER_BURST: f32 = 0.30;

/// Tell the swarm where its carriers are.
///
/// Only the LIVE ones, compacted: the shader takes a mote's hive index modulo
/// the length, so a carrier dying shortens this list and every fighter that
/// flew from it re-homes to whatever is left. No rule about it anywhere.
/// Put the squadron up, once, out of the flagship.
///
/// One mesh and one material for the whole squadron, built here and cloned
/// into every fighter: a corvette is a few thousand cells and meshing it
/// twelve times would cost twelve times as much for twelve identical
/// pictures. They carry no damage grid for the same reason they carry no hit
/// points, which is that nothing can hurt them yet.
pub(crate) fn launch_fighters(
    tick: Res<Tick>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    scene: Res<SceneSpec>,
    lead: Res<Lead>,
    flagship: Query<&Hull, With<Flagship>>,
    have: Query<(), With<Fighter>>,
    mut next: Local<u32>,
) {
    if scene.fighters == 0 {
        return;
    }
    let Ok(hull) = flagship.single() else { return };
    // Replaces losses rather than launching once. A squadron that could only
    // ever be spent would make `wear_fighters` a countdown to having none,
    // which is not a screen, it is a fuse.
    let out = have.iter().count() as u32;
    if out >= scene.fighters || tick.tick < *next {
        return;
    }
    *next = tick.tick + FIGHTER_REPLACE;
    // The whole squadron at once the first time, one at a time after that.
    // Pacing the FIRST launch would mean half a minute before the wing exists,
    // which is a screen that arrives after the fight it was meant to screen.
    let want = if out == 0 { scene.fighters } else { 1 };
    let radius = hull.model.radius();
    let m = load_hull(FIGHTERS_HULL);
    let sm = greedy_mesh(&m, None);
    let mats = surface_materials(&m, &tex, &mut materials);
    let scale = radius * FIGHTER_SCALE / m.radius();
    // One mesh per surface, made once, shared by every fighter.
    let parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)> = (0..SURF_COUNT)
        .filter(|&surf| sm.skin.get(surf).map(|x| x.quads()).unwrap_or(0) > 0)
        .map(|surf| {
            (
                meshes.add(to_mesh_where(&sm, |i| i == surf)),
                mats[surf].clone(),
            )
        })
        .collect();

    for n in out..(out + want).min(scene.fighters) {
        // Out of the flagship, spread round it, so a launch reads as coming
        // OFF the ship rather than appearing beside it.
        let a = n as f32 * 2.399_963_2;
        let off = Vec3::new(a.cos(), (n % 3) as f32 * 0.4 - 0.4, a.sin()) * radius * 0.9;
        let at = lead.pos + lead.rot * off;
        let f = commands
            .spawn((
                Transform::from_translation(at).with_scale(Vec3::splat(scale)),
                Visibility::default(),
                Fighter {
                    slot: n,
                    hp: 1.0,
                    vel: Vec3::ZERO,
                    goal: at,
                },
            ))
            .id();
        for (mesh, mat) in &parts {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::IDENTITY,
                ChildOf(f),
            ));
        }
    }
    if out == 0 {
        info!(
            "squadron up: {} of {} on {}",
            want, scene.fighters, FIGHTERS_HULL
        );
    }
}

/// Fly the squadron: a circuit of the swarm's own standoff, round the ship.
///
/// A fighter picks a point out where the cloud is, flies to it, and picks
/// another when it arrives. That is a PATROL rather than an intercept, and it
/// is deliberately not an intercept: the CPU cannot see where a mote is, so
/// there is nothing to intercept. Flying the circuit and firing into it is
/// what a screen actually does, and it puts the shots where the swarm has to
/// come through.
pub(crate) fn fly_fighters(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    lead: Res<Lead>,
    flagship: Query<&Hull, With<Flagship>>,
    mut fighters: Query<(&mut Fighter, &mut Transform)>,
) {
    let Ok(hull) = flagship.single() else { return };
    let dt = scene.step(&time);
    let radius = hull.model.radius();
    let station = radius * FIGHTER_STATION;
    for (mut f, mut xf) in &mut fighters {
        let to = f.goal - xf.translation;
        if to.length() < radius * 0.6 {
            // A new point on the sphere the swarm holds, hashed off the slot
            // and the tick rather than rolled, so a re-watch flies the same
            // circuit. Spread round the ship rather than randomly placed, or
            // twelve fighters converge on one patch and leave the rest open.
            let h = |k: u32| {
                swarm_core::rng::hash_cell(f.slot * 7919 + tick.tick + k) as f32 / u32::MAX as f32
            };
            let a = (f.slot as f32 * 2.399_963_2) + h(1) * 2.2;
            let y = h(2) * 1.6 - 0.8;
            let r = (1.0 - y * y).max(0.05).sqrt();
            f.goal = lead.pos + Vec3::new(r * a.cos(), y, r * a.sin()) * station;
        }
        let want = (f.goal - xf.translation).normalize_or(Vec3::Z) * radius * FIGHTER_SPEED;
        let dv = want - f.vel;
        let step = radius * FIGHTER_SPEED * 1.4 * dt;
        f.vel += if dv.length() > step {
            dv.normalize() * step
        } else {
            dv
        };
        xf.translation += f.vel * dt;
        if f.vel.length() > 1e-3 {
            // Negated for the reason every hull here is negated: Bevy's
            // forward is minus Z and a hull's bow is plus Z.
            let aim = Transform::from_translation(xf.translation)
                .looking_to(-f.vel.normalize(), Vec3::Y)
                .rotation;
            xf.rotation = xf.rotation.slerp(aim, (dt * FIGHTER_TURN).min(1.0));
        }
    }
}

/// A fighter is worn down by where it flies, and comes apart when it runs out.
///
/// It cannot be shot by a named mote, because no mote has a name on the CPU:
/// they live in a buffer and never come back. What IS knowable is where the
/// swarm holds, which is the ring round each ship, and a fighter patrols
/// inside that band on purpose. So attrition is depth into the band, which
/// puts the cost exactly where the risk is and needs nothing crossing the
/// boundary.
pub(crate) fn wear_fighters(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    cfg: Res<SwarmConfig>,
    mut commands: Commands,
    mut sparks: ResMut<SparkQueue>,
    mut fighters: Query<(Entity, &mut Fighter, &Transform)>,
) {
    let dt = scene.step(&time);
    for (e, mut f, xf) in &mut fighters {
        // How deep into the thickest part of the cloud it is, over every ship
        // the swarm is divided between.
        let mut deep = 0.0f32;
        for t in &cfg.targets {
            let d = xf.translation.distance(t.truncate());
            let band = t.w * FIGHTER_STATION;
            if d < band {
                deep = deep.max(1.0 - d / band.max(1e-4));
            }
        }
        if deep > 0.0 {
            f.hp -= deep * FIGHTER_WEAR * dt;
        } else {
            // Clear of it, and patching itself up.
            f.hp = (f.hp + FIGHTER_MEND * dt).min(1.0);
        }
        if f.hp > 0.0 {
            continue;
        }
        // Gone, and it goes the way everything else here goes: its own burst,
        // in the colours of the side it was on.
        let mut out: Vec<Spark> = Vec::new();
        blast_sparks(
            f.slot.wrapping_mul(7919) ^ tick.tick,
            xf.translation.to_array(),
            0.9,
            40,
            &mut out,
        );
        sparks.extend(out);
        commands.entity(e).despawn();
    }
}

/// And they shoot: a short burst ahead of where they are going.
///
/// A `Blast` of a small radius, which is a capsule of zero length, which is
/// the shape everything that kills a mote already is. The squadron therefore
/// needed no new weapon and no new resolution path: the swarm shader kills
/// whatever is inside the capsule and the CPU never learns what was.
pub(crate) fn fighters_fire(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    flagship: Query<&Hull, With<Flagship>>,
    fighters: Query<(&Fighter, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    let Ok(hull) = flagship.single() else { return };
    let radius = hull.model.radius();
    for (f, xf) in &fighters {
        // Staggered by slot, or twelve fighters fire on the same tick for
        // ever, which is one big burst rather than a screen putting up fire.
        if (tick.tick + f.slot * 5) % FIGHTER_CADENCE != 0 {
            continue;
        }
        let nose = xf.rotation * Vec3::Z;
        let at = xf.translation + nose * radius * 0.3;
        let burst = radius * FIGHTER_BURST;
        fx.blasts.push(Blast {
            at: at.to_array(),
            radius: burst,
            born: tick.tick,
        });
        let mut out: Vec<Spark> = Vec::new();
        muzzle_sparks(
            f.slot * 977 ^ tick.tick,
            at.to_array(),
            nose.to_array(),
            radius * 0.05,
            &mut out,
        );
        sparks.extend(out);
    }
}
