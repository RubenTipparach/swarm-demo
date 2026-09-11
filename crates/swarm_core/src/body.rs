//! A hull as a rigid body, read off the cells that are still alive: its
//! mass, where its centre of mass is, how it resists turning, and what a hit
//! at a point does to it.
//!
//! A voxel hull hands all of this over for nothing. Every live cell is a
//! unit mass at its own centre, so the mass, the centre and the inertia
//! tensor are three sums over the cells, and a ship with its bow shot off
//! weighs less, balances further aft and spins differently from a whole one
//! by construction rather than by a table somebody updates. Each cell also
//! carries the inertia of its own cube (`c^2 / 6` about each axis), which is
//! physically what it is and is also what keeps a single cell from being a
//! body that cannot be turned at all.
//!
//! An impulse at a point is the whole of the dynamics: `dv = j / m` and
//! `dw = I^-1 (r x j)`, with `r` from the centre of mass. The app integrates
//! the velocities it gets back; nothing here knows what a frame is. Every
//! quantity is in the MODEL's frame, so a caller working in the world takes
//! the point and the impulse into the hull's frame first and the two
//! velocities back out, and the tensor never has to be rotated.

use crate::voxel::{mat, VoxelModel};

/// A three by three matrix, row major.
pub type Mat3 = [[f32; 3]; 3];

/// What a hull weighs, where it balances, and how it resists a turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// In cells: every live cell weighs one.
    pub mass: f32,
    /// The centre of mass, in the model's frame.
    pub centre: [f32; 3],
    /// The inertia tensor about the centre of mass, in cell mass times model
    /// units squared.
    pub inertia: Mat3,
    /// Its inverse, kept because a kick needs it and a hull is kicked more
    /// often than it is re-weighed.
    pub inverse: Mat3,
}

impl Body {
    /// Weigh a model, counting only the cells `live` says are still there.
    /// `None` for a model with nothing live in it.
    pub fn of(m: &VoxelModel, live: impl Fn(usize) -> bool) -> Option<Body> {
        let mut mass = 0.0f32;
        let mut sum = [0.0f32; 3];
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY || !live(n) {
                continue;
            }
            let p = m.centre_of(n);
            mass += 1.0;
            for a in 0..3 {
                sum[a] += p[a];
            }
        }
        if mass <= 0.0 {
            return None;
        }
        let centre = [sum[0] / mass, sum[1] / mass, sum[2] / mass];
        // About the centre: sum over cells of (r.r) I - r r^T, plus each
        // cell's own cube.
        let own = m.cell * m.cell / 6.0;
        let mut inertia = [[0.0f32; 3]; 3];
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY || !live(n) {
                continue;
            }
            let p = m.centre_of(n);
            let r = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
            let rr = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
            for (a, row) in inertia.iter_mut().enumerate() {
                for (b, v) in row.iter_mut().enumerate() {
                    *v += if a == b {
                        rr - r[a] * r[a] + own
                    } else {
                        -r[a] * r[b]
                    };
                }
            }
        }
        let inverse = invert(&inertia)?;
        Some(Body {
            mass,
            centre,
            inertia,
            inverse,
        })
    }

    /// An impulse `j` applied at the point `at`: the change in velocity and
    /// the change in angular velocity, both in the model's frame.
    pub fn kick(&self, at: [f32; 3], j: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let dv = [j[0] / self.mass, j[1] / self.mass, j[2] / self.mass];
        let r = [
            at[0] - self.centre[0],
            at[1] - self.centre[1],
            at[2] - self.centre[2],
        ];
        let torque = [
            r[1] * j[2] - r[2] * j[1],
            r[2] * j[0] - r[0] * j[2],
            r[0] * j[1] - r[1] * j[0],
        ];
        (dv, apply(&self.inverse, torque))
    }
}

/// `m * v`.
pub fn apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (a, row) in m.iter().enumerate() {
        out[a] = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
    }
    out
}

/// The inverse of a three by three, or `None` when it has none to speak of.
pub fn invert(m: &Mat3) -> Option<Mat3> {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let co = [
        [e * i - f * h, -(d * i - f * g), d * h - e * g],
        [-(b * i - c * h), a * i - c * g, -(a * h - b * g)],
        [b * f - c * e, -(a * f - c * d), a * e - b * d],
    ];
    let det = a * co[0][0] + b * co[0][1] + c * co[0][2];
    // Guarded where it can leave its domain: a tensor this small is a hull
    // with nothing in it, and a division by it is every velocity wrong.
    if det.is_nan() || det.abs() <= 1e-12 {
        return None;
    }
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, v) in row.iter_mut().enumerate() {
            *v = co[k][r] / det;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(nx: usize, ny: usize, nz: usize) -> VoxelModel {
        let mut m = VoxelModel::new(nx, ny, nz, 0.25);
        for i in 0..nx {
            for j in 0..ny {
                for k in 0..nz {
                    m.set(i, j, k, mat::PLATE, 0x808080);
                }
            }
        }
        m
    }

    fn near(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_block_weighs_its_cells_and_balances_in_the_middle() {
        let m = block(4, 2, 6);
        let b = Body::of(&m, |_| true).unwrap();
        assert_eq!(b.mass, 48.0);
        for a in 0..3 {
            assert!(near(b.centre[a], 0.0, 1e-5), "centre {:?}", b.centre);
        }
    }

    #[test]
    fn the_inertia_of_a_block_is_the_lattice_formula() {
        let (nx, ny, nz) = (4usize, 2usize, 6usize);
        let m = block(nx, ny, nz);
        let b = Body::of(&m, |_| true).unwrap();
        let c2 = m.cell * m.cell;
        let n = (nx * ny * nz) as f32;
        let var = |k: usize| ((k * k) as f32 - 1.0) / 12.0;
        let expect = |p: usize, q: usize| c2 * n * (var(p) + var(q)) + n * c2 / 6.0;
        assert!(
            near(b.inertia[0][0], expect(ny, nz), 1e-3),
            "{}",
            b.inertia[0][0]
        );
        assert!(
            near(b.inertia[1][1], expect(nx, nz), 1e-3),
            "{}",
            b.inertia[1][1]
        );
        assert!(
            near(b.inertia[2][2], expect(nx, ny), 1e-3),
            "{}",
            b.inertia[2][2]
        );
        assert!(near(b.inertia[0][1], 0.0, 1e-4));
        // And the inverse really is one.
        let p = apply(&b.inverse, apply(&b.inertia, [1.0, 2.0, 3.0]));
        assert!(near(p[0], 1.0, 1e-4) && near(p[1], 2.0, 1e-4) && near(p[2], 3.0, 1e-4));
    }

    #[test]
    fn a_kick_through_the_centre_shoves_and_does_not_turn() {
        let b = Body::of(&block(4, 4, 8), |_| true).unwrap();
        let (dv, dw) = b.kick(b.centre, [0.0, 0.0, 96.0]);
        assert!(near(dv[2], 96.0 / b.mass, 1e-6));
        assert!(dw.iter().all(|w| w.abs() < 1e-6), "{dw:?}");
    }

    #[test]
    fn a_kick_at_the_rim_turns_it_about_the_right_axis() {
        let b = Body::of(&block(4, 4, 8), |_| true).unwrap();
        // Pushed along +x at a point above the centre: torque is r x j, which
        // for r up and j forward is about -z.
        let at = [b.centre[0], b.centre[1] + 0.5, b.centre[2]];
        let (_, dw) = b.kick(at, [3.0, 0.0, 0.0]);
        assert!(dw[2] < 0.0, "{dw:?}");
        assert!(dw[0].abs() < 1e-6 && dw[1].abs() < 1e-6, "{dw:?}");
        // And momentum is what was put in.
        let (dv, _) = b.kick(at, [3.0, 0.0, 0.0]);
        assert!(near(dv[0] * b.mass, 3.0, 1e-5));
    }

    #[test]
    fn losing_half_the_cells_moves_the_centre_toward_what_is_left() {
        let m = block(8, 2, 2);
        let whole = Body::of(&m, |_| true).unwrap();
        let half = Body::of(&m, |n| m.at(n).0 >= 4).unwrap();
        assert_eq!(half.mass, whole.mass / 2.0);
        assert!(half.centre[0] > whole.centre[0] + 0.4, "{:?}", half.centre);
        assert!(half.inertia[1][1] < whole.inertia[1][1]);
    }

    #[test]
    fn a_single_cell_is_still_a_body_and_nothing_is_none() {
        let m = block(1, 1, 1);
        assert!(Body::of(&m, |_| true).is_some());
        assert!(Body::of(&m, |_| false).is_none());
    }
}
