//! What a shot is, what a blast kills, and what comes off a thing that dies.
//!
//! The pictures are the app's business; the SHAPES are not. Whether a mote
//! standing here is inside that beam is a rule, and a rule with two
//! implementations is a rule that will be changed in one of them, so the
//! volumes and their tests live here and the shader is handed the same
//! numbers. The kill test is duplicated in WGSL because the swarm lives on the
//! GPU and cannot be asked; `beam_kills` and `blast_kills` are what that copy
//! is checked against, and the tests below are the check.
//!
//! Everything is a pure function of its inputs and of a tick. Nothing here
//! reads a clock or rolls a die: a spark's drift is hashed off the thing that
//! threw it, so two screens watching one explosion throw the same debris.

use crate::rng::{drift_of, hash_cell, Rng};
use crate::voxel::{mat, VoxelModel, SURF_WEAPON};

/// A beam, as the volume it sweeps: a capsule from `from` to `to`.
///
/// The beam a player SEES is shortened at whatever it hit; this is the volume
/// that was actually swept, and the two are different on a miss. redux-tribes
/// learned that measuring a beam against its weapon's range went red on a shot
/// that had simply missed: the full range endpoint is what a miss looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// How wide the beam bites, in world units.
    pub radius: f32,
    pub born: u32,
}

/// A blast: everything inside a sphere, for as long as it is expanding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub at: [f32; 3],
    pub radius: f32,
    pub born: u32,
}

/// How long a beam is live, in ticks. Long enough to see, short enough that a
/// mote flying through the line a moment later is not killed by a beam that
/// has stopped firing.
pub const BEAM_TICKS: u32 = 9;
/// How long a blast keeps expanding and killing.
pub const BLAST_TICKS: u32 = 24;

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

#[inline]
pub fn normalise(a: [f32; 3]) -> [f32; 3] {
    let l = length(a);
    if l > 1e-6 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// The nearest point on a segment to a point, as a fraction along it.
///
/// Clamped, which is what makes this a capsule rather than an infinite
/// cylinder: without the clamp a mote a hundred units behind the muzzle is
/// on the beam's line and dies for it.
pub fn closest_on_segment(a: [f32; 3], b: [f32; 3], p: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 < 1e-12 {
        return 0.0;
    }
    (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
}

impl Beam {
    /// How far a point is from the beam's own line, and how far along it the
    /// nearest approach was.
    ///
    /// The fraction is what shortens a beam at what it hit: redux-tribes'
    /// rule is that the full range endpoint is what a MISS looks like, and a
    /// shot that connected is drawn only as far as the thing it connected
    /// with.
    pub fn nearest(&self, p: [f32; 3]) -> (f32, f32) {
        let t = closest_on_segment(self.from, self.to, p);
        let near = [
            self.from[0] + (self.to[0] - self.from[0]) * t,
            self.from[1] + (self.to[1] - self.from[1]) * t,
            self.from[2] + (self.to[2] - self.from[2]) * t,
        ];
        (length(sub(p, near)), t)
    }

    /// Is a point inside the capsule this beam swept?
    pub fn kills(&self, p: [f32; 3]) -> bool {
        self.nearest(p).0 <= self.radius
    }

    /// Does it reach a sphere of radius `r` about `p`, and how far along?
    pub fn reaches(&self, p: [f32; 3], r: f32) -> Option<f32> {
        let (d, t) = self.nearest(p);
        (d <= self.radius + r).then_some(t)
    }

    /// The same beam, cut short at a fraction of its length.
    pub fn cut(&self, t: f32) -> Beam {
        let t = t.clamp(0.0, 1.0);
        Beam {
            to: [
                self.from[0] + (self.to[0] - self.from[0]) * t,
                self.from[1] + (self.to[1] - self.from[1]) * t,
                self.from[2] + (self.to[2] - self.from[2]) * t,
            ],
            ..*self
        }
    }

    pub fn live(&self, tick: u32) -> bool {
        tick >= self.born && tick - self.born < BEAM_TICKS
    }

    /// Nought when it has just fired, one as it goes out.
    pub fn age(&self, tick: u32) -> f32 {
        (tick.saturating_sub(self.born) as f32 / BEAM_TICKS as f32).clamp(0.0, 1.0)
    }

    pub fn direction(&self) -> [f32; 3] {
        normalise(sub(self.to, self.from))
    }
}

impl Blast {
    /// The radius right now: a blast opens fast and stops.
    ///
    /// `sqrt` rather than a line, so most of the growth is in the first few
    /// ticks, which is what an explosion looks like. It is also the one
    /// transcendental looking thing IEEE-754 specifies exactly, so the shader
    /// and this agree to the bit.
    pub fn radius_at(&self, tick: u32) -> f32 {
        let t = (tick.saturating_sub(self.born) as f32 / BLAST_TICKS as f32).clamp(0.0, 1.0);
        self.radius * t.sqrt()
    }

    pub fn kills(&self, p: [f32; 3], tick: u32) -> bool {
        let r = self.radius_at(tick);
        let d = sub(p, self.at);
        dot(d, d) <= r * r
    }

    pub fn live(&self, tick: u32) -> bool {
        tick >= self.born && tick - self.born < BLAST_TICKS
    }

    pub fn age(&self, tick: u32) -> f32 {
        (tick.saturating_sub(self.born) as f32 / BLAST_TICKS as f32).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------- embers --

/// The ember atlas is 4 x 4 tiles and a face picks one by a hash of its own
/// cell. One tile on every face of a wound reads as a repeating pattern rather
/// than as burning, and sixteen is enough that the eye stops finding it.
pub const ATLAS: u32 = 4;
const ATLAS_STEP: f32 = 1.0 / ATLAS as f32;
/// Inset by half a texel of a 64 pixel tile, so the sampler cannot bleed a
/// neighbouring tile in along the seam, which shows as a bright rim.
const ATLAS_PAD: f32 = 0.5 / (64.0 * ATLAS as f32);

/// The corner of the tile a cell shows, bit for bit `tileUV` in `wound.ts`.
pub fn ember_tile(cell: u32) -> [f32; 2] {
    let t = hash_cell(cell) % (ATLAS * ATLAS);
    [(t % ATLAS) as f32 * ATLAS_STEP, (t / ATLAS) as f32 * ATLAS_STEP]
}

/// A corner of that tile, `c` in the mesher's own corner order.
pub fn ember_uv(tile: [f32; 2], c: [f32; 2]) -> [f32; 2] {
    let span = ATLAS_STEP - 2.0 * ATLAS_PAD;
    [tile[0] + ATLAS_PAD + c[0] * span, tile[1] + ATLAS_PAD + c[1] * span]
}

// ---------------------------------------------------------------- sparks --

/// What a spark is FOR, which is the only thing that separates them: how fast
/// it slows, how it is coloured, how long it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SparkKind {
    /// Off a cell that has just been chewed away. Hot, brief, thrown outward.
    Breach,
    /// Off a mote that has just been shot. Its own colour, and wetter.
    Gore,
    /// A gun going off. Hangs at the muzzle rather than travelling.
    Muzzle,
    /// A ship coming apart. Big, slow, and it stays hot a long time.
    Blast,
}

impl SparkKind {
    pub fn code(self) -> u32 {
        match self {
            SparkKind::Breach => 0,
            SparkKind::Gore => 1,
            SparkKind::Muzzle => 2,
            SparkKind::Blast => 3,
        }
    }
}

/// One particle, as the thing that asks for it describes it. The app owns
/// where these are stored and how they are drawn; this is the request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    /// Linear RGB, and it may exceed one: a spark is what bloom is for.
    pub colour: [f32; 3],
    pub size: f32,
    /// Seconds.
    pub life: f32,
    pub kind: SparkKind,
}

/// The spray off a cell that has just been taken off a hull.
///
/// Hashed from the cell and the tick rather than rolled, so two screens
/// watching one wound throw the same sparks, and so a scrub back and forward
/// puts the same ones in the air.
pub fn breach_sparks(cell: u32, tick: u32, at: [f32; 3], outward: [f32; 3], scale: f32, out: &mut Vec<Spark>) {
    const N: u32 = 7;
    for i in 0..N {
        let d = drift_of(cell, tick.wrapping_add(i.wrapping_mul(0x9E37)));
        let speed = scale * (7.0 + 9.0 * (d[0] * 0.5 + 0.5));
        let vel = [
            (outward[0] * 1.6 + d[0]) * speed,
            (outward[1] * 1.6 + d[1]) * speed,
            (outward[2] * 1.6 + d[2]) * speed,
        ];
        // White hot through orange: the ember texture carries the colour of a
        // burn, and this is how hot this one is.
        let heat = 0.45 + 0.55 * (d[1] * 0.5 + 0.5);
        out.push(Spark {
            pos: at,
            vel,
            colour: [3.4 * heat, 1.5 * heat * heat, 0.35 * heat * heat * heat],
            size: scale * (0.5 + 0.9 * (d[2] * 0.5 + 0.5)),
            life: 0.35 + 0.75 * (d[2] * 0.5 + 0.5),
            kind: SparkKind::Breach,
        });
    }
}

/// The flash at a muzzle: a few big, short, very bright sparks that barely
/// move, so the gun reads as having gone off rather than as having emitted
/// something.
pub fn muzzle_sparks(seed: u32, at: [f32; 3], dir: [f32; 3], scale: f32, out: &mut Vec<Spark>) {
    const N: u32 = 4;
    for i in 0..N {
        let d = drift_of(seed, i);
        out.push(Spark {
            pos: [at[0] + dir[0] * scale * 0.6, at[1] + dir[1] * scale * 0.6, at[2] + dir[2] * scale * 0.6],
            vel: [dir[0] * scale * 4.0 + d[0], dir[1] * scale * 4.0 + d[1], dir[2] * scale * 4.0 + d[2]],
            colour: [7.0, 4.4, 1.9],
            size: scale * (1.4 + 0.8 * (d[0] * 0.5 + 0.5)),
            life: 0.10 + 0.08 * (d[1] * 0.5 + 0.5),
            kind: SparkKind::Muzzle,
        });
    }
}

/// The fireball of a ship coming apart, as a shell of slow hot sparks.
pub fn blast_sparks(seed: u32, at: [f32; 3], radius: f32, count: u32, out: &mut Vec<Spark>) {
    let mut rng = Rng::new(seed as u64 ^ 0x5EED_B1A5);
    for _ in 0..count {
        // A direction off a rejection sampled cube, which needs no
        // transcendental and is uniform enough for a fireball.
        let mut dir = [0.0f32; 3];
        for _ in 0..8 {
            let c = [rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)];
            let l2 = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
            if (0.05..=1.0).contains(&l2) {
                let l = l2.sqrt();
                dir = [c[0] / l, c[1] / l, c[2] / l];
                break;
            }
        }
        let speed = radius * rng.range(0.7, 2.4);
        let heat = rng.range(0.55, 1.0);
        out.push(Spark {
            pos: [
                at[0] + dir[0] * radius * 0.18,
                at[1] + dir[1] * radius * 0.18,
                at[2] + dir[2] * radius * 0.18,
            ],
            vel: [dir[0] * speed, dir[1] * speed, dir[2] * speed],
            colour: [5.0 * heat, 2.2 * heat * heat, 0.5 * heat * heat * heat],
            size: radius * rng.range(0.16, 0.42),
            life: rng.range(0.9, 2.4),
            kind: SparkKind::Blast,
        });
    }
}

// ------------------------------------------------------------------ guns --

/// A gun on a hull: where its muzzle is and which way it looks when nothing
/// has told it otherwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gun {
    /// In the hull's own frame.
    pub at: [f32; 3],
    /// Outward, normalised.
    pub out: [f32; 3],
    /// The cell it was derived from, for hashing its cadence off.
    pub cell: u32,
}

/// Every gun on a hull, from the cells that are gunnery.
///
/// The export carries which surface each cell draws in, and `SURF_WEAPON` is
/// exactly the cells redux-tribes counts as a gun. So the guns are read off
/// the ship rather than authored beside it, and a hull with no weapons has
/// none rather than having some invented for it.
///
/// One gun per connected cluster of weapon cells, at the cluster's OUTERMOST
/// cell rather than its centre, because a barrel fires from its end and a
/// muzzle flash inside a barbette is a light under a box.
pub fn guns_of(m: &VoxelModel) -> Vec<Gun> {
    let n = m.len();
    let is_gun = |c: usize| m.grid[c] != mat::EMPTY && m.surf[c] == SURF_WEAPON;
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if !is_gun(start) || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut cells = Vec::new();
        while let Some(c) = stack.pop() {
            cells.push(c);
            let (i, j, k) = m.at(c);
            for (di, dj, dk) in crate::voxel::NEIGHBOURS {
                let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                if !m.inside(ni, nj, nk) {
                    continue;
                }
                let x = m.index(ni as usize, nj as usize, nk as usize);
                if is_gun(x) && !seen[x] {
                    seen[x] = true;
                    stack.push(x);
                }
            }
        }
        // A stray cell or two is a fitting, not a gun.
        if cells.len() < 6 {
            continue;
        }
        // The cluster's centre, then the cell furthest from the hull's own
        // axis through it: that is the muzzle, and the way out is the way
        // from the centre to it.
        let mut mid = [0.0f32; 3];
        for &c in &cells {
            let p = m.centre_of(c);
            for a in 0..3 {
                mid[a] += p[a] / cells.len() as f32;
            }
        }
        let mut best = (f32::MIN, cells[0], [0.0f32, 1.0, 0.0]);
        for &c in &cells {
            let p = m.centre_of(c);
            // Outward from the hull's centreline, which is the direction a
            // mount on a flank or a deck actually looks.
            let radial = normalise([p[0], p[1], p[2] * 0.35]);
            let reach = dot(sub(p, mid), radial);
            if reach > best.0 {
                best = (reach, c, radial);
            }
        }
        let p = m.centre_of(best.1);
        out.push(Gun { at: p, out: best.2, cell: best.1 as u32 });
    }
    // In cell order, so two runs give the same guns in the same order.
    out.sort_by_key(|g| g.cell);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_beam_is_a_capsule_and_not_a_line() {
        let b = Beam { from: [0.0, 0.0, 0.0], to: [10.0, 0.0, 0.0], radius: 1.0, born: 0 };
        assert!(b.kills([5.0, 0.0, 0.0]));
        assert!(b.kills([5.0, 0.9, 0.0]));
        assert!(!b.kills([5.0, 1.1, 0.0]));
        // Along the line but past both ends: outside.
        assert!(b.kills([10.0, 0.0, 0.0]));
        assert!(!b.kills([11.5, 0.0, 0.0]));
        assert!(!b.kills([-1.5, 0.0, 0.0]));
        // The cap is round, not flat.
        assert!(b.kills([10.5, 0.5, 0.5]));
        assert!(!b.kills([10.9, 0.9, 0.0]));
    }

    /// The capsule test is written a second time in WGSL, because the swarm
    /// lives on the GPU and cannot be asked a question mid frame. That copy
    /// cannot be run from here, so what IS checked is that this one is right:
    /// against a brute force answer that walks the segment and takes the
    /// nearest sample, over a grid that straddles every boundary. A closed
    /// form that disagreed with the obvious slow answer is the way this
    /// arithmetic goes wrong, and it is the way the transcription would too.
    #[test]
    fn the_capsule_test_agrees_with_walking_the_segment() {
        let brute = |b: &Beam, p: [f32; 3]| -> f32 {
            let mut best = f32::MAX;
            const N: usize = 4000;
            for i in 0..=N {
                let t = i as f32 / N as f32;
                let q = [
                    b.from[0] + (b.to[0] - b.from[0]) * t,
                    b.from[1] + (b.to[1] - b.from[1]) * t,
                    b.from[2] + (b.to[2] - b.from[2]) * t,
                ];
                best = best.min(length(sub(p, q)));
            }
            best
        };
        let beams = [
            Beam { from: [0.0, 0.0, 0.0], to: [10.0, 0.0, 0.0], radius: 1.0, born: 0 },
            Beam { from: [-3.0, 2.0, 1.0], to: [4.0, -5.0, 6.0], radius: 2.0, born: 0 },
            Beam { from: [1.0, 1.0, 1.0], to: [1.0, 1.0, 1.0], radius: 1.5, born: 0 },
        ];
        let mut checked = 0;
        let mut inside = 0;
        for b in &beams {
            for i in -6..=6 {
                for j in -6..=6 {
                    for k in -6..=6 {
                        let p = [i as f32 * 1.1, j as f32 * 1.1, k as f32 * 1.1];
                        let d = brute(b, p);
                        // Points within a whisker of the surface are where a
                        // sampled answer and a solved one may legitimately
                        // differ, so they are not asserted on.
                        if (d - b.radius).abs() < 0.01 {
                            continue;
                        }
                        checked += 1;
                        let want = d <= b.radius;
                        if want {
                            inside += 1;
                        }
                        assert_eq!(b.kills(p), want, "{p:?} is {d} from {b:?}");
                    }
                }
            }
        }
        assert!(checked > 6000 && inside > 100, "{checked} points, {inside} inside");
    }

    #[test]
    fn a_beam_is_cut_at_what_it_reached() {
        let b = Beam { from: [0.0, 0.0, 0.0], to: [10.0, 0.0, 0.0], radius: 0.5, born: 0 };
        // A sphere of radius 2 at x = 6: reached, six tenths along.
        let t = b.reaches([6.0, 0.0, 0.0], 2.0).expect("reached");
        assert!((t - 0.6).abs() < 1e-5, "{t}");
        // Off to one side by more than both radii: not reached.
        assert!(b.reaches([6.0, 3.0, 0.0], 2.0).is_none());
        assert!(b.reaches([6.0, 2.4, 0.0], 2.0).is_some(), "the two radii add");
        // Cut: the direction is kept, the length is not, and nothing else moves.
        let c = b.cut(t);
        assert_eq!(c.from, b.from);
        assert_eq!(c.radius, b.radius);
        assert_eq!(c.born, b.born);
        assert!((c.to[0] - 6.0).abs() < 1e-4, "{:?}", c.to);
        assert_eq!(c.direction(), b.direction());
        // A cut beam kills nothing beyond where it was cut, which is what
        // stops a carrier being cover for nothing.
        assert!(b.kills([9.0, 0.0, 0.0]));
        assert!(!c.kills([9.0, 0.0, 0.0]));
        assert_eq!(b.cut(-1.0).to, b.from, "clamped at both ends");
        assert_eq!(b.cut(4.0).to, b.to);
    }

    #[test]
    fn a_zero_length_beam_is_a_sphere_and_does_not_divide_by_nought() {
        let b = Beam { from: [1.0, 2.0, 3.0], to: [1.0, 2.0, 3.0], radius: 2.0, born: 0 };
        assert!(b.kills([1.0, 2.0, 4.5]));
        assert!(!b.kills([1.0, 2.0, 6.0]));
    }

    #[test]
    fn a_blast_opens_fast_and_stops() {
        let b = Blast { at: [0.0; 3], radius: 10.0, born: 100 };
        assert_eq!(b.radius_at(100), 0.0);
        // Half the ticks is 71% of the radius, which is what sqrt is for.
        let half = b.radius_at(100 + BLAST_TICKS / 2);
        assert!((half - 10.0 * 0.5f32.sqrt()).abs() < 0.2, "{half}");
        assert_eq!(b.radius_at(100 + BLAST_TICKS), 10.0);
        assert_eq!(b.radius_at(100 + BLAST_TICKS * 4), 10.0, "and stops");
        assert!(b.live(100 + BLAST_TICKS - 1));
        assert!(!b.live(100 + BLAST_TICKS));
        assert!(!b.live(99), "a blast that has not happened kills nothing");
        assert!(!b.kills([9.0, 0.0, 0.0], 100));
        assert!(b.kills([9.0, 0.0, 0.0], 100 + BLAST_TICKS));
    }

    #[test]
    fn ember_tiles_are_one_of_sixteen_and_inset() {
        let mut seen = std::collections::HashSet::new();
        for c in 0..4000u32 {
            let t = ember_tile(c);
            assert!(t[0] >= 0.0 && t[0] < 1.0 && t[1] >= 0.0 && t[1] < 1.0);
            seen.insert((( t[0] * 4.0).round() as i32, (t[1] * 4.0).round() as i32));
            for corner in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                let uv = ember_uv(t, corner);
                assert!(uv[0] > t[0] - 1e-6 && uv[0] < t[0] + 0.25);
                assert!(uv[1] > t[1] - 1e-6 && uv[1] < t[1] + 0.25);
            }
        }
        assert_eq!(seen.len(), 16, "every tile of the atlas is used");
        assert_eq!(ember_tile(77), ember_tile(77));
    }

    #[test]
    fn sparks_are_a_function_of_what_threw_them() {
        let mut a = Vec::new();
        let mut b = Vec::new();
        breach_sparks(1234, 7, [1.0, 2.0, 3.0], [0.0, 1.0, 0.0], 0.1, &mut a);
        breach_sparks(1234, 7, [1.0, 2.0, 3.0], [0.0, 1.0, 0.0], 0.1, &mut b);
        assert_eq!(a, b);
        assert_eq!(a.len(), 7);
        for s in &a {
            assert!(s.life > 0.0 && s.life < 2.0);
            assert!(s.size > 0.0);
            assert!(s.colour[0] >= s.colour[1] && s.colour[1] >= s.colour[2], "a burn is red hot before it is white");
            assert!(s.colour[0] > 1.0, "a spark has to clear the bloom threshold");
        }
        // Thrown OUTWARD: the mean velocity goes the way the face looked.
        let mean: f32 = a.iter().map(|s| s.vel[1]).sum::<f32>() / a.len() as f32;
        assert!(mean > 0.0, "{mean}");
        let mut c = Vec::new();
        breach_sparks(1235, 7, [1.0, 2.0, 3.0], [0.0, 1.0, 0.0], 0.1, &mut c);
        assert_ne!(a, c, "a different cell throws different sparks");

        let mut m = Vec::new();
        muzzle_sparks(3, [0.0; 3], [0.0, 0.0, 1.0], 0.2, &mut m);
        assert_eq!(m.len(), 4);
        assert!(m.iter().all(|s| s.life < 0.25), "a flash does not linger");

        let mut e = Vec::new();
        blast_sparks(9, [0.0; 3], 6.0, 120, &mut e);
        assert_eq!(e.len(), 120);
        assert!(e.iter().all(|s| s.life > 0.5));
        // A shell, not a jet: the mean direction is nearly nothing.
        let mut sum = [0.0f32; 3];
        for s in &e {
            let d = normalise(s.vel);
            for a in 0..3 {
                sum[a] += d[a] / e.len() as f32;
            }
        }
        assert!(length(sum) < 0.25, "lopsided fireball {sum:?}");
        let mut e2 = Vec::new();
        blast_sparks(9, [0.0; 3], 6.0, 120, &mut e2);
        assert_eq!(e, e2);
    }

    #[test]
    fn the_terran_frigate_has_guns_and_every_one_looks_out() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/terran_frigate.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let guns = guns_of(&m);
        assert!(!guns.is_empty(), "a frigate has weapons");
        assert!(guns.len() <= 12, "{} guns is a cluster pass that has come apart", guns.len());
        for g in &guns {
            assert!((length(g.out) - 1.0).abs() < 1e-4);
            assert_eq!(m.surf[g.cell as usize], SURF_WEAPON);
            // A muzzle looks AWAY from the hull's own axis, never into it.
            let radial = normalise([g.at[0], g.at[1], 0.0]);
            if length([g.at[0], g.at[1], 0.0]) > 0.05 {
                assert!(dot(g.out, radial) > 0.0, "gun at {:?} looks in at {:?}", g.at, g.out);
            }
        }
        assert_eq!(guns_of(&m), guns, "the same hull gives the same guns");
        eprintln!("terran_frigate: {} guns", guns.len());
    }
}
