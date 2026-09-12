//! A ray through the lattice: which live cell it meets first, where, and
//! through which face.
//!
//! This is how a click lands on a hull. The camera's ray is taken into the
//! model's frame and walked cell by cell (Amanatides and Woo), so the answer
//! is the first cell that is actually there along the line, holes included:
//! a shot aimed into a crater lands on the crater's floor, exactly where a
//! chewer's bite would. Brute force is sampling the ray at fine steps, and
//! `march_agrees_with_walking_the_ray` holds the walk to that.

use crate::voxel::{mat, VoxelModel};

/// Where a ray met the hull.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub cell: usize,
    /// Distance along the ray, in the units the ray was given in.
    pub t: f32,
    /// The face it came in through, as a unit axis pointing OUT of the cell.
    pub normal: [f32; 3],
    pub point: [f32; 3],
}

/// The first live cell along `origin + t * dir`, for `t` in `0..=max_t`, in
/// the model's frame. `dir` need not be a unit vector; `t` is in its units.
pub fn march(
    m: &VoxelModel,
    live: impl Fn(usize) -> bool,
    origin: [f32; 3],
    dir: [f32; 3],
    max_t: f32,
) -> Option<Hit> {
    let n = [m.nx, m.ny, m.nz];
    let half = [
        m.nx as f32 * 0.5 * m.cell,
        m.ny as f32 * 0.5 * m.cell,
        m.nz as f32 * 0.5 * m.cell,
    ];
    // Clip to the lattice box. Which slab the ray came in through is the face
    // the first cell is entered by.
    let (mut t0, mut t1, mut entry_axis) = (0.0f32, max_t, None);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if origin[a].abs() > half[a] {
                return None;
            }
            continue;
        }
        let ta = (-half[a] - origin[a]) / dir[a];
        let tb = (half[a] - origin[a]) / dir[a];
        let (lo, hi) = if ta < tb { (ta, tb) } else { (tb, ta) };
        if lo > t0 {
            t0 = lo;
            entry_axis = Some(a);
        }
        t1 = t1.min(hi);
    }
    if t0 > t1 {
        return None;
    }
    // Nudge inside the box, so the first cell is the one just past the face
    // and not the one whose wall the point sits on.
    let mut t = t0 + 1e-5;
    let mut cell = [0i32; 3];
    let mut step = [0i32; 3];
    let mut t_max = [f32::INFINITY; 3];
    let mut t_delta = [f32::INFINITY; 3];
    for a in 0..3 {
        let p = origin[a] + dir[a] * t;
        cell[a] = ((p + half[a]) / m.cell)
            .floor()
            .clamp(0.0, n[a] as f32 - 1.0) as i32;
        if dir[a].abs() < 1e-9 {
            continue;
        }
        step[a] = if dir[a] > 0.0 { 1 } else { -1 };
        let next = if dir[a] > 0.0 {
            -half[a] + (cell[a] + 1) as f32 * m.cell
        } else {
            -half[a] + cell[a] as f32 * m.cell
        };
        t_max[a] = (next - origin[a]) / dir[a];
        t_delta[a] = m.cell / dir[a].abs();
    }
    let mut came_by = entry_axis;
    loop {
        let idx = m.index(cell[0] as usize, cell[1] as usize, cell[2] as usize);
        if m.grid[idx] != mat::EMPTY && live(idx) {
            let mut normal = [0.0; 3];
            match came_by {
                Some(a) => normal[a] = -(step[a] as f32),
                // Started inside a live cell: the only honest face is the
                // one the ray would have to leave by, backwards.
                None => {
                    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2])
                        .sqrt()
                        .max(1e-9);
                    normal = [-dir[0] / len, -dir[1] / len, -dir[2] / len];
                }
            }
            return Some(Hit {
                cell: idx,
                t,
                normal,
                point: [
                    origin[0] + dir[0] * t,
                    origin[1] + dir[1] * t,
                    origin[2] + dir[2] * t,
                ],
            });
        }
        // Step to the next cell along whichever wall is nearest.
        let a = (0..3)
            .min_by(|&x, &y| t_max[x].total_cmp(&t_max[y]))
            .unwrap_or(0);
        t = t_max[a];
        if t > t1 || t > max_t {
            return None;
        }
        cell[a] += step[a];
        if cell[a] < 0 || cell[a] >= n[a] as i32 {
            return None;
        }
        t_max[a] += t_delta[a];
        came_by = Some(a);
    }
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

    #[test]
    fn a_ray_from_outside_lands_on_the_near_face() {
        let m = block(6, 4, 4);
        let hit = march(&m, |_| true, [-3.0, 0.1, 0.1], [1.0, 0.0, 0.0], 10.0).unwrap();
        assert_eq!(m.at(hit.cell).0, 0);
        assert_eq!(hit.normal, [-1.0, 0.0, 0.0]);
        // The near face is at -3 cells of 0.25, so three units in from -3.
        assert!((hit.t - 2.25).abs() < 1e-3, "{}", hit.t);
    }

    #[test]
    fn a_ray_that_misses_returns_nothing() {
        let m = block(6, 4, 4);
        assert!(march(&m, |_| true, [-3.0, 2.0, 0.0], [1.0, 0.0, 0.0], 10.0).is_none());
        assert!(march(&m, |_| true, [-3.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 10.0).is_none());
        // Too short to reach.
        assert!(march(&m, |_| true, [-3.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0).is_none());
    }

    #[test]
    fn a_hole_lets_the_ray_through_to_the_cell_behind() {
        let m = block(6, 4, 4);
        let first = march(&m, |_| true, [-3.0, 0.1, 0.1], [1.0, 0.0, 0.0], 10.0).unwrap();
        let behind = march(
            &m,
            |n| n != first.cell,
            [-3.0, 0.1, 0.1],
            [1.0, 0.0, 0.0],
            10.0,
        )
        .unwrap();
        assert_eq!(m.at(behind.cell).0, 1);
        assert_eq!(behind.normal, [-1.0, 0.0, 0.0]);
        assert!(behind.t > first.t);
    }

    #[test]
    fn a_ray_from_above_comes_in_through_the_top() {
        let m = block(6, 4, 4);
        let hit = march(&m, |_| true, [0.1, 5.0, -0.1], [0.0, -1.0, 0.0], 10.0).unwrap();
        assert_eq!(m.at(hit.cell).1, 3);
        assert_eq!(hit.normal, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn march_agrees_with_walking_the_ray() {
        let mut m = block(8, 6, 10);
        // Chew a crater out of one flank so there are holes to fall into.
        for i in 0..3 {
            for j in 2..4 {
                for k in 3..6 {
                    m.set(i, j, k, mat::EMPTY, 0);
                }
            }
        }
        let mut rng = crate::rng::Rng::new(9);
        let mut agreed = 0;
        for _ in 0..400 {
            let origin = [
                rng.range(-4.0, 4.0),
                rng.range(-4.0, 4.0),
                rng.range(-4.0, 4.0),
            ];
            let dir = [
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            ];
            if dir.iter().all(|d| d.abs() < 0.05) {
                continue;
            }
            let walked = (0..20000).find_map(|s| {
                let t = s as f32 * 0.001;
                let p = [
                    origin[0] + dir[0] * t,
                    origin[1] + dir[1] * t,
                    origin[2] + dir[2] * t,
                ];
                let (i, j, k) = m.cell_of_point(p)?;
                let n = m.index(i, j, k);
                (m.grid[n] != mat::EMPTY).then_some(n)
            });
            let marched = march(&m, |_| true, origin, dir, 20.0).map(|h| h.cell);
            assert_eq!(marched, walked, "origin {origin:?} dir {dir:?}");
            agreed += 1;
        }
        assert!(agreed > 300);
    }
}
