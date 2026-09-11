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

#import bevy_pbr::mesh_view_bindings::view

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
    // How much of its OWN light this fragment makes. Nought for chitin, and
    // well over one for a drive at speed.
    @location(4) glow: f32,
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
    // WORLD to clip, straight off the view, exactly as the spark shader does
    // it. This used to be `mesh_position_local_to_clip(get_world_from_local(0u), ...)`,
    // and that hard coded `0u` is the bug the whole swarm was flipping on.
    //
    // `get_world_from_local` indexes Bevy's MESH INSTANCE buffer, which is
    // built per frame per view and holds every batched mesh in the scene. Slot
    // nought is not this entity: it is whichever mesh the batcher happened to
    // put first, and the batcher orders by pipeline and by distance, so the
    // answer CHANGES as the camera moves. The whole swarm was being drawn in
    // some other object's frame, every mote sharing the one wrong matrix, so
    // they all flipped together the moment the sort order changed and the
    // cloud looked like it was in a different reference frame from the world.
    // Turning the camera to a certain angle is exactly what reorders the sort.
    //
    // A mote position is already in world space: the compute pass writes world
    // coordinates and the entity's own transform is the identity. So there was
    // never a model matrix to look up, which is why reading the wrong one went
    // unnoticed for as long as slot nought happened to hold an identity.
    out.clip_position = view.clip_from_world * vec4<f32>(local, 1.0);
    out.color = vertex.color;
    out.normal = b * vertex.normal;
    out.tangent = vec4<f32>(b * vertex.tangent.xyz, vertex.tangent.w);
    out.uv = vertex.uv;
    // The lit marker rides in the vertex colour's ALPHA: nought means this
    // cell is its own light. There is nowhere else to put it, because nine
    // thousand motes are one instanced draw and a second mesh for their
    // engines would be a second draw per mote.
    //
    // Engines burn harder the faster it is going, so a launch is a flare and
    // a fighter holding station is an ember.
    let speed = length(vertex.i_vel_seed.xyz);
    out.glow = (1.0 - vertex.color.a) * (1.1 + 3.2 * clamp(speed / 14.0, 0.0, 1.0));
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
    let shade = 0.22 + 0.78 * max(dot(n, key), 0.0) + 0.15 * max(dot(n, -key), 0.0);
    // A little specular off the wet looking chitin.
    let h = normalize(key + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, h), 0.0), 24.0) * 0.35;
    // A lit cell is not shaded at all: it makes its own light, so the key has
    // nothing to say about it and the sum is well over one on purpose, which
    // is what puts a drive through the bloom threshold.
    let body = in.color.rgb * shade + vec3<f32>(spec);
    return vec4<f32>(mix(body, in.color.rgb * in.glow, min(in.glow, 1.0)), 1.0);
}
