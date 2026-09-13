//! What in this scene is a LIGHT: a drive plume and a fireball.
//!
//! Everything bright here was emissive and nothing else. A flame is geometry
//! with its colour written well over white, a fireball is a thousand additive
//! sparks, and both of them are bright PIXELS that light nothing: a frigate at
//! full burn lit the black behind it exactly as much as one drifting, and a
//! reactor going off a length from a carrier left that carrier the colour it
//! was on the frame before. Light with no source in it is a sticker.
//!
//! Two point lights, which is all it takes, and they are cheap for the reason
//! everything else in this design is cheap: there are DOZENS of hulls and a
//! handful of fireballs at a time, so this is the ECS's half of the split and
//! not the swarm's. A mote never gets one.

use crate::*;

/// The light a hull's drives throw, as a child of the hull.
///
/// A child, so it rides the ship's own frame and sits behind its stern without
/// anything recomputing where that is every frame; and one per HULL rather
/// than one per engine cluster, because six bells a length apart light the
/// same piece of space and six lights would be six times the cost for a
/// difference nobody can see.
#[derive(Component)]
pub(crate) struct PlumeLight;

/// A fireball's own light: short, bright, and gone.
#[derive(Component)]
pub(crate) struct Flash {
    pub(crate) born: u32,
    pub(crate) ticks: u32,
    /// What it starts at, in lumens.
    pub(crate) lumens: f32,
}

/// How far a drive's light reaches, in hull radii, and what it is worth at
/// full throttle. Warm, because a drive burns orange out at the bell whatever
/// the white hot core is doing.
const PLUME_RANGE: f32 = 6.0;
const PLUME_LUMENS: f32 = 620_000.0;
const PLUME_COLOUR: (f32, f32, f32) = (1.0, 0.55, 0.22);

/// And a fireball's: it reaches several times the blast's own radius, because
/// what a player should see is the ships AROUND it lighting up.
const FLASH_RANGE: f32 = 9.0;
const FLASH_TICKS: u32 = 26;

/// What a reactor is worth when it goes, and what a blast on the range is.
/// A reactor takes a whole ship with it; a shot on the range does not.
pub(crate) const REACTOR_LUMENS: f32 = 40_000_000.0;
pub(crate) const BLAST_LUMENS: f32 = 9_000_000.0;

/// Hang a plume light on a hull. Called once, where the hull is spawned.
pub(crate) fn add_plume_light(commands: &mut Commands, hull: Entity, radius: f32) {
    let light = commands
        .spawn((
            PointLight {
                color: Color::linear_rgb(PLUME_COLOUR.0, PLUME_COLOUR.1, PLUME_COLOUR.2),
                intensity: 0.0,
                range: radius * PLUME_RANGE,
                shadows_enabled: false,
                ..default()
            },
            // Behind the stern. The lattice runs stern to bow, so a hull's
            // own -Z is where its exhaust goes.
            Transform::from_xyz(0.0, 0.0, -radius * 0.9),
            PlumeLight,
        ))
        .id();
    commands.entity(hull).add_child(light);
}

/// A fireball lights what is around it.
///
/// `radius` is the blast's own, and the light reaches several times further,
/// because the point of it is the hulls standing off from the thing that went
/// up rather than the fire itself, which is already made of light.
pub(crate) fn flash_light(commands: &mut Commands, at: Vec3, radius: f32, lumens: f32, tick: u32) {
    commands.spawn((
        PointLight {
            color: Color::linear_rgb(1.0, 0.72, 0.38),
            intensity: lumens,
            range: radius * FLASH_RANGE,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(at),
        Flash {
            born: tick,
            ticks: FLASH_TICKS,
            lumens,
        },
    ));
}

/// Every hull's plume light follows the throttle that hull is actually
/// pulling, which is the same question `draw_flames` asks of the same hull:
/// the light and the flame are one fact drawn twice, so they are read off one
/// function and cannot disagree.
pub(crate) fn light_plumes(
    hulls: Query<(&Hull, &Transform, &Children, Option<&Hive>)>,
    mut lights: Query<&mut PointLight, With<PlumeLight>>,
) {
    for (hull, xf, children, hive) in &hulls {
        // A wreck's drives are not burning, whatever its cells still say.
        let want = if hull.dead_hull {
            0.0
        } else if hive.is_some() {
            // A carrier is always under way at a fixed low throttle, which is
            // exactly what `draw_flames` draws it at.
            0.5
        } else {
            let forward = xf.rotation * Vec3::Z;
            hull.engines
                .iter()
                .map(|e| throttle_of(hull, forward, e.gun.out[2]) * e.left(&hull.damage))
                .fold(0.0f32, f32::max)
        };
        for c in children.iter() {
            if let Ok(mut light) = lights.get_mut(c) {
                light.intensity = PLUME_LUMENS * want.clamp(0.0, 1.0);
            }
        }
    }
}

/// A flash fades on the same curve a spark does and then goes.
pub(crate) fn fade_flashes(
    tick: Res<Tick>,
    mut commands: Commands,
    mut flashes: Query<(Entity, &Flash, &mut PointLight)>,
) {
    for (e, f, mut light) in &mut flashes {
        let age = tick.tick.saturating_sub(f.born);
        if age >= f.ticks {
            commands.entity(e).despawn();
            continue;
        }
        let left = 1.0 - age as f32 / f.ticks as f32;
        // Squared, so it is bright for an instant and then mostly gone, which
        // is what a fireball does. A linear fade reads as a lamp on a dimmer.
        light.intensity = f.lumens * left * left;
    }
}
