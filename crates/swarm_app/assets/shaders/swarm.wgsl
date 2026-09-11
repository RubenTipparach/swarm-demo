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
    grid_n: u32,            // cells along one side of the density field
    pad2: u32,
    // The box the density field covers: min corner xyz, and how many world
    // units one cell is in w. It rides with the flagship, because that is
    // where the fight is.
    grid: vec4<f32>,
    // Which way the sun is (xyz, pointing AT it) and how much of a cell's
    // face one mote covers (w). The app owns both: the same vector it aims
    // the scene's key light along, so what the swarm shadows itself against
    // is the light it is actually lit by.
    sun: vec4<f32>,
    // Pairs: even = from.xyz + radius, odd = to.xyz + spare.
    shot: array<vec4<f32>, 64>,
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
};

struct Mote {
    pos_scale: vec4<f32>,   // xyz, w = drawn scale, 0 while dead
    vel_seed: vec4<f32>,    // xyz, w = seed in 0..1
    state: vec4<f32>,       // x = respawn countdown, y = the scale it had,
                            // z = which mothership it flies from,
                            // w = the run clock: how long is left on this leg,
                            //     positive boring in and negative breaking off
    // What the LIGHT does to this mote, which is the picture's state and not
    // the simulation's: x = how much of the sun reaches it through the cloud,
    // y = how much of the sky does, z = how much light it makes of its own,
    // w = spare. Written here and read by `mote.wgsl` as one more instance
    // attribute, because the draw cannot afford to work any of it out: the
    // mote buffer IS the instance buffer, so a field the tick fills is a
    // field the vertex shader already has.
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
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var<storage, read_write> motes: array<Mote>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> sparks: array<Spark>;
@group(0) @binding(3) var<storage, read_write> counter: Counter;
// How many motes stand in each cell of the field, counted fresh every tick.
@group(0) @binding(4) var<storage, read_write> density: array<atomic<u32>>;
// And what the light makes of that: x = how much of the sun reaches the cell
// through the rest of the swarm, y = how much of the sky does.
@group(0) @binding(5) var<storage, read_write> light: array<vec2<f32>>;

const SPARKS_PER_MOTE: u32 = 5u;
const RESPAWN: f32 = 0.9;

/// How many sparks a mote throws off the plating when it bites, and how often
/// a mote in contact takes one. Two numbers rather than one because the ring
/// is shared: the swarm holds thousands of motes against a hull at once, and
/// every one of them biting every tick would spend the whole ring in a frame
/// and leave nothing for anything that dies.
const BITE_SPARKS: u32 = 3u;
const BITE_CHANCE: f32 = 0.975;

/// How far out from a rock a mote starts turning, as a share of its radius.
const ROCK_MARGIN: f32 = 0.55;

/// What share of the swarm winds round a rock instead of going for the ship.
const VEIN_SHARE: f32 = 0.22;

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
            m.state.w = 0.9 + 2.6 * hash(i + 31u);
        }
        motes[i] = m;
        return;
    }

    let me = m.pos_scale.xyz;

    // Did anything reach it this tick? First one wins; a mote dies once.
    var killed = false;
    for (var s: u32 = 0u; s < p.shots; s = s + 1u) {
        let a = p.shot[s * 2u];
        let b = p.shot[s * 2u + 1u];
        if (capsule_hit(a.xyz, b.xyz, a.w, me)) { killed = true; break; }
    }
    if (killed) {
        // Its own colour, thrown out from where it was. The ring is claimed
        // with one atomic for the whole burst rather than one each, so a
        // mote's sparks stay together in the buffer and cannot interleave
        // with another's half written ones.
        let base = atomicAdd(&counter.gpu, SPARKS_PER_MOTE);
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
        m.state.y = max(m.pos_scale.w, m.state.y);
        m.state.x = RESPAWN;
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
    var focus = p.hull.xyz;
    var focus_r = p.hull.w;
    var vein = false;
    if (p.rocks > 0u && hash(sh + 907u) < VEIN_SHARE) {
        let rk = p.rock[sh % p.rocks];
        focus = rk.xyz;
        focus_r = rk.w;
        vein = true;
    }

    let to_hull = focus - me;
    let dist = max(length(to_hull), 0.001);
    let dir = to_hull / dist;

    // ---- the run clock ----
    //
    // A mote used to want ONE standoff for ever, and one standoff plus one
    // swirl axis is a torus: the cloud settled into a donut round the ship
    // and stayed there. A strike craft makes RUNS. The clock counts down the
    // current leg and flips at the end of it, so a mote bores in to contact,
    // breaks away well clear, and comes back round.
    var leg = abs(m.state.w) - p.dt;
    var inbound = m.state.w > 0.0;
    if (leg <= 0.0) {
        inbound = !inbound;
        let r = hash(i * 2246822519u + p.tick);
        // In is short and committed; the break is longer, because a pass
        // that turned round the moment it arrived would never get clear
        // enough for the next one to read as an approach.
        if (inbound) { leg = 1.3 + 1.7 * r; } else { leg = 1.6 + 2.2 * r; }
    }
    m.state.w = select(-leg, leg, inbound);

    // Where it wants to be on this leg. In, that is CONTACT, just off the
    // plating, which is what puts the swarm on the ship instead of in a ring
    // round it. Out, it is several lengths clear.
    let press = focus_r * (0.86 + 0.26 * hash(sh));
    let clear = focus_r * (3.0 + 3.6 * hash(sh + 3u));
    var want = select(clear, press, inbound);
    // A vein does not make runs. It HOLDS, close in, at its own radius, so
    // the ribbon stays wound round the rock instead of breathing in and out
    // of it.
    if (vein) { want = focus_r * (1.16 + 0.34 * hash(sh + 71u)); }

    // The pull is capped, or a fighter fifty units out would accelerate at
    // fifty and arrive as a bullet. It is the CAP that makes the approach
    // read as a flight rather than as a teleport.
    var acc = dir * clamp(dist - want, -8.0, 8.0) * 1.6;

    // ---- the swirl, about the mote's OWN axis ----
    //
    // This is the other half of the donut, and the bigger half. The swirl
    // used to be `cross(dir, up)` with one global up, so every mote in the
    // swarm circulated about the same axis: a torus by construction, however
    // the standoffs were spread. An axis per mote, from its own seed, gives
    // orbits at every inclination, which is a sphere of traffic rather than
    // a ring.
    var ax = vec3<f32>(hash(sh + 11u), hash(sh + 23u), hash(sh + 41u)) - 0.5;
    ax = ax / max(length(ax), 1e-4);
    var swirl = cross(dir, ax);
    let sl = length(swirl);
    if (sl > 1e-4) { swirl = swirl / sl; } else { swirl = vec3<f32>(0.0, 1.0, 0.0); }
    // Harder on the way in, so a pass CURVES across the hull rather than
    // arriving down the radius like a dart.
    // A vein runs FAST along its own orbit, which is what turns a set of
    // circles into a stream somebody can see moving.
    acc = acc + swirl * select(select(1.5, 3.6, inbound), 7.5, vein);

    // ---- the weave ----
    //
    // A lateral oscillation about the third axis of the mote's own frame, at
    // its own rate and its own phase. Without it a mote flies a clean arc and
    // ten thousand clean arcs read as a machine; with it the same cloud has
    // texture at the scale of a single craft.
    let wv = cross(dir, swirl);
    let rate = 1.6 + 3.4 * hash(sh + 57u);
    acc = acc + wv * sin(p.time * rate + seed * 71.0) * select(2.8, 0.9, vein);

    let jit = vec3<f32>(hash(i * 3u + u32(p.time * 7.0)), hash(i * 5u + u32(p.time * 5.0)), hash(i * 7u + u32(p.time * 3.0))) - 0.5;
    acc = acc + jit * 1.5;

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
            let push = clamp((margin - sdf) / max(margin, 1e-4), 0.0, 2.0);
            acc = acc + nrm * push * push * 38.0;
            // And take the INTO component off what it already has, or a mote
            // arriving fast carries its own momentum through the rock before
            // the push can turn it.
            let into = min(dot(m.vel_seed.xyz, nrm), 0.0);
            acc = acc - nrm * into * 3.0;
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
    if (!vein && inbound && dist < p.hull.w * 1.10 && hash(i * 40503u + p.tick) > BITE_CHANCE) {
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

    // Not through the hull. A sphere for now.
    let inside = p.hull.w * 1.05 - dist;
    if (inside > 0.0) { acc = acc - dir * inside * 40.0; }

    var v = (m.vel_seed.xyz + acc * p.dt) * 0.985;
    let s = length(v);
    // Fast enough to CROSS. The carriers stand fifty to seventy five units
    // off now, and at the three to six units a second this used to allow, a
    // fighter took twenty seconds to reach the fight and the cloud round the
    // ship never built at all. Eight to sixteen makes the transit five
    // seconds, which is a supply line a player can watch working.
    let vmax = 8.0 + 8.0 * hash(u32(seed * 1000.0));
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
    // And how much light it makes of its OWN, which the shading above has
    // nothing to say about. A drive burns harder the faster it is going, so a
    // launch is a flare and a fighter holding station is an ember. It is
    // worked out here rather than in the vertex shader because it is a fact
    // about the MOTE and there are two hundred vertices in one.
    let drive = 1.1 + 3.2 * clamp(length(v) / 14.0, 0.0, 1.0);
    m.shade = vec4<f32>(lf.x, lf.y, drive, 0.0);
    motes[i] = m;
}
