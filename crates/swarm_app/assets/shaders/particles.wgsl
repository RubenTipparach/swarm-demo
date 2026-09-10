// One tick of every spark, one thread each.
//
// A spark is inert: it carries where it is, how fast, how big, what colour and
// how long it has left, and this integrates it. Nothing here decides anything,
// which is why it can be a fixed ring buffer that two writers append to and
// nobody ever compacts: a dead spark is one with no life left, it is skipped
// here and drawn at zero size, and the ring writes over it in its own time.

struct Params {
    dt: f32,
    time: f32,
    count: u32,
    nbrs: u32,
    hull: vec4<f32>,
    tick: u32,
    shots: u32,
    spark_base: u32,
    spark_cap: u32,
    shot: array<vec4<f32>, 64>,
};

struct Spark {
    pos_life: vec4<f32>,
    vel_size: vec4<f32>,
    colour_seed: vec4<f32>,
};

@group(0) @binding(0) var<storage, read_write> sparks: array<Spark>;
@group(0) @binding(1) var<uniform> p: Params;

/// Space has no air, so nothing here is drag: what this models is a spark
/// throwing its own mass away as it burns, which slows it a little and is
/// what keeps a burst from reading as a firework of straight lines.
const SLOW: f32 = 1.6;

@compute @workgroup_size(256)
fn tick(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= arrayLength(&sparks)) { return; }
    var s = sparks[i];
    if (s.pos_life.w <= 0.0) { return; }
    let life = s.pos_life.w - p.dt;
    s.pos_life = vec4<f32>(s.pos_life.xyz + s.vel_size.xyz * p.dt, life);
    s.vel_size = vec4<f32>(s.vel_size.xyz * max(0.0, 1.0 - SLOW * p.dt), s.vel_size.w);
    sparks[i] = s;
}
