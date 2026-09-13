//! The tender: the one ship in the game that mends rather than breaks.
//!
//! Everything else here takes cells off something. A tender puts them back,
//! one at a time, for materials out of the bank, on whatever damaged hull it
//! is standing beside. That makes it a ship a player POSITIONS, which is the
//! same verb the whole game is about: it mends what it is near, so bringing
//! it to the hurt ship is the decision, and it costs the cargo a miner is
//! out there getting.

use crate::*;

/// How often a tender puts a cell back, in ticks, and what that costs in
/// materials.
///
/// Ten a second, which is fast enough to watch a crater close and slow
/// enough that a chewed cruiser is a long job. The cost is the yard's own
/// price for the same work, because it IS the same work: welding a cell
/// costs what a cell costs, and the difference between the field and the
/// yard is when you can afford to stop.
pub(crate) const MEND_TICKS: u32 = 6;
pub(crate) const MEND_COST: u32 = REPAIR_PER_CELL;

/// How far a tender reaches, in the hurt ship's own radii.
pub(crate) const TEND_REACH: f32 = 3.0;

/// What a tender may mend: anything of the fleet's that is not itself a
/// support ship, a carrier or a rock. `Without<Support>` is also what proves
/// this disjoint from the tenders' own query, since Bevy reads the FILTERS
/// rather than what anybody knows about the data.
pub(crate) type HurtShip<'a> = (Entity, &'a mut Hull, &'a Transform);
pub(crate) type HurtFilter = (Without<Support>, Without<Hive>, Without<Rock>);

/// A tender at work: the nearest hurt hull in reach, one cell at a time.
///
/// The queries are split by `With<Support>` and `Without<Support>`, which is
/// what proves them disjoint to Bevy: a tender never mends itself, and the
/// filter is what says so rather than a check inside the loop.
pub(crate) fn tend_repairs(
    tick: Res<Tick>,
    tenders: Query<(&Support, &Hull, &Transform)>,
    mut hurt: Query<HurtShip, HurtFilter>,
    mut bank: ResMut<Bank>,
) {
    if !tick.tick.is_multiple_of(MEND_TICKS) {
        return;
    }
    for (support, tender, at) in &tenders {
        if !support.role.mends() || tender.dead_hull {
            continue;
        }
        // Nothing in the bank, nothing welded: a repair in the field is paid
        // for out of the same pile an escort is, which is what makes mending
        // a choice rather than a trickle of free hull.
        if bank.materials < MEND_COST {
            return;
        }
        // The NEAREST hurt hull in reach, rather than the worst: a tender
        // mends what it is beside, so which ship gets the materials is the
        // player's decision and it is made by flying it there.
        let mut best: Option<(f32, Entity)> = None;
        for (e, hull, xf) in hurt.iter() {
            if hull.dead_hull || hull.damage.dead_count() == 0 {
                continue;
            }
            let reach = hull.model.radius() * TEND_REACH;
            let d = xf.translation.distance_squared(at.translation);
            if d > reach * reach {
                continue;
            }
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, e));
            }
        }
        let Some((_, e)) = best else { continue };
        let Ok((_, mut hull, _)) = hurt.get_mut(e) else {
            continue;
        };
        let hull = &mut *hull;
        // Outermost first, which is the order it was eaten in: plating
        // closed over a hole nothing has filled is a picture nobody
        // believes. `remesh_dirty` rebuilds the brick without being told
        // that this one went the other way.
        let Some(&cell) = hull.damage.holes(&hull.model, 1).first() else {
            continue;
        };
        if hull.damage.mend(cell) {
            bank.materials -= MEND_COST;
        }
    }
}
