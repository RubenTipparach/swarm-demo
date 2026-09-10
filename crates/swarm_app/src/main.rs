//! The harness. Draws what `swarm_core` says and ticks the swarm on the GPU.
//!
//!     cargo run --release -p swarm_app                 # a window
//!     cargo run --release -p swarm_app -- --headless   # no window: N frames, a PNG, exit
//!
//! What is on screen: one stock hull from the redux-tribes fleet, meshed by
//! brick so a bite re-meshes eight cells on a side and not the ship; a few
//! dozen CPU chewers eating it, cell by cell, throwing chunks; the four alien
//! archetypes lined up as a showcase; and the swarm, drawn instanced off the
//! buffer the compute pass ticks, never touching the CPU.

mod swarm;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::RenderAssetUsages,
    camera::RenderTarget,
    input::mouse::{MouseMotion, MouseWheel},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
    },
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use std::{collections::HashMap, time::Duration};
use swarm_core::{
    alien::{generate, Archetype},
    damage::{chunk_for, Chunk, DamageGrid},
    mesh::{greedy_mesh, mesh_region, srgb_to_linear, MeshData},
    rng::Rng,
    VoxelModel,
};
use swarm::{spawn_mote_mesh, SwarmClock, SwarmConfig, SwarmPlugin};

const HULLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/");
/// Shaders and textures, pinned at build time: Bevy otherwise looks beside
/// the executable, which is `target/release/`, and a shader that is not found
/// is a swarm that never ticks.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

struct Args {
    headless: bool,
    motes: u32,
    hull: String,
    out: String,
    frames: u32,
    width: u32,
    height: u32,
    chewers: usize,
}

fn parse_args() -> Args {
    let mut a = Args {
        headless: false,
        motes: 100_000,
        hull: "terran_frigate".into(),
        out: "screenshot.png".into(),
        frames: 240,
        width: 1280,
        height: 800,
        chewers: 48,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let next = || argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--headless" => a.headless = true,
            "--motes" => { a.motes = next().parse().expect("--motes N"); i += 1; }
            "--hull" => { a.hull = next(); i += 1; }
            "--out" => { a.out = next(); i += 1; }
            "--frames" => { a.frames = next().parse().expect("--frames N"); i += 1; }
            "--size" => {
                let s = next();
                let (w, h) = s.split_once('x').expect("--size WxH");
                a.width = w.parse().unwrap();
                a.height = h.parse().unwrap();
                i += 1;
            }
            "--chewers" => { a.chewers = next().parse().expect("--chewers N"); i += 1; }
            other => panic!("unknown argument {other}"),
        }
        i += 1;
    }
    a
}

fn main() {
    let args = parse_args();
    let mut app = App::new();
    let assets = AssetPlugin { file_path: ASSETS.into(), ..default() };
    if args.headless {
        app.add_plugins(
            DefaultPlugins
                .set(assets)
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(1)))
        .insert_resource(Headless { frames: args.frames, out: args.out.clone(), width: args.width, height: args.height, shot: false })
        .add_systems(Update, headless_capture);
    } else {
        app.add_plugins(DefaultPlugins.set(assets).set(WindowPlugin {
            primary_window: Some(Window {
                title: "swarm demo".into(),
                resolution: bevy::window::WindowResolution::new(args.width, args.height),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Update, orbit_input);
    }
    app.insert_resource(ClearColor(Color::srgb(0.015, 0.018, 0.035)))
        .insert_resource(SwarmConfig { count: args.motes, ..default() })
        .insert_resource(Scene { hull: args.hull.clone(), chewers: args.chewers })
        .init_resource::<Tick>()
        .init_resource::<ChunkMaterials>()
        .add_plugins(SwarmPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (advance_tick, chew, remesh_dirty, fly_chunks, spin_showcase, orbit_camera).chain())
        .run();
}

#[derive(Resource)]
struct Scene {
    hull: String,
    chewers: usize,
}

/// Sixty a second, accumulated from wall time and clamped, so the chewers eat
/// at one rate whatever the frame rate is doing.
#[derive(Resource, Default)]
struct Tick {
    tick: u32,
    acc: f32,
}

fn advance_tick(time: Res<Time>, mut t: ResMut<Tick>) {
    t.acc += time.delta_secs().min(0.25);
    while t.acc >= 1.0 / 60.0 {
        t.acc -= 1.0 / 60.0;
        t.tick += 1;
    }
}

// ---------------------------------------------------------------- hull --

/// A ship: its cells, its damage, and the mesh handle of every brick, skin
/// and wound, so a dirty brick is a mesh swap and nothing else.
#[derive(Component)]
struct Hull {
    model: VoxelModel,
    damage: DamageGrid,
    skin: Vec<Handle<Mesh>>,
    wound: Vec<Handle<Mesh>>,
    /// Where each chewer is standing, in the hull's frame.
    chewers: Vec<Chewer>,
    breaches: usize,
    last_heat_key: u32,
}

struct Chewer {
    at: Vec3,
    next: u32,
}

#[derive(Component)]
struct Showcase;

#[derive(Component)]
struct Debris {
    vel: Vec3,
    born: u32,
}

#[derive(Resource, Default)]
struct ChunkMaterials(HashMap<u32, Handle<StandardMaterial>>);

fn to_mesh(md: &MeshData) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    if md.is_empty() {
        // A brick with nothing to draw still owns a handle, so give the
        // renderer one degenerate triangle rather than an empty buffer.
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        return mesh;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, md.uvs.clone());
    let colours: Vec<[f32; 4]> = md
        .colours
        .iter()
        .map(|c| [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]])
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(md.indices.clone()));
    mesh
}

fn load_hull(key: &str) -> VoxelModel {
    let path = format!("{HULLS}{key}.ftvx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    VoxelModel::from_ftvx(&bytes).expect("a hull file this build understands")
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cfg: ResMut<SwarmConfig>,
    scene: Res<Scene>,
    headless: Option<Res<Headless>>,
    mut images: ResMut<Assets<Image>>,
) {
    // The hull, by brick.
    let model = load_hull(&scene.hull);
    let radius = model.radius();
    cfg.hull_radius = radius;
    let mut damage = DamageGrid::new(&model);
    let skin_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.55,
        metallic: 0.15,
        ..default()
    });
    let wound_mat = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    let mut skin = Vec::new();
    let mut wound = Vec::new();
    let mut quads = 0;
    let hull_entity = commands.spawn((Transform::IDENTITY, Visibility::default())).id();
    for b in 0..damage.brick_count() {
        let (lo, hi) = damage.brick_bounds(b);
        let s = mesh_region(&model, Some((&damage, 0)), lo, hi);
        quads += s.skin.quads();
        let sh = meshes.add(to_mesh(&s.skin));
        let wh = meshes.add(to_mesh(&s.wound));
        commands.entity(hull_entity).with_children(|p| {
            p.spawn((Mesh3d(sh.clone()), MeshMaterial3d(skin_mat.clone()), Transform::IDENTITY));
            p.spawn((Mesh3d(wh.clone()), MeshMaterial3d(wound_mat.clone()), Transform::IDENTITY));
        });
        skin.push(sh);
        wound.push(wh);
    }
    damage.take_dirty();
    info!("hull {}: {} cells, {} quads over {} bricks, radius {:.2}", scene.hull, model.solid_count(), quads, damage.brick_count(), radius);

    // Chewers stand on random exposed cells, one cell out along the open face.
    let whole = greedy_mesh(&model, None);
    let mut rng = Rng::new(7);
    let chewers: Vec<Chewer> = (0..scene.chewers)
        .map(|n| {
            let q = rng.int(0, whole.skin.quads() as i32 - 1) as usize;
            let cell = whole.skin.quad_cell[q] as usize;
            let nrm = whole.skin.normals[q * 4];
            let c = model.centre_of(cell);
            Chewer {
                at: Vec3::new(c[0] + nrm[0] * model.cell, c[1] + nrm[1] * model.cell, c[2] + nrm[2] * model.cell),
                next: (n as u32 * 7) % 40,
            }
        })
        .collect();
    commands.entity(hull_entity).insert(Hull { model, damage, skin, wound, chewers, breaches: 0, last_heat_key: 0 });

    // The showcase: one of each archetype, big, in a row in front of the hull.
    let alien_mat = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.7, metallic: 0.05, ..default() });
    for (n, arch) in Archetype::ALL.into_iter().enumerate() {
        let m = generate(arch, 11 + n as u64);
        let s = greedy_mesh(&m, None);
        let scale = 2.2 / (m.cell * m.nx as f32);
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(&s.skin))),
            MeshMaterial3d(alien_mat.clone()),
            Transform::from_xyz(-4.5 + n as f32 * 3.0, -radius * 0.75, radius * 1.15).with_scale(Vec3::splat(scale)),
            Showcase,
        ));
    }

    // The swarm's body: a drone, drawn once per mote off the GPU buffer.
    let drone = generate(Archetype::Drone, 1);
    let drone_mesh = to_mesh(&greedy_mesh(&drone, None).skin);
    spawn_mote_mesh(&mut commands, meshes.add(drone_mesh));

    // Light: one sun, low and warm, and the cool ambient set at boot.
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, color: Color::srgb(1.0, 0.93, 0.8), shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.9, -0.6, 0.0)),
    ));

    // The camera: framed on the hull from ahead and above, and OUTSIDE the
    // swarm. The cloud holds a standoff of up to about two radii, so a camera
    // any nearer than that is inside it, with the nearest motes on the lens
    // and the ship behind a wall of them. Which is what the first screenshot
    // showed.
    let dist = radius * 4.6;
    let orbit = Orbit { yaw: 0.6, pitch: 0.38, dist, target: Vec3::ZERO };
    let eye = Vec3::new(orbit.yaw.sin() * orbit.pitch.cos(), orbit.pitch.sin(), orbit.yaw.cos() * orbit.pitch.cos()) * dist;
    let mut cam = commands.spawn((
        Camera3d::default(),
        // Cool ambient, on the camera: in 0.18 it is a component that overrides
        // the global default, not a resource.
        AmbientLight { color: Color::srgb(0.6, 0.7, 1.0), brightness: 120.0, ..default() },
        Transform::from_translation(eye).looking_at(Vec3::ZERO, Vec3::Y),
        orbit,
        bevy::render::view::NoIndirectDrawing,
    ));
    if let Some(h) = headless {
        let mut img = Image::new_target_texture(h.width, h.height, TextureFormat::Rgba8UnormSrgb, None);
        img.texture_descriptor.usage |= TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
        let handle = images.add(img);
        cam.insert((RenderTarget::Image(handle.clone().into()), Msaa::Off));
        commands.insert_resource(HeadlessTarget(handle));
    }
}

// ------------------------------------------------------------ chewing --

fn chew(
    tick: Res<Tick>,
    mut hulls: Query<(&mut Hull, &Transform)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
    mut cube: Local<Option<Handle<Mesh>>>,
) {
    for (mut hull, xf) in &mut hulls {
        let hull = &mut *hull;
        let mut breaches: Vec<Chunk> = Vec::new();
        for c in &mut hull.chewers {
            while c.next <= tick.tick {
                c.next += 6;
                if let Some(b) = hull.damage.bite(&hull.model, c.at.to_array(), 9.0, tick.tick) {
                    // Follow the hole in: stand where the cell was.
                    c.at = Vec3::from(hull.model.centre_of(b.cell as usize));
                    breaches.push(chunk_for(&hull.model, &b));
                }
            }
        }
        if breaches.is_empty() {
            continue;
        }
        hull.breaches += breaches.len();
        let cube = cube.get_or_insert_with(|| meshes.add(Cuboid::from_length(hull.model.cell * 0.9))).clone();
        for ch in breaches {
            let mat = chunk_mats
                .0
                .entry(ch.colour)
                .or_insert_with(|| {
                    let [r, g, b, _] = swarm_core::mesh::rgb_of(ch.colour);
                    materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.8, ..default() })
                })
                .clone();
            let origin = xf.transform_point(Vec3::from(ch.origin));
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(origin),
                Debris { vel: Vec3::from(ch.velocity), born: ch.born },
            ));
        }
    }
}

/// Re-mesh what the chewers reached this frame, and repaint the whole wound
/// when its heat has moved a bucket.
fn remesh_dirty(tick: Res<Tick>, mut hulls: Query<&mut Hull>, mut meshes: ResMut<Assets<Mesh>>) {
    for mut hull in &mut hulls {
        let hull = &mut *hull;
        let mut dirty = hull.damage.take_dirty();
        let key = hull.damage.heat_key(tick.tick);
        if key != hull.last_heat_key {
            hull.last_heat_key = key;
            // Every brick with a wound in it. Cheap enough at 128 bricks and
            // once every 28 ticks; a wound brick list would be the next step.
            for b in 0..hull.damage.brick_count() {
                if !dirty.contains(&b) {
                    dirty.push(b);
                }
            }
        }
        for b in dirty {
            let (lo, hi) = hull.damage.brick_bounds(b);
            let s = mesh_region(&hull.model, Some((&hull.damage, tick.tick)), lo, hi);
            let _ = meshes.insert(hull.skin[b].id(), to_mesh(&s.skin));
            let _ = meshes.insert(hull.wound[b].id(), to_mesh(&s.wound));
        }
    }
}

fn fly_chunks(time: Res<Time>, tick: Res<Tick>, mut commands: Commands, mut q: Query<(Entity, &mut Transform, &Debris)>) {
    let dt = time.delta_secs();
    for (e, mut xf, d) in &mut q {
        xf.translation += d.vel * dt;
        xf.rotate_local_x(dt * 3.0);
        if tick.tick.saturating_sub(d.born) > 240 {
            commands.entity(e).despawn();
        }
    }
}

fn spin_showcase(time: Res<Time>, mut q: Query<&mut Transform, With<Showcase>>) {
    for mut xf in &mut q {
        xf.rotate_y(time.delta_secs() * 0.5);
    }
}

// -------------------------------------------------------------- camera --

#[derive(Component)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: Vec3,
}

fn orbit_input(
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut q: Query<&mut Orbit>,
) {
    let Ok(mut o) = q.single_mut() else { return };
    for m in motion.read() {
        if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
            o.yaw -= m.delta.x * 0.005;
            o.pitch = (o.pitch + m.delta.y * 0.005).clamp(-1.4, 1.4);
        }
    }
    for w in wheel.read() {
        o.dist = (o.dist * (1.0 - w.y * 0.08)).clamp(2.0, 200.0);
    }
}

fn orbit_camera(mut q: Query<(&Orbit, &mut Transform)>) {
    for (o, mut xf) in &mut q {
        let eye = o.target + Vec3::new(o.yaw.sin() * o.pitch.cos(), o.pitch.sin(), o.yaw.cos() * o.pitch.cos()) * o.dist;
        *xf = Transform::from_translation(eye).looking_at(o.target, Vec3::Y);
    }
}

// ------------------------------------------------------------ headless --

#[derive(Resource)]
struct Headless {
    frames: u32,
    out: String,
    width: u32,
    height: u32,
    shot: bool,
}

#[derive(Resource)]
struct HeadlessTarget(Handle<Image>);

fn headless_capture(
    mut h: ResMut<Headless>,
    target: Option<Res<HeadlessTarget>>,
    clock: Res<SwarmClock>,
    time: Res<Time>,
    hulls: Query<&Hull>,
    mut frames: Local<u32>,
    mut spent: Local<f32>,
    mut commands: Commands,
) {
    *frames += 1;
    *spent += time.delta_secs();
    if h.shot || *frames < h.frames {
        return;
    }
    let Some(t) = target else { return };
    h.shot = true;
    let breaches: usize = hulls.iter().map(|x| x.breaches).sum();
    let dead: usize = hulls.iter().map(|x| x.damage.dead_count()).sum();
    println!(
        "headless: {} frames in {:.1}s ({:.1} ms/frame mean), swarm ticks {}, chewed {} cells ({} breaches thrown)",
        *frames, *spent, *spent * 1000.0 / *frames as f32, clock.ticks, dead, breaches
    );
    let out = h.out.clone();
    commands
        .spawn(Screenshot::image(t.0.clone()))
        .observe(save_to_disk(out.clone()))
        .observe(move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            let img: &Image = &shot.event().image;
            let (w, hh) = (img.width() as usize, img.height() as usize);
            let data = img.data.as_deref().unwrap_or(&[]);
            let bpp = if data.len() >= w * hh * 4 { 4 } else { 0 };
            if bpp == 0 {
                println!("screenshot: no pixel data ({} bytes for {}x{}, {:?})", data.len(), w, hh, img.texture_descriptor.format);
                exit.write(AppExit::error());
                return;
            }
            // Lit pixels over the clear colour, whole frame and the middle
            // third where the hull is.
            let lit = |x0: usize, x1: usize, y0: usize, y1: usize| -> usize {
                let mut n = 0;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let o = (y * w + x) * 4;
                        if data[o] as u32 + data[o + 1] as u32 + data[o + 2] as u32 > 60 {
                            n += 1;
                        }
                    }
                }
                n
            };
            let all = lit(0, w, 0, hh);
            let mid = lit(w / 3, 2 * w / 3, hh / 4, 3 * hh / 4);
            println!("screenshot {out}: {}x{}, {} lit pixels ({:.1}%), {} in the middle third ({:.1}%)", w, hh, all, 100.0 * all as f32 / (w * hh) as f32, mid, 100.0 * mid as f32 / ((w / 3) * (hh / 2)) as f32);
            let ok = all > (w * hh) / 100 && mid > 0;
            exit.write(if ok { AppExit::Success } else { AppExit::error() });
        });
}
