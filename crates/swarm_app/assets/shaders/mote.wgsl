// Draw one mote mesh once per mote, reading the swarm buffer as instance data.
//
// Every mote is the same greedy meshed voxel body, turned to face the way it
// is flying and scaled by its own size, wearing the chitin normal map in its
// own tangent frame. Lit by one hard coded key so the cloud has shape at no
// cost: the PBR pipeline is for hulls.
//
// The instance attributes sit at 8 and 9, clear of every slot the mesh
// itself may fill: position 0, normal 1, uv 2, uv1 3, TANGENT 4, colour 5.
// They were at 3 and 4 once, which is fine for a cube and collides the moment
// the mesh carries tangents.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_clip}

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(8) i_pos_scale: vec4<f32>,
    @location(9) i_vel_seed: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

@group(3) @binding(0) var chitin: texture_2d<f32>;
@group(3) @binding(1) var chitin_sampler: sampler;

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
    out.tangent = vec4<f32>(b * vertex.tangent.xyz, vertex.tangent.w);
    out.uv = vertex.uv;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let key = normalize(vec3<f32>(0.4, 0.8, 0.3));
    let n0 = normalize(in.normal);
    let t = normalize(in.tangent.xyz);
    let bt = in.tangent.w * cross(n0, t);
    // The map is +Y up, the OpenGL convention the generator writes, and it is
    // tiled at half the rate of a finish so a scale spans two cells.
    let m = textureSample(chitin, chitin_sampler, in.uv * 0.5).xyz * 2.0 - 1.0;
    let n = normalize(t * m.x + bt * m.y + n0 * m.z);
    let lit = 0.22 + 0.78 * max(dot(n, key), 0.0) + 0.15 * max(dot(n, -key), 0.0);
    // A little specular off the wet looking chitin.
    let h = normalize(key + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, h), 0.0), 24.0) * 0.35;
    return vec4<f32>(in.color.rgb * lit + vec3<f32>(spec), 1.0);
}
