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
    // Pairs: even = from.xyz + radius, odd = to.xyz + spare.
    shot: array<vec4<f32>, 64>,
};

struct Mote {
    pos_scale: vec4<f32>,   // xyz, w = drawn scale, 0 while dead
    vel_seed: vec4<f32>,    // xyz, w = seed in 0..1
    state: vec4<f32>,       // x = respawn countdown, y = the scale it had, zw spare
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

const SPARKS_PER_MOTE: u32 = 5u;
const RESPAWN: f32 = 0.9;

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

/// A point on the shell the swarm holds, for a mote coming back.
fn shell_point(seed: u32) -> vec3<f32> {
    var d = vec3<f32>(hash(seed) - 0.5, (hash(seed + 7u) - 0.5) * 0.5, hash(seed + 19u) - 0.5);
    let l = max(length(d), 1e-4);
    d = d / l;
    // Just outside the standoff the living ones hold, not far outside it: a
    // shell at four radii is a cloud the camera has to sit inside of.
    return p.hull.xyz + d * p.hull.w * (2.0 + 0.7 * hash(seed + 31u));
}

@compute @workgroup_size(256)
fn tick(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= p.count) { return; }
    var m = motes[i];
    let seed = m.vel_seed.w;

    // Dead and waiting: count down, then come back at the shell.
    if (m.state.x > 0.0) {
        m.state.x = m.state.x - p.dt;
        if (m.state.x <= 0.0) {
            m.state.x = 0.0;
            m.pos_scale = vec4<f32>(shell_point(i * 2654435761u + p.tick), m.state.y);
            m.vel_seed = vec4<f32>(0.0, 0.0, 0.0, seed);
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
            // Gore, not fire: a mote is wet, so it throws its own green and
            // violet rather than the orange a hull does. Over one, because
            // this is what bloom is for.
            let hot = 0.6 + 0.8 * hash(base + k + 11u);
            sp.colour_seed = vec4<f32>(1.1 * hot, 2.6 * hot, 0.7 * hot, hash(base + k + 13u));
            sparks[slot] = sp;
        }
        m.state.y = max(m.pos_scale.w, m.state.y);
        m.state.x = RESPAWN;
        m.pos_scale = vec4<f32>(me, 0.0);
        motes[i] = m;
        return;
    }

    // Appetite: toward the hull, and round it. Each mote wants its own
    // standoff, so the cloud is a shell round the ship rather than a point.
    let to_hull = p.hull.xyz - me;
    let dist = max(length(to_hull), 0.001);
    let dir = to_hull / dist;
    let want = p.hull.w * (1.12 + 0.75 * hash(u32(seed * 65535.0)));
    var acc = dir * clamp(dist - want, -3.0, 3.0) * 1.2;
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let swirl = normalize(cross(dir, up) + vec3<f32>(1e-4, 0.0, 0.0));
    acc = acc + swirl * 2.2;
    let jit = vec3<f32>(hash(i * 3u + u32(p.time * 7.0)), hash(i * 5u + u32(p.time * 5.0)), hash(i * 7u + u32(p.time * 3.0))) - 0.5;
    acc = acc + jit * 1.5;

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
    let vmax = 3.0 + 3.0 * hash(u32(seed * 1000.0));
    if (s > vmax) { v = v * (vmax / s); }
    m.vel_seed = vec4<f32>(v, seed);
    m.pos_scale = vec4<f32>(me + v * p.dt, m.pos_scale.w);
    motes[i] = m;
}
