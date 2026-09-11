//! What a run is made of: the scene the arguments describe, the tick, the
//! frame cap, and `setup`, which spawns the whole thing.

use crate::*;

/// How fast the frame loop is allowed to go.
///
/// Vsync alone is not a cap, it is the MONITOR's cap: on a hundred and
/// forty four or two hundred and forty hertz panel a scene this cheap to
/// simulate will happily draw at the refresh rate and hold the GPU at full
/// clock the whole time, which is a hot room for frames nobody asked for.
///
/// A deadline rather than a fixed sleep, so the cap does not drift: each
/// frame waits until its own slot has come round. When a frame has already
/// overrun its budget the deadline is resynced to now rather than chased,
/// because catching up on a frame that took too long means running the next
/// few flat out, which is the thing this exists to prevent.
///
/// `sleep` rather than a spin, deliberately. A spin wait paces more precisely
/// and burns a core doing it, and burning a core is the problem.
#[derive(Resource)]
pub(crate) struct FrameLimit {
    pub(crate) budget: Duration,
    pub(crate) next: Instant,
}

impl FrameLimit {
    pub(crate) fn new(fps: u32) -> Self {
        FrameLimit { budget: Duration::from_secs_f64(1.0 / fps as f64), next: Instant::now() }
    }
}

pub(crate) fn limit_frames(mut limit: ResMut<FrameLimit>) {
    let now = Instant::now();
    if limit.next > now {
        std::thread::sleep(limit.next - now);
    }
    limit.next = limit.next.max(now) + limit.budget;
}

#[derive(Resource)]
pub(crate) struct SceneSpec {
    pub(crate) hull: String,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) chewers: usize,
    pub(crate) reinforce: u32,
    pub(crate) rocks: usize,
    pub(crate) fighters: u32,
    pub(crate) launch_delay: f32,
    pub(crate) zoom: f32,
    pub(crate) target: Vec3,
    pub(crate) explode: u32,
    pub(crate) cadence: u32,
    pub(crate) hives: usize,
    pub(crate) order: Option<Vec3>,
    pub(crate) aim: Option<Vec3>,
    pub(crate) showcase: bool,
    pub(crate) thickness: f32,
    pub(crate) fixed_dt: bool,
}

/// Sixty a second, accumulated from wall time and clamped, so the chewers eat
/// at one rate whatever the frame rate is doing.
#[derive(Resource, Default)]
pub(crate) struct Tick {
    pub(crate) tick: u32,
    pub(crate) acc: f32,
}

pub(crate) fn advance_tick(time: Res<Time>, scene: Res<SceneSpec>, mut t: ResMut<Tick>) {
    if scene.fixed_dt {
        t.tick += 1;
        return;
    }
    t.acc += time.delta_secs().min(swarm::STEP_CLAMP);
    while t.acc >= 1.0 / 60.0 {
        t.acc -= 1.0 / 60.0;
        t.tick += 1;
    }
}

pub(crate) fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cfg: ResMut<SwarmConfig>,
    scene: Res<SceneSpec>,
    tex: Res<Textures>,
    headless: Option<Res<Headless>>,
) {
    // ---- the flagship ----
    let (lead, radius) = spawn_hull(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene.hull,
        Transform::IDENTITY,
        scene.chewers as u32,
        0,
        None,
    );
    cfg.hull_radius = radius;
    if let Some(order) = scene.order {
        commands.entity(lead).insert(NavTo(order));
    }

    // A wave already on its way in, so a headless run can photograph one.
    for n in 0..scene.reinforce {
        call_one(&mut commands, &mut meshes, &mut materials, &tex, &scene.hull, radius, Vec3::ZERO, Quat::IDENTITY, (scene.chewers / 3) as u32, n);
    }

    // ---- the showcase: one of each archetype, big, in chitin ----
    // Chitin tiled at half the rate of a finish: a scale spans two cells, so
    // on a body a few cells across it reads as scales and not as grain.
    let alien_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.42,
        metallic: 0.05,
        normal_map_texture: tex.chitin.clone(),
        uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(0.5)),
        ..default()
    });
    if scene.showcase {
        for (n, arch) in Archetype::ALL.into_iter().enumerate() {
            let m = generate(arch, 11 + n as u64);
            let s = greedy_mesh(&m, None);
            let scale = 2.2 / (m.cell * m.nx as f32);
            let lit = meshes.add(to_mesh_where(&s, |i| i == SURF_DRIVE as usize));
            let lit_mat = materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(2.6, 2.6, 2.6)),
                unlit: true,
                ..default()
            });
            commands.spawn((
                Mesh3d(meshes.add(to_mesh_where(&s, |i| i != SURF_DRIVE as usize))),
                MeshMaterial3d(alien_mat.clone()),
                children![(Mesh3d(lit), MeshMaterial3d(lit_mat), Transform::IDENTITY)],
                Transform::from_xyz(-4.5 + n as f32 * 3.0, -radius * 0.75, radius * 1.15).with_scale(Vec3::splat(scale)),
                Showcase,
            ));
        }
    }

    // ---- the motherships ----
    //
    // The swarm used to come back at a shell round the target, which is a
    // cloud that simply exists. It comes out of these now, so the thing a
    // player is actually fighting has a place they can go and kill.
    //
    // Each is the Mother archetype at its own seed, blown up until it is half
    // again the size of the ship it is besieging: a carrier that a frigate
    // dwarfed would read as another fighter.
    let want = radius * HIVE_RADIUS;
    for n in 0..scene.hives {
        let m = generate(Archetype::Mother, 900 + n as u64);
        let scale = want / m.radius();
        // A golden angle spiral over the sphere, which spreads n points
        // evenly without any two ending up in the same place, whatever n is.
        let t = (n as f32 + 0.5) / scene.hives as f32;
        let y = 1.0 - 2.0 * t;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = n as f32 * 2.399_963_2;
        let dir = Vec3::new(r * a.cos(), y * 0.45, r * a.sin()).normalize();
        let at = dir * radius * (HIVE_NEAR + (HIVE_FAR - HIVE_NEAR) * ((n % 3) as f32 / 2.0));
        // A carrier is a SHIP now, through the same builder the frigate goes
        // through: a damage grid per cell, bricks, the four layer wound, the
        // chunks that come off and the re-mesh of only what changed. It used
        // to be one mesh with a floating hit point number on it, and there was
        // never a reason for that beyond the order things were built in.
        //
        // Its materials are the chitin, one per surface, with the cells the
        // generator marked as lit on their own unlit emissive so bloom can
        // find them. Built per carrier rather than shared, because a wound is
        // per ship and one shared material would burn all ten together.
        let mut surf: Vec<Handle<StandardMaterial>> = (0..SURF_COUNT)
            .map(|_| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.42,
                    metallic: 0.05,
                    normal_map_texture: tex.chitin.clone(),
                    uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(0.5)),
                    ..default()
                })
            })
            .collect();
        surf[SURF_DRIVE as usize] = materials.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(2.6, 2.6, 2.6)),
            unlit: true,
            ..default()
        });
        let engines: Vec<Drive> = engine_clusters(&m).into_iter().map(|(gun, _, cells)| Drive { gun, cells }).collect();
        let at_xf = Transform::from_translation(at).with_scale(Vec3::splat(scale));
        // No armour multiplier: the hundred is what makes the PLAYER's ship a
        // siege, and putting it on the carriers too would mean neither side
        // could hurt the other and the battle would never resolve.
        let (e, _) = spawn_ship(
            &mut commands,
            &mut meshes,
            &mut materials,
            &tex,
            m,
            surf,
            Vec::new(),
            at_xf,
            0,
            0x5EED ^ (n as u32).wrapping_mul(0x9E37),
            None,
            HIVE_ARMOUR,
            None,
        );
        commands.entity(e).insert(Hive {
            engines,
            // A slow drift ACROSS the line to the ship rather than toward it:
            // a carrier that closed would arrive, and then there is nothing
            // left to fly the fighters anywhere.
            vel: dir.cross(Vec3::Y).normalize_or(Vec3::X) * radius * 0.07,
            radius: want,
            scale,
            seed: 900 + n as u64,
        });
    }
    info!("{} motherships at {:.1} to {:.1} units, radius {:.2} each", scene.hives, radius * HIVE_NEAR, radius * HIVE_FAR, want);

    // ---- the asteroid field ----
    //
    // Drawn as voxel lumps and NAVIGATED as spheres, and the gap between
    // those two is deliberate. A mote pays for every rock every tick, so the
    // navigation shape has to have a closed form; the sphere is inscribed
    // rather than circumscribed and the margin in the shader stands the swarm
    // off outside it, so what a player sees is a cloud flowing round a rock
    // and not a cloud flowing round an invisible ball.
    let mut rocks: Vec<Vec4> = Vec::new();
    let mut rng = Rng::new(4242);
    for n in 0..scene.rocks.min(swarm::MAX_ROCKS) {
        let m = swarm_core::rock::generate(ROCK_LATTICE, radius * ROCK_CELL, 700 + n as u64);
        let rr = m.volume_radius();
        // Strewn between the ship and the carriers, off the plane, so they
        // are cover on the way out rather than scenery at the edge.
        let t = (n as f32 + 0.5) / scene.rocks.max(1) as f32;
        let a = n as f32 * 2.399_963_2;
        let out = radius * (ROCK_NEAR + (ROCK_FAR - ROCK_NEAR) * t);
        let at = Vec3::new(a.cos() * out, (rng.range(-1.0, 1.0)) * radius * 2.6, a.sin() * out);
        let sm = greedy_mesh(&m, None);
        // One child per surface, through the hull's own material builder, so
        // the ore keeps its own finish instead of wearing the stone's. A
        // surface is a material is a draw call here exactly as it is on a
        // ship: a rock is two of them, and a rock with nothing in a surface
        // emits nothing for it.
        let mats = surface_materials(&m, &tex, &mut materials);
        let rock = commands
            .spawn((
                Transform::from_translation(at).with_rotation(Quat::from_euler(
                    EulerRot::YXZ,
                    rng.range(0.0, 6.283),
                    rng.range(0.0, 6.283),
                    rng.range(0.0, 6.283),
                )),
                Visibility::default(),
                Rock,
            ))
            .id();
        for surf in 0..SURF_COUNT {
            if sm.skin.get(surf).map(|x| x.quads()).unwrap_or(0) == 0 {
                continue;
            }
            let mesh = meshes.add(to_mesh_where(&sm, |i| i == surf));
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mats[surf].clone()),
                Transform::IDENTITY,
                ChildOf(rock),
            ));
        }
        // The sphere the swarm is told about, and it is the VOLUME radius,
        // not the bounding one. `radius()` measures to the furthest corner of
        // the furthest cell, so on a rock stretched half again on one axis it
        // is set entirely by that axis and stands well clear of the surface
        // everywhere else. Motes then held station on a sphere with nothing in
        // it: the cloud was visibly in orbit round empty space beside the
        // asteroid, which is what "they are orbiting nothing" was.
        rocks.push(at.extend(rr * ROCK_HULL));
    }
    if !rocks.is_empty() {
        info!(
            "{} asteroids from {:.1} to {:.1} units, {:.2} to {:.2} units across",
            rocks.len(),
            radius * ROCK_NEAR,
            radius * ROCK_FAR,
            rocks.iter().map(|r| r.w * 2.0).fold(f32::MAX, f32::min),
            rocks.iter().map(|r| r.w * 2.0).fold(0.0, f32::max),
        );
    }
    cfg.rocks = rocks;
    cfg.launch_delay = scene.launch_delay;

    // The swarm's body: a drone, drawn once per mote off the GPU buffer.
    let drone = generate(Archetype::Drone, 1);
    // And how much of a cell's face one of them covers, which is the only
    // place the thickness of the cloud is decided: it is what turns a count of
    // motes in a cell of the density field into an optical depth. A disc the
    // size of the drone's own silhouette, halved, because a drone is limbs and
    // the gaps between them rather than a ball.
    let area = std::f32::consts::PI * drone.radius() * drone.radius() * 0.5 * scene.thickness.max(0.0);
    cfg.sun = SUN.normalize().extend(area);
    info!("a mote covers {:.3} square units of the light, thickness {:.2}", area, scene.thickness);
    spawn_mote_mesh(&mut commands, meshes.add(to_mote_mesh(&greedy_mesh(&drone, None))));

    // And one unit quad, drawn once per spark off the other half of it. The
    // quad is turned to face the eye in the shader; this is only its corners.
    spawn_spark_mesh(&mut commands, meshes.add(Rectangle::new(1.0, 1.0)));

    // The beams, as one mesh rebuilt every frame: there are a few dozen and
    // they are a function of where the eye is, so there is nothing to cache.
    let beam_mesh = meshes.add(empty_mesh());
    commands.spawn((
        Mesh3d(beam_mesh.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            alpha_mode: AlphaMode::Add,
            // A beam is turned edge on to the eye every frame, so which way
            // its winding comes out depends on where the camera is: culled,
            // it disappeared from half the orbit.
            cull_mode: None,
            ..default()
        })),
        Transform::IDENTITY,
        bevy::camera::visibility::NoFrustumCulling,
        BeamMesh,
    ));
    commands.insert_resource(BeamHandle(beam_mesh));

    // The nav disc, on the same footing as the beams: one mesh rebuilt every
    // frame, because it is a function of where the eye is and there are at
    // most a few hundred vertices in it.
    let nav_mesh = meshes.add(empty_mesh());
    commands.spawn((
        Mesh3d(nav_mesh.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        })),
        Transform::IDENTITY,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.insert_resource(NavHandle(nav_mesh));

    // The flames, on the same footing: geometry rebuilt every frame. Additive
    // and BACK FACE CULLED, which the nav disc is not, and the difference is
    // the whole look. Additive on a closed surface lays its colour down twice
    // per ray, once on the way in and once on the way out, so the silhouette
    // and the middle come out the same brightness and the cone reads as a
    // smear of light rather than as a shape. Culled, a ray crosses one facet
    // and the facet's own flat colour is what arrives.
    let flame_mesh = meshes.add(empty_mesh());
    commands.spawn((
        Mesh3d(flame_mesh.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        })),
        Transform::IDENTITY,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.insert_resource(FlameHandle(flame_mesh));

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
        let lum: Vec<f32> = texels.iter().map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).collect();
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
        Extent3d { width: SKY_SIZE as u32, height: SKY_SIZE as u32, depth_or_array_layers: 6 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    sky.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    let sky = images.add(sky);
    info!("sky: {} faces of {}x{} baked in {:.0} ms", 6, SKY_SIZE, SKY_SIZE, t0.elapsed().as_secs_f32() * 1000.0);

    // The stars, as points on a shell that rides the eye.
    let stars = starfield(&preset);
    let mut star_mesh = Mesh::new(PrimitiveTopology::PointList, RenderAssetUsages::default());
    star_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, stars.iter().map(|s| [s.dir[0] * STAR_RADIUS, s.dir[1] * STAR_RADIUS, s.dir[2] * STAR_RADIUS]).collect::<Vec<_>>());
    star_mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        stars.iter().map(|s| { let b = 0.6 + s.size * 0.5; [s.tint[0] * b, s.tint[1] * b, s.tint[2] * b, 1.0] }).collect::<Vec<_>>(),
    );
    commands.spawn((
        Mesh3d(meshes.add(star_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() })),
        Transform::IDENTITY,
        bevy::camera::visibility::NoFrustumCulling,
        AtInfinity,
    ));
    info!("stars: {}", stars.len());

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
        DirectionalLight { illuminance: 9000.0, color: sun_colour, shadows_enabled: false, ..default() },
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

    // ---- the camera: framed on the hull from ahead and above, OUTSIDE the
    // swarm, whose standoff reaches about two radii ----
    let dist = radius * scene.zoom;
    let orbit = Orbit { yaw: scene.yaw, pitch: scene.pitch, dist, target: scene.target, follow: false };
    let eye = scene.target + Vec3::new(orbit.yaw.sin() * orbit.pitch.cos(), orbit.pitch.sin(), orbit.yaw.cos() * orbit.pitch.cos()) * dist;
    let mut cam = commands.spawn((
        Camera3d::default(),
        bevy::render::view::Hdr,
        Projection::from(PerspectiveProjection { far: FAR_PLANE, near: 0.05, ..default() }),
        bevy::post_process::bloom::Bloom::NATURAL,
        // Brightness is what texel 1.0 maps to in cd/m^2, and the default
        // exposure puts about a thousand of those at white. The archive's sky
        // is authored to be shown as is, so a thousand shows it as is; less
        // keeps the ground genuinely dark on an eight bit canvas.
        // Well down from six hundred. What a nebula is FOR here is depth behind
        // the fight, and a backdrop that competes with the things in front of
        // it is not a backdrop. The bake is unchanged and still logs what it
        // made: this is the display, not the texture.
        bevy::core_pipeline::Skybox { image: sky.clone(), brightness: 260.0, ..default() },
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
        bevy::light::GeneratedEnvironmentMapLight { environment_map: sky, intensity: 90.0, ..default() },
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
        AmbientLight { color: Color::srgb(0.55, 0.62, 0.80), brightness: 2.4, ..default() },
        Transform::from_translation(eye).looking_at(scene.target, Vec3::Y),
        orbit,
        bevy::render::view::NoIndirectDrawing,
    ));
    if let Some(h) = headless {
        let mut img = Image::new_target_texture(h.width, h.height, TextureFormat::Rgba8UnormSrgb, None);
        img.texture_descriptor.usage |= TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
        let handle = images.add(img);
        // And it is the UI's camera too. Bevy hands UI to whichever camera
        // renders the primary window, and a headless run has no primary
        // window, so every node was laid out and drawn to nothing: `--hud`
        // produced a screenshot with no HUD in it, which is the one outcome a
        // flag for proving the HUD must not have.
        cam.insert((RenderTarget::Image(handle.clone().into()), Msaa::Off, IsDefaultUiCamera));
        commands.insert_resource(HeadlessTarget(handle));
    }
}
