// Every spark as one camera facing quad, in one instanced draw.
//
// Additive and unlit: a spark is light, not a surface, so nothing about the
// scene's lighting reaches it and everything it touches gets brighter. It
// samples the ember atlas, which is the same texture a wound burns with, so a
// piece of a hull in the air and the hole it came out of are the same fire.
//
// Colours come out well over one on purpose. The camera is HDR and bloom
// thresholds just under white after tone mapping, so a spark has to CLEAR
// that to glow rather than merely to be bright.

#import bevy_pbr::mesh_view_bindings::view

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(8) i_pos_life: vec4<f32>,
    @location(9) i_vel_size: vec4<f32>,
    @location(10) i_colour_seed: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) colour: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@group(3) @binding(2) var ember: texture_2d<f32>;
@group(3) @binding(3) var ember_sampler: sampler;

const ATLAS: f32 = 4.0;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let life = v.i_pos_life.w;
    if (life <= 0.0) {
        // A spent slot. Collapsed to a point behind the eye rather than
        // skipped, because an instanced draw has no way to skip one.
        out.clip_position = vec4<f32>(0.0, 0.0, -10.0, 1.0);
        out.colour = vec3<f32>(0.0);
        out.uv = vec2<f32>(0.0);
        return out;
    }
    // The camera's own right and up, so the quad faces the eye whatever the
    // orbit is doing. No per spark rotation: a billboard that rolled with the
    // camera would make a whole burst spin together.
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    // Sparks shrink as they go out, which reads as cooling rather than as
    // fading: a light that dims without shrinking looks like a fog patch.
    let size = v.i_vel_size.w * min(1.0, life * 2.4);
    let world = v.i_pos_life.xyz + (right * v.position.x + up * v.position.y) * size;
    out.clip_position = view.clip_from_world * vec4<f32>(world, 1.0);

    // One of the atlas's sixteen tiles, by the spark's own seed.
    let t = floor(v.i_colour_seed.w * 15.999);
    let tile = vec2<f32>(t % ATLAS, floor(t / ATLAS)) / ATLAS;
    out.uv = tile + (v.position.xy + 0.5) / ATLAS;
    out.colour = v.i_colour_seed.rgb * min(1.0, life * 3.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let e = textureSample(ember, ember_sampler, in.uv);
    // The ember's STRUCTURE, not its colour.
    //
    // The atlas is a burn seen on a hull: mostly char, with molten patches in
    // it, and the wound multiplies it by a heat ramp so the char is what shows
    // once a hole has cooled. Multiplied into a spark it does the same thing
    // and it is wrong there: a spark IS the molten part, so sampling a random
    // point of a mostly black tile put most of a burst out. Its luminance
    // modulates instead, over a floor, which keeps the mottling and never
    // takes a spark below the colour it was thrown with.
    let grain = 0.45 + 1.15 * dot(e.rgb, vec3<f32>(0.3333));
    // A round falloff over the tile, so a spark is a glowing dot rather than a
    // square of texture.
    let d = length(in.uv * ATLAS - floor(in.uv * ATLAS) - 0.5);
    let mask = clamp(1.0 - d * 2.0, 0.0, 1.0);
    let a = mask * mask;
    return vec4<f32>(in.colour * grain * a * 2.2, a);
}
