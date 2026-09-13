// Draw one mote mesh once per mote, reading the swarm buffer as instance data.
//
// Every mote is the same greedy meshed voxel body, turned to face the way it
// is flying and scaled by its own size, wearing the chitin normal map in its
// own tangent frame. Lit by the scene's own key light so the cloud has shape
// at no cost: the PBR pipeline is for hulls.
//
// The instance attributes sit at 8, 9, 10 and 11, clear of every slot the mesh
// itself may fill: position 0, normal 1, uv 2, uv1 3, TANGENT 4, colour 5.
// They were at 3 and 4 once, which is fine for a cube and collides the moment
// the mesh carries tangents.
//
// TWO CHANNELS, and the whole of the shading here is deciding which of them a
// term belongs in. The LIT channel is light that ARRIVED: the key, the fill,
// the specular, and every one of them is attenuated by how much of the swarm
// stands between this mote and where that light came from. The EMISSIVE
// channel is light this mote MAKES, its drives and its eyes and the wound in
// it, and nothing in the cloud can take any of that away. A lamp does not go
// out because the thing next to it is in shadow.

#import bevy_pbr::mesh_view_bindings::{view, lights}

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(8) i_pos_scale: vec4<f32>,
    @location(9) i_vel_seed: vec4<f32>,
    // How much of the mote is left, in x: one whole, nought about to come
    // apart.
    @location(10) i_life: vec4<f32>,
    // And what the cloud does to the light on it: x how much of the sun
    // reaches it through the rest of the swarm, y how much of the sky does,
    // z the beat a wound throbs on, w how much a mote that has just burst
    // nearby is lighting it.
    @location(11) i_shade: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    // How much of its OWN light this fragment makes. Nought for chitin, and
    // over one for a drive at transit speed.
    @location(4) glow: f32,
    // x = the sun that got through, y = the sky that got through, z = the
    // beat a wound throbs on, w = the fire of a burst standing next to it.
    @location(5) shade: vec4<f32>,
    // How hurt it is: nought whole, one about to come apart.
    @location(6) hurt: f32,
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

/// The key, off the SCENE's own light rather than a number written down here.
///
/// It was hard coded, on the grounds that this draw binds the view and the
/// chitin and nothing else. It binds more than that: the mesh VIEW bind group
/// is group nought, and the lights are binding one of it, so the sun the scene
/// is actually lit by is there to be read for nothing. That matters now the
/// swarm shadows itself, because the density field marches along the vector
/// the app publishes and this has to be the same one. Two numbers written down
/// in two places drift, and a cloud shadowed from one side of the sky with its
/// highlight on the other is the single thing an eye will not forgive.
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
    //
    // TONED DOWN. It was 1.1 to 4.3, which put every drive in the cloud
    // through the bloom threshold at any speed, so a million motes were a
    // million bloom sources and the swarm read as a green haze with bodies
    // somewhere in it. Half to just under two now: a fighter at rest is an
    // ember below the threshold, and only one at full transit crosses it.
    //
    // The seven is the swarm's own top speed and has to move with it. It was
    // fourteen against a cap of eight to sixteen; the cap is halved now, and
    // left alone this would have quietly taken the brightest drive in the
    // cloud down to 1.3 and put the flare at full transit under the bloom
    // threshold, which is the sentence above going silently false.
    let speed = length(vertex.i_vel_seed.xyz);
    out.glow = (1.0 - vertex.color.a) * (0.5 + 1.4 * clamp(speed / 7.0, 0.0, 1.0));
    out.shade = vertex.i_shade;
    out.hurt = clamp(1.0 - vertex.i_life.x, 0.0, 1.0);
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
    // HARSH, on purpose. The ambient floor and the back fill are both at forty
    // percent of what they once were (0.22 to 0.088, 0.15 to 0.06) and the
    // direct term is a full one: a mote is lit by a sun in vacuum, so the side
    // away from it is nearly black and the terminator is a hard line. The old
    // floor gave every body a grey lift that read as fog over the cloud.
    //
    // And every one of those terms is light that came from somewhere else, so
    // every one is attenuated by what the swarm did to it on the way. The sun
    // is one direction, so what stands in its way is the cloud along that
    // line; the sky and the bounce are everywhere, so what stands in theirs is
    // the cloud immediately round this mote. That is the whole of the self
    // shadowing, and it is what stops a thick cloud reading as a flat sheet of
    // lit specks: a mote on the near face of a clump is as bright as it ever
    // was, and one behind ten thousand of its own kind is nearly out.
    let sun = in.shade.x;
    let sky = in.shade.y;
    let shade = 0.088 * sky + max(dot(n, key), 0.0) * sun + 0.06 * max(dot(n, -key), 0.0) * sky;
    // ---- and a mote that burst beside this one is a LIGHT ----
    //
    // The one term in this channel that the field does NOT attenuate, and for
    // the reason the extinction exists at all: the sun and the sky arrive from
    // outside the cloud and have the whole of it to cross, and a fireball
    // fifteen feet away is INSIDE it, with nothing in between to stop. It is
    // also the only way an explosion buried in a dense swarm can be seen to be
    // buried in one, because the shading deliberately puts that part of the
    // cloud out: what says where a kill happened is the dozen bodies around it
    // coming up warm for a third of a second.
    //
    // It lights the BODY, in the body's own colour, so a violet bug goes warm
    // rather than white, and it sits inside the lit channel so the burn fades
    // it out along with everything else: a mote that is itself on fire is not
    // also being lit by its neighbour's.
    // A tighter, brighter specular off the wet looking chitin, since a harsh
    // key is what a highlight needs to read. It is the sun again, so it goes
    // out with the sun.
    let h = normalize(key + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.5 * sun;
    let flare = vec3<f32>(1.30, 0.86, 0.52) * in.shade.w;
    let body = in.color.rgb * (shade + flare) + vec3<f32>(spec);

    // ---- the emissive channel ----
    //
    // ADDED, never mixed. A lit cell used to REPLACE the shaded body wherever
    // it was over one, which made a glow and a shadow two settings of the same
    // knob; with the cloud shading itself that is backwards, because the motes
    // whose own lamps are worth looking at are the ones buried deepest in it.
    // Nothing above touches this line: a mote in the dark heart of the swarm
    // is a dark body with its drives still lit, which is the picture.
    //
    // A HURT mote BURNS, and it burns the colour everything else in this game
    // that has been damaged burns: hot yellow through orange, the hull's own
    // heat ramp.
    //
    // It was violet, on the reasoning that violet is what a mote bleeds. That
    // is the colour of the ANIMAL rather than the colour of an injury, and laid
    // on a body that is already violet chitin it is a bug that happens to be a
    // brighter purple: a player reads it as another kind of mote, not as one
    // they have just hit. Everything else that takes damage here goes orange,
    // from a chewed frigate to a carrier coming apart, so a damaged mote goes
    // orange too. What stays violet is the GORE that comes out when it finally
    // bursts: burning is what a hit does to it, bleeding is what is inside it.
    //
    // OVER THE BLOOM THRESHOLD, and it has to be. The first cut was a fifth of
    // this, and at the one hit a mote actually survives (`SHOT_BITE` is 0.6 off
    // a whole one, so the square is 0.36) it came out under one in every
    // channel and never bloomed at all. At these numbers that same single hit
    // clears white in the red and the wound flares.
    //
    // And it BEATS, on the clock the tick handed over in `shade.z`. A steady
    // glow is a colour; a pulse is an injury, and a pulse is what carries at
    // the one or two pixels a mote is usually drawn at.
    //
    // It burns on the CHITIN and nowhere else. The lit cells are a bug's eyes
    // and its drive, and those are the two things that say which way it is
    // facing and that it is alive at all: painting the burn over them puts out
    // the only marks on a mote that were ever readable, and a bug on fire from
    // eye to exhaust is a shape with no parts. The vertex colour's alpha is
    // already the marker for which cells are which, so the burn simply takes
    // the other half of it.
    let plate = in.color.a;
    let burn = clamp(in.hurt * in.hurt * in.shade.z, 0.0, 1.0) * plate;
    let wound = vec3<f32>(5.0, 2.2, 0.30) * burn;

    // ---- and a burn BEATS the shadow ----
    //
    // The lit channel is faded out under it rather than added to, which is the
    // one place in this shader where the two channels are not simply summed,
    // and it is deliberate: a mote that is on fire is lit by the fire. Adding
    // left the shading underneath still deciding how dark the body was, so the
    // same hit on the near face of the cloud and in the middle of it came out
    // as two different colours, and the ones in the middle, which is where the
    // fighting is, were the dim ones. What a player has just hit should not
    // depend on where the cloud happened to be standing.
    return vec4<f32>(body * (1.0 - burn) + in.color.rgb * in.glow + wound, 1.0);
}
