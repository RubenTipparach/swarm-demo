//! The scenery, spawned once and kept across every scene: the sky, the
//! stars, the sun, the planets, the camera, and the meshes that are rebuilt
//! every frame from state.

use crate::*;

/// Bodies sit in this band, outside the fight and inside the far plane.
pub(crate) const NEAR_BAND: f32 = 250.0;

pub(crate) const FAR_BAND: f32 = 660.0;

pub(crate) const FAR_PLANE: f32 = 6000.0;

pub(crate) const STAR_RADIUS: f32 = 4500.0;

pub(crate) const SKY_SIZE: usize = 256;

#[derive(Component)]
pub(crate) struct Showcase;

/// Where the sun is, pointing AT it.
///
/// One vector with two consumers, and they have to be the same one. The key
/// light is aimed along it, and the swarm marches its density field along it
/// to work out what of that light reaches a mote buried in the cloud. Lit
/// from one side of the sky and shadowed from the other is the single thing
/// an eye will not forgive, and it is exactly what two numbers written down
/// in two places drift into.
pub(crate) const SUN: Vec3 = Vec3::new(0.42, 0.66, -0.62);

pub(crate) fn spin_showcase(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    mut q: Query<&mut Transform, With<Showcase>>,
) {
    let dt = scene.step(&time);
    for mut xf in &mut q {
        xf.rotate_y(dt * 0.5);
    }
}

/// The scenery, spawned once and kept across every scene: the sky, the
/// stars, the sun, the planets, the camera, and the meshes that are rebuilt
/// every frame from state (the beams, the nav disc, the flames, the motes
/// and the sparks). `mark_keep` runs after it and tags all of it, so the
/// teardown between two fights knows what to leave alone.
pub(crate) fn spawn_backdrop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    scene: Res<SceneSpec>,
    headless: Option<Res<Headless>>,
) {
    spawn_fx_meshes(&mut commands, &mut meshes, &mut materials);
    let (sky, preset) = bake_sky(&mut images);
    spawn_stars(&mut commands, &mut meshes, &mut materials, &preset);
    spawn_sun_and_planets(&mut commands, &mut meshes, &mut materials);
    spawn_camera(&mut commands, &mut images, sky, &scene, headless.as_deref());
}

/// The meshes that are rebuilt every frame from state, and the two the GPU
/// draws once per mote and once per spark.
fn spawn_fx_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // The swarm's body: a drone, drawn once per mote off the GPU buffer.
    let drone = generate(Archetype::Drone, 1);
    spawn_mote_mesh(
        commands,
        meshes.add(to_mote_mesh(&greedy_mesh(&drone, None))),
    );

    // And one unit quad, drawn once per spark off the other half of it. The
    // quad is turned to face the eye in the shader; this is only its corners.
    spawn_spark_mesh(commands, meshes.add(Rectangle::new(1.0, 1.0)));

    // Five meshes rebuilt every frame, each one a function of where the eye
    // is: there are at most a few hundred vertices in any of them and nothing
    // to cache. They differ in exactly two things, so `live_mesh` takes those
    // two and the spawn is written once.
    let beam = live_mesh(commands, meshes, materials, AlphaMode::Add, None);
    commands.entity(beam.0).insert(BeamMesh);
    commands.insert_resource(BeamHandle(beam.1));

    let nav = live_mesh(commands, meshes, materials, AlphaMode::Add, None);
    commands.insert_resource(NavHandle(nav.1));

    // The sensors manager's furniture: lines, so additive like the disc.
    let furniture = live_mesh(commands, meshes, materials, AlphaMode::Add, None);
    commands.insert_resource(SensorHandle(furniture.1));

    // And its GROUND, which is the one thing in this scene that is alpha
    // blended rather than additive, and it has to be: what it says is that
    // unwatched space is DARK, and additive blending can only ever add light.
    // Its own entity for the same reason, because a blend mode is a material
    // and a material is a draw.
    let ground = live_mesh(commands, meshes, materials, AlphaMode::Blend, None);
    commands.insert_resource(GroundHandle(ground.1));

    // The flames are BACK FACE CULLED, which the nav disc is not, and that is
    // the whole difference in the look. Additive on a closed surface lays its
    // colour down twice per ray, once on the way in and once on the way out,
    // so the silhouette and the middle come out the same brightness and the
    // cone reads as a smear of light rather than as a shape. Culled, a ray
    // crosses one facet and the facet's own flat colour is what arrives.
    let flames = live_mesh(
        commands,
        meshes,
        materials,
        AlphaMode::Add,
        Some(bevy::render::render_resource::Face::Back),
    );
    commands.insert_resource(FlameHandle(flames.1));
}

/// One unlit mesh rebuilt every frame, by its blend mode and its culling.
///
/// Hands back the entity and the handle, because a caller wants the handle to
/// keep in a resource and the beams want the entity as well, to mark.
fn live_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    alpha_mode: AlphaMode,
    cull_mode: Option<bevy::render::render_resource::Face>,
) -> (Entity, Handle<Mesh>) {
    let mesh = meshes.add(empty_mesh());
    let e = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                alpha_mode,
                cull_mode,
                ..default()
            })),
            Transform::IDENTITY,
            bevy::camera::visibility::NoFrustumCulling,
        ))
        .id();
    (e, mesh)
}

/// The nebula, baked once on the CPU into a half float cubemap.
fn bake_sky(images: &mut Assets<Image>) -> (Handle<Image>, SkyPreset) {
    // ---- the sky ----
    //
    // Baked once, on the CPU, into a half float cubemap: a nebula is a slow
    // gradient across a dark range, and eight bits of it magnified four times
    // is a contour map. The same cubemap lights the hulls as an environment
    // map, so a shadowed flank picks up the colour of the sky it flies in.
    // `duel`, not `skirmish`. Skirmish is the archive's own green over near
    // black purple, and it is faithful to the scene it was cut from, but a
    // green nebula bright enough to see is a green cast over the whole
    // picture: the ships, the rocks and the swarm all sit in front of it and
    // all read against it. The blue over near black is the same graph with
    // two different colours in it, which is the only thing a mission is
    // allowed to vary, and it leaves the frame dark enough that a lit thing
    // is the brightest thing in it.
    let preset = SkyPreset::duel();
    let t0 = std::time::Instant::now();
    let texels = bake_cubemap(&preset, SKY_SIZE);
    {
        // Say what was baked, in numbers: a sky that comes out one colour is
        // a bake with no structure or a display that lifts its floor, and the
        // picture cannot tell those apart.
        let lum: Vec<f32> = texels
            .iter()
            .map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2])
            .collect();
        let (mut lo, mut hi, mut sum) = (f32::MAX, f32::MIN, 0.0f32);
        for &l in &lum {
            lo = lo.min(l);
            hi = hi.max(l);
            sum += l;
        }
        let floor = 0.2126 * preset.b[0] + 0.7152 * preset.b[1] + 0.0722 * preset.b[2];
        let gas = lum.iter().filter(|&&l| l > floor + 0.02).count();
        let dense = lum.iter().filter(|&&l| l > floor + 0.06).count();
        info!(
            "sky texels: luminance floor {:.4}, min {:.4}, mean {:.4}, max {:.4}; gas over {:.1}% of the sky, dense over {:.1}%",
            floor, lo, sum / lum.len() as f32, hi.max(lo), 100.0 * gas as f32 / lum.len() as f32, 100.0 * dense as f32 / lum.len() as f32
        );
    }
    let mut data = Vec::with_capacity(texels.len() * 8);
    for c in &texels {
        for v in [c[0], c[1], c[2], 1.0] {
            data.extend_from_slice(&to_half(v).to_le_bytes());
        }
    }
    let mut sky = Image::new(
        Extent3d {
            width: SKY_SIZE as u32,
            height: SKY_SIZE as u32,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    sky.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    let sky = images.add(sky);
    info!(
        "sky: {} faces of {}x{} baked in {:.0} ms",
        6,
        SKY_SIZE,
        SKY_SIZE,
        t0.elapsed().as_secs_f32() * 1000.0
    );

    (sky, preset)
}

/// The stars, as points on a shell that rides the eye.
fn spawn_stars(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    preset: &SkyPreset,
) {
    // The stars, as points on a shell that rides the eye.
    let stars = starfield(preset);
    let mut star_mesh = Mesh::new(PrimitiveTopology::PointList, RenderAssetUsages::default());
    star_mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        stars
            .iter()
            .map(|s| {
                [
                    s.dir[0] * STAR_RADIUS,
                    s.dir[1] * STAR_RADIUS,
                    s.dir[2] * STAR_RADIUS,
                ]
            })
            .collect::<Vec<_>>(),
    );
    star_mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        stars
            .iter()
            .map(|s| {
                let b = 0.6 + s.size * 0.5;
                [s.tint[0] * b, s.tint[1] * b, s.tint[2] * b, 1.0]
            })
            .collect::<Vec<_>>(),
    );
    commands.spawn((
        Mesh3d(meshes.add(star_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        })),
        Transform::IDENTITY,
        bevy::camera::visibility::NoFrustumCulling,
        AtInfinity,
    ));
    info!("stars: {}", stars.len());
}

/// One sun, one key light, two bodies.
fn spawn_sun_and_planets(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // ---- the backdrop: one sun, one key light, two bodies (skirmish) ----
    //
    // ONE vector, from `SUN`, and nothing copies it: the swarm's density field
    // marches along what the app publishes, and `mote.wgsl` takes its key off
    // this light itself through the view bind group it already binds. A cloud
    // shadowed from one side of the sky with its highlight on the other is
    // what two numbers written down in two places drift into.
    let sun = SUN.normalize();
    let sun_colour = Color::srgb_u8(0xff, 0xf0, 0xd2);
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            color: sun_colour,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, -sun)),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(FAR_BAND * 0.06).mesh().ico(3).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(40.0, 36.0, 28.0),
            ..default()
        })),
        Transform::from_translation(sun * FAR_BAND * 1.35),
        AtInfinity,
    ));
    for (at, r, colour, shade) in [
        ([-0.55f32, -0.18, -0.81], 62.0f32, 0x3c6b4a, 0x0a1410u32),
        ([0.78, -0.34, 0.52], 22.0, 0x6b5a44, 0x120e0a),
    ] {
        let dir = Vec3::from(at).normalize();
        let t = (dir.x.abs() + dir.z.abs()) * 0.5;
        let dist = NEAR_BAND + (FAR_BAND - NEAR_BAND) * t;
        let sh = Color::srgb_u8((shade >> 16) as u8, (shade >> 8) as u8, shade as u8).to_linear();
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(r).mesh().uv(48, 24))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb_u8((colour >> 16) as u8, (colour >> 8) as u8, colour as u8),
                emissive: sh,
                perceptual_roughness: 1.0,
                ..default()
            })),
            Transform::from_translation(dir * dist),
        ));
    }
}

/// The camera, at a placeholder distance until a field aims it, and its
/// render target when there is no window.
fn spawn_camera(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    sky: Handle<Image>,
    scene: &SceneSpec,
    headless: Option<&Headless>,
) {
    // ---- the camera: framed on the hull from ahead and above, OUTSIDE the
    // swarm, whose standoff reaches about two radii ----
    let dist = 3.3 * scene.zoom;
    let orbit = Orbit {
        yaw: scene.yaw,
        pitch: scene.pitch,
        dist,
        eye: dist,
        target: scene.target,
        follow: false,
    };
    let eye = scene.target
        + Vec3::new(
            orbit.yaw.sin() * orbit.pitch.cos(),
            orbit.pitch.sin(),
            orbit.yaw.cos() * orbit.pitch.cos(),
        ) * dist;
    let mut cam = commands.spawn((
        Camera3d::default(),
        bevy::render::view::Hdr,
        Projection::from(PerspectiveProjection {
            far: FAR_PLANE,
            near: 0.05,
            ..default()
        }),
        bevy::post_process::bloom::Bloom::NATURAL,
        // Brightness is what texel 1.0 maps to in cd/m^2, and the default
        // exposure puts about a thousand of those at white. The archive's sky
        // is authored to be shown as is, so a thousand shows it as is; less
        // keeps the ground genuinely dark on an eight bit canvas.
        // Well down from six hundred. What a nebula is FOR here is depth behind
        // the fight, and a backdrop that competes with the things in front of
        // it is not a backdrop. The bake is unchanged and still logs what it
        // made: this is the display, not the texture.
        bevy::core_pipeline::Skybox {
            image: sky.clone(),
            brightness: 260.0,
            ..default()
        },
        // The same cubemap lights the hulls. Kept well under the sky's own
        // brightness: at the sky's level it washed a purple chitin grey.
        //
        // And WELL under what it was. There is no fog anywhere in this
        // renderer, and there never was: nothing fades with range, no shader
        // reads a depth, and the far plane clips rather than blending. What
        // read as distance fog was this light and the ambient below it. A
        // green cubemap at 250 puts the sky's own colour on every surface in
        // the scene, so a rock forty units out and the nebula behind it came
        // out nearly the same green and the rock's contrast against it went
        // to almost nothing. That is what aerial perspective IS, arrived at
        // from the other direction, and it is exactly the thing space does
        // not have: there is no medium between the camera and the rock.
        bevy::light::GeneratedEnvironmentMapLight {
            environment_map: sky,
            intensity: 90.0,
            ..default()
        },
        // Nought, which is the honest number for vacuum. An ambient term is a
        // stand in for light bounced off the air and the ground, and there is
        // neither out here: what lights a shadowed flank is the sky, which is
        // the environment map above, and the rim light below it. A flat lift
        // on every surface is a grey floor under the whole picture, and a
        // grey floor is the other half of what read as haze.
        // Barely there, rather than nought. Vacuum has no ambient at all and
        // the honest number is zero, but zero puts a hull's shadowed flank at
        // exactly the same value as the gap between two stars, and a
        // silhouette with no interior is a hole in the picture rather than a
        // ship. Six is under a tenth of what it was: enough to keep a dark
        // side readable and far too little to lift the scene toward the sky.
        //
        // And then DIMMED BY SIXTY PERCENT, to 2.4, for a harsher picture:
        // the dark side of a hull is nearly the sky now and the terminator
        // is a line, which is what a sun in vacuum does. The motes' own
        // shader took the same cut to its floor (`mote.wgsl`), because a
        // cloud lit softer than the ships it is attacking reads as a
        // different picture laid over the first.
        AmbientLight {
            color: Color::srgb(0.55, 0.62, 0.80),
            brightness: 2.4,
            ..default()
        },
        Transform::from_translation(eye).looking_at(scene.target, Vec3::Y),
        orbit,
        bevy::render::view::NoIndirectDrawing,
    ));
    if let Some(h) = headless {
        headless_target(images, &mut cam, h);
    }
}

/// The picture a run with no window draws into, and the UI camera with it:
/// Bevy hands UI to whichever camera renders the primary window, and a
/// headless run has no primary window, so every node was laid out and drawn
/// to nothing until the camera said it was the UI's.
fn headless_target(images: &mut Assets<Image>, cam: &mut EntityCommands, h: &Headless) {
    let mut img = Image::new_target_texture(h.width, h.height, TextureFormat::Rgba8UnormSrgb, None);
    img.texture_descriptor.usage |= TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
    let handle = images.add(img);
    // And it is the UI's camera too. Bevy hands UI to whichever camera
    // renders the primary window, and a headless run has no primary
    // window, so every node was laid out and drawn to nothing: `--hud`
    // produced a screenshot with no HUD in it, which is the one outcome a
    // flag for proving the HUD must not have.
    cam.insert((
        RenderTarget::Image(handle.clone().into()),
        Msaa::Off,
        IsDefaultUiCamera,
    ));
    cam.commands().insert_resource(HeadlessTarget(handle));
}
