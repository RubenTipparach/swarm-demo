// Draw one mote mesh once per mote, reading the swarm buffer as instance data.
//
// Every mote is the same greedy meshed voxel body, turned to face the way it
// is flying and scaled by its own size. Lit by one hard coded key so the cloud
// has shape at no cost: the PBR pipeline is for hulls.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_clip}

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(3) i_pos_scale: vec4<f32>,
    @location(4) i_vel_seed: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
};

fn basis_from(v: vec3<f32>) -> mat3x3<f32> {
    let f = normalize(v + vec3<f32>(0.0, 0.0, 1e-4));
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(dot(f, up)) > 0.98) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let r = normalize(cross(up, f));
    let u = cross(f, r);
    return mat3x3<f32>(r, u, f);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let b = basis_from(vertex.i_vel_seed.xyz);
    let local = b * (vertex.position * vertex.i_pos_scale.w) + vertex.i_pos_scale.xyz;
    var out: VertexOutput;
    out.clip_position = mesh_position_local_to_clip(get_world_from_local(0u), vec4<f32>(local, 1.0));
    out.color = vertex.color;
    out.normal = b * vertex.normal;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let key = normalize(vec3<f32>(0.4, 0.8, 0.3));
    let n = normalize(in.normal);
    let lit = 0.25 + 0.75 * max(dot(n, key), 0.0) + 0.15 * max(dot(n, -key), 0.0);
    return vec4<f32>(in.color.rgb * lit, 1.0);
}
