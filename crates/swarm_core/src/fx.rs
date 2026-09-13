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

use crate::damage::DamageGrid;
use crate::rng::{drift_of, hash_cell, Rng};
use crate::voxel::{mat, purpose, VoxelModel, SURF_DRIVE, SURF_WEAPON};

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
/// How long a beam lasts, in ticks, which is a second at sixty.
///
/// Nine was a flash: it lit, it killed what was on its line at that instant,
/// and it was gone before anything it set off could be watched. A beam is a
/// SWEEP now and a sweep needs time to travel, so this is what decides how far
/// the arc it carves actually goes.
pub const BEAM_TICKS: u32 = 60;
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
    [
        (t % ATLAS) as f32 * ATLAS_STEP,
        (t / ATLAS) as f32 * ATLAS_STEP,
    ]
}

/// A corner of that tile, `c` in the mesher's own corner order.
pub fn ember_uv(tile: [f32; 2], c: [f32; 2]) -> [f32; 2] {
    let span = ATLAS_STEP - 2.0 * ATLAS_PAD;
    [
        tile[0] + ATLAS_PAD + c[0] * span,
        tile[1] + ATLAS_PAD + c[1] * span,
    ]
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
pub fn breach_sparks(
    cell: u32,
    tick: u32,
    at: [f32; 3],
    outward: [f32; 3],
    scale: f32,
    out: &mut Vec<Spark>,
) {
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
            pos: [
                at[0] + dir[0] * scale * 0.6,
                at[1] + dir[1] * scale * 0.6,
                at[2] + dir[2] * scale * 0.6,
            ],
            vel: [
                dir[0] * scale * 4.0 + d[0],
                dir[1] * scale * 4.0 + d[1],
                dir[2] * scale * 4.0 + d[2],
            ],
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
            let c = [
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            ];
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

/// Every connected cluster of cells drawing in one surface, as a placement.
///
/// `guns_of` and `drives_of` are the same walk with a different surface and a
/// different idea of which way the thing points, so they are this walk twice
/// rather than two copies of it. `aft` is what separates them: a gun looks
/// out from the hull's own axis, and a drive looks backwards along it,
/// because that is what a drive is.
/// How near the centreline the BOW GUN has to be, as a share of the hull's
/// own half beam.
///
/// Measured across the fleet rather than guessed: every gun within 0.05 of
/// the centreline is either a bow gun or a belly turret, and the next one out
/// is a Benefactor cruiser's own starboard sponson at 0.48, so the band
/// between them is empty and a quarter is the middle of it.
///
/// It is a test rather than part of the search because "farthest forward"
/// alone picks a SPONSON on the hulls whose foremost deck mount is out on a
/// flank, which is what half let through: the Benefactor's starboard battery
/// came out pointing down the bow with its port twin still looking outboard.
/// A pair on the foremost station is therefore refused outright rather than
/// resolved, and only a hull with a genuine centreline mount has a bow gun.
pub const BOW_BEAM: f32 = 0.25;

/// And how high above the mid plane, as a share of the half depth.
///
/// Loose on purpose, because it is here to say TOP and nothing more: a bow
/// gun under the keel should still look down and out rather than up the nose.
/// The ones that qualify sit from 0.34 to 0.50, and the first cut had this at
/// 0.35 and missed the Terran cruiser and destroyer by a hundredth. A
/// threshold set from the FRIGATE is a threshold that is wrong on the rung
/// nobody looked at.
pub const BOW_DECK: f32 = 0.15;

/// The half extents of a model's SOLID cells, which is what a share is of.
///
/// The lattice, not the ship: a hull drawn in a 32x32x64 box does not fill it,
/// so a share of the lattice would mean something different on every rung.
fn half_extents(m: &VoxelModel) -> [f32; 3] {
    let mut h = [0.0f32; 3];
    for c in 0..m.len() {
        if m.grid[c] == mat::EMPTY {
            continue;
        }
        let p = m.centre_of(c);
        for a in 0..3 {
            h[a] = h[a].max(p[a].abs());
        }
    }
    [h[0].max(1e-6), h[1].max(1e-6), h[2].max(1e-6)]
}

/// Which of a hull's guns is its BOW GUN: the one on top, farthest in front.
///
/// ONE per ship, never two, which is what makes this a rule about a ship
/// rather than about a region of one. It is the mount standing on the deck
/// over the centreline with NO OTHER GUN ON THE SHIP forward of it; a hull
/// with no such mount has no bow gun and every one of its guns keeps looking
/// outboard.
///
/// **"Farthest in front" is measured against the other GUNS, not against a
/// line drawn through the middle of the hull**, and the first cut got that
/// wrong. Amidships reads as the obvious test and it threw out every corvette
/// in the fleet: a corvette is a needle whose foremost deck mount sits a
/// couple of cells abaft its own midpoint with the whole of its nose ahead of
/// it carrying nothing at all. What that test was actually protecting against
/// is a hull whose only centreline deck mount is at the STERN with its real
/// battery out on the flanks forward of it, which is the Rogue and Benefactor
/// destroyer, and "nothing is forward of it" refuses those by saying so.
///
/// It takes the clusters rather than the model because the answer is "which of
/// these", and a second walk over the cells to ask it would be a second answer
/// to the question of what a gun even is.
///
/// This is the same rule redux-tribes' own `bowRing` keeps, and it has to be:
/// that project TURNS the cells and this one derives the facing they look
/// along, so a hull the two disagree about is a barrel drawn one way and a
/// beam leaving it another. `the_bow_gun_is_the_foremost_deck_mount_over_the_centreline`
/// is what holds them together over the shipped fleet.
pub fn bow_gun(m: &VoxelModel, guns: &[(Gun, [f32; 3], Vec<usize>)]) -> Option<usize> {
    let h = half_extents(m);
    let fore = guns
        .iter()
        .fold(f32::NEG_INFINITY, |z, (_, mid, _)| z.max(mid[2]));
    guns.iter()
        .enumerate()
        .filter(|(_, (_, mid, _))| {
            mid[2] >= fore && (mid[0] / h[0]).abs() < BOW_BEAM && mid[1] / h[1] > BOW_DECK
        })
        // A hull whose foremost station carries a PAIR has neither of them on
        // the centreline, so the nearer to it wins and the beam test has
        // already refused both if they are really a broadside.
        .min_by(|a, b| (a.1 .1[0]).abs().total_cmp(&(b.1 .1[0]).abs()))
        .map(|(i, _)| i)
}

fn clusters_of(
    m: &VoxelModel,
    surf: u8,
    purp: Option<u8>,
    least: usize,
    aft: bool,
) -> Vec<(Gun, [f32; 3], Vec<usize>)> {
    let n = m.len();
    let is = |c: usize| {
        m.grid[c] != mat::EMPTY && m.surf[c] == surf && purp.is_none_or(|p| m.purp[c] == p)
    };
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if !is(start) || seen[start] {
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
                if is(x) && !seen[x] {
                    seen[x] = true;
                    stack.push(x);
                }
            }
        }
        // A stray cell or two is a fitting, not a placement.
        if cells.len() < least {
            continue;
        }
        let mut mid = [0.0f32; 3];
        for &c in &cells {
            let p = m.centre_of(c);
            for a in 0..3 {
                mid[a] += p[a] / cells.len() as f32;
            }
        }
        // Which way this cluster points. A gun looks out from the hull's own
        // axis; a drive looks along it, AWAY from the middle of the ship,
        // which is what makes a retro point forward and a main engine aft
        // without either being a special case.
        let along: [f32; 3] = if aft {
            [0.0, 0.0, if mid[2] < 0.0 { -1.0 } else { 1.0 }]
        } else {
            [0.0, 0.0, 0.0]
        };
        let mut best = (f32::MIN, cells[0], along);
        for &c in &cells {
            let p = m.centre_of(c);
            let dir = if aft {
                along
            } else {
                normalise([p[0], p[1], p[2] * 0.35])
            };
            let reach = dot(sub(p, mid), dir);
            if reach > best.0 {
                best = (reach, c, dir);
            }
        }
        // The muzzle is the cluster's own MIDDLE carried out to its outer
        // face, not the cell that happened to reach furthest.
        //
        // On a drive bell three cells square, a dozen cells tie at the same
        // reach along the axis and the first one found wins, which is a
        // CORNER. Every flame was therefore drawn off the corner of its own
        // engine rather than out of the middle of it, and on a block of six
        // bells that reads as the whole set being misaligned. Projecting the
        // midpoint along the same axis puts it where the bell actually is.
        let tip = m.centre_of(best.1);
        let reach = dot(sub(tip, mid), best.2);
        let at = [
            mid[0] + best.2[0] * reach,
            mid[1] + best.2[1] * reach,
            mid[2] + best.2[2] * reach,
        ];
        out.push((
            Gun {
                at,
                out: best.2,
                cell: best.1 as u32,
            },
            mid,
            cells,
        ));
    }
    // In cell order, so two runs give the same placements in the same order.
    out.sort_by_key(|(g, _, _)| g.cell);
    out
}

/// Every ENGINE on a hull: the main drive, and not the attitude thrusters.
///
/// Both wear `SURF_DRIVE`, because a finish cannot tell a bell from a bell,
/// and clustering on the surface alone put an "engine" half way up the bow of
/// a Terran frigate. What separates them is what they are FOR, which the
/// export carries beside the surface. A plume comes out of an engine; a
/// thruster plumed constantly would be a ship that never stops spinning.
///
/// It points AFT, which is the other difference from a gun: a plume leaves the
/// back of an engine and a muzzle flash the front of a barrel, and each is the
/// outermost cell of its cluster along its own line.
pub fn engines_of(m: &VoxelModel) -> Vec<Gun> {
    engine_clusters(m).into_iter().map(|(g, _, _)| g).collect()
}

/// The same drives, with the cells each one is made of and where it pivots.
pub fn engine_clusters(m: &VoxelModel) -> Vec<(Gun, [f32; 3], Vec<usize>)> {
    // Two cells, not six: the purpose is already the filter that matters, and
    // a mote's engines are single cells that only touch where their columns
    // happen to end level with each other.
    clusters_of(m, SURF_DRIVE, Some(purpose::PROPULSION), 2, true)
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
    gun_clusters(m).into_iter().map(|(g, _, _)| g).collect()
}

/// The guns, with the CELLS each one is made of and the point it turns about.
///
/// A turret that swivels has to be drawn separately from the hull it is bolted
/// to, which means knowing which cells are the turret and where its pivot is.
/// The cluster walk already found both and threw them away.
pub fn gun_clusters(m: &VoxelModel) -> Vec<(Gun, [f32; 3], Vec<usize>)> {
    let mut guns = clusters_of(m, SURF_WEAPON, None, 6, false);
    // And the bow gun looks down the BOW. Radially it looked half up and half
    // out, which puts a ship's foremost mount at the sky: a gun bolted to the
    // foredeck fires over the nose, which is what every hull with one is for.
    if let Some(n) = bow_gun(m, &guns) {
        let (gun, mid, cells) = &mut guns[n];
        gun.out = [0.0, 0.0, 1.0];
        // The muzzle moves with it: it is the cluster's own middle carried out
        // to its FORWARD face now rather than to whichever face the radial
        // direction happened to reach, or the barrel would be drawn coming out
        // of the side of its own turret.
        let mut reach = 0.0f32;
        for &c in cells.iter() {
            let p = m.centre_of(c);
            if p[2] - mid[2] > reach {
                reach = p[2] - mid[2];
                gun.cell = c as u32;
            }
        }
        gun.at = [mid[0], mid[1], mid[2] + reach];
    }
    guns
}

/// Which cells of a gun cluster TURN, and which are the base it turns on.
///
/// A turret is a mount standing on a barbette, and only the mount comes round:
/// the ring it is seated in is part of the ship. Lifting the whole cluster out
/// and turning all of it swings the base too, so a gun tracking a target
/// screws its own seating round with it, which is the one part of a turret a
/// player knows does not move.
///
/// **The base is where it MEETS THE SHIP**, which is a fact about the geometry
/// rather than a direction anybody has to choose: a cluster cell with a face
/// against solid hull that is not itself part of the cluster is seated on the
/// ship, and everything else is standing on those. That is right whatever pose
/// the mount was authored in, which matters because they are not all alike: a
/// sponson is bolted outboard and a bow gun lies down the nose, so any rule
/// written along one axis is a rule that is wrong on the other.
///
/// A cluster that is ALL contact layer has nothing to split, and the whole of
/// it turns: a gun one cell deep is a barrel with no barbette under it, and
/// leaving it behind would be a mount that never moves at all.
pub fn turret_split(m: &VoxelModel, cells: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mine: std::collections::BTreeSet<usize> = cells.iter().copied().collect();
    let mut base = Vec::new();
    let mut mount = Vec::new();
    for &c in cells {
        let (x, y, z) = m.at(c);
        let seated = [
            (1i32, 0i32, 0i32),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ]
        .iter()
        .any(|(dx, dy, dz)| {
            let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
            if nx < 0 || ny < 0 || nz < 0 {
                return false;
            }
            let (nx, ny, nz) = (nx as usize, ny as usize, nz as usize);
            if nx >= m.nx || ny >= m.ny || nz >= m.nz {
                return false;
            }
            let n = m.index(nx, ny, nz);
            m.grid[n] != mat::EMPTY && !mine.contains(&n)
        });
        if seated {
            base.push(c);
        } else {
            mount.push(c);
        }
    }
    if mount.is_empty() {
        (Vec::new(), base)
    } else {
        (base, mount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turret_turns_on_a_base_that_stays_with_the_ship() {
        // A slab of hull with a two cell tall mount standing on it: the lower
        // layer touches the deck and the upper one touches only the mount.
        let mut m = VoxelModel::new(8, 8, 8, 0.1);
        for x in 0..8 {
            for z in 0..8 {
                let n = m.index(x, 2, z);
                m.grid[n] = mat::PLATE;
            }
        }
        let seat = m.index(4, 3, 4);
        let top = m.index(4, 4, 4);
        m.grid[seat] = mat::PLATE;
        m.grid[top] = mat::PLATE;
        let (base, mount) = turret_split(&m, &[seat, top]);
        assert_eq!(base, vec![seat], "the cell on the deck is the barbette");
        assert_eq!(mount, vec![top], "what stands on it is what turns");

        // And a mount one cell deep is all barrel: it has no base to leave
        // behind, so the whole of it comes round.
        let (base, mount) = turret_split(&m, &[seat]);
        assert!(base.is_empty());
        assert_eq!(mount, vec![seat]);
    }

    #[test]
    fn a_beam_is_a_capsule_and_not_a_line() {
        let b = Beam {
            from: [0.0, 0.0, 0.0],
            to: [10.0, 0.0, 0.0],
            radius: 1.0,
            born: 0,
        };
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
            Beam {
                from: [0.0, 0.0, 0.0],
                to: [10.0, 0.0, 0.0],
                radius: 1.0,
                born: 0,
            },
            Beam {
                from: [-3.0, 2.0, 1.0],
                to: [4.0, -5.0, 6.0],
                radius: 2.0,
                born: 0,
            },
            Beam {
                from: [1.0, 1.0, 1.0],
                to: [1.0, 1.0, 1.0],
                radius: 1.5,
                born: 0,
            },
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
        assert!(
            checked > 6000 && inside > 100,
            "{checked} points, {inside} inside"
        );
    }

    #[test]
    fn a_beam_is_cut_at_what_it_reached() {
        let b = Beam {
            from: [0.0, 0.0, 0.0],
            to: [10.0, 0.0, 0.0],
            radius: 0.5,
            born: 0,
        };
        // A sphere of radius 2 at x = 6: reached, six tenths along.
        let t = b.reaches([6.0, 0.0, 0.0], 2.0).expect("reached");
        assert!((t - 0.6).abs() < 1e-5, "{t}");
        // Off to one side by more than both radii: not reached.
        assert!(b.reaches([6.0, 3.0, 0.0], 2.0).is_none());
        assert!(
            b.reaches([6.0, 2.4, 0.0], 2.0).is_some(),
            "the two radii add"
        );
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
        let b = Beam {
            from: [1.0, 2.0, 3.0],
            to: [1.0, 2.0, 3.0],
            radius: 2.0,
            born: 0,
        };
        assert!(b.kills([1.0, 2.0, 4.5]));
        assert!(!b.kills([1.0, 2.0, 6.0]));
    }

    #[test]
    fn a_blast_opens_fast_and_stops() {
        let b = Blast {
            at: [0.0; 3],
            radius: 10.0,
            born: 100,
        };
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
            seen.insert(((t[0] * 4.0).round() as i32, (t[1] * 4.0).round() as i32));
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
            assert!(
                s.colour[0] >= s.colour[1] && s.colour[1] >= s.colour[2],
                "a burn is red hot before it is white"
            );
            assert!(
                s.colour[0] > 1.0,
                "a spark has to clear the bloom threshold"
            );
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
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/hulls/terran_frigate.ftvx"
        ))
        .unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let guns = guns_of(&m);
        assert!(!guns.is_empty(), "a frigate has weapons");
        assert!(
            guns.len() <= 12,
            "{} guns is a cluster pass that has come apart",
            guns.len()
        );
        // A muzzle looks AWAY from the hull's own axis, never into it, with
        // the one exception every hull is allowed exactly one of: its BOW GUN,
        // which looks down the bow and whose radial component is nought by
        // construction rather than negative.
        let clusters = gun_clusters(&m);
        let bow = bow_gun(&m, &clusters);
        let mut found = 0;
        for (n, (g, _, _)) in clusters.iter().enumerate() {
            assert!((length(g.out) - 1.0).abs() < 1e-4);
            assert_eq!(m.surf[g.cell as usize], SURF_WEAPON);
            if bow == Some(n) {
                found += 1;
                assert_eq!(g.out, [0.0, 0.0, 1.0], "the bow gun looks forward");
                continue;
            }
            let radial = normalise([g.at[0], g.at[1], 0.0]);
            if length([g.at[0], g.at[1], 0.0]) > 0.05 {
                assert!(
                    dot(g.out, radial) > 0.0,
                    "gun at {:?} looks in at {:?}",
                    g.at,
                    g.out
                );
            }
        }
        assert_eq!(found, 1, "a hull has one bow gun, never two");
        assert_eq!(guns_of(&m), guns, "the same hull gives the same guns");

        // And its ENGINES, which are the same walk over a different surface
        // and point AFT rather than out.
        let engines = engines_of(&m);
        assert!(!engines.is_empty(), "a frigate has engines");
        for e in &engines {
            assert_eq!(m.surf[e.cell as usize], SURF_DRIVE);
            assert_eq!(m.purp[e.cell as usize], purpose::PROPULSION);
            // Its plume leaves along the hull's axis, AWAY from the middle.
            // Aft for a main engine and forward for a RETRO, which is also
            // propulsion and also an engine and sits in the bow: asserting
            // every engine was aft failed on exactly those, and the assertion
            // was what was wrong.
            assert_eq!(e.out[0], 0.0);
            assert_eq!(e.out[1], 0.0);
            assert_eq!(e.out[2].abs(), 1.0);
            assert_eq!(
                e.out[2] < 0.0,
                e.at[2] < 0.0,
                "engine at z {} plumes {}",
                e.at[2],
                e.out[2]
            );
        }
        assert!(
            engines.iter().any(|e| e.at[2] < 0.0),
            "a ship has a main drive at the stern"
        );
        let thrusters = (0..m.len())
            .filter(|&n| m.purp[n] == purpose::ATTITUDE)
            .count();
        assert!(thrusters > 0, "and it has thrusters, which are not engines");
        eprintln!(
            "terran_frigate: {} guns, {} engines, {} thruster cells",
            guns.len(),
            engines.len(),
            thrusters
        );
    }
}

#[cfg(test)]
mod muzzle_tests {
    use super::*;
    use crate::voxel::{mat, VoxelModel, SURF_DRIVE};

    /// A drive's flame comes out of the MIDDLE of its bell.
    ///
    /// Built as one square block of drive cells, which is the shape that
    /// exposed this: every cell on its outer face ties for "furthest along the
    /// axis", so picking the winner picks a corner, and the flame was drawn
    /// half a bell up and half a bell across from where the engine is.
    /// Put two guns on one hull, one on the deck over the centreline forward
    /// and one out on a flank, and hold each to the facing its PLACE earns it.
    ///
    /// Built rather than loaded, because the rule is about where a gun stands
    /// and not about any one class: a hull file would make this a test of the
    /// Terran frigate, which is the thing it must not be.
    #[test]
    fn the_gun_on_top_farthest_in_front_looks_down_the_bow() {
        let mut m = VoxelModel::new(16, 16, 32, 0.25);
        // A body, so the half extents are the ship's rather than one turret's.
        for k in 4..28 {
            for j in 5..11 {
                for i in 5..11 {
                    let n = m.index(i, j, k);
                    m.grid[n] = mat::PLATE;
                    m.surf[n] = crate::voxel::SURF_ARMOUR;
                }
            }
        }
        let mut gun = |i0: usize, j0: usize, k0: usize| {
            for k in k0..k0 + 2 {
                for j in j0..j0 + 2 {
                    for i in i0..i0 + 2 {
                        let n = m.index(i, j, k);
                        m.grid[n] = mat::MACHINE;
                        m.surf[n] = SURF_WEAPON;
                    }
                }
            }
        };
        // On the deck, on the centreline, well forward.
        gun(7, 11, 23);
        // And a sponson out on the port flank, amidships.
        gun(3, 7, 14);
        let guns = guns_of(&m);
        assert_eq!(guns.len(), 2, "two blocks are two guns");
        let bow = guns
            .iter()
            .max_by(|a, b| a.at[2].total_cmp(&b.at[2]))
            .expect("a forward gun");
        assert_eq!(
            bow.out,
            [0.0, 0.0, 1.0],
            "the gun on top farthest in front looks down the bow"
        );
        assert_eq!(
            guns.iter().filter(|g| g.out == [0.0, 0.0, 1.0]).count(),
            1,
            "and it is the only one: a hull has one bow gun, never two"
        );
        let flank = guns
            .iter()
            .min_by(|a, b| a.at[0].total_cmp(&b.at[0]))
            .expect("a flank gun");
        assert!(
            flank.out[0] < -0.5,
            "a sponson still looks outboard, not forward: {:?}",
            flank.out
        );
        assert!(
            flank.out[2].abs() < 0.5,
            "and a sponson amidships has no business pointing down the bow: {:?}",
            flank.out
        );
    }

    /// The two cases "forward of amidships" got wrong, which is why the rule
    /// measures against the other GUNS instead.
    ///
    /// A CORVETTE is a needle whose foremost mount sits abaft its own midpoint
    /// with nothing ahead of it, and an amidships test threw every one of them
    /// out. A hull whose only centreline deck mount is at the STERN with its
    /// battery out on the flanks forward of it is what that test was actually
    /// protecting against, and it is refused by saying exactly that.
    #[test]
    fn a_bow_gun_is_the_foremost_gun_rather_than_a_forward_one() {
        let body = |m: &mut VoxelModel, k0: usize, k1: usize| {
            for k in k0..k1 {
                for j in 5..11 {
                    for i in 5..11 {
                        let n = m.index(i, j, k);
                        m.grid[n] = mat::PLATE;
                        m.surf[n] = crate::voxel::SURF_ARMOUR;
                    }
                }
            }
        };
        let gun = |m: &mut VoxelModel, i0: usize, j0: usize, k0: usize| {
            for k in k0..k0 + 2 {
                for j in j0..j0 + 2 {
                    for i in i0..i0 + 2 {
                        let n = m.index(i, j, k);
                        m.grid[n] = mat::MACHINE;
                        m.surf[n] = SURF_WEAPON;
                    }
                }
            }
        };

        // A needle with its one mount abaft the middle and a long bare nose.
        let mut corvette = VoxelModel::new(16, 16, 32, 0.25);
        body(&mut corvette, 4, 28);
        gun(&mut corvette, 7, 11, 12);
        let guns = guns_of(&corvette);
        assert_eq!(guns.len(), 1);
        assert_eq!(
            guns[0].out,
            [0.0, 0.0, 1.0],
            "nothing is forward of it, so it is the bow gun wherever it sits"
        );

        // And a stern mount with a flank battery well forward of it.
        let mut destroyer = VoxelModel::new(16, 16, 32, 0.25);
        body(&mut destroyer, 4, 28);
        gun(&mut destroyer, 7, 11, 6);
        gun(&mut destroyer, 3, 7, 22);
        gun(&mut destroyer, 11, 7, 22);
        let guns = guns_of(&destroyer);
        assert_eq!(guns.len(), 3, "three blocks are three guns");
        assert!(
            guns.iter().all(|g| g.out != [0.0, 0.0, 1.0]),
            "a stern mount with a battery ahead of it is not a bow gun: {:?}",
            guns.iter().map(|g| g.out).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_drive_plumes_from_the_centre_of_its_bell() {
        let mut m = VoxelModel::new(16, 16, 16, 0.25);
        // A 4x4x2 block of drive, centred on the lattice in x and y, aft in z.
        for k in 2..4 {
            for j in 6..10 {
                for i in 6..10 {
                    let n = m.index(i, j, k);
                    m.grid[n] = mat::MACHINE;
                    m.surf[n] = SURF_DRIVE;
                    m.purp[n] = crate::voxel::purpose::PROPULSION;
                }
            }
        }
        let drives = engines_of(&m);
        assert_eq!(drives.len(), 1, "one block is one drive");
        let g = &drives[0];
        // The lattice centre in x and y, which is where the block is centred.
        let mid = m.centre_of(m.index(8, 8, 3));
        assert!(
            (g.at[0] - (mid[0] - m.cell * 0.5)).abs() < m.cell * 0.51,
            "muzzle x {} is off the bell centre {}",
            g.at[0],
            mid[0] - m.cell * 0.5
        );
        assert!(
            (g.at[1] - (mid[1] - m.cell * 0.5)).abs() < m.cell * 0.51,
            "muzzle y {} is off the bell centre {}",
            g.at[1],
            mid[1] - m.cell * 0.5
        );
        // And it points aft, because the block is aft of the middle.
        assert!(
            g.out[2] < -0.5,
            "a drive aft of the middle plumes forward: {:?}",
            g.out
        );
    }
}

// ------------------------------------------------------------- reactor --

/// How big the reactor is, as a radius in cells about its own seed.
///
/// A ball rather than a share of the hull, because a reactor is a MACHINE of
/// a size and not a proportion of whatever it was bolted into: the same
/// vessel on a corvette and on a heavy cruiser is the point of a ladder built
/// out of one cell size. It is scaled by the hull's own lattice all the same,
/// so a model on a bigger grid gets a bigger ball of cells for the same
/// physical thing.
const REACTOR_R: f32 = 3.6;

/// The cells of the hull's reactor: a compact core, as deep inside the ship
/// as the ship goes.
///
/// Redux-tribes' eight purposes have no reactor in them (`PURPOSE_ORDER` is
/// propulsion, attitude, gun, ordnance, command, crew, boarding, structure),
/// so there is nothing in the export to read and inventing a ninth would mean
/// changing that project, which is reference only. This DERIVES it instead,
/// and derives it from the one thing the owner said about it: it is buried.
///
/// Depth is a multi source breadth first search inward from every empty cell,
/// so a cell's depth is how many cells of solid material stand between it and
/// the nearest gap, interior voids included. The deepest cell is therefore
/// the most buried place in the ship by construction, and the reactor is the
/// ball of solid cells around it. Nothing can reach it without chewing
/// through everything over it, which is what the rule is for.
///
/// Ties are broken toward the cell nearest the lattice's own middle, so a
/// hull with a long flat run of equally buried cells gets its reactor
/// amidships rather than at whichever end the scan happened to reach first.
pub fn reactor_of(m: &VoxelModel) -> Vec<usize> {
    let n = m.len();
    if n == 0 {
        return Vec::new();
    }
    let solid: Vec<bool> = m.grid.iter().map(|&x| x != mat::EMPTY).collect();
    // Multi source BFS: every empty cell is depth nought, and the wave walks
    // inward through solid material one face step at a time.
    let mut depth = vec![u32::MAX; n];
    let mut queue: Vec<usize> = Vec::new();
    for (i, &s) in solid.iter().enumerate() {
        if !s {
            depth[i] = 0;
            queue.push(i);
        }
    }
    // A model with no empty cell at all is a solid block: every cell is
    // equally buried, so seed the wave from the lattice wall instead of
    // returning nothing.
    if queue.is_empty() {
        for k in 0..m.nz {
            for j in 0..m.ny {
                for i in 0..m.nx {
                    if i == 0 || j == 0 || k == 0 || i == m.nx - 1 || j == m.ny - 1 || k == m.nz - 1
                    {
                        let q = m.index(i, j, k);
                        depth[q] = 0;
                        queue.push(q);
                    }
                }
            }
        }
    }
    let mut head = 0;
    while head < queue.len() {
        let c = queue[head];
        head += 1;
        let i = c % m.nx;
        let j = (c / m.nx) % m.ny;
        let k = c / (m.nx * m.ny);
        let d = depth[c] + 1;
        const STEPS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        for (dx, dy, dz) in STEPS {
            let (a, b, e) = (i as i32 + dx, j as i32 + dy, k as i32 + dz);
            if a < 0 || b < 0 || e < 0 || a >= m.nx as i32 || b >= m.ny as i32 || e >= m.nz as i32 {
                continue;
            }
            let q = m.index(a as usize, b as usize, e as usize);
            if solid[q] && depth[q] == u32::MAX {
                depth[q] = d;
                queue.push(q);
            }
        }
    }

    // The seed: deepest wins, and the nearest to the middle breaks a tie.
    let mid = [m.nx as f32 * 0.5, m.ny as f32 * 0.5, m.nz as f32 * 0.5];
    let from_mid = |c: usize| -> f32 {
        let (i, j, k) = (c % m.nx, (c / m.nx) % m.ny, c / (m.nx * m.ny));
        let d = [i as f32 - mid[0], j as f32 - mid[1], k as f32 - mid[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    let mut seed = usize::MAX;
    let (mut best_d, mut best_m) = (0u32, f32::MAX);
    for c in 0..n {
        if !solid[c] || depth[c] == u32::MAX {
            continue;
        }
        let t = from_mid(c);
        if depth[c] > best_d || (depth[c] == best_d && t < best_m) {
            best_d = depth[c];
            best_m = t;
            seed = c;
        }
    }
    if seed == usize::MAX {
        return Vec::new();
    }

    // The ball about it, scaled by the lattice so the reactor is the same
    // physical machine on any grid this is asked about.
    let scale = (m.nx.max(m.ny).max(m.nz) as f32 / 64.0).max(0.5);
    let r = REACTOR_R * scale;
    let (si, sj, sk) = (seed % m.nx, (seed / m.nx) % m.ny, seed / (m.nx * m.ny));
    let reach = r.ceil() as i32;
    let mut out = Vec::new();
    for dk in -reach..=reach {
        for dj in -reach..=reach {
            for di in -reach..=reach {
                let (a, b, e) = (si as i32 + di, sj as i32 + dj, sk as i32 + dk);
                if a < 0
                    || b < 0
                    || e < 0
                    || a >= m.nx as i32
                    || b >= m.ny as i32
                    || e >= m.nz as i32
                {
                    continue;
                }
                if (di * di + dj * dj + dk * dk) as f32 > r * r {
                    continue;
                }
                let q = m.index(a as usize, b as usize, e as usize);
                // Solid cells only: a reactor is made of the ship, so the
                // voids inside the ball are not part of it and counting them
                // would mean a reactor that is already half destroyed.
                if solid[q] {
                    out.push(q);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod reactor_tests {
    use super::*;
    use crate::voxel::{mat, VoxelModel};

    fn block(nx: usize, ny: usize, nz: usize) -> VoxelModel {
        let mut m = VoxelModel::new(nx, ny, nz, 0.1);
        for k in 1..nz - 1 {
            for j in 1..ny - 1 {
                for i in 1..nx - 1 {
                    let n = m.index(i, j, k);
                    m.grid[n] = mat::PLATE;
                }
            }
        }
        m
    }

    /// The whole premise: nothing in the reactor may be touching space. If a
    /// reactor cell has a face open to the outside then a single bite reaches
    /// it, and "buried deep inside the ship" is a claim rather than a rule.
    #[test]
    fn the_reactor_is_buried() {
        let m = block(20, 20, 28);
        let core = reactor_of(&m);
        assert!(!core.is_empty(), "no reactor at all");
        for &c in &core {
            let (i, j, k) = (c % m.nx, (c / m.nx) % m.ny, c / (m.nx * m.ny));
            const STEPS: [(i32, i32, i32); 6] = [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ];
            for (dx, dy, dz) in STEPS {
                let (a, b, e) = (i as i32 + dx, j as i32 + dy, k as i32 + dz);
                assert!(
                    a >= 0
                        && b >= 0
                        && e >= 0
                        && a < m.nx as i32
                        && b < m.ny as i32
                        && e < m.nz as i32,
                    "a reactor cell sits on the lattice wall"
                );
                let q = m.index(a as usize, b as usize, e as usize);
                assert_ne!(
                    m.grid[q],
                    mat::EMPTY,
                    "a reactor cell has a face open to space"
                );
            }
        }
    }

    /// It is a core, not a region: every cell of it is solid, and it is a
    /// small enough part of the ship that reaching it is an achievement.
    #[test]
    fn the_reactor_is_a_small_solid_core() {
        let m = block(20, 20, 28);
        let core = reactor_of(&m);
        let cells = m.solid_count();
        for &c in &core {
            assert_ne!(m.grid[c], mat::EMPTY, "a reactor cell is not solid");
        }
        let share = core.len() as f32 / cells as f32;
        assert!(
            share > 0.002,
            "the reactor is {share} of the hull, which is nothing"
        );
        assert!(
            share < 0.25,
            "the reactor is {share} of the hull, which is most of it"
        );
    }

    /// And it sits in the middle, which is what "deep inside" means on a
    /// block: a reactor that came out at one end would pass every test above
    /// and still be a reactor anybody can shoot from the front.
    #[test]
    fn the_reactor_is_amidships() {
        let m = block(20, 20, 28);
        let core = reactor_of(&m);
        let mut mean = [0.0f32; 3];
        for &c in &core {
            mean[0] += (c % m.nx) as f32;
            mean[1] += ((c / m.nx) % m.ny) as f32;
            mean[2] += (c / (m.nx * m.ny)) as f32;
        }
        for v in mean.iter_mut() {
            *v /= core.len() as f32;
        }
        let want = [m.nx as f32 * 0.5, m.ny as f32 * 0.5, m.nz as f32 * 0.5];
        for a in 0..3 {
            assert!(
                (mean[a] - want[a]).abs() < 2.0,
                "axis {a}: reactor at {} of {}",
                mean[a],
                want[a]
            );
        }
    }

    /// A model with nothing in it answers with nothing rather than panicking
    /// on an index that was never found.
    #[test]
    fn an_empty_model_has_no_reactor() {
        let m = VoxelModel::new(8, 8, 8, 0.1);
        assert!(reactor_of(&m).is_empty());
    }
}

// ------------------------------------------------------------- wrecks --

/// What a hull that has gone critical breaks INTO: the big pieces, each one
/// connected run of live cells, and the dust that is too small to be a piece.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shatter {
    /// Biggest first. Each is a list of cell indices in the model's lattice.
    pub pieces: Vec<Vec<usize>>,
    /// Everything under `min_piece` cells, to be thrown as single chunks.
    pub dust: Vec<usize>,
}

/// The six face neighbours: two cells meeting at an edge are not one piece.
const SIX: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Break what is left of a hull into wreck pieces.
///
/// A ship that dies leaves a WRECK, and a wreck is a few big pieces rather
/// than a spray of cells. The reactor has already taken the ball around it
/// (`DamageGrid::blast_cells`); this cuts the survivors along two planes
/// through the blast, hashed off the seed so two screens watching one ship
/// die see the same pieces, and then takes each sector's connected runs of
/// live cells. The first plane lies near ACROSS the long axis, so a hull
/// breaks into a bow and a stern; the second runs along it at a hashed roll,
/// so each of those breaks port from starboard, or deck from keel. Two
/// planes rather than three, because eight pieces of a frigate are not
/// giant, and giant is the point. Anything under `min_piece` cells is dust.
pub fn shatter(
    m: &VoxelModel,
    d: &DamageGrid,
    centre: [f32; 3],
    seed: u32,
    min_piece: usize,
) -> Shatter {
    let n = m.len();
    let live = |c: usize| m.grid[c] != mat::EMPTY && !d.is_dead(c);
    let mut rng = Rng::new(seed as u64 ^ 0x5EED_C0DE);
    let n1 = normalise([rng.range(-0.35, 0.35), rng.range(-0.35, 0.35), 1.0]);
    let roll = rng.range(0.0, std::f32::consts::TAU);
    let n2 = normalise([roll.cos(), roll.sin(), rng.range(-0.25, 0.25)]);
    let sector = |c: usize| -> u8 {
        let p = sub(m.centre_of(c), centre);
        ((dot(p, n1) >= 0.0) as u8) | (((dot(p, n2) >= 0.0) as u8) << 1)
    };
    let mut seen = vec![false; n];
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    let mut dust = Vec::new();
    for start in 0..n {
        if seen[start] || !live(start) {
            continue;
        }
        let s = sector(start);
        let mut stack = vec![start];
        let mut piece = Vec::new();
        seen[start] = true;
        while let Some(c) = stack.pop() {
            piece.push(c);
            let (i, j, k) = m.at(c);
            for (di, dj, dk) in SIX {
                let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                if !m.inside(ni, nj, nk) {
                    continue;
                }
                let nb = m.index(ni as usize, nj as usize, nk as usize);
                if seen[nb] || !live(nb) || sector(nb) != s {
                    continue;
                }
                seen[nb] = true;
                stack.push(nb);
            }
        }
        if piece.len() >= min_piece {
            piece.sort_unstable();
            pieces.push(piece);
        } else {
            dust.extend(piece);
        }
    }
    pieces.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    dust.sort_unstable();
    Shatter { pieces, dust }
}

#[cfg(test)]
mod shatter_tests {
    use super::*;

    fn block(nx: usize, ny: usize, nz: usize) -> VoxelModel {
        let mut m = VoxelModel::new(nx, ny, nz, 0.1);
        for k in 1..nz - 1 {
            for j in 1..ny - 1 {
                for i in 1..nx - 1 {
                    let n = m.index(i, j, k);
                    m.grid[n] = mat::PLATE;
                }
            }
        }
        m
    }

    /// How many connected runs a set of cells is, on six neighbours.
    fn runs(m: &VoxelModel, cells: &[usize]) -> usize {
        let mut inside = vec![false; m.len()];
        for &c in cells {
            inside[c] = true;
        }
        let mut seen = vec![false; m.len()];
        let mut count = 0;
        for &start in cells {
            if seen[start] {
                continue;
            }
            count += 1;
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(c) = stack.pop() {
                let (i, j, k) = m.at(c);
                for (di, dj, dk) in SIX {
                    let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
                    if !m.inside(ni, nj, nk) {
                        continue;
                    }
                    let nb = m.index(ni as usize, nj as usize, nk as usize);
                    if inside[nb] && !seen[nb] {
                        seen[nb] = true;
                        stack.push(nb);
                    }
                }
            }
        }
        count
    }

    /// The whole point: a hull comes apart into a FEW BIG pieces, every one
    /// of them in one piece, and between them and the dust they account for
    /// every live cell exactly once.
    #[test]
    fn a_wreck_breaks_into_big_pieces_and_dust() {
        let m = block(12, 12, 28);
        let mut d = DamageGrid::new(&m);
        // The reactor takes the ball around it first.
        let hole = Blast {
            at: [0.0; 3],
            radius: m.cell * 4.5,
            born: 7,
        };
        let taken = d.blast_cells(&m, &hole, 7).len();
        assert!(taken > 200, "the hole took {taken} cells");
        let live: Vec<bool> = (0..m.len())
            .map(|c| m.grid[c] != mat::EMPTY && !d.is_dead(c))
            .collect();
        let alive = live.iter().filter(|&&l| l).count();

        let sh = shatter(&m, &d, [0.0; 3], 3, 30);
        assert!(
            (3..=8).contains(&sh.pieces.len()),
            "{} pieces",
            sh.pieces.len()
        );
        for p in &sh.pieces {
            assert!(p.len() >= 30, "a piece of {} cells is dust", p.len());
            assert_eq!(runs(&m, p), 1, "a piece must be one piece");
        }
        assert!(
            sh.pieces[0].len() * 8 >= alive,
            "the biggest piece is {} of {alive} live cells",
            sh.pieces[0].len()
        );
        // Sorted biggest first, and the biggest is not the whole hull.
        assert!(sh.pieces.windows(2).all(|w| w[0].len() >= w[1].len()));
        assert!(
            sh.pieces[0].len() * 2 < alive,
            "one piece of {} took the whole {alive}",
            sh.pieces[0].len()
        );

        let mut count = vec![0u8; m.len()];
        for p in &sh.pieces {
            for &c in p {
                count[c] += 1;
            }
        }
        for &c in &sh.dust {
            count[c] += 1;
        }
        for c in 0..m.len() {
            assert_eq!(
                count[c],
                u8::from(live[c]),
                "cell {c}: dead cells are nobody's and live cells are exactly one's"
            );
        }
        // A function of its inputs.
        assert_eq!(shatter(&m, &d, [0.0; 3], 3, 30), sh);
    }

    #[test]
    fn an_empty_hull_leaves_nothing() {
        let m = VoxelModel::new(4, 4, 4, 0.1);
        let d = DamageGrid::new(&m);
        assert_eq!(shatter(&m, &d, [0.0; 3], 1, 30), Shatter::default());
    }
}
