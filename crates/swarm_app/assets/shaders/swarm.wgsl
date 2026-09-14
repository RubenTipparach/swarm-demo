// One tick of the swarm, one thread per mote, and what happens when a shot
// reaches one.
//
// The CPU cannot see where a mote is: they live here and never come back. So a
// shot is not resolved on the CPU and told to the swarm, it is a VOLUME pushed
// into this uniform and resolved here. Every shot is a capsule, which is the
// one shape that covers both: a beam is a capsule from muzzle to endpoint, and
// a blast is a capsule of zero length whose radius the CPU has already grown
// for this tick. One test, no branches on kind.
//
// `swarm_core::fx::Beam::kills` is the same test in Rust and is what this copy
// is checked against. Two implementations of one rule is exactly the thing
// CLAUDE.md warns about; the boundary here is real (a GPU cannot be asked a
// question mid frame), so the answer is to keep them the same arithmetic and
// pin it with a test rather than to pretend one of them does not exist.

struct Params {
    dt: f32,
    time: f32,
    count: u32,
    nbrs: u32,
    hull: vec4<f32>,        // centre xyz, radius w
    tick: u32,
    shots: u32,             // how many capsules are live
    spark_base: u32,        // where the GPU's half of the spark ring starts
    spark_cap: u32,         // how many it may use
    hives: u32,             // how many motherships are still flying
    rocks: u32,             // how many asteroids are in the field
    targets: u32,           // how many ships the swarm may attack
    grid_n: u32,            // cells along one side of the density field
    // The box the density field covers: min corner xyz, and how many world
    // units one cell is in w. It rides with the flagship, because that is
    // where the fight is.
    grid: vec4<f32>,
    // Which way the sun is (xyz, pointing AT it) and how much of a cell's
    // face one mote covers (w). The app owns both: the same vector it aims
    // the scene's key light along, so what the swarm shadows itself against
    // is the light it is actually lit by.
    sun: vec4<f32>,
    // Pairs, so sixty four capsules is a hundred and twenty eight vectors:
    // even = from.xyz + radius, odd = to.xyz + spare.
    shot: array<vec4<f32>, 128>,
    // Where each mothership is and how big it is. Compacted every frame to
    // the LIVE ones, so a hive that dies simply shortens the list and the
    // motes that flew from it re-home by modulo.
    hive: array<vec4<f32>, 16>,
    // The asteroid field, as spheres: centre xyz, radius w. A sphere because
    // the distance to one has a closed form and a mote can afford a handful
    // of those and nothing more. The rocks are DRAWN as voxel lumps, so this
    // is the navigation shape and not the picture: the margin below is what
    // keeps the two from disagreeing where it would show.
    rock: array<vec4<f32>, 32>,
    // Every live hull, centre xyz and radius w. The cloud used to chase one
    // published centre, so it could only ever be ONE animal on ONE ship and
    // "divide the swarm by where you put your ships" could not be expressed at
    // all. A mote takes its index modulo the count, so a ship dying shortens
    // the list and its share re-homes to whatever is left, which is the rule
    // the carriers already keep.
    // `ship`, not `target`: that word is reserved in WGSL and the composer
    // refuses it by name.
    ship: array<vec4<f32>, 16>,
};

struct Mote {
    pos_scale: vec4<f32>,   // xyz, w = drawn scale, 0 while dead
    vel_seed: vec4<f32>,    // xyz, w = seed in 0..1
    state: vec4<f32>,       // x = respawn countdown, y = the scale it had,
                            // z = which mothership it flies from,
                            // w = the route parameter, for a vein
    extra: vec4<f32>,       // x = how much of it is left, one down to nought
                            // y = which leg of its life it is on, PH_*
                            // z = how long is left on that leg
                            // w = where it sits round its ship's ring
    // What the LIGHT does to this mote, which is the picture's state and not
    // the simulation's: x = how much of the sun reaches it through the cloud,
    // y = how much of the sky does, z = how hot its wound still is, and w =
    // what a burst standing next to it lights it with. Written here and read by
    // `mote.wgsl` as one more instance attribute, because the draw cannot
    // afford to work it out: the mote buffer IS the instance buffer, so a
    // field the tick fills is one the vertex shader already has.
    shade: vec4<f32>,
};

struct Spark {
    pos_life: vec4<f32>,
    vel_size: vec4<f32>,
    colour_seed: vec4<f32>,
};

// Sixteen bytes exactly, and the padding is three plain u32 rather than a
// vec3: a vec3<u32> aligns to sixteen, so the struct came out thirty two and
// the binding refused a sixteen byte buffer by name.
struct Counter {
    gpu: atomic<u32>,
    /// Where the next death wave goes in the ring below.
    wave: atomic<u32>,
    pad1: u32,
    pad2: u32,
};

/// A shock from a mote coming apart: centre xyz, and the time it went off.
///
/// Written by whichever mote died and read by every mote on the NEXT tick,
/// which is the only order available: nothing here can see what another thread
/// is doing this tick. One tick of lag on a wave that lasts most of a second
/// is not a thing anybody can see.
struct Wave {
    at: vec4<f32>,
};

@group(0) @binding(0) var<storage, read_write> motes: array<Mote>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> sparks: array<Spark>;
@group(0) @binding(3) var<storage, read_write> counter: Counter;
@group(0) @binding(4) var<storage, read_write> waves: array<Wave>;
// How many motes stand in each cell of the field, counted fresh every tick.
@group(0) @binding(5) var<storage, read_write> density: array<atomic<u32>>;
// And what the light makes of that: x = how much of the sun reaches the cell
// through the rest of the swarm, y = how much of the sky does.
@group(0) @binding(6) var<storage, read_write> light: array<vec2<f32>>;

/// What comes off a mote that dies: the spray, the pieces, and the ONE flash.
///
/// Five and four was a puff. A mote coming apart is the thing a player's guns
/// are for, and at the size a mote is drawn nine small additive particles
/// read as a sparkle rather than as a kill, which is why a volley into the
/// cloud looked like nothing was happening.
///
/// The spray is what went up and the flash is what it looked like: one big
/// short lived particle at the mote's own place, which is the shape of an
/// explosion. The DEBRIS is deliberately not raised with them, because that
/// is the half that lasts: four of them a kill at a second and a half each,
/// over a swarm losing dozens a second, is the violet haze this file already
/// warned about once. A kill is bright and BRIEF.
const SPARKS_PER_MOTE: u32 = 11u;

/// What a hit that does NOT kill throws. Small, because the thing it has to
/// say is "that one was struck" and not "that one is gone", and the ring is
/// shared with everything that dies: a beam sweeping a dense cloud lands on
/// dozens of motes in a tick, and this is paid per landing.
const HIT_SPARKS: u32 = 3u;
const RESPAWN: f32 = 0.9;

/// How many sparks a mote throws off the plating when it bites, and how often
/// a mote in contact takes one. Two numbers rather than one because the ring
/// is shared: the swarm holds thousands of motes against a hull at once, and
/// every one of them biting every tick would spend the whole ring in a frame
/// and leave nothing for anything that dies.
const BITE_SPARKS: u32 = 3u;
const BITE_CHANCE: f32 = 0.975;

/// The legs of a mote's life.
///
/// It used to have one: fly at the ship for ever, with a clock flipping it
/// between two standoffs. That is a cloud, not an animal. A mote crosses from
/// its carrier, joins the RING round the ship it was given, leaves the ring to
/// make a pass, and goes home to its carrier to be put back together if the
/// pass cost it too much.
const PH_TRANSIT: f32 = 0.0;
const PH_CIRCLE: f32 = 1.0;
const PH_ATTACK: f32 = 2.0;
const PH_RETURN: f32 = 3.0;
/// And a fifth that is not a leg of its life but the end of it: a mote that
/// has taken a mortal hit is not gone, it is DYING, and for half a second it
/// is out of control and on fire before it bursts.
const PH_DYING: f32 = 4.0;

/// The death spiral: how long it lasts, how hard the corkscrew pulls, how
/// fast that pull goes round, and what is left of the mote's speed a second
/// later.
///
/// A kill used to be one tick: hit, burst, gone, and the burst was the only
/// thing that ever said a mote had been killed. That reads as motes winking
/// out, because the thing a player is actually looking at, the bug, is not in
/// the picture on the frame anything happens to it. Half a second of a body
/// tumbling out of control and burning is what makes a kill an EVENT, and it
/// costs one phase and no memory: the mote is already integrating a velocity
/// and already drawing itself along it, so a corkscrew is an acceleration and
/// the tumble comes for free from `basis_from` in the draw.
const DEATH_SPIN: f32 = 0.55;
const DEATH_TUMBLE: f32 = 30.0;
const DEATH_RATE: f32 = 17.0;
const DEATH_DRAG: f32 = 0.55;
/// How often a dying mote sheds a spark, per tick. It is a TRAIL: what says
/// the thing is coming down rather than flying.
const DEATH_TRAIL: f32 = 0.5;
/// And how hard it burns while it goes down. Under one on purpose: see the
/// note where it is written.
const DEATH_BURN: f32 = 0.62;

/// How long a mote's wound takes to go out, in seconds.
///
/// A hit used to light a mote for as long as it stayed hurt, which is until it
/// got home and was put back together: a pass through a beam left a cloud of
/// bugs glowing orange for the rest of their lives, and a swarm where every
/// mote that had ever been touched was still on fire says nothing about which
/// of them was hit just now. A wound is an EVENT. It burns, it cools, and what
/// is left afterwards is the char, which is the hull's own rule (`heat_of`
/// over `COOL_TICKS`) at the scale a mote lives at: fifteen seconds is most of
/// a frigate's siege and most of a mote's whole life.
const MOTE_COOL: f32 = 2.5;

/// What a shot takes off a mote, and how little it can have left before it
/// breaks off for home. Two hits kill; one sends it limping.
const SHOT_BITE: f32 = 0.6;
const RETURN_AT: f32 = 0.5;

/// The ring, in target radii, and the pass.
const RING_R: f32 = 2.7;
const ATTACK_R: f32 = 0.90;
/// How hard a mote is pulled into its ship's own ring PLANE. This is what
/// makes a ring rather than a shell: a standoff alone spreads the traffic over
/// a sphere, and a sphere has no formation in it.
const RING_FLAT: f32 = 5.0;

/// How many pieces a mote comes apart into, on top of its own spark burst.
const DEBRIS: u32 = 4u;

/// The shock a dying mote leaves behind: how many are remembered at once, how
/// long one lasts, how wide it opens and how hard it shoves.
///
/// A ring rather than a list, overwritten oldest first, because a swarm loses
/// dozens a second and nothing is going to tidy up after them. Every mote
/// tests every slot every tick, so this is a budget: sixty four is about a
/// microsecond of the tick at a million motes.
const WAVES: u32 = 64u;
const WAVE_LIFE: f32 = 0.7;
const WAVE_R: f32 = 5.0;
const WAVE_PUSH: f32 = 60.0;

/// And what a burst LIGHTS: how far the light carries, in world units, and
/// what it is worth at the middle of it.
///
/// A mote beside one that has just gone up is lit by it, which is the only way
/// an explosion inside a cloud can be seen to be inside one: the shading makes
/// the deep swarm dark on purpose, so a burst buried in it has nothing to show
/// against unless it lights its neighbours.
///
/// It rides the shock ring rather than a list of its own, because the ring is
/// already the record of what has just died, already read by every mote every
/// tick, and already the right size. One number, no new pass, no new memory.
///
/// A HARD reach, and that is the whole of the tuning. The first cut was an
/// inverse square with a soft core and no cutoff, which is the honest physics
/// and exactly wrong here: sixty four live bursts each throwing a fifth of
/// their light at two units and a twenty fifth at five add up to a floor under
/// the entire swarm, and the cloud came out uniformly lit violet with every
/// bit of the self shadowing washed off it. What a fireball has to do is light
/// the dozen bodies AROUND it, so the falloff goes to nought at a fixed
/// distance and the far field is nought rather than small.
const WAVE_LIGHT_R: f32 = 3.2;
const WAVE_LIGHT: f32 = 2.6;

/// How far out from a BLAST a mote starts getting out of the way, as a share
/// of the radius that blast has grown to, and how hard it is shoved.
///
/// Wider and harder than a rock's, because a rock is a thing standing still
/// that a mote has all day to round and a blast is a wall arriving: the margin
/// has to be crossed in the fraction of a second the fireball takes to reach
/// it, or the push only ever lands on motes that are already dead.
const BLAST_MARGIN: f32 = 1.6;
const BLAST_SHOVE: f32 = 70.0;

/// How far out from a rock a mote starts turning, as a share of its radius.
const ROCK_MARGIN: f32 = 0.55;

/// What share of the swarm runs a route between rocks instead of going for
/// the ship, how much of a route ends at the SHIP rather than at another rock,
/// how fast a route is walked (in routes a second), and how wide the tube of
/// traffic is as a share of the rock it leaves.
const VEIN_SHARE: f32 = 0.22;
const VEIN_TO_SHIP: f32 = 0.34;
const VEIN_RATE: f32 = 0.085;
const VEIN_TUBE: f32 = 0.34;

/// How many cells toward the sun a cell looks before it gives up.
///
/// One cell a step, so this is the reach of a shadow in cells: sixteen of them
/// across a field that spans the carriers is about fifty units, which is wider
/// than any clump the swarm actually makes. A longer march buys nothing a
/// player could see and costs the whole grid.
const SHADOW_STEPS: u32 = 16u;

/// What is left of the sun in the very middle of the thickest cloud, and what
/// is left of the sky.
///
/// Not nought, and that is the same rule the ambient term in the scene keeps:
/// a mote lit by nothing at all is the colour of the gap between two stars,
/// and a swarm whose middle is a hole is worse than one with no shading in it.
const SHADOW_FLOOR: f32 = 0.09;
const SKY_FLOOR: f32 = 0.20;

/// How much more the cloud immediately round a mote shuts out the sky than it
/// shuts out the sun. The sun comes from one direction and the sky from all of
/// them, so the near neighbours count for more.
const SKY_GAIN: f32 = 2.4;

fn hash(n: u32) -> f32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    x = (x >> 22u) ^ x;
    return f32(x) / 4294967295.0;
}

/// `closest_on_segment` in `fx.rs`, clamped for the same reason: without it a
/// mote a hundred units behind the muzzle is on the beam's line and dies.
fn capsule_hit(a: vec3<f32>, b: vec3<f32>, r: f32, pt: vec3<f32>) -> bool {
    let ab = b - a;
    let len2 = dot(ab, ab);
    var t = 0.0;
    if (len2 > 1e-12) { t = clamp(dot(pt - a, ab) / len2, 0.0, 1.0); }
    let d = pt - (a + ab * t);
    return dot(d, d) <= r * r;
}

// ------------------------------------------------------------- the field --
//
// The swarm is a field and this is the part of it that is literally one: how
// many motes stand in each cell of a coarse grid, and what the sun makes of
// that. It is what a mote is SHADED by, and the reason it is a grid rather
// than something each mote works out for itself is the arithmetic the whole
// design rests on.
//
// A mote cannot march toward the sun on its own account. A million of them at
// sixteen steps each is sixteen million samples a tick against a budget of
// sixteen nanoseconds a mote, which is the same sum that says a mote is not an
// entity. The GRID marches instead: a quarter of a million cells whatever the
// swarm costs, so a shadow is a property of the cloud rather than a per mote
// expense, and a mote pays one trilinear read to find out how dark it stands.
//
// Nothing casts on anything else. A hull does not shadow the swarm and the
// swarm does not shadow a hull: this is SELF shadowing, which is the whole of
// what a cloud needs to stop reading as a flat sheet of lit specks.

fn cells() -> u32 {
    return p.grid_n * p.grid_n * p.grid_n;
}

/// World to grid, in cell units: cell (i, j, k) covers i..i+1 in x.
fn to_grid(w: vec3<f32>) -> vec3<f32> {
    return (w - p.grid.xyz) / p.grid.w;
}

fn cell_index(c: vec3<i32>) -> u32 {
    let n = i32(p.grid_n);
    return u32(c.x + c.y * n + c.z * n * n);
}

fn in_grid(c: vec3<i32>) -> bool {
    let n = i32(p.grid_n);
    return c.x >= 0 && c.y >= 0 && c.z >= 0 && c.x < n && c.y < n && c.z < n;
}

/// Outside the box is empty space, which is the honest answer: the field rides
/// with the fight and everything beyond it is a mote on its own in the dark.
fn motes_in(c: vec3<i32>) -> f32 {
    if (!in_grid(c)) { return 0.0; }
    return f32(atomicLoad(&density[cell_index(c)]));
}

fn light_in(c: vec3<i32>) -> vec2<f32> {
    if (!in_grid(c)) { return vec2<f32>(1.0, 1.0); }
    return light[cell_index(c)];
}

/// Trilinear, and it has to be.
///
/// A cell is a few units across and a mote is a fraction of one, so a nearest
/// read would hand every mote in a cell exactly the same shade and the swarm
/// would fly through visible cubes of it. Eight reads and seven mixes is the
/// price of a field that changes smoothly across a cloud.
fn light_at(w: vec3<f32>) -> vec2<f32> {
    // Half a cell back, so the cell CENTRES land on the integers and a mote
    // standing dead centre reads its own cell and nothing else.
    let g = to_grid(w) - 0.5;
    let b = floor(g);
    let f = g - b;
    let c = vec3<i32>(b);
    var acc = vec2<f32>(0.0);
    for (var k: i32 = 0; k < 8; k = k + 1) {
        let o = vec3<i32>(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        let wx = mix(1.0 - f.x, f.x, f32(o.x));
        let wy = mix(1.0 - f.y, f.y, f32(o.y));
        let wz = mix(1.0 - f.z, f.z, f32(o.z));
        acc = acc + light_in(c + o) * (wx * wy * wz);
    }
    return acc;
}

/// The optical depth one mote in a cell adds: how much of the cell's face it
/// covers. A mote is about a third of a unit across and a cell is a few, so a
/// handful of them in one cell is already something the sun has to get past
/// and a few hundred is a wall.
fn extinction() -> f32 {
    return p.sun.w / (p.grid.w * p.grid.w);
}

/// Empty the field. One thread a cell.
@compute @workgroup_size(256)
fn clear_field(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= cells()) { return; }
    atomicStore(&density[gid.x], 0u);
}

/// Count what is where. One thread a mote, one atomic each, and a mote still
/// inside its carrier is not in the picture and does not shade anything.
@compute @workgroup_size(256)
fn splat_field(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= p.count) { return; }
    let m = motes[i];
    if (m.pos_scale.w <= 0.0) { return; }
    let c = vec3<i32>(floor(to_grid(m.pos_scale.xyz)));
    if (!in_grid(c)) { return; }
    atomicAdd(&density[cell_index(c)], 1u);
}

/// What the light makes of it. One thread a cell, and the only pass in the
/// whole tick whose cost does not move when the swarm does.
@compute @workgroup_size(256)
fn light_field(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= cells()) { return; }
    let n = i32(p.grid_n);
    let c = vec3<i32>(i32(i) % n, (i32(i) / n) % n, i32(i) / (n * n));
    let sigma = extinction();

    // Toward the sun, one cell a step, starting at the NEXT one: a mote does
    // not stand in its own light, and one on its own out in the dark has to
    // come out fully lit or the shading would read as a fog.
    var at = vec3<f32>(c) + 0.5;
    var tau = 0.0;
    for (var s: u32 = 0u; s < SHADOW_STEPS; s = s + 1u) {
        at = at + p.sun.xyz;
        tau = tau + motes_in(vec3<i32>(floor(at))) * sigma;
    }

    // And how much sky it can see, which is the cloud immediately round it
    // rather than the cloud between it and the sun. Its own cell and its six
    // faces: an ambient term is light arriving from everywhere, so what shuts
    // it out is everything nearby and not anything in one direction.
    var near = motes_in(c);
    near = near + motes_in(c + vec3<i32>(1, 0, 0)) + motes_in(c - vec3<i32>(1, 0, 0));
    near = near + motes_in(c + vec3<i32>(0, 1, 0)) + motes_in(c - vec3<i32>(0, 1, 0));
    near = near + motes_in(c + vec3<i32>(0, 0, 1)) + motes_in(c - vec3<i32>(0, 0, 1));

    // Beer's law both times, which is the one thing a density grid is for.
    light[i] = vec2<f32>(
        SHADOW_FLOOR + (1.0 - SHADOW_FLOOR) * exp(-tau),
        SKY_FLOOR + (1.0 - SKY_FLOOR) * exp(-near * sigma * SKY_GAIN)
    );
}

/// Which way a mote leaves its mothership.
///
/// A direction rather than a point, because the launch needs both: where it
/// appears is the hive's skin along this, and how it leaves is a shove along
/// the same. The swarm used to come back at a shell round the target, which
/// is a cloud that simply exists; it comes out of a carrier now, so what a
/// player sees is a stream with a source they can go and kill.
fn launch_dir(seed: u32) -> vec3<f32> {
    var d = vec3<f32>(hash(seed) - 0.5, hash(seed + 7u) - 0.5, hash(seed + 19u) - 0.5);
    return d / max(length(d), 1e-4);
}

@compute @workgroup_size(256)
fn tick(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= p.count) { return; }
    var m = motes[i];
    let seed = m.vel_seed.w;

    // Dead and waiting: count down, then launch out of a mothership.
    if (m.state.x > 0.0) {
        m.state.x = m.state.x - p.dt;
        if (m.state.x <= 0.0) {
            // Nowhere to come from: every carrier is gone, so it stays gone.
            // This is what winning looks like, and it needs no rule of its
            // own beyond an empty list.
            if (p.hives == 0u) {
                m.state.x = 0.0001;
                motes[i] = m;
                return;
            }
            m.state.x = 0.0;
            let h = p.hive[u32(max(m.state.z, 0.0)) % p.hives];
            let d = launch_dir(i * 2654435761u + p.tick);
            m.pos_scale = vec4<f32>(h.xyz + d * h.w * 1.15, m.state.y);
            // Shoved out hard, so a launch reads as a launch. The appetite
            // below takes over once it is clear of the hull it came from.
            m.vel_seed = vec4<f32>(d * h.w * (2.2 + 1.6 * hash(i + 5u)), seed);
            // It comes out of the tube already on a run, and on its OWN
            // clock: a wave that all flipped together would breathe in and
            // out as one animal rather than read as a thousand of them.
            m.extra = vec4<f32>(1.0, PH_TRANSIT, 0.0, m.extra.w);
            // And COLD. It died burning, and the heat below only ever cools:
            // without this a mote that was killed launches out of its carrier
            // still on fire from the last life.
            m.shade.z = 0.0;
        }
        motes[i] = m;
        return;
    }

    let me = m.pos_scale.xyz;

    // Did anything reach it this tick? A mote is HURT now rather than simply
    // removed: a hit takes a share off it, two kill, and one leaves it able to
    // fly but not to fight.
    var hp = m.extra.x;
    var leg = m.extra.y;
    var fuse = m.extra.z;
    // How hot its wound still is, one at the moment of the hit and nought once
    // it has gone out. It is the mote's own `heat_of`, kept in the lane the
    // shading already hands the draw rather than in a sixth vector.
    var heat = m.shade.z;
    var hit = false;
    for (var s: u32 = 0u; s < p.shots; s = s + 1u) {
        let a = p.shot[s * 2u];
        let b = p.shot[s * 2u + 1u];
        if (capsule_hit(a.xyz, b.xyz, a.w, me)) { hit = true; break; }
    }
    // A mote that is already dying is past being hurt: it is going to burst on
    // its own clock and a second shot cannot make that happen twice.
    if (hit && leg != PH_DYING) {
        hp = hp - SHOT_BITE;
        // The wound is lit as hot as a mote going down burns, and the whole of
        // the difference between the two is that this one COOLS. A hit and a
        // mortal hit are the same event; what tells them apart is what happens
        // over the next half second. Not one: `DEATH_BURN` is under one
        // because the tone mapper turns a full burn white, and a white flash
        // with a halo is a spark rather than a burning animal.
        heat = DEATH_BURN;
        // ---- the hit itself ----
        //
        // A few sparks off the body on the tick the shot lands, which is the
        // one moment a hit is a MOMENT. Without it the only sign of a
        // non fatal hit is the wound coming up on the body over the next few
        // frames, and at the one or two pixels a mote is usually drawn at
        // that is a colour changing rather than a thing being struck.
        //
        // Orange, because it is a hit, and the same colour the wound burns:
        // what is violet on a mote is the gore that comes out when it finally
        // bursts, and this is not that.
        let hb = atomicAdd(&counter.gpu, HIT_SPARKS);
        for (var k: u32 = 0u; k < HIT_SPARKS; k = k + 1u) {
            let slot = p.spark_base + ((hb + k) % p.spark_cap);
            let h = vec3<f32>(hash(hb + k), hash(hb + k + 31u), hash(hb + k + 67u)) - 0.5;
            var sp: Spark;
            sp.pos_life = vec4<f32>(me, 0.16 + 0.16 * hash(hb + k + 5u));
            sp.vel_size = vec4<f32>(m.vel_seed.xyz * 0.3 + h * 9.0,
                                    m.pos_scale.w * (0.07 + 0.07 * hash(hb + k + 7u)));
            let hot = 0.7 + 0.6 * hash(hb + k + 11u);
            sp.colour_seed = vec4<f32>(4.4 * hot, 2.0 * hot, 0.5 * hot, hash(hb + k + 13u));
            sparks[slot] = sp;
        }
    }

    // ---- a mortal hit starts the SPIRAL ----
    //
    // Not the burst. The burst is what it ENDS in, half a second later, and
    // everything between the two is the only part of a kill a player can
    // actually watch.
    if (hit && hp <= 0.0 && leg != PH_DYING) {
        leg = PH_DYING;
        fuse = DEATH_SPIN;
        hp = 0.0;
    }

    if (leg == PH_DYING) {
        fuse = fuse - p.dt;
    }

    if (leg == PH_DYING && fuse > 0.0) {
        // ---- the death spiral ----
        //
        // Out of control and on fire. The corkscrew is a lateral acceleration
        // whose direction goes round the velocity as the fuse burns down, so
        // the path is a helix that tightens as the drag takes the speed off
        // it; and because a mote is DRAWN along its own velocity, a velocity
        // that is turning is a body that is tumbling. Nothing here had to be
        // added to the draw at all.
        var v = m.vel_seed.xyz;
        let sp0 = max(length(v), 1e-4);
        let fwd = v / sp0;
        var up = vec3<f32>(0.0, 1.0, 0.0);
        if (abs(dot(fwd, up)) > 0.95) { up = vec3<f32>(1.0, 0.0, 0.0); }
        let right = normalize(cross(up, fwd));
        let over = cross(fwd, right);
        // Its own phase, so a volley of kills is a dozen bugs going down
        // differently rather than a dozen copies of one animation.
        let th = (DEATH_SPIN - fuse) * DEATH_RATE + seed * 71.0;
        v = v + (right * cos(th) + over * sin(th)) * DEATH_TUMBLE * p.dt;
        // And it is losing way. Per second rather than per tick, or the spiral
        // would be a different shape at a different frame rate, which is the
        // trap the swarm's own drag was in.
        v = v * pow(DEATH_DRAG, p.dt);
        m.vel_seed = vec4<f32>(v, seed);
        m.pos_scale = vec4<f32>(me + v * p.dt, m.pos_scale.w);
        // It TRAILS. One spark most ticks, thrown off the body and left
        // behind, which is what says the thing is coming down.
        if (hash(i * 9176u + p.tick) < DEATH_TRAIL) {
            let tb = atomicAdd(&counter.gpu, 1u);
            let slot = p.spark_base + (tb % p.spark_cap);
            let h = vec3<f32>(hash(tb + 3u), hash(tb + 29u), hash(tb + 71u)) - 0.5;
            var sp: Spark;
            sp.pos_life = vec4<f32>(me, 0.3 + 0.3 * hash(tb + 5u));
            sp.vel_size = vec4<f32>(v * 0.2 + h * 2.0,
                                    m.pos_scale.w * (0.10 + 0.10 * hash(tb + 7u)));
            let hot = 0.5 + 0.6 * hash(tb + 11u);
            sp.colour_seed = vec4<f32>(2.6 * hot, 1.0 * hot, 0.25 * hot, hash(tb + 13u));
            sparks[slot] = sp;
        }
        // Burning steadily, which is the one place a wound does not throb: a
        // mote that is going down is not pulsing, it is alight. The shading is
        // left where it was, because a body on fire is lit by the fire and the
        // draw fades the lit channel out under the burn anyway.
        //
        // And it burns at well under a FULL burn, which is the tone mapper's
        // rule and the same one the drive flames are built on. The wound is
        // authored at 5.0 red against 0.30 blue, and `NeutralToneMapping`
        // desaturates a highlight by scaling every channel toward the peak, so
        // laid on at one a dying mote arrives WHITE with a halo round it: a
        // hot spark rather than a burning animal, and indistinguishable from
        // the flash it is about to become. At this it comes out orange, still
        // well over the bloom threshold in the red, and still plainly brighter
        // than the throb a mote that merely took a hit is carrying.
        m.extra = vec4<f32>(hp, leg, fuse, m.extra.w);
        m.shade = vec4<f32>(m.shade.x, m.shade.y, DEATH_BURN, m.shade.w);
        motes[i] = m;
        return;
    }

    if (leg == PH_DYING) {
        // It comes APART. The ring is claimed with one atomic for the whole
        // burst rather than one each, so a mote's pieces stay together in the
        // buffer and cannot interleave with another's half written ones.
        let base = atomicAdd(&counter.gpu, SPARKS_PER_MOTE + DEBRIS + 1u);
        for (var k: u32 = 0u; k < SPARKS_PER_MOTE; k = k + 1u) {
            let slot = p.spark_base + ((base + k) % p.spark_cap);
            let h = vec3<f32>(hash(base + k), hash(base + k + 101u), hash(base + k + 211u)) - 0.5;
            let v = m.vel_seed.xyz * 0.35 + h * (6.0 + 10.0 * hash(base + k + 3u));
            var sp: Spark;
            sp.pos_life = vec4<f32>(me, 0.25 + 0.5 * hash(base + k + 5u));
            sp.vel_size = vec4<f32>(v, m.pos_scale.w * (0.10 + 0.16 * hash(base + k + 7u)));
            // Gore, not fire, and PURPLE: the swarm is chitin over violet and
            // that is what comes out of one. It used to throw green, which is
            // the colour of the lights ON a mote rather than the colour of the
            // animal, so a kill read as a lamp going out instead of as a thing
            // coming apart. Over one, because this is what bloom is for.
            let hot = 0.6 + 0.8 * hash(base + k + 11u);
            sp.colour_seed = vec4<f32>(2.3 * hot, 0.55 * hot, 3.1 * hot, hash(base + k + 13u));
            sparks[slot] = sp;
        }
        // And the DEBRIS: bigger, slower, longer lived and much dimmer than
        // the burst, so what is left after the flash has gone is pieces of a
        // mote tumbling away rather than nothing at all. They are the same
        // particles as the flash and cost the same, which is the point: a mote
        // that came apart into real meshes would be nine thousand entities the
        // moment a volley landed.
        for (var k: u32 = 0u; k < DEBRIS; k = k + 1u) {
            let slot = p.spark_base + ((base + SPARKS_PER_MOTE + k) % p.spark_cap);
            let g = base + SPARKS_PER_MOTE + k;
            let h = vec3<f32>(hash(g + 17u), hash(g + 53u), hash(g + 97u)) - 0.5;
            var sp: Spark;
            sp.pos_life = vec4<f32>(me, 0.7 + 0.9 * hash(g + 5u));
            sp.vel_size = vec4<f32>(m.vel_seed.xyz * 0.5 + h * (1.6 + 3.4 * hash(g + 3u)),
                                    m.pos_scale.w * (0.22 + 0.30 * hash(g + 7u)));
            // Barely over white: a chunk of shell is LIT by the scene, and the
            // only tool here is an additive particle, so it is kept dim enough
            // to read as matter instead of as another spark.
            // Dim, and this is the line to watch. Additive particles stack:
            // four a kill at two seconds each, over a swarm losing dozens a
            // second, is a violet haze over the whole battle rather than
            // pieces of anything. Bright debris is the mothership's green
            // emissive by another route, so it is kept under a third.
            let d = 0.16 + 0.16 * hash(g + 11u);
            sp.colour_seed = vec4<f32>(0.55 * d, 0.30 * d, 0.78 * d, hash(g + 13u));
            sparks[slot] = sp;
        }
        // ---- and the FLASH ----
        //
        // One particle, at the mote's own place, several times its size and
        // well over white. This is what makes a kill read as a kill: a spray
        // of small pieces says something came apart somewhere, and a flash
        // says where.
        //
        // A third of a second, and no shorter, which is a trap in the spark
        // shader rather than a taste: a spark's colour is scaled by
        // `min(1, life * 3)` at birth, so a flash asked to live a tenth of a
        // second is born at a third of the brightness it was thrown with. It
        // is brief because it FADES, not because it is cut off.
        {
            let slot = p.spark_base + ((base + SPARKS_PER_MOTE + DEBRIS) % p.spark_cap);
            var sp: Spark;
            sp.pos_life = vec4<f32>(me, 0.34);
            // It drifts with what was flying, so the flash is where the mote
            // was going rather than pinned to a point in space.
            sp.vel_size = vec4<f32>(m.vel_seed.xyz * 0.25, m.pos_scale.w * 1.15);
            // WHITE HOT, and warm rather than violet. The spray that goes
            // with it is the animal's own gore and stays violet, but the
            // flash is the moment of bursting rather than anything that came
            // out, and every other thing in this game that bursts flashes
            // white: a violet flash on a violet body is a kill that reads as
            // one more purple mote for the frame it lasts.
            sp.colour_seed = vec4<f32>(5.6, 4.2, 3.0, hash(base + 71u));
            sparks[slot] = sp;
        }

        // And it leaves a HOLE the others fly round. A mote coming apart is a
        // thing in the way for a moment, so the swarm opens where one died
        // instead of closing straight over it.
        let w = atomicAdd(&counter.wave, 1u) % WAVES;
        waves[w].at = vec4<f32>(me, p.time);

        m.state.y = max(m.pos_scale.w, m.state.y);
        m.state.x = RESPAWN;
        m.extra = vec4<f32>(1.0, PH_TRANSIT, 0.0, m.extra.w);
        m.pos_scale = vec4<f32>(me, 0.0);
        motes[i] = m;
        return;
    }

    let sh = u32(seed * 65535.0);

    // ---- veins ----
    //
    // A share of the swarm does not go for the ship at all: it winds round a
    // rock. The target is the asteroid rather than the hull, at a standoff of
    // a fraction of the rock's own radius, so the same steering that makes an
    // attack run makes a ribbon hugging a surface instead. Combined with the
    // per mote swirl axis that is what draws a VEIN: a few hundred motes on
    // near circular orbits at every inclination read as a braid winding round
    // the rock rather than as a shell round it.
    //
    // It costs one hash and one select. The rest of the tick does not know
    // which it is, which is the point: a vein is not a second behaviour, it is
    // the same behaviour pointed at something else.
    // ---- which ship this mote is on ----
    //
    // Seeded, so the division is stable: a mote does not change its mind every
    // tick, and the same seed gives the same share of the cloud to each ship.
    // Put two ships far apart and the swarm splits between them; bring them
    // together and it converges. That is the whole game, and it is one line.
    var tgt = p.hull;
    var ring_seed = 7777u;
    if (p.targets > 0u) {
        let ti = sh % p.targets;
        tgt = p.ship[ti];
        // The ring's own axis is hashed off WHICH ship it is round, so every
        // mote on that ship shares it and two ships do not ring the same way.
        ring_seed = ti * 2654435761u;
    }
    var focus = tgt.xyz;
    var focus_r = tgt.w;
    // A mote going home aims at its own carrier instead.
    if (m.extra.y == PH_RETURN && p.hives > 0u) {
        let h = p.hive[u32(max(m.state.z, 0.0)) % p.hives];
        focus = h.xyz;
        focus_r = h.w;
    }
    var vein = false;
    if (p.rocks > 0u && hash(sh + 907u) < VEIN_SHARE) {
        vein = true;
        // A ROUTE, not an orbit. The first cut gave a vein mote its own
        // standoff round one rock and its own swirl axis, which is a sphere of
        // orbits: the swarm came out as a solid BALL round every asteroid,
        // because that is what a thousand orbits at every inclination is. An
        // ant does not orbit, it follows a path that other ants are on.
        //
        // So a vein mote belongs to a route between two anchors and rides a
        // point that slides along it. Every mote on the same route is on the
        // same line, which is what makes a line of traffic rather than a
        // shell, and the asteroid avoidance below bends the line round
        // anything standing in it: a trail that weaves is a trail that met
        // something.
        let ra = sh % p.rocks;
        let anchor_a = p.rock[ra];
        var anchor_b = p.hull;
        if (p.rocks > 1u && hash(sh + 1301u) > VEIN_TO_SHIP) {
            let rb = (ra + 1u + (sh / 13u) % (p.rocks - 1u)) % p.rocks;
            anchor_b = p.rock[rb];
        }
        var ab = anchor_b.xyz - anchor_a.xyz;
        let span = max(length(ab), 1e-4);
        let along = ab / span;
        // Surface to surface, so a route starts off the rock it leaves rather
        // than inside it.
        let a0 = anchor_a.xyz + along * anchor_a.w * 1.14;
        let b0 = anchor_b.xyz - along * anchor_b.w * 1.14;

        // How far along it is. `state.w` is the run clock for everything else
        // and a vein makes no runs, so it carries the route parameter here
        // instead: one field, two meanings, and nothing reads the wrong one
        // because `vein` is decided from the seed and never changes.
        var s = fract(m.state.w + VEIN_RATE * p.dt * (0.65 + 0.7 * hash(sh + 71u)));
        m.state.w = s;

        // A TUBE rather than a line: a fixed offset per mote about the route,
        // so the traffic has a cross section a few motes wide instead of every
        // one of them trying to be at the same point.
        var e1 = cross(along, vec3<f32>(0.0, 1.0, 0.0));
        if (length(e1) < 1e-4) { e1 = cross(along, vec3<f32>(1.0, 0.0, 0.0)); }
        e1 = e1 / max(length(e1), 1e-4);
        let e2 = cross(along, e1);
        let ph = hash(sh + 211u) * 6.2831853;
        let tube = anchor_a.w * VEIN_TUBE * (0.25 + 0.75 * hash(sh + 313u));
        focus = mix(a0, b0, s) + (e1 * cos(ph) + e2 * sin(ph)) * tube;
        // It wants to BE there, not to stand off from it.
        focus_r = 0.0;
    }

    let to_hull = focus - me;
    let dist = max(length(to_hull), 0.001);
    let dir = to_hull / dist;

    // ---- the leg it is on ----
    //
    // Cross from the carrier, join the RING round the ship it was given, leave
    // the ring to make a pass, and go home to be put back together if the pass
    // cost too much. Four legs, and which one a mote is on decides the one
    // number the steering below actually reads: where it wants to be.
    var phase = m.extra.y;
    var timer = m.extra.z - p.dt;
    let r = hash(i * 2246822519u + p.tick);
    if (phase == PH_TRANSIT) {
        // Arrived: near enough the ring to be part of it.
        if (dist < focus_r * RING_R * 1.7) {
            phase = PH_CIRCLE;
            timer = 2.2 + 3.4 * r;
        }
    } else if (phase == PH_CIRCLE) {
        if (timer <= 0.0) {
            phase = PH_ATTACK;
            timer = 1.3 + 1.7 * r;
        }
    } else if (phase == PH_ATTACK) {
        if (timer <= 0.0) {
            // Hurt enough and it goes home rather than round again. This is
            // the only thing that ever sends a mote back to its carrier, and
            // it is why a mote has hit points at all.
            if (hp < RETURN_AT) {
                phase = PH_RETURN;
                timer = 0.0;
            } else {
                phase = PH_CIRCLE;
                timer = 2.2 + 3.4 * r;
            }
        }
    } else {
        // Home. Docked, repaired, and out again.
        if (dist < focus_r * 1.35) {
            hp = 1.0;
            phase = PH_TRANSIT;
            timer = 0.0;
        }
    }
    m.extra = vec4<f32>(hp, phase, timer, m.extra.w);

    // Where it wants to be, which is the whole of what the leg decides.
    var want = focus_r * RING_R;
    if (phase == PH_ATTACK) { want = focus_r * ATTACK_R; }
    if (phase == PH_RETURN) { want = focus_r * 1.2; }
    // A vein has no legs and no standoff: it rides its point.
    if (vein) { want = 0.0; }

    // The pull is capped, or a fighter fifty units out would accelerate at
    // fifty and arrive as a bullet. It is the CAP that makes the approach read
    // as a flight rather than as a teleport.
    var acc = dir * clamp(dist - want, -8.0, 8.0) * 1.6;

    // ---- the ring ----
    //
    // The swirl axis belongs to the TARGET, not to the mote, and that is what
    // makes a ring rather than a shell. An axis per mote gives orbits at every
    // inclination, which is a sphere of traffic: fine for a cloud that is only
    // milling about, and not a formation. One axis per ship means every mote
    // circling that ship is going round the same way on the same plane.
    var ax = vec3<f32>(hash(ring_seed + 11u), hash(ring_seed + 23u), hash(ring_seed + 41u)) - 0.5;
    ax = ax / max(length(ax), 1e-4);
    var swirl = cross(dir, ax);
    let sl = length(swirl);
    if (sl > 1e-4) { swirl = swirl / sl; } else { swirl = vec3<f32>(0.0, 1.0, 0.0); }
    // Hard while circling, barely at all on the way in or home.
    var spin = 1.0;
    if (phase == PH_CIRCLE) { spin = 6.0; }
    if (phase == PH_ATTACK) { spin = 2.2; }
    if (vein) { spin = 0.0; }
    acc = acc + swirl * spin;

    // And pulled INTO the plane. Without this the standoff spreads the traffic
    // over a sphere and there is no ring to see: this is the term that flattens
    // it into one.
    if (!vein && (phase == PH_CIRCLE || phase == PH_ATTACK)) {
        let off_plane = dot(me - focus, ax);
        acc = acc - ax * off_plane * RING_FLAT;
    }

    // ---- the weave ----
    //
    // A lateral oscillation about the third axis of the mote's own frame, at
    // its own rate and its own phase. Without it a mote flies a clean arc and
    // ten thousand clean arcs read as a machine; with it the same cloud has
    // texture at the scale of a single craft.
    let wv = cross(dir, swirl);
    let rate = 1.6 + 3.4 * hash(sh + 57u);
    acc = acc + wv * sin(p.time * rate + seed * 71.0) * select(2.2, 0.6, vein);

    let jit = vec3<f32>(hash(i * 3u + u32(p.time * 7.0)), hash(i * 5u + u32(p.time * 5.0)), hash(i * 7u + u32(p.time * 3.0))) - 0.5;
    acc = acc + jit * select(1.5, 0.5, vein);

    // ---- the asteroid field ----
    //
    // An analytic distance field, one sphere per rock, and the steering is
    // its GRADIENT: push out along the normal, harder the nearer the surface
    // is. Squared, so a mote well outside the margin is barely deflected and
    // one about to touch is turned hard, which is what makes it read as
    // rounding an obstacle rather than as hitting a wall.
    for (var r: u32 = 0u; r < p.rocks; r = r + 1u) {
        let rk = p.rock[r];
        let off = me - rk.xyz;
        let dr = max(length(off), 1e-4);
        // A vein's OWN rock pushes it far less, or the avoidance term would
        // shove the ribbon straight off the thing it is supposed to be wound
        // round: the standoff above is inside this margin by design.
        let mine = vein && dot(rk.xyz - focus, rk.xyz - focus) < 1e-6;
        let margin = rk.w * select(ROCK_MARGIN, 0.06, mine);
        let sdf = dr - rk.w;
        if (sdf < margin) {
            let nrm = off / dr;
            // Steer AROUND, do not push away. A radial push outside the
            // surface is a force with nothing to spend itself on: it balances
            // against the pull toward the ship at some radius and every mote
            // that arrives is held there, which builds a standing shell round
            // the rock out of the traffic that was only meant to pass it. So
            // outside the surface the only correction is to cancel the part of
            // the velocity going INTO the rock and to keep the part going
            // along it, which is a mote sliding past an obstacle.
            let into = dot(m.vel_seed.xyz, nrm);
            if (into < 0.0) {
                acc = acc - nrm * into * 4.0;
                var tang = m.vel_seed.xyz - nrm * into;
                let tl = length(tang);
                if (tl > 1e-4) { acc = acc + (tang / tl) * 9.0; }
            }
            // A real push only once it is actually INSIDE, where there is
            // something to be pushed out of.
            if (sdf < 0.0) { acc = acc + nrm * (-sdf) * 40.0; }
        }
    }

    // ---- out of the way of a BLAST ----
    //
    // The swarm knew about a blast in exactly one way, which was dying in it:
    // a shell went off in the middle of the cloud and the motes outside its
    // radius flew on through the fire as though nothing had happened. A blast
    // is a thing EXPANDING, so what it should do to the cloud is open it.
    //
    // The same sphere field the rocks and the shocks steer on, over the shots
    // the app already publishes, so it costs no new data and no new pass. Only
    // the BLASTS: a capsule of zero length. A beam is a line, it is gone in a
    // second, and a mote that swerved off one would be dodging something it
    // cannot see; a blast is a place to be away from.
    //
    // The radius here is the one this tick has GROWN to, which is what makes
    // the cloud open as the fireball does rather than all at once.
    for (var s: u32 = 0u; s < p.shots; s = s + 1u) {
        let a = p.shot[s * 2u];
        let b = p.shot[s * 2u + 1u];
        let along = b.xyz - a.xyz;
        if (dot(along, along) > 1e-6) { continue; }
        let off = me - a.xyz;
        let dr = max(length(off), 1e-4);
        let margin = a.w * BLAST_MARGIN;
        let sdf = dr - a.w;
        if (sdf < margin) {
            // Squared, as the rocks are: barely a nudge out at the edge of the
            // margin and a hard shove for a mote about to be inside it.
            let push = clamp((margin - sdf) / max(margin, 1e-4), 0.0, 2.0);
            acc = acc + (off / dr) * push * push * BLAST_SHOVE;
        }
    }

    // ---- the shocks ----
    //
    // Every mote that died recently is a sphere to get out of, opening over
    // its life and fading as it goes. Same shape as the rock field and the
    // same gradient steering, so a swarm flying through its own dead reads as
    // one flowing round obstacles rather than as one ignoring them.
    var flare = 0.0;
    for (var w: u32 = 0u; w < WAVES; w = w + 1u) {
        let wv = waves[w].at;
        let age = p.time - wv.w;
        if (age < 0.0 || age > WAVE_LIFE) { continue; }
        let off = me - wv.xyz;
        let dr = max(length(off), 1e-4);
        let fade = 1.0 - age / WAVE_LIFE;
        // ---- what it LIGHTS ----
        //
        // Outside the push, and reaching further than it, because a fireball
        // throws light further than it throws anything else. Squared into its
        // own reach and squared again in the fade, so a mote standing in one
        // is washed out, one at the edge of it is untouched, and a fire dies
        // faster than it fills.
        if (dr < WAVE_LIGHT_R) {
            let near = 1.0 - dr / WAVE_LIGHT_R;
            flare = flare + WAVE_LIGHT * fade * fade * near * near;
        }
        // ---- and what it SHOVES ----
        //
        // Opens fast and stops, which is what a shock does, and is the same
        // `sqrt` curve a blast's radius already uses.
        let grow = sqrt(age / WAVE_LIFE);
        let r = WAVE_R * grow * m.pos_scale.w;
        if (dr < r) {
            acc = acc + (off / dr) * (1.0 - dr / r) * WAVE_PUSH * fade;
        }
    }

    // ---- biting ----
    //
    // A mote in contact throws sparks off the plating. The CPU cannot be told
    // that it happened (nothing here ever goes back), and it does not need to
    // be: the chewers are what actually take cells off, and this is what says
    // on screen that the cloud is the reason. Gated hard, because thousands
    // of motes are in contact at once and the ring is shared with everything
    // that dies.
    if (!vein && phase == PH_ATTACK && dist < tgt.w * 1.10 && hash(i * 40503u + p.tick) > BITE_CHANCE) {
        let base = atomicAdd(&counter.gpu, BITE_SPARKS);
        for (var k: u32 = 0u; k < BITE_SPARKS; k = k + 1u) {
            let slot = p.spark_base + ((base + k) % p.spark_cap);
            let h = vec3<f32>(hash(base + k + 71u), hash(base + k + 137u), hash(base + k + 251u)) - 0.5;
            var sp: Spark;
            // Off the HULL, not off the mote: a spark struck from armour
            // starts where the armour is, which is a little further in than
            // the thing chewing it.
            sp.pos_life = vec4<f32>(me + dir * p.hull.w * 0.06, 0.12 + 0.20 * hash(base + k + 3u));
            sp.vel_size = vec4<f32>(h * 7.0 - dir * 2.0, m.pos_scale.w * 0.09);
            // Orange, and over one: this is plating being cut, so it is the
            // colour a hull burns and not the green a mote bleeds.
            let hot = 0.7 + 0.7 * hash(base + k + 17u);
            sp.colour_seed = vec4<f32>(3.0 * hot, 1.2 * hot, 0.25 * hot, hash(base + k + 29u));
            sparks[slot] = sp;
        }
    }

    // Separation, from a few others picked by stride.
    var sep = vec3<f32>(0.0);
    for (var k: u32 = 1u; k <= p.nbrs; k = k + 1u) {
        let j = (i + k * 7919u) % p.count;
        let off = me - motes[j].pos_scale.xyz;
        let d2 = dot(off, off) + 1e-5;
        if (d2 < 0.36) { sep = sep + off / d2; }
    }
    acc = acc + sep * 0.15;

    // Not through the hull, and measured against the HULL rather than against
    // whatever this mote's focus happens to be.
    //
    // This is where the balls came from. `dist` and `dir` are relative to the
    // FOCUS, which for an attacking mote is the ship and for a vein mote is a
    // point on its route: guarding a vein against its own route point pushed
    // it out to a shell of the SHIP's radius round that point, at up to a
    // hundred and forty against a pull of at most thirteen, so it could never
    // get in. Every vein settled onto a sphere three and a half units across
    // centred on wherever its route had reached, which is a ball, and the
    // routes run between rocks, so the balls sat on the rocks. A guard that
    // silently changed what it was guarding against the day the focus became
    // a variable.
    let to_ship = tgt.xyz - me;
    let ship_d = max(length(to_ship), 1e-4);
    let inside = tgt.w * 1.05 - ship_d;
    if (inside > 0.0) { acc = acc - (to_ship / ship_d) * inside * 40.0; }

    // The drag is raised to the STEP, so a frozen cloud (dt of nought) keeps
    // its speed: at a flat 0.985 a pass, ten seconds of freeze restarted the
    // whole swarm at a crawl.
    var v = (m.vel_seed.xyz + acc * p.dt) * pow(0.985, p.dt * 60.0);
    let s = length(v);
    // Fast enough to CROSS, and no faster. The carriers stand fifty to
    // seventy five units off, and at the three to six units a second the
    // swarm once allowed a fighter took twenty seconds to reach the fight and
    // the cloud round the ship never built at all.
    //
    // HALVED from the eight to sixteen that fixed that, which made the
    // transit five seconds and everything after it a blur: a mote arrived,
    // crossed the ring and was gone again before a player could pick it out,
    // and a gun laid on one was laid on where it had been. Four to eight puts
    // the transit at ten seconds and leaves the fighting legible, which is
    // what the speed is actually for. It is the CAP, so nothing else in the
    // tick changes: the pulls and the swirl are what they were, and a mote
    // simply stops gaining once it is here.
    let vmax = 4.0 + 4.0 * hash(u32(seed * 1000.0));
    if (s > vmax) { v = v * (vmax / s); }
    m.vel_seed = vec4<f32>(v, seed);
    var at = me + v * p.dt;
    // And nothing is ever INSIDE a rock. The steering above turns a mote and
    // steering can be beaten: a mote shoved by a blast, or launched from a
    // carrier that has drifted over one, arrives with more speed than a
    // gradient can take off it in a tick. This is the guarantee, and it is
    // cheap because it does nothing at all in the normal case.
    for (var r: u32 = 0u; r < p.rocks; r = r + 1u) {
        let rk = p.rock[r];
        let off = at - rk.xyz;
        let dr = length(off);
        if (dr < rk.w && dr > 1e-4) {
            let nrm = off / dr;
            at = rk.xyz + nrm * rk.w;
            // Slide along the surface rather than stopping dead on it.
            v = v - nrm * min(dot(v, nrm), 0.0);
        }
    }
    m.vel_seed = vec4<f32>(v, seed);
    m.pos_scale = vec4<f32>(at, m.pos_scale.w);

    // ---- what the light does to it ----
    //
    // Read where the mote IS rather than where it is going, because that is
    // where the field was counted from this tick. It is a tick behind by
    // construction and that is nothing: a mote crosses a fifth of a unit in a
    // tick and a cell is several across.
    let lf = light_at(me);
    // ---- and what a WOUND does ----
    //
    // It COOLS, and that is the whole of this. It used to be a throb: a sine
    // on the mote's own rate and phase, lit for as long as `hp` was under one,
    // which is until it got home and was rebuilt. So a pass through a beam
    // left every mote it touched glowing for the rest of its life, and a swarm
    // in which everything that had ever been hit was still on fire cannot say
    // which of them was hit just now. The pulse was standing in for a signal
    // in TIME, and a wound that flares and fades is that signal for real.
    //
    // The char is not here and must not be: what is left once the fire is out
    // is a burnt body, and a burnt body does not un-burn itself. That rides
    // `hp`, which the draw already has, exactly as the hull's crust rides its
    // own dead cell rather than its heat.
    heat = max(heat - p.dt / MOTE_COOL, 0.0);
    m.shade = vec4<f32>(lf.x, lf.y, heat, min(flare, 3.0));
    motes[i] = m;
}
