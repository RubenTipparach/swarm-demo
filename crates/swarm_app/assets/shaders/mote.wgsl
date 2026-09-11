// Draw one mote mesh once per mote, reading the swarm buffer as instance data.
//
// Every mote is the same greedy meshed voxel body, turned to face the way it
// is flying and scaled by its own size, wearing the chitin normal map in its
// own tangent frame. Lit by the scene's own key light so the cloud has shape
// at no cost: the PBR pipeline is for hulls.
//
// The instance attributes sit at 8, 9 and 10, clear of every slot the mesh
// itself may fill: position 0, normal 1, uv 2, uv1 3, TANGENT 4, colour 5.
// They were at 3 and 4 once, which is fine for a cube and collides the moment
// the mesh carries tangents.
//
// TWO CHANNELS, and the whole of the shading here is which of them a term
// belongs in. The LIT channel is light that arrived: the key, the fill, the
// specular, and every one of them is attenuated by how much of the swarm
// stands between this mote and where that light came from. The EMISSIVE
// channel is light this mote MAKES, and nothing in the cloud can take it
// away. A lamp does not go out because the thing next to it is in shadow.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_clip}
#import bevy_pbr::mesh_view_bindings::lights

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(8) i_pos_scale: vec4<f32>,
    @location(9) i_vel_seed: vec4<f32>,
    // What the swarm's own tick worked out about the light here: x is how
    // much of the sun reaches this mote through the rest of the cloud, y how
    // much of the sky does, z how much light it makes of its own.
    @location(10) i_shade: vec4<f32>,
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
    // x = the sun that got through, y = the sky that got through.
    @location(5) shade: vec2<f32>,
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

/// The key, from the SCENE's own light rather than from a number written down
/// here.
///
/// It used to be a hard coded `(0.4, 0.8, 0.3)` against a sun the app aims
/// along `(0.42, 0.66, -0.62)`, which is a cloud lit from one side of the sky
/// and a fleet lit from the other. It never showed while the swarm had no
/// shading worth the name; it would show the moment it had, because a shadow
/// cast one way and a highlight the other is the one thing an eye cannot
/// forgive. The view bind group is already bound for this draw, so the light
/// is simply there to be read, and the density field marches along the same
/// vector the app hands the swarm.
fn key_light() -> vec3<f32> {
    if (lights.n_directional_lights > 0u) {
        return normalize(lights.directional_lights[0].direction_to_light);
    }
    return normalize(vec3<f32>(0.42, 0.66, -0.62));
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
    // The lit marker rides in the vertex colour's ALPHA: nought means this
    // cell is its own light. There is nowhere else to put it, because nine
    // thousand motes are one instanced draw and a second mesh for their
    // engines would be a second draw per mote.
    //
    // How hard it burns is the mote's, not the vertex's: the tick works it
    // out once from the speed and this reads it, rather than two hundred
    // vertices each taking the length of the same velocity.
    out.glow = (1.0 - vertex.color.a) * vertex.i_shade.z;
    out.shade = vertex.i_shade.xy;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let key = key_light();
    let n0 = normalize(in.normal);
    let t = normalize(in.tangent.xyz);
    let bt = in.tangent.w * cross(n0, t);
    // The map is +Y up, the OpenGL convention the generator writes, and it is
    // tiled at half the rate of a finish so a scale spans two cells.
    let m = textureSample(chitin, chitin_sampler, in.uv * 0.5).xyz * 2.0 - 1.0;
    let n = normalize(t * m.x + bt * m.y + n0 * m.z);

    // ---- the lit channel ----
    //
    // Every term here is light that came from somewhere else, so every term
    // is attenuated by what the swarm did to it on the way. The sun is one
    // direction, so what stands in its way is the cloud along that line; the
    // sky and the bounce are everywhere, so what stands in their way is the
    // cloud immediately round this mote. That is the whole of the self
    // shadowing, and it is why the cloud stops being a flat sheet of lit
    // specks the moment it gets thick: a mote on the near face of a clump is
    // as bright as it ever was, and one behind ten thousand of its own kind
    // is nearly out.
    let sun = in.shade.x;
    let sky = in.shade.y;
    let direct = max(dot(n, key), 0.0) * sun;
    let fill = max(dot(n, -key), 0.0) * sky;
    // A little specular off the wet looking chitin, which is the sun again
    // and so goes out with it.
    let h = normalize(key + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, h), 0.0), 24.0) * 0.35 * sun;
    let body = in.color.rgb * (0.22 * sky + 0.78 * direct + 0.15 * fill) + vec3<f32>(spec);

    // ---- the emissive channel ----
    //
    // ADDED, never mixed. It used to replace the lit result wherever it was
    // over one, which made a glow and a shadow two settings of the same knob;
    // with the cloud shading itself that is exactly backwards, because the
    // motes whose own lamps are worth looking at are the ones buried deepest
    // in it. Nothing above touches this line: a mote in the dark heart of the
    // swarm is a dark body with its drive still lit, which is the picture.
    return vec4<f32>(body + in.color.rgb * in.glow, 1.0);
}
