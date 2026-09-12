//! What a wound LOOKS like: the heat ramp a fresh hole cools along, the
//! crust it leaves, the soot round it, and what a cell is made of in hit
//! points.
//!
//! Split out of `damage` when that file went over this project's own nine
//! hundred lines. The line is the one the module docs already drew: this
//! answers what a hit looks like, and `damage` answers what a hit DOES.
//! Ported from redux-tribes `wound.ts`.

use crate::voxel::mat;

/// How long a fresh wound burns, in ticks. Sixty ticks a second, so a hole is
/// glowing for fifteen seconds and is char after that.
pub const COOL_TICKS: u32 = 900;

/// The ramp is quantised so a cooling wound is a repaint every 28 ticks rather
/// than every frame, and so two faces at nearly the same heat merge.
pub const HEAT_STEPS: u32 = 32;

/// How far the soot spreads from a hole, in cells, and how much of it there
/// is at each remove.
///
/// A shot that takes cells out of a flank does not leave the plate beside it
/// factory fresh, and that is what it looked like: the survivors of a partly
/// eaten quad were rebuilt in the hull's own colour, so the edge of every
/// wound was a clean cut through clean paint. The near ring is nearly all
/// soot and the far one is a smudge.
pub const SCORCH_RINGS: [f32; 2] = [0.78, 0.34];

/// The colour soot is. Not black: a burnt mark on plating is char, and flat
/// black reads as a hole rather than as a stain.
pub const CHAR: [f32; 3] = [0.09, 0.075, 0.07];

/// The heat ramp, hottest first, from `wound.ts`. White hot cores through
/// orange to char, and the last stop is not black: a cold wound is a hole with
/// a burnt edge, and flat black reads as a gap in the mesh.
const HEAT: [[f32; 4]; 5] = [
    [1.00, 1.00, 1.00, 1.00],
    [0.70, 1.00, 0.82, 0.62],
    [0.40, 0.88, 0.44, 0.22],
    [0.18, 0.52, 0.20, 0.09],
    [0.00, 0.10, 0.085, 0.08],
];

/// Where a cell sits on the ramp, given how long ago it died.
pub fn heat_of(died_at: u32, tick: u32) -> f32 {
    let age = tick.saturating_sub(died_at) as f32;
    (1.0 - age / COOL_TICKS as f32).clamp(0.0, 1.0)
}

/// The colour at a heat.
pub fn ramp(heat: f32) -> [f32; 3] {
    for i in 1..HEAT.len() {
        let hi = HEAT[i - 1];
        let lo = HEAT[i];
        if heat > lo[0] || i == HEAT.len() - 1 {
            let span = hi[0] - lo[0];
            let t = if span > 0.0 {
                ((heat - lo[0]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return [
                lo[1] + (hi[1] - lo[1]) * t,
                lo[2] + (hi[2] - lo[2]) * t,
                lo[3] + (hi[3] - lo[3]) * t,
            ];
        }
    }
    [HEAT[4][1], HEAT[4][2], HEAT[4][3]]
}

/// How much of the char crust still covers what it burned. A third stays once
/// the embers are gone: a crust does not un-char itself.
pub const COLD_CRUST: f32 = 0.34;
pub fn crust_alpha(heat: f32) -> f32 {
    COLD_CRUST + heat * (1.0 - COLD_CRUST)
}

/// Hit points a cell starts with, by what it is made of. Plate is what armour
/// is for; a glowing cell is a light and a light is fragile.
pub fn hp_for(m: u8) -> f32 {
    match m {
        mat::PLATE | mat::SKINNED => 100.0,
        mat::FRAME => 60.0,
        mat::CASE => 50.0,
        mat::MACHINE | mat::ACCENT => 40.0,
        mat::GLOW => 30.0,
        _ => 0.0,
    }
}
