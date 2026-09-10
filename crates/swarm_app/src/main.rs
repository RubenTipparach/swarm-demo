//! The harness. Draws what `swarm_core` says and ticks the swarm on the GPU.
//!
//!     cargo run --release -p swarm_app                 # a window
//!     cargo run --release -p swarm_app -- --headless   # no window: N frames, a PNG, exit
//!
//! What is on screen: one stock hull from the redux-tribes fleet, drawn as
//! redux-tribes draws it (one material per surface with that surface's finish
//! normal map, windows cut into the plating wearing their three map decals),
//! meshed by brick so a bite re-meshes eight cells on a side and not the
//! ship; a few dozen CPU chewers eating it and throwing chunks; the four alien
//! archetypes in chitin; the swarm, drawn instanced off the buffer the compute
//! pass ticks; and the sky the archive shipped, baked at launch.

mod swarm;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::{LoadState, RenderAssetUsages},
    camera::RenderTarget,
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    input::mouse::{MouseMotion, MouseWheel},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension},
        view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
    },
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use std::{collections::HashMap, time::Duration};
use swarm_core::{
    alien::{generate, Archetype},
    damage::{chunk_for, Chunk, DamageGrid, Vent},
    fx::{blast_sparks, breach_sparks, guns_of, muzzle_sparks, Beam, Blast, Gun, Spark, SparkKind},
    mesh::{greedy_mesh, mesh_region, srgb_to_linear, MeshData, Surfaces},
    rng::{drift_of, Rng},
    sky::{bake_cubemap, starfield, to_half, SkyPreset},
    VoxelModel, SURF_COUNT,
};
use swarm::{spawn_mote_mesh, spawn_spark_mesh, Capsule, FxTextures, Shots, SparkQueue, SwarmClock, SwarmConfig, SwarmPlugin};

const HULLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hulls/");
/// Shaders and textures, pinned at build time: Bevy otherwise looks beside
/// the executable, which is `target/release/`, and a shader that is not found
/// is a swarm that never ticks.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

/// Bodies sit in this band, outside the fight and inside the far plane.
const NEAR_BAND: f32 = 250.0;
const FAR_BAND: f32 = 660.0;
const FAR_PLANE: f32 = 6000.0;
const STAR_RADIUS: f32 = 4500.0;
const SKY_SIZE: usize = 256;

struct Args {
    headless: bool,
    motes: u32,
    hull: String,
    out: String,
    frames: u32,
    width: u32,
    height: u32,
    chewers: usize,
    /// Camera distance in hull radii.
    zoom: f32,
    /// What the camera looks at, in world units. The hull's centre unless
    /// asked otherwise: the showcase aliens sit below and ahead of it.
    target: Vec3,
    /// Force the reactor at this tick. Nought leaves it to the hull's own
    /// state, which goes critical once enough of it is gone.
    explode: u32,
    /// Ticks between one gun firing and the next. Nought silences them.
    cadence: u32,
    /// Advance exactly one tick a frame rather than by the wall clock.
    ///
    /// A software rasteriser draws at four frames a second, so a frame here
    /// is fourteen ticks and a screenshot cannot be aimed at one: the shot
    /// meant for the fireball arrives four hundred ticks after it went out.
    /// This is for the harness, and it is what makes a headless render a
    /// function of its frame count rather than of how fast the machine is.
    fixed_dt: bool,
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
        zoom: 4.6,
        target: Vec3::ZERO,
        explode: 0,
        cadence: 70,
        fixed_dt: false,
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
            "--zoom" => { a.zoom = next().parse().expect("--zoom R"); i += 1; }
            "--target" => {
                let s = next();
                let v: Vec<f32> = s.split(',').map(|x| x.parse().expect("--target x,y,z")).collect();
                a.target = Vec3::new(v[0], v[1], v[2]);
                i += 1;
            }
            "--size" => {
                let s = next();
                let (w, h) = s.split_once('x').expect("--size WxH");
                a.width = w.parse().unwrap();
                a.height = h.parse().unwrap();
                i += 1;
            }
            "--chewers" => { a.chewers = next().parse().expect("--chewers N"); i += 1; }
            "--explode" => { a.explode = next().parse().expect("--explode TICK"); i += 1; }
            "--cadence" => { a.cadence = next().parse().expect("--cadence TICKS"); i += 1; }
            "--fixed-dt" => a.fixed_dt = true,
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
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(SwarmConfig { count: args.motes, fixed_dt: args.fixed_dt, ..default() })
        .insert_resource(Scene {
            hull: args.hull.clone(),
            chewers: args.chewers,
            zoom: args.zoom,
            target: args.target,
            explode: args.explode,
            cadence: args.cadence,
            fixed_dt: args.fixed_dt,
        })
        .init_resource::<Tick>()
        .init_resource::<ChunkMaterials>()
        .add_plugins(SwarmPlugin)
        .init_resource::<Shots>()
        .init_resource::<SparkQueue>()
        .init_resource::<LiveFx>()
        .init_resource::<BeamQuads>()
        .add_systems(Startup, (load_textures, setup).chain())
        .add_systems(
            Update,
            (
                advance_tick,
                (fire_guns, chew, vent_smoke, go_critical),
                (age_fx, draw_beams),
                remesh_dirty,
                (fly_chunks, spin_showcase, orbit_camera, ride_the_eye),
            )
                .chain(),
        )
        .run();
}

#[derive(Resource)]
struct Scene {
    hull: String,
    chewers: usize,
    zoom: f32,
    target: Vec3,
    explode: u32,
    cadence: u32,
    fixed_dt: bool,
}

/// Sixty a second, accumulated from wall time and clamped, so the chewers eat
/// at one rate whatever the frame rate is doing.
#[derive(Resource, Default)]
struct Tick {
    tick: u32,
    acc: f32,
}

fn advance_tick(time: Res<Time>, scene: Res<Scene>, mut t: ResMut<Tick>) {
    if scene.fixed_dt {
        t.tick += 1;
        return;
    }
    t.acc += time.delta_secs().min(0.25);
    while t.acc >= 1.0 / 60.0 {
        t.acc -= 1.0 / 60.0;
        t.tick += 1;
    }
}

// ------------------------------------------------------------ textures --

/// Every texture the picture uses, by name, loaded ONE way each: a finish or
/// a chitin is a normal map (linear, repeating), a window's colour and glow
/// are sRGB and clamped, its normal linear and clamped. `textures.ts` in
/// redux-tribes is the one loader for the same reason: a fourth caller cannot
/// spell a path or a colour space its own way if it has nowhere to spell it.
#[derive(Resource, Default)]
struct Textures {
    finishes: HashMap<String, Handle<Image>>,
    windows: HashMap<String, WindowMaps>,
    chitin: Option<Handle<Image>>,
    ember: Option<Handle<Image>>,
}

#[derive(Clone)]
struct WindowMaps {
    colour: Handle<Image>,
    emissive: Handle<Image>,
    normal: Handle<Image>,
}

const FINISHES: [&str; 9] = ["plate", "ribbed", "hex", "cracked", "tread", "greeble", "weave", "battered", "crate"];
const WINDOW_KINDS: [&str; 9] = ["panes", "porthole", "strip", "bridge", "promenade", "beacons", "louvre", "cargo", "hangar"];

fn sampler(repeat: bool) -> ImageSampler {
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..default()
    })
}

fn load_textures(mut commands: Commands, assets: Res<AssetServer>) {
    let load = |path: String, srgb: bool, repeat: bool| -> Handle<Image> {
        assets.load_with_settings(path, move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = sampler(repeat);
        })
    };
    let mut t = Textures::default();
    for f in FINISHES {
        t.finishes.insert(f.into(), load(format!("textures/surf/armour_{f}_n.png"), false, true));
    }
    for k in WINDOW_KINDS {
        t.windows.insert(
            k.into(),
            WindowMaps {
                colour: load(format!("textures/surf/window_{k}_c.png"), true, false),
                emissive: load(format!("textures/surf/window_{k}_e.png"), true, false),
                normal: load(format!("textures/surf/window_{k}_n.png"), false, false),
            },
        );
    }
    let chitin = load("textures/alien_chitin_n.png".into(), false, true);
    // The ember atlas is the one texture a burn comes off, wherever it is: the
    // inside of a hole, the soot round it, and every spark in the air. One
    // fire, one picture of it.
    let ember = load("textures/ember.png".into(), true, false);
    commands.insert_resource(FxTextures { chitin: chitin.clone(), ember: ember.clone() });
    t.chitin = Some(chitin);
    t.ember = Some(ember);
    commands.insert_resource(t);
}

// ---------------------------------------------------------------- hull --

/// The one entity per (brick, surface) that draws a piece of a hull, created
/// the first time that surface has a face in that brick and hidden when it
/// stops having one, so a hole that reaches the frame under a plate gets a
/// frame mesh where there was none.
#[derive(Default)]
struct Piece {
    entity: Option<Entity>,
    mesh: Option<Handle<Mesh>>,
}

#[derive(Default)]
struct Brick {
    skin: Vec<Piece>,
    /// The machinery a hole uncovered, the burn over it, and the soot on the
    /// plating round the rim. Three layers on the same faces, and they are
    /// three materials because they are three different things.
    inner: Piece,
    wound: Piece,
    scorch: Piece,
    windows: Vec<Piece>,
}

/// A ship: its cells, its damage, its materials, and every piece of it on
/// the map, so a dirty brick is a mesh swap and nothing else.
#[derive(Component)]
struct Hull {
    model: VoxelModel,
    damage: DamageGrid,
    bricks: Vec<Brick>,
    surface_mats: Vec<Handle<StandardMaterial>>,
    window_mats: Vec<Handle<StandardMaterial>>,
    inner_mat: Handle<StandardMaterial>,
    wound_mat: Handle<StandardMaterial>,
    scorch_mat: Handle<StandardMaterial>,
    chewers: Vec<Chewer>,
    /// Where this hull's weapons are and which way they look, read off the
    /// cells the export says are gunnery.
    guns: Vec<Gun>,
    breaches: usize,
    last_heat_key: u32,
    /// How many cells it started with, so "enough of it is gone" is a share
    /// rather than a number that means something different on every class.
    cells: usize,
    dead_hull: bool,
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

/// Rides the eye: a thing with a direction and no position, which a camera
/// move must not slide across the sky.
#[derive(Component)]
struct AtInfinity;

#[derive(Resource, Default)]
struct ChunkMaterials(HashMap<u32, Handle<StandardMaterial>>);

fn to_mesh(md: &MeshData) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    if md.is_empty() {
        // A piece with nothing to draw still owns a handle, so give the
        // renderer one degenerate triangle rather than an empty buffer.
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        return mesh;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, md.uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, md.tangents.clone());
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

/// One material per surface, as `hullMaterials` in hull.ts builds them: the
/// design's own finish, metalness and roughness for each, vertex colours
/// carrying the livery. `smooth` is no normal map at all.
fn surface_materials(model: &VoxelModel, tex: &Textures, materials: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    (0..SURF_COUNT)
        .map(|s| {
            let (finish, metal, rough) = model
                .surfaces
                .get(s)
                .map(|x| (x.finish.as_str(), x.metal, x.rough))
                .unwrap_or(("plate", 0.25, 0.55));
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                metallic: metal,
                perceptual_roughness: rough,
                normal_map_texture: tex.finishes.get(finish).cloned(),
                ..default()
            })
        })
        .collect()
}

/// One material per window kind, as `windowMaterial` in textures.ts: the
/// colour map multiplies the plating's paint down to glass, emission lights
/// the panes that are on and is the only part that survives with no light on
/// it, and the normal seats the frame into the plate.
fn window_materials(model: &VoxelModel, tex: &Textures, materials: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    model
        .window_kinds
        .iter()
        .map(|k| {
            let maps = tex.windows.get(k);
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: maps.map(|m| m.colour.clone()),
                emissive: LinearRgba::WHITE * 1.6,
                emissive_texture: maps.map(|m| m.emissive.clone()),
                normal_map_texture: maps.map(|m| m.normal.clone()),
                metallic: 0.15,
                perceptual_roughness: 0.35,
                ..default()
            })
        })
        .collect()
}

/// Put a mesh on a piece: spawn it the first time, swap the mesh after, hide
/// it when there is nothing to draw.
fn upsert(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    piece: &mut Piece,
    md: &MeshData,
    material: &Handle<StandardMaterial>,
    parent: Entity,
) {
    if md.is_empty() {
        if let Some(e) = piece.entity {
            commands.entity(e).insert(Visibility::Hidden);
        }
        return;
    }
    let mesh = to_mesh(md);
    match (&piece.entity, &piece.mesh) {
        (Some(e), Some(h)) => {
            let _ = meshes.insert(h.id(), mesh);
            commands.entity(*e).insert(Visibility::Inherited);
        }
        _ => {
            let h = meshes.add(mesh);
            let e = commands
                .spawn((Mesh3d(h.clone()), MeshMaterial3d(material.clone()), Transform::IDENTITY, ChildOf(parent)))
                .id();
            piece.entity = Some(e);
            piece.mesh = Some(h);
        }
    }
}

fn place_brick(commands: &mut Commands, meshes: &mut Assets<Mesh>, hull: &mut Hull, b: usize, s: &Surfaces, parent: Entity) {
    let brick = &mut hull.bricks[b];
    if brick.skin.len() < SURF_COUNT {
        brick.skin.resize_with(SURF_COUNT, Piece::default);
    }
    if brick.windows.len() < s.windows.len() {
        brick.windows.resize_with(s.windows.len(), Piece::default);
    }
    for (i, md) in s.skin.iter().enumerate() {
        upsert(commands, meshes, &mut brick.skin[i], md, &hull.surface_mats[i], parent);
    }
    upsert(commands, meshes, &mut brick.inner, &s.inner, &hull.inner_mat, parent);
    upsert(commands, meshes, &mut brick.wound, &s.wound, &hull.wound_mat, parent);
    upsert(commands, meshes, &mut brick.scorch, &s.scorch, &hull.scorch_mat, parent);
    for (k, md) in s.windows.iter().enumerate() {
        upsert(commands, meshes, &mut brick.windows[k], md, &hull.window_mats[k], parent);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cfg: ResMut<SwarmConfig>,
    scene: Res<Scene>,
    tex: Res<Textures>,
    headless: Option<Res<Headless>>,
) {
    // ---- the hull, by brick and by surface ----
    let model = load_hull(&scene.hull);
    let radius = model.radius();
    cfg.hull_radius = radius;
    let mut damage = DamageGrid::new(&model);
    let hull_entity = commands.spawn((Transform::IDENTITY, Visibility::default())).id();
    let guns = guns_of(&model);
    let cells = model.solid_count();
    let mut hull = Hull {
        surface_mats: surface_materials(&model, &tex, &mut materials),
        window_mats: window_materials(&model, &tex, &mut materials),
        // The inside of a ship is machinery, so it wears what machinery wears.
        inner_mat: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.62,
            metallic: 0.55,
            normal_map_texture: tex.finishes.get("greeble").cloned(),
            ..default()
        }),
        // The burn over it. Unlit, because a fire is its own light, and well
        // over white so a fresh hole clears the bloom threshold: the ramp is
        // nought to one by construction (it is a colour), and how BRIGHT that
        // colour is put on the hull is the picture's business, not the ramp's.
        wound_mat: materials.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(3.4, 3.4, 3.4)),
            base_color_texture: tex.ember.clone(),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 2.0,
            ..default()
        }),
        // And the soot around it, which is a stain rather than a light: lit,
        // dark, and biased off the plate it is laid on so it does not fight
        // the panel underneath for the same depth.
        scorch_mat: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: tex.ember.clone(),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 1.0,
            ..default()
        }),
        bricks: (0..damage.brick_count()).map(|_| Brick::default()).collect(),
        chewers: Vec::new(),
        guns,
        breaches: 0,
        last_heat_key: 0,
        cells,
        dead_hull: false,
        model,
        damage,
    };
    let (mut quads, mut windows) = (0, 0);
    for b in 0..hull.damage.brick_count() {
        let (lo, hi) = hull.damage.brick_bounds(b);
        let s = mesh_region(&hull.model, Some((&hull.damage, 0)), lo, hi);
        quads += s.skin_quads();
        windows += s.window_quads();
        place_brick(&mut commands, &mut meshes, &mut hull, b, &s, hull_entity);
    }
    hull.damage.take_dirty();
    let used: Vec<String> = (0..SURF_COUNT)
        .filter(|&s| hull.bricks.iter().any(|b| b.skin.get(s).and_then(|p| p.entity).is_some()))
        .map(|s| format!("{s}:{}", hull.model.surfaces.get(s).map(|x| x.finish.as_str()).unwrap_or("?")))
        .collect();
    info!(
        "hull {}: {} cells, {} quads over {} bricks, {} window faces of {} kinds, {} guns, surfaces [{}], radius {:.2}",
        scene.hull, hull.cells, quads, hull.damage.brick_count(), windows, hull.model.window_kinds.len(), hull.guns.len(), used.join(" "), radius
    );

    // Chewers stand on random exposed cells, one cell out along the open face.
    let whole = greedy_mesh(&hull.model, None).skin_all();
    let mut rng = Rng::new(7);
    hull.chewers = (0..scene.chewers)
        .map(|n| {
            let q = rng.int(0, whole.quads() as i32 - 1) as usize;
            let cell = whole.quad_cell[q] as usize;
            let nrm = whole.normals[q * 4];
            let c = hull.model.centre_of(cell);
            let cs = hull.model.cell;
            Chewer { at: Vec3::new(c[0] + nrm[0] * cs, c[1] + nrm[1] * cs, c[2] + nrm[2] * cs), next: (n as u32 * 7) % 40 }
        })
        .collect();
    damage = DamageGrid::new(&hull.model);
    let _ = damage;
    commands.entity(hull_entity).insert(hull);

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
    for (n, arch) in Archetype::ALL.into_iter().enumerate() {
        let m = generate(arch, 11 + n as u64);
        let s = greedy_mesh(&m, None).skin_all();
        let scale = 2.2 / (m.cell * m.nx as f32);
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(&s))),
            MeshMaterial3d(alien_mat.clone()),
            Transform::from_xyz(-4.5 + n as f32 * 3.0, -radius * 0.75, radius * 1.15).with_scale(Vec3::splat(scale)),
            Showcase,
        ));
    }

    // The swarm's body: a drone, drawn once per mote off the GPU buffer.
    let drone = generate(Archetype::Drone, 1);
    spawn_mote_mesh(&mut commands, meshes.add(to_mesh(&greedy_mesh(&drone, None).skin_all())));

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

    // ---- the sky ----
    //
    // Baked once, on the CPU, into a half float cubemap: a nebula is a slow
    // gradient across a dark range, and eight bits of it magnified four times
    // is a contour map. The same cubemap lights the hulls as an environment
    // map, so a shadowed flank picks up the colour of the sky it flies in.
    let preset = SkyPreset::skirmish();
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
    let sun = Vec3::new(0.42, 0.66, -0.62).normalize();
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
    let orbit = Orbit { yaw: 0.6, pitch: 0.38, dist, target: scene.target };
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
        bevy::core_pipeline::Skybox { image: sky.clone(), brightness: 600.0, ..default() },
        // The same cubemap lights the hulls. Kept well under the sky's own
        // brightness: at the sky's level it washed a purple chitin grey.
        bevy::light::GeneratedEnvironmentMapLight { environment_map: sky, intensity: 250.0, ..default() },
        AmbientLight { color: Color::srgb(0.6, 0.7, 1.0), brightness: 40.0, ..default() },
        Transform::from_translation(eye).looking_at(scene.target, Vec3::Y),
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
    mut sparks: ResMut<SparkQueue>,
    mut cube: Local<Option<Handle<Mesh>>>,
) {
    for (mut hull, xf) in &mut hulls {
        let hull = &mut *hull;
        if hull.dead_hull {
            continue;
        }
        let mut breaches: Vec<Chunk> = Vec::new();
        for c in &mut hull.chewers {
            while c.next <= tick.tick {
                c.next += 6;
                if let Some(b) = hull.damage.bite(&hull.model, c.at.to_array(), 9.0, tick.tick) {
                    // Follow the hole in: stand where the cell was.
                    c.at = Vec3::from(hull.model.centre_of(b.cell as usize));
                    // The spray comes off the FACE that opened, in world
                    // space: a spark thrown in the hull's own frame would
                    // fly off in the wrong direction the moment a ship moves.
                    let at = xf.transform_point(Vec3::from(hull.model.centre_of(b.cell as usize)));
                    let out = xf.rotation * Vec3::from(b.outward);
                    let mut list = Vec::new();
                    breach_sparks(b.cell, b.tick, at.to_array(), out.to_array(), hull.model.cell, &mut list);
                    sparks.extend(list);
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
fn remesh_dirty(tick: Res<Tick>, mut hulls: Query<(Entity, &mut Hull)>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    for (entity, mut hull) in &mut hulls {
        let hull = &mut *hull;
        let mut dirty = hull.damage.take_dirty();
        let key = hull.damage.heat_key(tick.tick);
        if key != hull.last_heat_key {
            hull.last_heat_key = key;
            for b in 0..hull.damage.brick_count() {
                if hull.bricks[b].wound.entity.is_some() && !dirty.contains(&b) {
                    dirty.push(b);
                }
            }
        }
        for b in dirty {
            let (lo, hi) = hull.damage.brick_bounds(b);
            let s = mesh_region(&hull.model, Some((&hull.damage, tick.tick)), lo, hi);
            place_brick(&mut commands, &mut meshes, hull, b, &s, entity);
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

// ------------------------------------------------------------------ fx --

/// Every shot and every blast that is still live.
///
/// They are here rather than as entities because they are read as one batch
/// every frame by two things (the mesh that draws them and the uniform the
/// swarm is handed) and neither wants a query: a few dozen of anything is a
/// Vec, and an entity per beam would be an archetype move per shot fired.
#[derive(Resource, Default)]
struct LiveFx {
    beams: Vec<Beam>,
    blasts: Vec<Blast>,
    /// Totals over the whole run, for the headless report. A picture with no
    /// beam in it and a picture of a beam that was never fired look the same,
    /// and only one of them is a bug in this file.
    fired: usize,
    sparked: usize,
}

#[derive(Component)]
struct BeamMesh;

#[derive(Resource)]
struct BeamHandle(Handle<Mesh>);

/// How many quads the beam mesh carried this frame.
#[derive(Resource, Default)]
struct BeamQuads(usize);

/// A mesh with nothing in it that the renderer will still accept.
fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

/// Guns go off on their own cadence, staggered so a broadside is a rattle
/// rather than one bang, and sweep so the beams rake the cloud instead of
/// drilling the same hole in it forever.
fn fire_guns(
    tick: Res<Tick>,
    scene: Res<Scene>,
    hulls: Query<(&Hull, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let reach = hull.model.radius() * 3.0;
        for (n, g) in hull.guns.iter().enumerate() {
            // Staggered by the gun's own cell, so two hulls of one class do
            // not fire in lockstep and the pattern does not read as a clock.
            let phase = (swarm_core::rng::hash_cell(g.cell) % scene.cadence) as u32;
            if (tick.tick + phase) % scene.cadence != 0 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(g.at));
            let out = (xf.rotation * Vec3::from(g.out)).normalize_or_zero();
            // A slow rake across the shell. Trigonometry is fine here and
            // nowhere near the core: this decides where a light is drawn.
            let t = tick.tick as f32 * 0.03 + n as f32 * 1.7;
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(out);
            let dir = (out + side * (t.sin() * 0.45) + up * ((t * 0.7).cos() * 0.30)).normalize();
            fx.beams.push(Beam {
                from: at.to_array(),
                to: (at + dir * reach).to_array(),
                radius: hull.model.cell * 2.2,
                born: tick.tick,
            });
            fx.fired += 1;
            let mut list = Vec::new();
            muzzle_sparks(g.cell.wrapping_add(tick.tick), at.to_array(), dir.to_array(), hull.model.cell, &mut list);
            sparks.extend(list);
        }
    }
}

/// A hull that has lost enough of itself goes critical.
///
/// A share rather than a count, because "enough" means something different on
/// a corvette and a heavy cruiser, and the same number would kill one instantly
/// and never kill the other.
const CRITICAL_SHARE: f32 = 0.10;

fn go_critical(
    tick: Res<Tick>,
    scene: Res<Scene>,
    mut hulls: Query<(Entity, &mut Hull, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
) {
    for (entity, mut hull, xf) in &mut hulls {
        let hull = &mut *hull;
        if hull.dead_hull {
            continue;
        }
        let share = hull.damage.dead_count() as f32 / hull.cells.max(1) as f32;
        let forced = scene.explode > 0 && tick.tick >= scene.explode;
        if !forced && share < CRITICAL_SHARE {
            continue;
        }
        hull.dead_hull = true;

        let radius = hull.model.radius();
        let centre = xf.translation;
        let blast = Blast { at: centre.to_array(), radius: radius * 1.8, born: tick.tick };
        // What it does to the HULL is a smaller sphere than what it does to
        // the swarm: a reactor takes the ship it is in, and the pressure wave
        // goes further than the wreck does.
        let hull_blast = Blast { at: [0.0; 3], radius: radius * 1.5, born: tick.tick };
        info!(
            "hull went critical at tick {} ({:.1}% of its cells gone{}): blast radius {:.2}",
            tick.tick,
            share * 100.0,
            if forced { ", forced" } else { "" },
            blast.radius
        );

        // The hull itself: everything inside the sphere is gone at once, and
        // every cell that went is thrown.
        let breaches = hull.damage.blast_cells(&hull.model, &hull_blast, tick.tick);
        let cube = meshes.add(Cuboid::from_length(hull.model.cell * 0.9));
        // A cap, because a cruiser inside its own blast is ten thousand cells
        // and ten thousand entities is a stall, not an explosion. The ones
        // that are not thrown are simply gone, which is what the re-mesh
        // draws anyway.
        const MAX_DEBRIS: usize = 450;
        let step = (breaches.len() / MAX_DEBRIS).max(1);
        for b in breaches.iter().step_by(step) {
            let ch = chunk_for(&hull.model, b);
            let mat = chunk_mats
                .0
                .entry(ch.colour)
                .or_insert_with(|| {
                    let [r, g, bl, _] = swarm_core::mesh::rgb_of(ch.colour);
                    materials.add(StandardMaterial { base_color: Color::srgb(r, g, bl), perceptual_roughness: 0.8, ..default() })
                })
                .clone();
            let origin = xf.transform_point(Vec3::from(ch.origin));
            let away = (origin - centre).normalize_or_zero();
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(origin),
                Debris { vel: Vec3::from(ch.velocity) + away * radius * 1.6, born: ch.born },
            ));
        }

        // The fireball is SPARKS, not a shell.
        //
        // It was a sphere mesh drawn additively, and it came out as a solid
        // orange disc with the wreck somewhere behind it: additive blending on
        // a closed surface lays the same colour down twice per ray, once going
        // in and once coming out, so a shell bright enough to read at its rim
        // is opaque everywhere else. redux-tribes learned the same thing on its
        // movement envelope and answered it with a fresnel; the answer here is
        // that a particle system already exists and an explosion is a great
        // many burning pieces, which is a thing it can draw and a sphere is
        // not. What is left of the shell is the FLASH below: small, very
        // bright, and gone in a tenth of a second.
        let mut list = Vec::new();
        blast_sparks(tick.tick, centre.to_array(), blast.radius, 1400, &mut list);
        sparks.extend(list);

        // The detonation itself: a handful of very large, very short sparks at
        // the centre, which is what makes the first frame read as a flash
        // rather than as debris that was always there.
        let mut flash = Vec::new();
        blast_sparks(tick.tick ^ 0xF1A5, centre.to_array(), blast.radius * 0.22, 26, &mut flash);
        for f in &mut flash {
            f.size *= 5.0;
            f.life = 0.10 + f.life * 0.06;
            f.colour = [9.0, 6.0, 3.2];
            f.kind = SparkKind::Blast;
        }
        sparks.extend(flash);
        fx.blasts.push(blast);
        // The whole hull re-meshes: a sphere of it just went.
        hull.damage.mark_all_dirty();
        let _ = entity;
    }
}

/// Smoke out of the holes: slow, dark, and only a few at a time, because a
/// plume from every vent on a chewed hull is a fog bank.
fn vent_smoke(tick: Res<Tick>, hulls: Query<(&Hull, &Transform)>, mut sparks: ResMut<SparkQueue>) {
    if tick.tick % 4 != 0 {
        return;
    }
    for (hull, xf) in &hulls {
        let vents: Vec<Vent> = hull.damage.vents(&hull.model, 120);
        if vents.is_empty() {
            continue;
        }
        for n in 0..vents.len().min(6) {
            let v = vents[(tick.tick as usize / 4 + n * 17) % vents.len()];
            let d = drift_of(v.cell, tick.tick.wrapping_add(n as u32));
            let at = xf.transform_point(Vec3::from(v.at));
            // Outward is the way INTO the hole, so smoke leaves along its
            // opposite: a plume that went the other way would go through the
            // ship.
            let out = -(xf.rotation * Vec3::from(v.outward));
            let vel = out * hull.model.cell * 2.4 + Vec3::from(d) * hull.model.cell * 1.2;
            sparks.push(Spark {
                pos: at.to_array(),
                vel: vel.to_array(),
                // Barely over black: smoke is what a fire leaves, and it is
                // the one thing here that must NOT bloom.
                colour: [0.30, 0.20, 0.16],
                size: hull.model.cell * (2.0 + 1.5 * (d[0] * 0.5 + 0.5)),
                life: 1.4 + 1.2 * (d[1] * 0.5 + 0.5),
                kind: SparkKind::Breach,
            });
        }
    }
}

/// Drop what has gone out, and hand what is left to the swarm as capsules.
///
/// The blast's radius is grown HERE rather than in the shader, which is what
/// lets a beam and a blast be one shape on the other side: the shader tests a
/// capsule and never learns there are two kinds.
fn age_fx(tick: Res<Tick>, mut fx: ResMut<LiveFx>, mut shots: ResMut<Shots>, sparks: Res<SparkQueue>) {
    fx.sparked += sparks.0.len();
    fx.beams.retain(|b| b.live(tick.tick));
    fx.blasts.retain(|b| b.live(tick.tick));
    shots.0.clear();
    for b in &fx.beams {
        shots.0.push(Capsule { from: Vec3::from(b.from), to: Vec3::from(b.to), radius: b.radius });
    }
    for b in &fx.blasts {
        let at = Vec3::from(b.at);
        shots.0.push(Capsule { from: at, to: at, radius: b.radius_at(tick.tick) });
    }
}

/// Rebuild the beam mesh: one strip of three quads per beam, turned edge on to
/// the eye.
///
/// Three quads rather than one so the beam has a soft edge: the outer columns
/// carry no alpha and the inner two carry all of it, which is a bright core
/// with a falloff either side. One quad could only be a flat slab.
fn draw_beams(
    tick: Res<Tick>,
    fx: Res<LiveFx>,
    handle: Option<Res<BeamHandle>>,
    cam: Query<&Transform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quads: ResMut<BeamQuads>,
) {
    let Some(handle) = handle else { return };
    let Ok(eye) = cam.single() else { return };
    let eye = eye.translation;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    const OFFSETS: [f32; 4] = [-1.0, -0.34, 0.34, 1.0];
    const ALPHAS: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
    for b in &fx.beams {
        let (from, to) = (Vec3::from(b.from), Vec3::from(b.to));
        let dir = (to - from).normalize_or_zero();
        let mid = (from + to) * 0.5;
        // Edge on to the eye, so the beam is the same width from anywhere.
        let across = dir.cross(eye - mid).normalize_or(Vec3::Y.cross(dir).normalize_or(Vec3::X));
        // It fires bright and goes out; the tail thins as it does.
        let fade = 1.0 - b.age(tick.tick);
        let w = b.radius * (0.35 + 0.65 * fade);
        // Well over white: the camera is HDR and bloom thresholds just under
        // one after tone mapping, so a beam has to CLEAR that to glow rather
        // than merely to be a pale blue line.
        let hot = Vec3::new(3.0, 5.0, 9.0) * fade;
        let base = pos.len() as u32;
        for c in 0..4 {
            let off = across * OFFSETS[c] * w;
            pos.push((from + off).to_array());
            pos.push((to + off).to_array());
            // The far end of a beam is dimmer than the muzzle, which is what
            // makes it read as travelling rather than as a painted line.
            col.push([hot.x, hot.y, hot.z, ALPHAS[c]]);
            col.push([hot.x * 0.5, hot.y * 0.5, hot.z * 0.5, ALPHAS[c] * 0.55]);
        }
        for c in 0..3u32 {
            let a = base + c * 2;
            idx.extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    quads.0 = idx.len() / 6;
    if pos.is_empty() {
        mesh = empty_mesh();
    } else {
        let n = pos.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        mesh.insert_indices(Indices::U32(idx));
    }
    let _ = meshes.insert(handle.0.id(), mesh);
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

/// Copy the eye onto everything at infinity, translation only: a star has a
/// direction and no position, and must not turn with the camera either.
fn ride_the_eye(cam: Query<&Transform, (With<Camera3d>, Without<AtInfinity>)>, mut far: Query<&mut Transform, With<AtInfinity>>) {
    let Ok(c) = cam.single() else { return };
    for mut xf in &mut far {
        let keep = xf.translation - xf.translation; // zero: everything here is placed about the eye
        xf.translation = c.translation + keep + (xf.rotation * Vec3::ZERO);
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
    fx: Res<LiveFx>,
    quads: Res<BeamQuads>,
    tex: Res<Textures>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
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
    println!(
        "fx: {} beams fired, {} live and {} quads on the last frame, {} blasts live, {} sparks queued",
        fx.fired, fx.beams.len(), quads.0, fx.blasts.len(), fx.sparked
    );
    // PROVE the textures loaded rather than asserting it: a normal map that
    // failed to decode is a material with no pixels in it, and a hull drawn
    // in flat paint looks exactly like a finish that was never applied.
    let mut missing = 0;
    let mut check = |name: String, handle: &Handle<Image>| {
        let state = assets.get_load_state(handle.id());
        let pixels = images.get(handle).and_then(|i| i.data.as_ref()).map_or(0, |d| d.len());
        let ok = pixels > 0 && !matches!(state, Some(LoadState::Failed(_)));
        if !ok {
            missing += 1;
            println!("texture {name}: MISSING ({state:?}, {pixels} bytes)");
        }
    };
    for (k, hd) in &tex.finishes {
        check(format!("armour_{k}_n"), hd);
    }
    for (k, m) in &tex.windows {
        check(format!("window_{k}_c"), &m.colour);
        check(format!("window_{k}_e"), &m.emissive);
        check(format!("window_{k}_n"), &m.normal);
    }
    if let Some(c) = &tex.chitin {
        check("alien_chitin_n".into(), c);
    }
    let total = tex.finishes.len() + tex.windows.len() * 3 + 1;
    println!("textures: {} of {} loaded with pixels", total - missing, total);
    let out = h.out.clone();
    let textures_ok = missing == 0;
    commands
        .spawn(Screenshot::image(t.0.clone()))
        .observe(save_to_disk(out.clone()))
        .observe(move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            let img: &Image = &shot.event().image;
            let (w, hh) = (img.width() as usize, img.height() as usize);
            let data = img.data.as_deref().unwrap_or(&[]);
            if data.len() < w * hh * 4 {
                println!("screenshot: no pixel data ({} bytes for {}x{}, {:?})", data.len(), w, hh, img.texture_descriptor.format);
                exit.write(AppExit::error());
                return;
            }
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
            let ok = all > (w * hh) / 100 && mid > 0 && textures_ok;
            exit.write(if ok { AppExit::Success } else { AppExit::error() });
        });
}
