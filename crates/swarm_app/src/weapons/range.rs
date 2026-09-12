//! The range's weapons: what each does to the cells it lands on and how hard
//! it shoves the ship it landed on. The damage is the same path a fight
//! uses (a bite, a beam's ring of bites, a blast of cells); the two new ones
//! are a slug, which bores a column, and a torpedo, which is a bigger blast
//! that takes its time arriving.

use crate::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Weapon {
    Beam,
    Flak,
    Slug,
    Torpedo,
    Bite,
}

impl Weapon {
    pub(crate) const ALL: [Weapon; 5] = [
        Weapon::Beam,
        Weapon::Flak,
        Weapon::Slug,
        Weapon::Torpedo,
        Weapon::Bite,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Weapon::Beam => "beam",
            Weapon::Flak => "flak",
            Weapon::Slug => "slug",
            Weapon::Torpedo => "torpedo",
            Weapon::Bite => "bite",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Weapon> {
        Weapon::ALL.into_iter().find(|w| w.label() == s)
    }

    /// The key that arms it: one to five, in the order of `ALL`.
    pub(crate) fn key(self) -> KeyCode {
        match self {
            Weapon::Beam => KeyCode::Digit1,
            Weapon::Flak => KeyCode::Digit2,
            Weapon::Slug => KeyCode::Digit3,
            Weapon::Torpedo => KeyCode::Digit4,
            Weapon::Bite => KeyCode::Digit5,
        }
    }

    pub(crate) fn number(self) -> usize {
        Weapon::ALL.iter().position(|w| *w == self).unwrap_or(0) + 1
    }

    /// The shove, as a change of velocity in the target's own units a
    /// second if it landed through the centre of mass: the impulse is this
    /// times the target's mass. A slug on a frigate hit at the rim comes out
    /// at about twenty five degrees a second, which is a tumble a player can
    /// watch; a bite has no mass behind it.
    pub(crate) fn kick(self) -> f32 {
        match self {
            Weapon::Beam => 0.02,
            Weapon::Flak => 0.25,
            Weapon::Slug => 0.6,
            Weapon::Torpedo => 0.9,
            Weapon::Bite => 0.0,
        }
    }

    /// Whether holding the button keeps it firing.
    pub(crate) fn held(self) -> bool {
        matches!(self, Weapon::Beam)
    }

    /// Whether it flies before it lands.
    pub(crate) fn flies(self) -> bool {
        matches!(self, Weapon::Torpedo)
    }
}

/// What a weapon does to the cells where a ray met the hull, in the model's
/// frame: the breaches it opens. `dir` is the unit direction the shot came
/// in along.
pub(crate) fn land(
    weapon: Weapon,
    model: &VoxelModel,
    damage: &mut DamageGrid,
    hit: &swarm_core::ray::Hit,
    dir: [f32; 3],
    tick: u32,
) -> Vec<Breach> {
    let cell = model.cell;
    match weapon {
        Weapon::Beam => {
            // The ring of bites a beam already takes off a carrier, round
            // the point rather than at it, so a beam opens a crater the size
            // of a beam.
            let seed = if dir[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let u = swarm_core::fx::normalise(cross(dir, seed));
            let v = cross(dir, u);
            (0..BEAM_BITES)
                .filter_map(|k| {
                    let a = k as f32 / BEAM_BITES as f32 * std::f32::consts::TAU;
                    let r = cell * 1.4;
                    let p = [
                        hit.point[0] + (u[0] * a.cos() + v[0] * a.sin()) * r,
                        hit.point[1] + (u[1] * a.cos() + v[1] * a.sin()) * r,
                        hit.point[2] + (u[2] * a.cos() + v[2] * a.sin()) * r,
                    ];
                    damage.bite(model, p, BEAM_DAMAGE, tick)
                })
                .collect()
        }
        Weapon::Flak => damage.blast_cells(
            model,
            &Blast {
                at: hit.point,
                radius: cell * 2.2,
                born: tick,
            },
            tick,
        ),
        Weapon::Slug => damage.bore(model, hit.point, dir, cell * 7.0, tick),
        Weapon::Torpedo => damage.blast_cells(
            model,
            &Blast {
                at: hit.point,
                radius: cell * 4.0,
                born: tick,
            },
            tick,
        ),
        // What a chewer does, one cell, at the point.
        Weapon::Bite => damage
            .bite(model, hit.point, 9.0, tick)
            .into_iter()
            .collect(),
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
