// One tick of the swarm, one thread per mote.
//
// M0: every mote steers on the same three things, none of which is a rule yet.
// A pull toward the hull (the stand in for a flow field), a swirl so the cloud
// has structure, and separation from a handful of other motes picked by stride,
// which stands in for the spatial hash. The hull is a sphere here and nothing
// more: the SDF that lets a mote crawl the actual silhouette is M1.

struct Params {
    dt: f32,
    time: f32,
    count: u32,
    nbrs: u32,
    hull: vec4<f32>,   // centre xyz, radius w
    _pad: vec4<f32>,
};

struct Mote {
    pos: vec4<f32>,   // xyz, w = scale
    vel: vec4<f32>,   // xyz, w = seed
};

@group(0) @binding(0) var<storage, read_write> motes: array<Mote>;
@group(0) @binding(1) var<uniform> p: Params;

fn hash(n: u32) -> f32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    x = (x >> 22u) ^ x;
    return f32(x) / 4294967295.0;
}

@compute @workgroup_size(256)
fn tick(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= p.count) { return; }
    var m = motes[i];
    let me = m.pos.xyz;
    let seed = m.vel.w;

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
    // A little of its own mind, hashed off its seed and the clock.
    let jit = vec3<f32>(hash(i * 3u + u32(p.time * 7.0)), hash(i * 5u + u32(p.time * 5.0)), hash(i * 7u + u32(p.time * 3.0))) - 0.5;
    acc = acc + jit * 1.5;

    // Separation, from a few others picked by stride.
    var sep = vec3<f32>(0.0);
    for (var k: u32 = 1u; k <= p.nbrs; k = k + 1u) {
        let j = (i + k * 7919u) % p.count;
        let off = me - motes[j].pos.xyz;
        let d2 = dot(off, off) + 1e-5;
        if (d2 < 0.36) { sep = sep + off / d2; }
    }
    acc = acc + sep * 0.15;

    // Not through the hull. A sphere for now.
    let inside = p.hull.w * 1.05 - dist;
    if (inside > 0.0) { acc = acc - dir * inside * 40.0; }

    var v = (m.vel.xyz + acc * p.dt) * 0.985;
    let s = length(v);
    let vmax = 3.0 + 3.0 * hash(u32(seed * 1000.0));
    if (s > vmax) { v = v * (vmax / s); }
    m.vel = vec4<f32>(v, seed);
    m.pos = vec4<f32>(me + v * p.dt, m.pos.w);
    motes[i] = m;
}
