//! The carriers: where they stand, how they drift, and how they bleed.

use crate::*;

/// How big a mothership is, in hull radii, and what it takes to kill one.
pub(crate) const HIVE_RADIUS: f32 = 1.6;

/// What a carrier's chitin is worth, against the bare material.
///
/// NOT the hundred the player's plating carries. The armour multiplier is
/// what makes the ship a siege, and putting it on both sides would mean
/// neither could hurt the other and the battle would never resolve. Eight is
/// enough that a carrier takes sustained fire and not so much that it cannot
/// be killed inside a match.
pub(crate) const HIVE_ARMOUR: f32 = 8.0;

/// Where the carriers sit, in hull radii. They used to be at seven, which put
/// them inside the swarm's own standoff and made the whole picture one clump;
/// a carrier is a thing you have to CROSS the battlefield to reach.
pub(crate) const HIVE_NEAR: f32 = 14.0;

pub(crate) const HIVE_FAR: f32 = 22.0;

/// A mothership: where the swarm comes from, and the thing worth killing.
#[derive(Component)]
pub(crate) struct Hive {
    /// Read off its own cells, the same query a ship's engines come from, and
    /// with those cells, so a carrier that has had its drives chewed off stops
    /// burning them exactly as a ship does.
    pub(crate) engines: Vec<Drive>,
    /// In world units, which is what the shader is told and what a beam is
    /// tested against. The model's own radius times the scale it is drawn at.
    pub(crate) radius: f32,
    /// What it is drawn at, which is what turns a cell offset into a world
    /// one anywhere the model's own coordinates are used.
    pub(crate) scale: f32,
    pub(crate) vel: Vec3,
    pub(crate) seed: u64,
}

/// Carriers drift across the line to the ship, and turn as they go.
pub(crate) fn move_hives(time: Res<Time>, scene: Res<SceneSpec>, mut hives: Query<(&Hive, &mut Transform)>) {
    let dt = scene.step(&time);
    for (h, mut xf) in &mut hives {
        xf.translation += h.vel * dt;
        xf.rotate_y(dt * 0.13);
    }
}

pub(crate) fn publish_hives(hives: Query<(&Hive, &Transform, &Hull)>, mut cfg: ResMut<SwarmConfig>) {
    cfg.hives.clear();
    for (h, xf, hull) in &hives {
        // A wreck launches nothing. `go_critical` takes a carrier the same way
        // it takes a ship now, so "still flying" is the hull's own answer
        // rather than a second hit point number kept beside it.
        if hull.dead_hull {
            continue;
        }
        cfg.hives.push(xf.translation.extend(h.radius));
    }
}

/// A carrier that has been hit shows it, by glowing hotter as it comes apart.
///
/// Its own material rather than one shared by all ten, which is what a health
/// bar costs when there is no HUD: a player has to be able to see which of
/// them is nearly dead, and the only place to say so is the thing itself.
/// A carrier that has been opened up BLEEDS.
///
/// This used to set the material's emissive to a green that grew with damage,
/// and emissive applies over the whole material rather than over the places
/// that were hit: at 2.2 it did not mark a wound, it turned the entire hull
/// into a uniform green bulb the shape of a mothership. The tint is gone
/// altogether, because a carrier now carries the same per cell damage a ship
/// does and the wound is drawn where the wound IS.
///
/// What is left is the bleed: purple, off the torn cells, thicker the worse it
/// is. Purple because the swarm is chitin over violet, and green is the colour
/// of the lamps ON a carrier rather than the colour of the animal.
pub(crate) fn bleed_hives(
    tick: Res<Tick>,
    hives: Query<(&Hive, &Transform, &Hull)>,
    mut sparks: ResMut<SparkQueue>,
) {
    for (h, xf, hull) in &hives {
        if hull.dead_hull {
            continue;
        }
        let hurt = (hull.damage.dead_count() as f32 / hull.cells.max(1) as f32).clamp(0.0, 1.0);
        if hurt < 0.004 {
            continue;
        }
        let every = (6.0 - 4.0 * (hurt * 8.0).min(1.0)).max(1.0) as u32;
        if tick.tick % every != 0 {
            continue;
        }
        let n = (1.0 + 4.0 * (hurt * 8.0).min(1.0)) as u32;
        let mut out: Vec<Spark> = Vec::new();
        for k in 0..n {
            let seed = h.seed as u32 ^ tick.tick.wrapping_mul(2654435761) ^ k;
            let d = |q: u32| swarm_core::rng::hash_cell(seed + q) as f32 / u32::MAX as f32 - 0.5;
            let off = Vec3::new(d(1), d(7), d(19)).normalize_or(Vec3::Y) * h.radius * 0.92;
            breach_sparks(seed, tick.tick, (xf.translation + off).to_array(), off.normalize_or(Vec3::Y).to_array(), h.radius * 0.10, &mut out);
        }
        for sp in &mut out {
            sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
        }
        sparks.extend(out);
    }
}
