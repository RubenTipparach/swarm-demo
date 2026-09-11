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
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension},
        view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
    },
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use swarm_core::{
    alien::{generate, Archetype},
    damage::{chunk_for, Chunk, DamageGrid, Vent},
    fx::{blast_sparks, breach_sparks, engines_of, gun_clusters, muzzle_sparks, reactor_of, Beam, Blast, Gun, Spark, SparkKind},
    mesh::{greedy_mesh, mesh_region, srgb_to_linear, MeshData, Surfaces},
    rng::{drift_of, Rng},
    sky::{bake_cubemap, starfield, to_half, SkyPreset},
    VoxelModel, SURF_COUNT,
};
use swarm_core::voxel::{mat, SURF_DRIVE};
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
    /// How many reinforcements are already inbound when the app starts, so a
    /// headless run can photograph a wave flying in.
    reinforce: u32,
    /// How many asteroids to strew between the ship and the carriers.
    rocks: usize,
    /// How many fighters the ship puts up.
    fighters: u32,
    /// How long the carriers sit before the first mote comes out. Ten by
    /// default, and nought is what a headless render wants: a shot aimed at
    /// tick ninety cannot wait ten seconds for the swarm to exist.
    launch_delay: f32,
    /// Where the camera sits, in radians. Exposed so a headless run can take
    /// the SAME tick from two different angles and compare them: "it looks
    /// wrong at some angles" is a claim about the projection, and the only way
    /// to answer it is to hold everything else still and turn the camera.
    yaw: f32,
    pitch: f32,
    /// Draw the HUD in a headless run. It is a window's furniture and there is
    /// nobody to press it, but a screenshot is the only way to PROVE it draws
    /// rather than assert it, which is the rule the textures already keep.
    hud: bool,
    /// Start paused, with the menu open, for the same reason.
    paused: bool,
    /// Whether `--launch-delay` was actually given. A headless run wants the
    /// swarm to exist on frame one: it is a HARNESS, and a harness that waits
    /// ten seconds for its subject to appear is ten seconds of every check
    /// spent rendering an empty sky. So headless defaults the delay to nought
    /// and a window defaults it to ten, and this is how "the user asked for
    /// nought" is told apart from "nobody said".
    delay_set: bool,
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
    /// How many motherships the swarm flies from.
    hives: usize,
    /// Issue one move order at startup, so a headless render can show the
    /// ship under way with its nav disc up.
    order: Option<Vec3>,
    /// Draw the four archetypes in a row beside the hull. Off by default now
    /// that the carriers are the aliens on show.
    showcase: bool,
    /// Frames a second, at most. Nought lifts the cap.
    fps: u32,
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
        reinforce: 0,
        rocks: 14,
        fighters: 12,
        launch_delay: 10.0,
        yaw: 0.6,
        pitch: 0.38,
        hud: false,
        paused: false,
        delay_set: false,
        zoom: 4.6,
        target: Vec3::ZERO,
        explode: 0,
        cadence: 70,
        hives: 10,
        order: None,
        showcase: false,
        fps: 120,
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
            "--hives" => { a.hives = next().parse().expect("--hives N"); i += 1; }
            "--move" => {
                let v: Vec<f32> = next().split(',').map(|x| x.parse().expect("--move x,y,z")).collect();
                a.order = Some(Vec3::new(v[0], v[1], v[2]));
                i += 1;
            }
            "--showcase" => a.showcase = true,
            "--hud" => a.hud = true,
            "--yaw" => { a.yaw = next().parse().expect("--yaw RADIANS"); i += 1; }
            "--pitch" => { a.pitch = next().parse().expect("--pitch RADIANS"); i += 1; }
            "--paused" => { a.hud = true; a.paused = true; }
            "--reinforce" => { a.reinforce = next().parse().expect("--reinforce N"); i += 1; }
            "--rocks" => { a.rocks = next().parse().expect("--rocks N"); i += 1; }
            "--launch-delay" => { a.launch_delay = next().parse().expect("--launch-delay SECONDS"); a.delay_set = true; i += 1; }
            "--fighters" => { a.fighters = next().parse().expect("--fighters N"); i += 1; }
            "--fps" => { a.fps = next().parse().expect("--fps N, or 0 for no cap"); i += 1; }
            other => panic!("unknown argument {other}"),
        }
        i += 1;
    }
    a
}

fn main() {
    let mut args = parse_args();
    // The opening beat is for a player. A headless run is a harness and wants
    // its subject on the first frame.
    if args.headless && !args.delay_set {
        args.launch_delay = 0.0;
    }
    let args = args;
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
        // BEFORE the camera reads it. These were two separate
        // `add_systems` calls with no ordering between them, so Bevy was free
        // to run them either way round and could pick differently from one
        // frame to the next: a drag then arrived a frame late on some frames
        // and not others, which is jitter that looks like a second camera
        // fighting the first. There is only ever one camera; there were two
        // possible orders.
        .add_systems(Update, orbit_input.before(orbit_camera));
    }
    // The HUD belongs to a window: a headless run has no pointer and nobody to
    // read a label. `--hud` builds it anyway, because a screenshot is the only
    // way to PROVE it draws rather than assert it, which is the rule the
    // finishes and the window maps already keep.
    if !args.headless || args.hud {
        app.add_systems(Startup, build_hud)
            .add_systems(
                Update,
                (toggle_pause, hud_feedback, tick_fps, select_input, draw_marquee, draw_bars, pick_hull),
            );
    }
    if args.fps > 0 {
        app.insert_resource(FrameLimit::new(args.fps)).add_systems(Last, limit_frames);
    }
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(SwarmConfig { count: args.motes, fixed_dt: args.fixed_dt, ..default() })
        .insert_resource(Scene {
            hull: args.hull.clone(),
            chewers: args.chewers,
            yaw: args.yaw,
            pitch: args.pitch,
            reinforce: args.reinforce,
            rocks: args.rocks,
            fighters: args.fighters,
            launch_delay: args.launch_delay,
            zoom: args.zoom,
            target: args.target,
            explode: args.explode,
            cadence: args.cadence,
            hives: args.hives.min(swarm::MAX_HIVES),
            order: args.order,
            showcase: args.showcase,
            fixed_dt: args.fixed_dt,
        })
        .init_resource::<Tick>()
        .init_resource::<ChunkMaterials>()
        .add_plugins(SwarmPlugin)
        .init_resource::<Shots>()
        .init_resource::<SparkQueue>()
        .init_resource::<LiveFx>()
        .init_resource::<NavOrder>()
        .init_resource::<BeamQuads>()
        .init_resource::<Lead>()
        .init_resource::<Marquee>()
        .init_resource::<BarPool>()
        .insert_resource(Hud { paused: args.paused, ..default() })
        .add_systems(Startup, (load_textures, setup).chain())
        .add_systems(
            Update,
            (
                // Everything that MOVES stops while the menu is open. The
                // swarm's own clock is stopped by `SwarmConfig.paused`, which
                // is a separate flag because the cloud is in the render world
                // and cannot see this one; these are the CPU half, and without
                // the gate a paused game would still fly its ships, fire its
                // guns and chew its armour behind a menu that said Paused.
                // Orders are given whether or not the world is running, which
                // is the whole point of a pause key in an RTS, so `nav_input`
                // sits outside the gate with the camera and the drawing.
                nav_input,
                (
                    advance_tick,
                    (apply_nav_to, call_reinforcements, fly_hull, publish_hull).chain(),
                    move_hives,
                    (fire_guns, fire_flak).chain(),
                    (launch_fighters, fly_fighters, fighters_fire, wear_fighters).chain(),
                    resolve_beams,
                    (chew, vent_smoke, go_critical, bleed_hives),
                    publish_hives,
                    fly_tracers,
                    age_fx,
                    fly_chunks,
                    spin_showcase,
                )
                    .chain()
                    .run_if(running),
                // And everything that only DRAWS keeps running, or a paused
                // frame would show the last thing that was built rather than
                // the world as it stands: the beams, the nav disc and the
                // flames are all rebuilt every frame from state, so skipping
                // them empties their meshes and the picture goes blank behind
                // the menu.
                (draw_beams, draw_nav, draw_flames, glow_engines, aim_turrets),
                remesh_dirty,
                (orbit_camera, ride_the_eye),
            )
                .chain(),
        )
        .run();
}

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
struct FrameLimit {
    budget: Duration,
    next: Instant,
}

impl FrameLimit {
    fn new(fps: u32) -> Self {
        FrameLimit { budget: Duration::from_secs_f64(1.0 / fps as f64), next: Instant::now() }
    }
}

fn limit_frames(mut limit: ResMut<FrameLimit>) {
    let now = Instant::now();
    if limit.next > now {
        std::thread::sleep(limit.next - now);
    }
    limit.next = limit.next.max(now) + limit.budget;
}

#[derive(Resource)]
struct Scene {
    hull: String,
    yaw: f32,
    pitch: f32,
    chewers: usize,
    reinforce: u32,
    rocks: usize,
    fighters: u32,
    launch_delay: f32,
    zoom: f32,
    target: Vec3,
    explode: u32,
    cadence: u32,
    hives: usize,
    order: Option<Vec3>,
    showcase: bool,
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
    t.acc += time.delta_secs().min(swarm::STEP_CLAMP);
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
    /// And its engines, read off the cells the export says are propulsion.
    /// Not the thrusters: those wear the same surface and are all over a
    /// hull, and a thruster plumed constantly is a ship that never stops
    /// spinning.
    engines: Vec<Gun>,
    /// Where it has been told to go, in world units, or nothing.
    order: Option<Vec3>,
    vel: Vec3,
    /// Last frame's change in velocity, which is what decides whether the
    /// main engines are burning or the retros are.
    accel: Vec3,
    breaches: usize,
    last_heat_key: u32,
    /// How many cells it started with, so "enough of it is gone" is a share
    /// rather than a number that means something different on every class.
    cells: usize,
    dead_hull: bool,
    /// The cells of its reactor, derived from the hull's own geometry: the
    /// most buried place in the ship and a ball around it. Half of these gone
    /// is what takes the ship, and nothing else does.
    reactor: Vec<usize>,
    /// This SHIP's own seed, mixed into everything hashed off a cell.
    ///
    /// Two ships of a class have the same cells, so a gun's firing phase,
    /// hashed off its cell alone, came out the same on every hull in the
    /// formation: four frigates fired in one volley, on the same tick, for
    /// ever. A seed per ship is what staggers them.
    seed: u32,
}

/// Where the flagship is, was heading and how fast, for whoever has to fly in
/// formation on it.
///
/// A resource rather than a second query, because `fly_hull` already holds
/// every hull's `Transform` mutably and Bevy refuses a read of the same
/// component in the same system. Published a frame behind, which a formation
/// cannot see: a ship a sixtieth of a second stale is a ship one centimetre
/// out of place.
#[derive(Resource, Default)]
struct Lead {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
}

/// An order handed to a hull that does not exist yet.
///
/// `spawn_hull` inserts the `Hull` through `Commands`, so nothing in the same
/// system can reach into it; this is the order waiting for it, applied the
/// moment it is really there.
#[derive(Component)]
struct NavTo(Vec3);

/// The ship the player commands. The camera follows it, the nav disc gives it
/// orders and the swarm chases it.
///
/// Exactly one hull carries this, and the three systems that want THE ship
/// rather than A ship ask for it: before reinforcements there was one hull and
/// `single()` said so, which silently became "do nothing at all" the moment
/// there were two.
#[derive(Component)]
struct Flagship;

/// A ship that came in as a reinforcement.
///
/// It has no orders of its own: it keeps station on the flagship, at an offset
/// in the FLAGSHIP's own frame rather than the world's, so a formation turns
/// with the ship it is flying beside instead of sliding round it.
#[derive(Component)]
struct Escort {
    station: Vec3,
}

struct Chewer {
    at: Vec3,
    next: u32,
}

#[derive(Component)]
struct Showcase;

/// One of the ship's own fighters.
///
/// It has no damage grid and no hit points: the swarm cannot shoot back, so a
/// fighter is a gun the player owns that flies rather than a ship that can be
/// lost. When the swarm can kill one, this grows a hull like everything else.
#[derive(Component)]
struct Fighter {
    /// Which slot of the squadron it is, which is what spreads the patrol
    /// stations and staggers the firing.
    slot: u32,
    /// One down to nought. A fighter patrols inside the ring the swarm holds,
    /// which is the densest part of the cloud, so it is worn down by BEING
    /// there rather than by any particular mote: the CPU cannot see where a
    /// mote is, so attrition by depth into the swarm's own band is the honest
    /// approximation and it puts the cost where the risk is.
    hp: f32,
    vel: Vec3,
    /// Where it is heading right now, in the world. Re-picked when it gets
    /// there, so a fighter flies a circuit of the cloud instead of parking.
    goal: Vec3,
}

/// One gun, drawn as its own object so it can turn.
///
/// A turret's cells are lifted OUT of the hull's own mesh and given a child
/// entity that pivots on the cluster's middle. The alternative is redux-tribes'
/// approach of rewriting the turret's quads inside the hull's geometry every
/// frame, which it does for a reason that does not apply here: its hulls carve
/// holes through the same buffers. Here a gun is three to a ship and a child
/// transform is free, so the mesh is built once and only a rotation changes.
#[derive(Component)]
struct Turret {
    /// Which gun of the hull's list this is, so the aim matches the fire.
    slot: usize,
    /// Where it rests, in the hull's frame, when it has nothing to shoot at.
    rest: Vec3,
}

/// How fast a turret slews, in radians a second. Slow enough to watch.
const TURRET_SLEW: f32 = 1.9;

/// An asteroid. Drawn and navigated round, and that is all it does: it has no
/// damage grid, because nothing in the game can hurt a rock yet and a grid
/// per rock is eight thousand cells of book keeping for a fact nobody asks.
#[derive(Component)]
struct Rock;

/// How big a mothership is, in hull radii, and what it takes to kill one.
const HIVE_RADIUS: f32 = 1.6;
/// What a carrier's chitin is worth, against the bare material.
///
/// NOT the hundred the player's plating carries. The armour multiplier is
/// what makes the ship a siege, and putting it on both sides would mean
/// neither could hurt the other and the battle would never resolve. Eight is
/// enough that a carrier takes sustained fire and not so much that it cannot
/// be killed inside a match.
const HIVE_ARMOUR: f32 = 8.0;
/// What one beam does to each cell it bites, and how many bites it takes.
///
/// A ring of bites rather than one, because a beam that took a single cell
/// off a mothership would need thousands of shots to show and the picture
/// would never change. Eight opens a crater the size of the weapon.
const BEAM_DAMAGE: f32 = 260.0;
const BEAM_BITES: u32 = 8;
/// How far a gun shoots, in hull radii. Far enough to reach the carriers,
/// which stand well off.
const BEAM_RANGE: f32 = 34.0;
/// The asteroid field: how many cells a rock is built on, what one of those
/// cells is worth against the ship's own radius, where the belt sits in hull
/// radii, and how much of a rock's own radius the navigation sphere fills.
const ROCK_LATTICE: usize = 22;
const ROCK_CELL: f32 = 0.16;
const ROCK_NEAR: f32 = 5.0;
const ROCK_FAR: f32 = 13.0;
/// Against the VOLUME radius, which is already the rock's mean size, so this
/// is a small trim rather than the deep inset a bounding sphere needed. A
/// sphere that CONTAINED a lumpy rock stands the swarm off well clear of the
/// thin axes, and a cloud swerving round empty space is worse than one
/// clipping a corner.
const ROCK_HULL: f32 = 0.95;
/// How far off a rock a SHIP holds, in its own radii, on top of the rock's.
const ROCK_CLEAR: f32 = 1.6;

/// How fast the camera pans, as a share of its own distance per second.
const PAN_RATE: f32 = 0.9;

/// The most one mouse event may turn the camera, in pixels. A real drag is a
/// few dozen a frame; anything past this is the window system, not a hand.
const MAX_DRAG: f32 = 120.0;

/// What a warship's plating is worth, against the bare material.
///
/// ONE, which is what it always was before a hundred was tried. The hundred
/// made a ship that could not be hurt, and what actually needed fixing was
/// never the plating: it was that losing a tenth of your cells anywhere at all
/// blew the ship up. `REACTOR_LOSS` is what fixed that, and it does the job on
/// its own. A cell comes off in a few bites again, so the swarm visibly eats a
/// hull, and the ship still survives it, because a hole in the plating is not
/// a hole in the reactor.
pub const ARMOUR: f32 = 1.0;

/// The ship's own strike craft: how many it puts up, how far out they work,
/// how fast they fly and how often each one fires.
///
/// A dozen, and they are ENTITIES rather than motes, for the reason the whole
/// design turns on: the ECS holds what there are dozens of. They share one
/// mesh and one material between them, so twelve fighters are twelve
/// transforms and one upload, which is why a dozen is free and a thousand
/// would not be.
const FIGHTERS_HULL: &str = "terran_corvette";
/// How big a fighter is drawn, against the flagship's own radius.
const FIGHTER_SCALE: f32 = 0.16;
/// Where they work, in flagship radii: out at the swarm's own standoff.
const FIGHTER_STATION: f32 = 3.4;
const FIGHTER_SPEED: f32 = 9.0;
const FIGHTER_TURN: f32 = 2.6;
/// Ticks between one fighter's bursts, and how big a burst is in flagship
/// radii. Short ranged: a fighter has to GO to the cloud.
const FIGHTER_CADENCE: u32 = 14;
/// How fast a fighter is worn down at the very centre of the swarm's band, in
/// share of its life per second, and how fast it patches up once clear of it.
/// How many ticks between replacements, so a wing rebuilds rather than
/// popping back into existence the frame it was lost.
const FIGHTER_REPLACE: u32 = 150;
const FIGHTER_WEAR: f32 = 0.22;
const FIGHTER_MEND: f32 = 0.10;
const FIGHTER_BURST: f32 = 0.30;

/// How many escorts a wing holds, and how many one press of R brings.
const WING_MAX: u32 = 6;
const WING_WAVE: u32 = 2;

/// Where the carriers sit, in hull radii. They used to be at seven, which put
/// them inside the swarm's own standoff and made the whole picture one clump;
/// a carrier is a thing you have to CROSS the battlefield to reach.
const HIVE_NEAR: f32 = 14.0;
const HIVE_FAR: f32 = 22.0;

/// A mothership: where the swarm comes from, and the thing worth killing.
#[derive(Component)]
struct Hive {
    /// Read off its own cells, the same query a ship's engines come from.
    engines: Vec<Gun>,
    /// In world units, which is what the shader is told and what a beam is
    /// tested against. The model's own radius times the scale it is drawn at.
    radius: f32,
    /// What it is drawn at, which is what turns a cell offset into a world
    /// one anywhere the model's own coordinates are used.
    scale: f32,
    vel: Vec3,
    seed: u64,
}

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

/// Concatenate the surfaces a predicate keeps, as one mesh.
/// Move every vertex of a mesh, so a part built in the ship's coordinates can
/// be drawn about its own pivot instead.
fn shift_mesh(mesh: &mut Mesh, by: Vec3) {
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for v in p.iter_mut() {
            v[0] += by.x;
            v[1] += by.y;
            v[2] += by.z;
        }
    }
}

fn to_mesh_where(s: &Surfaces, keep: impl Fn(usize) -> bool) -> Mesh {
    let mut all = MeshData::default();
    for (i, md) in s.skin.iter().enumerate() {
        if keep(i) {
            all.append(md);
        }
    }
    to_mesh(&all)
}

/// A mote, as one mesh with its LIT cells marked.
///
/// The motes are one instanced draw, so their lit cells cannot be a second
/// mesh the way a carrier's are: there is nowhere to put it. The marker rides
/// in the vertex colour's ALPHA instead, which every other picture ignores
/// because they are all opaque, and which `mote.wgsl` reads as "this cell is
/// its own light".
fn to_mote_mesh(s: &Surfaces) -> Mesh {
    let mut all = MeshData::default();
    for (i, md) in s.skin.iter().enumerate() {
        let mut copy = md.clone();
        let lit = i == SURF_DRIVE as usize;
        for c in copy.colours.iter_mut() {
            c[3] = if lit { 0.0 } else { 1.0 };
        }
        all.append(&copy);
    }
    to_mesh(&all)
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

/// Build one ship: its materials, its bricks, its guns and engines, and put
/// it in the world at `at`.
///
/// Factored out of `setup` the day reinforcements arrived, because a ship
/// that can only be made while the app is starting is a ship a player can
/// never call for. Everything about a hull that is per SHIP rather than per
/// class lives here, which is why the materials are made fresh per call: two
/// frigates sharing one wound material would burn together.
#[allow(clippy::too_many_arguments)]
fn spawn_hull(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    which: &str,
    at: Transform,
    chewers: u32,
    seed: u32,
    station: Option<Vec3>,
) -> (Entity, f32) {
    let model = load_hull(which);
    let surf = surface_materials(&model, tex, materials);
    let win = window_materials(&model, tex, materials);
    spawn_ship(commands, meshes, materials, tex, model, surf, win, at, chewers, seed, station, ARMOUR, Some(which))
}

/// One ship of any kind: a hull off the shelf, or a mothership, or anything
/// else that is a voxel model somebody can shoot cells off.
///
/// Factored out of `spawn_hull` the day the carriers got a damage grid. They
/// used to be a single mesh with one floating hit point number on it, and the
/// whole per cell model, the bricks, the four layer wound, the chunks that
/// come off and the re-mesh of only what changed, existed on one side of the
/// battle. There was never a reason for that beyond the order things were
/// built in: a carrier is a voxel model, and everything here works on a voxel
/// model.
#[allow(clippy::too_many_arguments)]
fn spawn_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    model: VoxelModel,
    surface_mats: Vec<Handle<StandardMaterial>>,
    window_mats: Vec<Handle<StandardMaterial>>,
    at: Transform,
    chewers: u32,
    seed: u32,
    station: Option<Vec3>,
    armour: f32,
    log: Option<&str>,
) -> (Entity, f32) {
    let radius = model.radius();
    // ---- the turrets come out of the hull first ----
    //
    // A gun that turns cannot be part of the mesh it is bolted to, so its
    // cells are taken off the model the hull is built from and drawn as their
    // own child. The clusters are read BEFORE the cells are removed, or there
    // would be nothing left to find.
    let turrets: Vec<(Gun, [f32; 3], Vec<usize>)> = gun_clusters(&model);
    let mut model = model;
    for (_, _, cells) in &turrets {
        for &c in cells {
            model.grid[c] = swarm_core::mat::EMPTY;
        }
    }
    let model = model;
    let damage = DamageGrid::with_armour(&model, armour);
    let hull_entity = commands.spawn((at, Visibility::default())).id();
    match station {
        Some(station) => {
            commands.entity(hull_entity).insert(Escort { station });
        }
        // Only a ship that is LOGGED is the flagship. A carrier is neither a
        // flagship nor an escort, and marking one would put the nav disc, the
        // camera and the swarm's own target on a mothership.
        None if log.is_some() => {
            // Selected out of the box, so the very first right button opens an
            // order instead of doing nothing at all. A game that starts with
            // nothing picked is a game whose first click teaches you nothing.
            commands.entity(hull_entity).insert((Flagship, Selected));
        }
        None => {}
    }
    let guns: Vec<Gun> = turrets.iter().map(|(g, _, _)| *g).collect();
    let engines = engines_of(&model);
    let cells = model.solid_count();
    let mut hull = Hull {
        surface_mats,
        window_mats,
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
            // Well clear of the plate it is laid on. A wound quad and the
            // face it replaces are the SAME plane, so whichever the depth
            // test happens to prefer changes from pixel to pixel and from
            // angle to angle, which is the shimmer along a torn edge. A bias
            // of two was enough at arm's length and not at the far end of a
            // cruiser, because depth precision is not linear: the same offset
            // in depth units is a smaller offset in world units the further
            // out it is applied.
            depth_bias: 24.0,
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
            // Under the burn and over the plate, and both gaps wide enough to
            // survive the far end of the depth range.
            depth_bias: 12.0,
            ..default()
        }),
        bricks: (0..damage.brick_count()).map(|_| Brick::default()).collect(),
        chewers: Vec::new(),
        guns,
        engines,
        order: None,
        vel: Vec3::ZERO,
        accel: Vec3::ZERO,
        breaches: 0,
        last_heat_key: 0,
        cells,
        dead_hull: false,
        reactor: reactor_of(&model),
        seed,
        model,
        damage,
    };
    let (mut quads, mut windows) = (0, 0);
    for b in 0..hull.damage.brick_count() {
        let (lo, hi) = hull.damage.brick_bounds(b);
        let s = mesh_region(&hull.model, Some((&hull.damage, 0)), lo, hi);
        quads += s.skin_quads();
        windows += s.window_quads();
        place_brick(commands, meshes, &mut hull, b, &s, hull_entity);
    }
    hull.damage.take_dirty();

    // And the guns, one child each, built from their own cells about their own
    // pivot. The mesh is in the PIVOT's frame, so rotating the child rotates
    // the turret about its base instead of about the middle of the ship.
    for (slot, (gun, mid, cells)) in turrets.iter().enumerate() {
        let mut only = VoxelModel::new(hull.model.nx, hull.model.ny, hull.model.nz, hull.model.cell);
        only.surfaces = hull.model.surfaces.clone();
        for &c in cells {
            only.grid[c] = mat::MACHINE;
            only.colour[c] = hull.model.colour[c];
            only.surf[c] = hull.model.surf[c];
            only.tone[c] = hull.model.tone[c];
        }
        let s = greedy_mesh(&only, None);
        let pivot = Vec3::from(*mid);
        for surf in 0..SURF_COUNT {
            if s.skin.get(surf).map(|x| x.quads()).unwrap_or(0) == 0 {
                continue;
            }
            let mut mesh = to_mesh_where(&s, |i| i == surf);
            shift_mesh(&mut mesh, -pivot);
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(hull.surface_mats[surf].clone()),
                Transform::from_translation(pivot),
                Turret { slot, rest: Vec3::from(gun.out).normalize_or(Vec3::Z) },
                ChildOf(hull_entity),
            ));
        }
    }

    // Chewers stand on random exposed cells, one cell out along the open face.
    if chewers > 0 {
        let whole = greedy_mesh(&hull.model, None).skin_all();
        let mut rng = Rng::new(7 + seed as u64);
        hull.chewers = (0..chewers)
            .map(|n| {
                let q = rng.int(0, whole.quads() as i32 - 1) as usize;
                let cell = whole.quad_cell[q] as usize;
                let nrm = whole.normals[q * 4];
                let c = hull.model.centre_of(cell);
                let cs = hull.model.cell;
                Chewer { at: Vec3::new(c[0] + nrm[0] * cs, c[1] + nrm[1] * cs, c[2] + nrm[2] * cs), next: (n * 7) % 40 }
            })
            .collect();
    }

    if let Some(which) = log {
        let used: Vec<String> = (0..SURF_COUNT)
            .filter(|&s| hull.bricks.iter().any(|b| b.skin.get(s).and_then(|p| p.entity).is_some()))
            .map(|s| format!("{s}:{}", hull.model.surfaces.get(s).map(|x| x.finish.as_str()).unwrap_or("?")))
            .collect();
        info!(
            "hull {}: {} cells, {} quads over {} bricks, {} window faces of {} kinds, {} guns, {} engines, surfaces [{}], radius {:.2}",
            which, hull.cells, quads, hull.damage.brick_count(), windows, hull.model.window_kinds.len(),
            hull.guns.len(), hull.engines.len(), used.join(" "), radius
        );
    }
    commands.entity(hull_entity).insert(hull);
    (hull_entity, radius)
}

/// Call one reinforcement in, off the map, on the `n`th station of a wing.
///
/// The stations are a shallow V behind the flagship and alternating to port
/// and starboard, which is the formation the whole point of a wing is: a ship
/// that sat directly astern would be the one place its leader's own engines
/// are, and a column would put every gun in the line of the one in front.
///
/// An escort carries chewers of its own, fewer than the flagship. The GPU
/// swarm knows one hull centre and chases the flagship alone, so without them
/// a wing would be four ships that add guns and can never be hurt: calling
/// reinforcements would be free, which is the one thing a reinforcement must
/// not be.
fn call_one(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    which: &str,
    radius: f32,
    lead_pos: Vec3,
    lead_rot: Quat,
    chewers: u32,
    n: u32,
) {
    let rank = (n / 2) as f32 + 1.0;
    let side = if n % 2 == 0 { 1.0 } else { -1.0 };
    let station = Vec3::new(side * rank * radius * 2.6, 0.0, -rank * radius * 2.2);
    // It ARRIVES: dropped well outside the formation, on the far side of its
    // own station, so a wave flies in past the camera rather than appearing
    // in the middle of the fight.
    let at = lead_pos + lead_rot * (station + Vec3::new(side * radius * 4.0, radius * 1.5, -radius * 9.0));
    let xf = Transform::from_translation(at).looking_to(lead_rot * Vec3::NEG_Z, Vec3::Y);
    spawn_hull(commands, meshes, materials, tex, which, xf, chewers, 0x9E37 + n * 0x4F1B, Some(station));
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
        let engines = engines_of(&m);
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
        AmbientLight { color: Color::srgb(0.55, 0.62, 0.80), brightness: 6.0, ..default() },
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
    /// Rounds in the air. A flak burst used to appear where it was going to
    /// go off, which is a gun with no shell: the muzzle flashed, the target
    /// flashed, and nothing crossed the gap between them. A tracer is that
    /// gap, drawn as a short bright bolt travelling at a real speed, and the
    /// blast is pushed when it ARRIVES rather than when the trigger is
    /// pulled.
    tracers: Vec<Tracer>,
    /// Totals over the whole run, for the headless report. A picture with no
    /// beam in it and a picture of a beam that was never fired look the same,
    /// and only one of them is a bug in this file.
    fired: usize,
    flak: usize,
    sparked: usize,
}

/// One round in flight.
#[derive(Clone, Copy)]
struct Tracer {
    from: Vec3,
    to: Vec3,
    /// How far along it is, nought to one.
    t: f32,
    /// How much of the flight one tick covers.
    rate: f32,
    /// What it does when it lands, in world units.
    burst: f32,
    colour: [f32; 3],
}

#[derive(Component)]
struct BeamMesh;

#[derive(Resource)]
struct BeamHandle(Handle<Mesh>);

#[derive(Resource)]
struct NavHandle(Handle<Mesh>);

#[derive(Resource)]
struct FlameHandle(Handle<Mesh>);

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

/// Carriers drift across the line to the ship, and turn as they go.
fn move_hives(time: Res<Time>, scene: Res<Scene>, mut hives: Query<(&Hive, &mut Transform)>) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (h, mut xf) in &mut hives {
        xf.translation += h.vel * dt;
        xf.rotate_y(dt * 0.13);
    }
}

/// Tell the swarm where its carriers are.
///
/// Only the LIVE ones, compacted: the shader takes a mote's hive index modulo
/// the length, so a carrier dying shortens this list and every fighter that
/// flew from it re-homes to whatever is left. No rule about it anywhere.
/// Put the squadron up, once, out of the flagship.
///
/// One mesh and one material for the whole squadron, built here and cloned
/// into every fighter: a corvette is a few thousand cells and meshing it
/// twelve times would cost twelve times as much for twelve identical
/// pictures. They carry no damage grid for the same reason they carry no hit
/// points, which is that nothing can hurt them yet.
fn launch_fighters(
    tick: Res<Tick>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    scene: Res<Scene>,
    lead: Res<Lead>,
    flagship: Query<&Hull, With<Flagship>>,
    have: Query<(), With<Fighter>>,
    mut next: Local<u32>,
) {
    if scene.fighters == 0 {
        return;
    }
    let Ok(hull) = flagship.single() else { return };
    // Replaces losses rather than launching once. A squadron that could only
    // ever be spent would make `wear_fighters` a countdown to having none,
    // which is not a screen, it is a fuse.
    let out = have.iter().count() as u32;
    if out >= scene.fighters || tick.tick < *next {
        return;
    }
    *next = tick.tick + FIGHTER_REPLACE;
    // The whole squadron at once the first time, one at a time after that.
    // Pacing the FIRST launch would mean half a minute before the wing exists,
    // which is a screen that arrives after the fight it was meant to screen.
    let want = if out == 0 { scene.fighters } else { 1 };
    let radius = hull.model.radius();
    let m = load_hull(FIGHTERS_HULL);
    let sm = greedy_mesh(&m, None);
    let mats = surface_materials(&m, &tex, &mut materials);
    let scale = radius * FIGHTER_SCALE / m.radius();
    // One mesh per surface, made once, shared by every fighter.
    let parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)> = (0..SURF_COUNT)
        .filter(|&surf| sm.skin.get(surf).map(|x| x.quads()).unwrap_or(0) > 0)
        .map(|surf| (meshes.add(to_mesh_where(&sm, |i| i == surf)), mats[surf].clone()))
        .collect();

    for n in out..(out + want).min(scene.fighters) {
        // Out of the flagship, spread round it, so a launch reads as coming
        // OFF the ship rather than appearing beside it.
        let a = n as f32 * 2.399_963_2;
        let off = Vec3::new(a.cos(), (n % 3) as f32 * 0.4 - 0.4, a.sin()) * radius * 0.9;
        let at = lead.pos + lead.rot * off;
        let f = commands
            .spawn((
                Transform::from_translation(at).with_scale(Vec3::splat(scale)),
                Visibility::default(),
                Fighter { slot: n, hp: 1.0, vel: Vec3::ZERO, goal: at },
            ))
            .id();
        for (mesh, mat) in &parts {
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::IDENTITY, ChildOf(f)));
        }
    }
    if out == 0 {
        info!("squadron up: {} of {} on {}", want, scene.fighters, FIGHTERS_HULL);
    }
}

/// Fly the squadron: a circuit of the swarm's own standoff, round the ship.
///
/// A fighter picks a point out where the cloud is, flies to it, and picks
/// another when it arrives. That is a PATROL rather than an intercept, and it
/// is deliberately not an intercept: the CPU cannot see where a mote is, so
/// there is nothing to intercept. Flying the circuit and firing into it is
/// what a screen actually does, and it puts the shots where the swarm has to
/// come through.
fn fly_fighters(
    time: Res<Time>,
    scene: Res<Scene>,
    tick: Res<Tick>,
    lead: Res<Lead>,
    flagship: Query<&Hull, With<Flagship>>,
    mut fighters: Query<(&mut Fighter, &mut Transform)>,
) {
    let Ok(hull) = flagship.single() else { return };
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    let radius = hull.model.radius();
    let station = radius * FIGHTER_STATION;
    for (mut f, mut xf) in &mut fighters {
        let to = f.goal - xf.translation;
        if to.length() < radius * 0.6 {
            // A new point on the sphere the swarm holds, hashed off the slot
            // and the tick rather than rolled, so a re-watch flies the same
            // circuit. Spread round the ship rather than randomly placed, or
            // twelve fighters converge on one patch and leave the rest open.
            let h = |k: u32| swarm_core::rng::hash_cell(f.slot * 7919 + tick.tick + k) as f32 / u32::MAX as f32;
            let a = (f.slot as f32 * 2.399_963_2) + h(1) * 2.2;
            let y = h(2) * 1.6 - 0.8;
            let r = (1.0 - y * y).max(0.05).sqrt();
            f.goal = lead.pos + Vec3::new(r * a.cos(), y, r * a.sin()) * station;
        }
        let want = (f.goal - xf.translation).normalize_or(Vec3::Z) * radius * FIGHTER_SPEED;
        let dv = want - f.vel;
        let step = radius * FIGHTER_SPEED * 1.4 * dt;
        f.vel += if dv.length() > step { dv.normalize() * step } else { dv };
        xf.translation += f.vel * dt;
        if f.vel.length() > 1e-3 {
            // Negated for the reason every hull here is negated: Bevy's
            // forward is minus Z and a hull's bow is plus Z.
            let aim = Transform::from_translation(xf.translation)
                .looking_to(-f.vel.normalize(), Vec3::Y)
                .rotation;
            xf.rotation = xf.rotation.slerp(aim, (dt * FIGHTER_TURN).min(1.0));
        }
    }
}

/// A fighter is worn down by where it flies, and comes apart when it runs out.
///
/// It cannot be shot by a named mote, because no mote has a name on the CPU:
/// they live in a buffer and never come back. What IS knowable is where the
/// swarm holds, which is the ring round each ship, and a fighter patrols
/// inside that band on purpose. So attrition is depth into the band, which
/// puts the cost exactly where the risk is and needs nothing crossing the
/// boundary.
fn wear_fighters(
    time: Res<Time>,
    scene: Res<Scene>,
    tick: Res<Tick>,
    cfg: Res<SwarmConfig>,
    mut commands: Commands,
    mut sparks: ResMut<SparkQueue>,
    mut fighters: Query<(Entity, &mut Fighter, &Transform)>,
) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (e, mut f, xf) in &mut fighters {
        // How deep into the thickest part of the cloud it is, over every ship
        // the swarm is divided between.
        let mut deep = 0.0f32;
        for t in &cfg.targets {
            let d = xf.translation.distance(t.truncate());
            let band = t.w * FIGHTER_STATION;
            if d < band {
                deep = deep.max(1.0 - d / band.max(1e-4));
            }
        }
        if deep > 0.0 {
            f.hp -= deep * FIGHTER_WEAR * dt;
        } else {
            // Clear of it, and patching itself up.
            f.hp = (f.hp + FIGHTER_MEND * dt).min(1.0);
        }
        if f.hp > 0.0 {
            continue;
        }
        // Gone, and it goes the way everything else here goes: its own burst,
        // in the colours of the side it was on.
        let mut out: Vec<Spark> = Vec::new();
        blast_sparks(f.slot.wrapping_mul(7919) ^ tick.tick, xf.translation.to_array(), 0.9, 40, &mut out);
        sparks.extend(out);
        commands.entity(e).despawn();
    }
}

/// And they shoot: a short burst ahead of where they are going.
///
/// A `Blast` of a small radius, which is a capsule of zero length, which is
/// the shape everything that kills a mote already is. The squadron therefore
/// needed no new weapon and no new resolution path: the swarm shader kills
/// whatever is inside the capsule and the CPU never learns what was.
fn fighters_fire(
    tick: Res<Tick>,
    scene: Res<Scene>,
    flagship: Query<&Hull, With<Flagship>>,
    fighters: Query<(&Fighter, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    let Ok(hull) = flagship.single() else { return };
    let radius = hull.model.radius();
    for (f, xf) in &fighters {
        // Staggered by slot, or twelve fighters fire on the same tick for
        // ever, which is one big burst rather than a screen putting up fire.
        if (tick.tick + f.slot * 5) % FIGHTER_CADENCE != 0 {
            continue;
        }
        let nose = xf.rotation * Vec3::Z;
        let at = xf.translation + nose * radius * 0.3;
        let burst = radius * FIGHTER_BURST;
        fx.blasts.push(Blast { at: at.to_array(), radius: burst, born: tick.tick });
        let mut out: Vec<Spark> = Vec::new();
        muzzle_sparks(f.slot * 977 ^ tick.tick, at.to_array(), nose.to_array(), radius * 0.05, &mut out);
        sparks.extend(out);
    }
}

/// How many ticks a round takes to cross to where it is going.
const TRACER_TICKS: u32 = 7;
/// How long a bolt is drawn, as a share of its whole flight, and how wide.
const TRACER_LEN: f32 = 0.22;
const TRACER_WIDTH: f32 = 0.06;

/// Fly the rounds, and set off the ones that have arrived.
///
/// The blast is pushed HERE rather than where the trigger was pulled, so what
/// kills a mote is the shell reaching it. That also means a player can watch a
/// burst travel into the cloud and see the hole it makes appear at the end of
/// its own flight, which is the whole reason for having a shell at all.
fn fly_tracers(tick: Res<Tick>, mut fx: ResMut<LiveFx>, mut sparks: ResMut<SparkQueue>) {
    let mut landed: Vec<Tracer> = Vec::new();
    for t in fx.tracers.iter_mut() {
        t.t += t.rate;
        if t.t >= 1.0 {
            landed.push(*t);
        }
    }
    fx.tracers.retain(|t| t.t < 1.0);
    for t in landed {
        fx.blasts.push(Blast { at: t.to.to_array(), radius: t.burst, born: tick.tick });
        let mut list = Vec::new();
        blast_sparks(tick.tick.wrapping_add(t.to.x.to_bits()), t.to.to_array(), t.burst, 26, &mut list);
        for s in &mut list {
            s.life *= 0.34;
            s.size *= 0.9;
        }
        fx.sparked += list.len();
        sparks.extend(list);
    }
}

/// Swing every turret onto whatever its ship is shooting at.
///
/// The aim is the SAME answer `fire_guns` uses, so the barrel and the beam
/// agree: a turret that pointed somewhere the beam did not come out of would
/// be a decoration rather than a gun. It eases on a slew cap so a gun takes
/// time to come round, and it stands down to the facing its own cluster looks
/// out along when there is nothing in reach.
///
/// Everything is in the HULL's frame. The child's transform is relative to its
/// parent already, so the target has to be taken into that frame first, and
/// the rotation is then a plain `looking_to` with no ship pose in it at all.
fn aim_turrets(
    time: Res<Time>,
    scene: Res<Scene>,
    // Both of these must say `Without<Turret>`. Bevy proves two queries
    // disjoint from their FILTERS, not from what you know about the data: it
    // cannot tell that nothing is both a carrier and a turret, so a plain
    // `&Transform` on the carriers conflicts with the `&mut Transform` here.
    hulls: Query<(&Hull, &Transform), Without<Turret>>,
    hives: Query<(&Hive, &Transform, &Hull), Without<Turret>>,
    mut turrets: Query<(&Turret, &ChildOf, &mut Transform)>,
) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (t, parent, mut xf) in &mut turrets {
        let Ok((hull, ship)) = hulls.get(parent.parent()) else { continue };
        if hull.dead_hull {
            continue;
        }
        let Some(gun) = hull.guns.get(t.slot) else { continue };
        let radius = hull.model.radius();
        let muzzle = ship.transform_point(Vec3::from(gun.at));

        // The nearest carrier it could reach, which is what `fire_guns` picks.
        let mut best: Option<(f32, Vec3)> = None;
        for (h, hxf, hhull) in &hives {
            if hhull.dead_hull {
                continue;
            }
            let d = hxf.translation.distance(muzzle) - h.radius;
            if d < radius * BEAM_RANGE && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, hxf.translation));
            }
        }

        // Into the hull's own frame, because a child's rotation is relative to
        // its parent and the ship is turning underneath it.
        let want_world = match best {
            Some((_, to)) => (to - muzzle).normalize_or(ship.rotation * t.rest),
            None => ship.rotation * t.rest,
        };
        let want_local = (ship.rotation.inverse() * want_world).normalize_or(t.rest);
        // Negated for the reason every hull here is negated: Bevy's forward is
        // minus Z and a barrel points along plus Z.
        let goal = Transform::IDENTITY.looking_to(-want_local, Vec3::Y).rotation;
        xf.rotation = xf.rotation.slerp(goal, (dt * TURRET_SLEW).min(1.0));
    }
}

/// The band box, and the bars over your own ships.
///
/// Both are UI NODES positioned from a world to screen projection rather than
/// meshes in the scene, and that is the right call for exactly these two
/// things: a health bar is a fixed number of pixels tall whatever the range,
/// and a selection box is in screen space by definition. Anything that has to
/// hold its size in the WORLD stays a mesh, which is why the nav disc and the
/// beams are not here.
#[derive(Component)]
struct MarqueeBox;

/// One bar over one ship.
#[derive(Component)]
struct HealthBar(Entity);

/// The pool of bars, kept between frames rather than respawned.
#[derive(Resource, Default)]
struct BarPool(Vec<Entity>);

fn draw_marquee(marquee: Res<Marquee>, mut q: Query<&mut Node, With<MarqueeBox>>) {
    let Ok(mut n) = q.single_mut() else { return };
    match marquee.from {
        Some(from) => {
            let lo = from.min(marquee.to);
            let hi = from.max(marquee.to);
            // A few pixels is a click, not a box, and drawing one for it is a
            // flicker on every single selection.
            if (hi - lo).length() < 6.0 {
                n.display = Display::None;
                return;
            }
            n.display = Display::Flex;
            n.left = Val::Px(lo.x);
            n.top = Val::Px(lo.y);
            n.width = Val::Px(hi.x - lo.x);
            n.height = Val::Px(hi.y - lo.y);
        }
        None => n.display = Display::None,
    }
}

/// A bar over every ship of yours, and only yours.
///
/// The swarm's carriers get none on purpose: a health bar over an enemy turns
/// a siege into a progress bar, and what tells you a carrier is hurt is that it
/// is bleeding and burning, which it already does.
fn draw_bars(
    mut commands: Commands,
    hud: Res<Hud>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ships: Query<
        (Entity, &Transform, Option<&Hull>, Option<&Fighter>, Option<&Selected>),
        (Or<(With<Hull>, With<Fighter>)>, Without<Hive>),
    >,
    mut pool: ResMut<BarPool>,
    mut bars: Query<(&mut Node, &mut BackgroundColor, &Children), With<HealthBar>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), Without<HealthBar>>,
) {
    let Ok((cam, cam_xf)) = cams.single() else { return };
    let mut want: Vec<(Vec2, f32, bool)> = Vec::new();
    for (_, xf, hull, fighter, sel) in &ships {
        let (share, radius): (f32, f32) = match (hull, fighter) {
            (Some(h), _) => {
                if h.dead_hull {
                    continue;
                }
                let core = h.reactor.len().max(1);
                let gone = h.reactor.iter().filter(|&&c| h.damage.is_dead(c)).count();
                // What the bar MEANS is how close the reactor is to going, not
                // how much plating is left. Plating comes off and the ship
                // keeps flying; the reactor is the only thing that kills it,
                // so it is the only honest thing to put on a bar.
                (1.0 - (gone as f32 / core as f32) / REACTOR_LOSS, h.model.radius())
            }
            (_, Some(f)) => (f.hp, 0.6),
            _ => continue,
        };
        let Ok(p) = cam.world_to_viewport(cam_xf, xf.translation + Vec3::Y * radius * 1.3) else { continue };
        want.push((p, share.clamp(0.0, 1.0), sel.is_some()));
    }

    while pool.0.len() < want.len() {
        let fill = commands
            .spawn((
                Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                BackgroundColor(Color::srgb(0.3, 0.95, 0.4)),
                Pickable::IGNORE,
            ))
            .id();
        let bar = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(BAR_W),
                    height: Val::Px(BAR_H),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                HealthBar(Entity::PLACEHOLDER),
                Pickable::IGNORE,
                children![],
            ))
            .id();
        commands.entity(bar).add_child(fill);
        pool.0.push(bar);
    }

    for (n, &bar) in pool.0.iter().enumerate() {
        let Ok((mut node, mut bg, kids)) = bars.get_mut(bar) else { continue };
        match want.get(n) {
            Some(&(p, share, sel)) if hud.show_bars => {
                node.display = Display::Flex;
                node.left = Val::Px(p.x - BAR_W * 0.5);
                node.top = Val::Px(p.y);
                // Selected reads as a brighter frame round the same bar rather
                // than a second widget: one thing on screen per ship.
                bg.0 = if sel {
                    Color::srgba(0.55, 0.95, 1.0, 0.85)
                } else {
                    Color::srgba(0.0, 0.0, 0.0, 0.55)
                };
                if let Some(&fill) = kids.iter().next().as_ref() {
                    if let Ok((mut fnode, mut fbg)) = fills.get_mut(fill) {
                        fnode.width = Val::Percent(share * 100.0);
                        // Green through amber to red, so the state reads
                        // without anybody having to measure the length.
                        fbg.0 = Color::srgb(
                            (1.6 - share * 1.6).clamp(0.15, 1.0),
                            (0.25 + share * 0.85).clamp(0.15, 1.0),
                            0.25,
                        );
                    }
                }
            }
            _ => node.display = Display::None,
        }
    }
}

/// How big a health bar is, in pixels.
const BAR_W: f32 = 42.0;
const BAR_H: f32 = 5.0;

/// The on screen controls.
///
/// There were none at all: every binding in the game was a key somebody had to
/// be told about, so "where is my button to call in reinforcements" is the
/// only question a player could possibly have. A button that is only a key is
/// a feature nobody can find, which is the rule redux-tribes keeps about move
/// mode being buried in a rail.
///
/// It is a real button as well as a label. Clicking it calls the wave, and the
/// key still works, because the two are the same action asked for twice.
#[derive(Component)]
struct CallButton;

/// The line under the button, which says what the wing is doing.
#[derive(Component)]
struct CallLabel;

/// What the HUD is showing and whether the game is running.
///
/// One resource rather than a flag on each widget, because "is the game
/// paused" is a fact about the SESSION and three different things need to
/// agree on it: the swarm's clock, every gameplay system, and the menu that
/// says so on screen.
#[derive(Resource)]
struct Hud {
    show_fps: bool,
    show_bars: bool,
    paused: bool,
    /// A smoothed frame rate, in frames a second.
    fps: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Hud { show_fps: true, show_bars: true, paused: false, fps: 0.0 }
    }
}

/// The frame rate in the corner.
#[derive(Component)]
struct FpsText;

/// The whole pause overlay, shown and hidden by its `display`.
#[derive(Component)]
struct PauseMenu;

/// The row in the menu that turns the counter off and on.
#[derive(Component)]
struct FpsToggle;

/// And the one for the bars over your ships.
#[derive(Component)]
struct BarToggle;

/// And the one that puts you back in the game.
#[derive(Component)]
struct ResumeButton;

/// Is the game running? Everything that moves asks this.
fn running(hud: Res<Hud>) -> bool {
    !hud.paused
}

/// Escape opens the menu and Escape closes it.
///
/// It sets `SwarmConfig.paused` as well as its own flag, because the swarm
/// lives in the render world on the other side of an extract and does not see
/// this resource: `advance_clock` already reads that one and hands the tick a
/// dt of nought, so the cloud freezes where it is instead of being stepped by
/// a frame that was not simulated.
fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    mut hud: ResMut<Hud>,
    mut cfg: ResMut<SwarmConfig>,
    mut menu: Query<&mut Node, With<PauseMenu>>,
    resume: Query<&Interaction, (Changed<Interaction>, With<ResumeButton>)>,
    order: Res<NavOrder>,
    mut started: Local<bool>,
) {
    // The menu is built hidden, so a session that was asked to START paused
    // has to be shown once. Done here rather than in `build_hud` because the
    // display is this system's to own: two places writing it is two places to
    // keep in step.
    if !*started {
        *started = true;
        if hud.paused {
            cfg.paused = true;
            for mut n in &mut menu {
                n.display = Display::Flex;
            }
            return;
        }
    }
    let clicked = resume.iter().any(|i| *i == Interaction::Pressed);
    // Escape belongs to the ORDER first. One escape cancels an open move and
    // does nothing else; the next one, with nothing open, reaches the menu.
    // Space is the plain pause, which is the key an RTS puts it on.
    if keys.just_pressed(KeyCode::Space) || (keys.just_pressed(KeyCode::Escape) && !order.active) {
        hud.paused = !hud.paused;
    } else if clicked {
        hud.paused = false;
    } else {
        return;
    }
    cfg.paused = hud.paused;
    for mut n in &mut menu {
        n.display = if hud.paused { Display::Flex } else { Display::None };
    }
}

/// The counter itself, off the REAL clock.
///
/// `Time` is the virtual clock and Bevy clamps its delta at 250 ms so one
/// stalled frame cannot fling everything forward, which means a frame slower
/// than that reports as 250 ms however long it really took. That is exactly
/// the trap the frame cap fell into once already, and an FPS counter built on
/// it would read a floor of four however bad things got. `Time<Real>` is the
/// wall clock and is the only honest input here.
///
/// Smoothed on a time constant rather than over a fixed number of frames, so
/// the number settles at the same rate whatever the frame rate is.
fn tick_fps(
    real: Res<Time<Real>>,
    mut hud: ResMut<Hud>,
    mut text: Query<(&mut Text, &mut Node), With<FpsText>>,
) {
    let dt = real.delta_secs();
    if dt > 0.0 {
        let now = 1.0 / dt;
        let k = 1.0 - (-dt / FPS_SMOOTH).exp();
        hud.fps = if hud.fps <= 0.0 { now } else { hud.fps + (now - hud.fps) * k };
    }
    let Ok((mut t, mut n)) = text.single_mut() else { return };
    n.display = if hud.show_fps { Display::Flex } else { Display::None };
    if !hud.show_fps {
        return;
    }
    // The frame TIME beside it, because a frame rate alone cannot be compared
    // against a budget: sixteen point seven milliseconds is a number somebody
    // can hold against sixty, and "59 fps" is not.
    let want = format!("{:.0} fps   {:.1} ms", hud.fps, 1000.0 / hud.fps.max(1e-3));
    if t.0 != want {
        t.0 = want;
    }
}

/// How long the frame rate takes to settle, in seconds.
const FPS_SMOOTH: f32 = 0.4;

/// The hulls the dropdown offers, one per navy and per rung, so a player can
/// see what the ladder actually looks like without editing a command line.
///
/// A picked subset rather than all twenty three: the point is to try DIFFERENT
/// ships, and four corvettes from four navies tell you less than a corvette, a
/// frigate, a destroyer and a cruiser do.
const PICKABLE: [&str; 8] = [
    "terran_frigate",
    "terran_destroyer",
    "terran_cruiser",
    "karisen_frigate",
    "karisen_cruiser",
    "rogue_destroyer",
    "benefactor_cruiser",
    "civil_hauler",
];

/// One row of the ship dropdown.
#[derive(Component)]
struct HullPick(usize);

/// The list itself, shown and hidden by its `display`.
#[derive(Component)]
struct HullMenu;

/// The button that opens it.
#[derive(Component)]
struct HullButton;

/// Swap the flagship for another class.
///
/// It DESPAWNS and respawns rather than editing the hull in place, because a
/// ship here is its model, its damage grid, its bricks, its materials, its
/// turret children and its reactor: every one of those is derived from the
/// class at spawn, and there is no such thing as changing the class of a hull
/// that already exists. Spawning a fresh one is the same code the game starts
/// with, which is the only version of it worth having.
#[allow(clippy::too_many_arguments)]
fn pick_hull(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    mut scene: ResMut<Scene>,
    mut cfg: ResMut<SwarmConfig>,
    picks: Query<(&Interaction, &HullPick), Changed<Interaction>>,
    opener: Query<&Interaction, (Changed<Interaction>, With<HullButton>)>,
    mut menu: Query<&mut Node, With<HullMenu>>,
    old: Query<Entity, (With<Flagship>, Without<Hive>)>,
    escorts: Query<Entity, With<Escort>>,
    wing: Query<Entity, With<Fighter>>,
) {
    if opener.iter().any(|i| *i == Interaction::Pressed) {
        for mut n in &mut menu {
            n.display = if n.display == Display::None { Display::Flex } else { Display::None };
        }
    }
    let Some((_, pick)) = picks.iter().find(|(i, _)| **i == Interaction::Pressed) else { return };
    let Some(&which) = PICKABLE.get(pick.0) else { return };
    for mut n in &mut menu {
        n.display = Display::None;
    }
    if which == scene.hull {
        return;
    }
    // The wing and the squadron go with it: an escort is a copy of the
    // flagship's class and a fighter flies off it, so leaving either behind
    // would be a formation of the ship you just replaced.
    for e in old.iter().chain(escorts.iter()).chain(wing.iter()) {
        commands.entity(e).despawn();
    }
    scene.hull = which.into();
    let (_, radius) = spawn_hull(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        which,
        Transform::IDENTITY,
        scene.chewers as u32,
        0,
        None,
    );
    cfg.hull_radius = radius;
    info!("flagship is a {which} now, radius {radius:.2}");
}

/// A set the whole HUD hangs off, so a headless run can skip it.
fn build_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.55)),
                BackgroundColor(Color::srgba(0.04, 0.10, 0.16, 0.80)),
                CallButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Call reinforcements  (R)"),
                    TextFont { font_size: 15.0, ..default() },
                    TextColor(Color::srgb(0.80, 0.94, 1.0)),
                    Pickable::IGNORE,
                ));
            });
            p.spawn((
                Text::new(""),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgba(0.70, 0.82, 0.92, 0.85)),
                CallLabel,
                Pickable::IGNORE,
            ));
            p.spawn((
                Text::new(
                    "left drag select   middle drag orbit   WASD pan   Q E up down   F focus\n                     right button move disc, right again to confirm, esc cancels\n                     shift lifts the target off the plane   space pauses   esc opens the menu",
                ),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgba(0.62, 0.74, 0.86, 0.72)),
                Pickable::IGNORE,
            ));
        });

    // The band box. One node, moved and resized in screen pixels.
    commands.spawn((
        Node { position_type: PositionType::Absolute, display: Display::None, border: UiRect::all(Val::Px(1.0)), ..default() },
        BorderColor::all(Color::srgba(0.50, 0.95, 1.0, 0.90)),
        BackgroundColor(Color::srgba(0.30, 0.75, 1.0, 0.10)),
        MarqueeBox,
        Pickable::IGNORE,
    ));

    // The ship picker, top left, with its list folded away under it.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Button,
                Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.55)),
                BackgroundColor(Color::srgba(0.04, 0.10, 0.16, 0.85)),
                HullButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Ship \u{25be}"),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.80, 0.94, 1.0)),
                    Pickable::IGNORE,
                ));
            });
            p.spawn((
                Node { flex_direction: FlexDirection::Column, display: Display::None, ..default() },
                HullMenu,
            ))
            .with_children(|list| {
                for (n, name) in PICKABLE.iter().enumerate() {
                    list.spawn((
                        Button,
                        Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.25)),
                        BackgroundColor(Color::srgba(0.03, 0.08, 0.13, 0.95)),
                        HullPick(n),
                    ))
                    .with_children(|t| {
                        t.spawn((
                            Text::new(name.replace('_', " ")),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(Color::srgb(0.78, 0.90, 1.0)),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        });

    // The counter, in the opposite corner from the controls so it never sits
    // over anything a player has to press.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(12.0),
            ..default()
        },
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::srgba(0.72, 0.86, 0.98, 0.80)),
        FpsText,
        Pickable::IGNORE,
    ));

    // The pause overlay. Built once and hidden by its own `display` rather
    // than spawned and despawned, so the buttons keep their identity and
    // nothing has to rebuild a menu on the frame somebody pressed escape.
    //
    // `Display::None` and not `Visibility::Hidden`: a hidden node is still
    // laid out and still picked, so an invisible Resume button would have gone
    // on swallowing clicks in the middle of the screen the whole time the game
    // was running.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.04, 0.72)),
            PauseMenu,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::axes(Val::Px(26.0), Val::Px(22.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    min_width: Val::Px(260.0),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.45)),
                BackgroundColor(Color::srgba(0.03, 0.07, 0.12, 0.95)),
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new("Paused"),
                    TextFont { font_size: 22.0, ..default() },
                    TextColor(Color::srgb(0.86, 0.95, 1.0)),
                    Pickable::IGNORE,
                ));
                for (marker, label) in
                    [("fps", "FPS counter: on"), ("bars", "Health bars: on"), ("resume", "Resume  (esc)")]
                {
                    let mut b = c.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(9.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.40)),
                        BackgroundColor(Color::srgba(0.05, 0.12, 0.18, 0.90)),
                    ));
                    if marker == "fps" {
                        b.insert(FpsToggle);
                    } else if marker == "bars" {
                        b.insert(BarToggle);
                    } else {
                        b.insert(ResumeButton);
                    }
                    b.with_children(|t| {
                        t.spawn((
                            Text::new(label),
                            TextFont { font_size: 15.0, ..default() },
                            TextColor(Color::srgb(0.80, 0.94, 1.0)),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        });
}

/// The button's own colours, and what it says the wing is at.
fn hud_feedback(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
    toggled: Query<&Interaction, (Changed<Interaction>, With<FpsToggle>)>,
    toggle_text: Query<&Children, With<FpsToggle>>,
    bars_toggled: Query<&Interaction, (Changed<Interaction>, With<BarToggle>)>,
    bars_text: Query<&Children, With<BarToggle>>,
    mut texts: Query<&mut Text, Without<CallLabel>>,
    escorts: Query<(), With<Escort>>,
    mut label: Query<&mut Text, With<CallLabel>>,
    mut hud: ResMut<Hud>,
) {
    if bars_toggled.iter().any(|i| *i == Interaction::Pressed) {
        hud.show_bars = !hud.show_bars;
        let want = if hud.show_bars { "Health bars: on" } else { "Health bars: off" };
        for kids in &bars_text {
            for k in kids.iter() {
                if let Ok(mut t) = texts.get_mut(k) {
                    t.0 = want.into();
                }
            }
        }
    }
    if toggled.iter().any(|i| *i == Interaction::Pressed) {
        hud.show_fps = !hud.show_fps;
        // The row says what it will DO next time, which means it has to say
        // what the counter is doing now. A toggle whose label never changes is
        // a control nobody can read the state of, which is the rail rule
        // redux-tribes keeps.
        let want = if hud.show_fps { "FPS counter: on" } else { "FPS counter: off" };
        for kids in &toggle_text {
            for k in kids.iter() {
                if let Ok(mut t) = texts.get_mut(k) {
                    t.0 = want.into();
                }
            }
        }
    }
    for (i, mut bg) in &mut buttons {
        bg.0 = match i {
            Interaction::Pressed => Color::srgba(0.16, 0.38, 0.52, 0.95),
            Interaction::Hovered => Color::srgba(0.09, 0.22, 0.32, 0.90),
            Interaction::None => Color::srgba(0.04, 0.10, 0.16, 0.80),
        };
    }
    let out = escorts.iter().count();
    if let Ok(mut t) = label.single_mut() {
        let want = if out >= WING_MAX as usize {
            format!("wing full: {out} of {WING_MAX}")
        } else {
            format!("wing {out} of {WING_MAX}, {WING_WAVE} per call")
        };
        if t.0 != want {
            t.0 = want;
        }
    }
}

fn publish_hives(hives: Query<(&Hive, &Transform, &Hull)>, mut cfg: ResMut<SwarmConfig>) {
    cfg.hives.clear();
    for (h, xf, hull) in &hives {
        // A wreck launches nothing. `go_critical` takes a carrier the same way
        // it takes a ship now, so "still flying" is the hull's own answer
        // rather than a second hit point number kept beside it.
        if hull.dead_hull {
            continue;
        }
        cfg.hives.push(xf.translation.extend(h.radius));
    }
}

/// A beam is fired at its full range and CUT at what it reached.
///
/// redux-tribes' own rule, and the reason for it: the full range endpoint is
/// what a MISS looks like, so a beam that ran out into space is not a defect
/// and a beam that carried on through what it hit is. Cutting it also means a
/// carrier is cover: the capsule handed to the swarm is the short one, so the
/// fighters behind it live.
fn resolve_beams(
    tick: Res<Tick>,
    mut fx: ResMut<LiveFx>,
    mut hives: Query<(Entity, &Hive, &Transform, &mut Hull)>,
    mut sparks: ResMut<SparkQueue>,
) {
    for b in fx.beams.iter_mut() {
        // Only the ones fired this tick: a beam is resolved once, when it
        // goes off, and lives out its nine ticks as whatever it became.
        if b.born != tick.tick {
            continue;
        }
        // The nearest carrier along it, by fraction of the beam rather than
        // by distance from the muzzle, which is the same order and is the
        // number the cut wants anyway.
        let mut hit: Option<(f32, Entity)> = None;
        for (e, h, xf, hull) in &hives {
            if hull.dead_hull {
                continue;
            }
            let Some(t) = b.reaches(xf.translation.to_array(), h.radius) else { continue };
            if hit.map_or(true, |(bt, _)| t < bt) {
                hit = Some((t, e));
            }
        }
        let Some((t, target)) = hit else { continue };
        *b = b.cut(t);

        let end = Vec3::from(b.to);
        // The splash where it landed, thrown back along the beam.
        let back = (Vec3::from(b.from) - end).normalize_or(Vec3::Y);
        let mut list = Vec::new();
        breach_sparks(tick.tick.wrapping_mul(2_654_435_761), tick.tick, end.to_array(), back.to_array(), 0.5, &mut list);
        for sp in &mut list {
            // PURPLE. What a beam splashes off a carrier is the animal, and
            // the animal is chitin over violet: it used to throw the green its
            // own lamps are lit with, so a hit read as a light rather than as
            // a thing being opened up.
            sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
            sp.size *= 1.6;
        }
        sparks.extend(list);

        // And it takes CELLS off, the way it does on a ship. The hit point is
        // taken into the carrier's own frame first, because `bite` works in
        // model coordinates and a carrier is drawn scaled, turned and a long
        // way from the origin.
        if let Ok((_, _, xf, mut hull)) = hives.get_mut(target) {
            let local = xf.to_matrix().inverse().transform_point3(end);
            let hull = &mut *hull;
            // Several bites in a ring round the impact rather than one, so a
            // beam opens a crater the size of a beam instead of taking a
            // single cell out of a mothership.
            for k in 0..BEAM_BITES {
                let h = |q: u32| swarm_core::rng::hash_cell(tick.tick.wrapping_mul(2654435761) ^ (k * 977) ^ q) as f32 / u32::MAX as f32 - 0.5;
                let jit = Vec3::new(h(1), h(7), h(19)) * hull.model.cell * 2.4;
                if let Some(br) = hull.damage.bite(&hull.model, (local + jit).to_array(), BEAM_DAMAGE, tick.tick) {
                    let at = xf.transform_point(Vec3::from(hull.model.centre_of(br.cell as usize)));
                    let out = xf.rotation * Vec3::from(br.outward);
                    let mut spray = Vec::new();
                    breach_sparks(br.cell, br.tick, at.to_array(), out.to_array(), hull.model.cell * hull.model.cell.max(0.02), &mut spray);
                    for sp in &mut spray {
                        sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
                    }
                    sparks.extend(spray);
                    hull.breaches += 1;
                }
            }
        }
    }
}

/// Point defence: the ship rakes the cloud around itself, constantly.
///
/// A flak burst is a `Blast`, which is to say a capsule of ZERO LENGTH, which
/// is the shape the shot path already carries. So this needed no new kind of
/// anything: it is placed out along a gun's own line at the standoff the
/// swarm holds, the shader kills whatever is inside it, and the CPU never
/// learns where a mote was. That is the whole point of resolving a shot as a
/// volume rather than as a target.
///
/// Fast and weak against the long slow beams that go for the carriers, so the
/// two read as two different weapons doing two different jobs.
fn fire_flak(
    tick: Res<Tick>,
    scene: Res<Scene>,
    // Not the carriers, which are hulls now: a mothership does not
    // carry the fleet's guns and would otherwise open fire on its own side.
    hulls: Query<(&Hull, &Transform), Without<Hive>>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
) {
    if scene.cadence == 0 {
        return;
    }
    // A quarter of the beam cadence, so a hull is always putting something up.
    let every = (scene.cadence / 4).max(2);
    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let radius = hull.model.radius();
        for (n, g) in hull.guns.iter().enumerate() {
            let phase = (swarm_core::rng::hash_cell(g.cell ^ 0x51A7 ^ hull.seed) % every) as u32;
            if (tick.tick + phase) % every != 0 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(g.at));
            let out = (xf.rotation * Vec3::from(g.out)).normalize_or_zero();
            // Swept fast and wide, so the bursts walk round the hull rather
            // than punching the same hole in the cloud.
            let t = tick.tick as f32 * 0.21 + n as f32 * 2.3;
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(out);
            let dir = (out + side * (t.sin() * 0.8) + up * ((t * 1.3).cos() * 0.55)).normalize();
            // Out where the swarm actually holds: a burst inside the standoff
            // would go off in clear space every time.
            let reach = radius * (1.2 + 0.7 * ((t * 0.37).sin() * 0.5 + 0.5));
            let centre = at + dir * reach;
            // A ROUND, not an explosion at the far end. A flak burst used to
            // appear where it was going to go off, which is a gun with no
            // shell in it: the muzzle flashed, the target flashed, and
            // nothing at all crossed the gap between them. It travels now,
            // and the blast is pushed when it ARRIVES.
            fx.tracers.push(Tracer {
                from: at,
                to: centre,
                t: 0.0,
                rate: 1.0 / TRACER_TICKS as f32,
                burst: radius * 0.42,
                colour: [4.2, 2.1, 0.55],
            });
            fx.flak += 1;

            // The muzzle flash, where the gun is. Bigger than it was, because
            // a capital ship's point defence going off ought to light the
            // plating round it.
            let mut list = Vec::new();
            blast_sparks(tick.tick.wrapping_add(g.cell), at.to_array(), radius * 0.20, 14, &mut list);
            for s in &mut list {
                s.life *= 0.22;
                s.size *= 0.7;
            }
            sparks.extend(list);
            let mut flash = Vec::new();
            muzzle_sparks(g.cell ^ hull.seed ^ tick.tick, at.to_array(), dir.to_array(), hull.model.cell * 0.7, &mut flash);
            sparks.extend(flash);
        }
    }
}

/// A carrier that has been hit shows it, by glowing hotter as it comes apart.
///
/// Its own material rather than one shared by all ten, which is what a health
/// bar costs when there is no HUD: a player has to be able to see which of
/// them is nearly dead, and the only place to say so is the thing itself.
/// A carrier that has been opened up BLEEDS.
///
/// This used to set the material's emissive to a green that grew with damage,
/// and emissive applies over the whole material rather than over the places
/// that were hit: at 2.2 it did not mark a wound, it turned the entire hull
/// into a uniform green bulb the shape of a mothership. The tint is gone
/// altogether, because a carrier now carries the same per cell damage a ship
/// does and the wound is drawn where the wound IS.
///
/// What is left is the bleed: purple, off the torn cells, thicker the worse it
/// is. Purple because the swarm is chitin over violet, and green is the colour
/// of the lamps ON a carrier rather than the colour of the animal.
fn bleed_hives(
    tick: Res<Tick>,
    hives: Query<(&Hive, &Transform, &Hull)>,
    mut sparks: ResMut<SparkQueue>,
) {
    for (h, xf, hull) in &hives {
        if hull.dead_hull {
            continue;
        }
        let hurt = (hull.damage.dead_count() as f32 / hull.cells.max(1) as f32).clamp(0.0, 1.0);
        if hurt < 0.004 {
            continue;
        }
        let every = (6.0 - 4.0 * (hurt * 8.0).min(1.0)).max(1.0) as u32;
        if tick.tick % every != 0 {
            continue;
        }
        let n = (1.0 + 4.0 * (hurt * 8.0).min(1.0)) as u32;
        let mut out: Vec<Spark> = Vec::new();
        for k in 0..n {
            let seed = h.seed as u32 ^ tick.tick.wrapping_mul(2654435761) ^ k;
            let d = |q: u32| swarm_core::rng::hash_cell(seed + q) as f32 / u32::MAX as f32 - 0.5;
            let off = Vec3::new(d(1), d(7), d(19)).normalize_or(Vec3::Y) * h.radius * 0.92;
            breach_sparks(seed, tick.tick, (xf.translation + off).to_array(), off.normalize_or(Vec3::Y).to_array(), h.radius * 0.10, &mut out);
        }
        for sp in &mut out {
            sp.colour = [sp.colour[0] * 1.5, sp.colour[1] * 0.28, sp.colour[2] * 2.0];
        }
        sparks.extend(out);
    }
}


/// Guns go off on their own cadence, staggered so a broadside is a rattle
/// rather than one bang, and sweep so the beams rake the cloud instead of
/// drilling the same hole in it forever.
fn fire_guns(
    tick: Res<Tick>,
    scene: Res<Scene>,
    // Not the carriers, which are hulls now: a mothership does not
    // carry the fleet's guns and would otherwise open fire on its own side.
    hulls: Query<(&Hull, &Transform), Without<Hive>>,
    hives: Query<(&Hive, &Transform)>,
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
        let reach = hull.model.radius() * BEAM_RANGE;
        for (n, g) in hull.guns.iter().enumerate() {
            // Staggered by the gun's own cell, so two hulls of one class do
            // not fire in lockstep and the pattern does not read as a clock.
            let phase = (swarm_core::rng::hash_cell(g.cell ^ hull.seed) % scene.cadence) as u32;
            if (tick.tick + phase) % scene.cadence != 0 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(g.at));
            let out = (xf.rotation * Vec3::from(g.out)).normalize_or_zero();
            // A gun looks for a CARRIER, because the CPU can see one: a hive
            // is an entity and a mote is not. What it cannot do is pick a
            // mote, which is the whole reason a shot is a volume the shader
            // resolves rather than a target the CPU chose.
            //
            // Nearest one it can actually bear on, so a mount on the far
            // flank does not swing through its own ship to reach something.
            let mut target: Option<(f32, Vec3)> = None;
            for (_, hxf) in &hives {
                let to = hxf.translation - at;
                if to.normalize_or_zero().dot(out) < 0.15 {
                    continue;
                }
                let d = to.length_squared();
                if target.map_or(true, |(bd, _)| d < bd) {
                    target = Some((d, hxf.translation));
                }
            }
            // A slow rake when there is nothing to shoot at, so the guns are
            // not simply silent between carriers. Trigonometry is fine here
            // and nowhere near the core: it decides where a light is drawn.
            let t = tick.tick as f32 * 0.03 + n as f32 * 1.7;
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(out);
            let dir = match target {
                // Aimed, with a little spread, so some shots miss and run out
                // into space. A gun that never missed would make the carriers
                // a countdown rather than a fight.
                Some((_, to)) => {
                    let aim = (to - at).normalize_or(out);
                    (aim + side * (t.sin() * 0.06) + up * ((t * 0.7).cos() * 0.05)).normalize()
                }
                None => (out + side * (t.sin() * 0.45) + up * ((t * 0.7).cos() * 0.30)).normalize(),
            };
            fx.beams.push(Beam {
                from: at.to_array(),
                to: (at + dir * reach).to_array(),
                // Wide. A beam a couple of cells across cut a thread through
                // the cloud and killed almost nothing you could see; the point
                // of firing into a swarm is the swath.
                radius: hull.model.cell * BEAM_WIDTH,
                born: tick.tick,
            });
            fx.fired += 1;
            let mut list = Vec::new();
            muzzle_sparks(g.cell.wrapping_add(tick.tick), at.to_array(), dir.to_array(), hull.model.cell, &mut list);
            sparks.extend(list);
        }
    }
}

/// How wide a beam bites, in hull cells, and how far its far end SWEEPS while
/// it is alive, in radians.
///
/// A beam used to be a fixed segment for its whole life: it killed whatever
/// was on that line at the tick it went off and nothing after. Sweeping it
/// carves an ARC through the cloud over the nine ticks it lives, which is
/// what sets off a line of kills a player can watch travel.
const BEAM_WIDTH: f32 = 5.5;
const BEAM_SWEEP: f32 = 0.55;

/// How much of the REACTOR has to be gone before a ship goes up.
///
/// This replaces a share of the whole hull, and the difference is the whole
/// point. "A tenth of its cells" is a hit point bar with extra steps: it does
/// not matter where the damage landed, so a ship scoured evenly all over died
/// exactly as fast as one drilled through the middle, and aiming at anything
/// bought nothing. A reactor that must actually be REACHED makes the layout
/// the damage model, which is redux-tribes' own rule: the plating, the
/// machinery behind it and finally the core, in that order, because that is
/// the order they physically stand in.
///
/// `fx::reactor_of` derives it as the most buried cells in the hull, so
/// getting to it means chewing a hole all the way through, and half of it is
/// what it takes.
const REACTOR_LOSS: f32 = 0.50;

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
        // Only the reactor counts. A hull can be shot to pieces everywhere
        // else and keep flying, which is what makes a wreck that is still
        // fighting possible at all.
        let core = hull.reactor.len();
        let gone = hull.reactor.iter().filter(|&&c| hull.damage.is_dead(c)).count();
        let share = if core == 0 { 0.0 } else { gone as f32 / core as f32 };
        let forced = scene.explode > 0 && tick.tick >= scene.explode;
        if !forced && share < REACTOR_LOSS {
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
            "hull went critical at tick {} ({:.0}% of its {} reactor cells gone{}): blast radius {:.2}",
            tick.tick,
            share * 100.0,
            core,
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
    for b in fx.beams.iter_mut() {
        // SWEPT. The far end walks across while the beam is alive, so what the
        // swarm is handed each tick is a different segment and the beam carves
        // an arc rather than cutting one thread. The mesh is rebuilt from the
        // same endpoints, so what is drawn is what kills.
        let from = Vec3::from(b.from);
        let along = Vec3::from(b.to) - from;
        let len = along.length();
        if len > 1e-4 {
            let dir = along / len;
            // About an axis of its own, so two guns firing together sweep
            // different ways instead of scything in step.
            let seed = swarm_core::rng::hash_cell(b.born ^ (len.to_bits()));
            let mut ax = Vec3::new(
                (seed & 0xFF) as f32 / 255.0 - 0.5,
                ((seed >> 8) & 0xFF) as f32 / 255.0 - 0.5,
                ((seed >> 16) & 0xFF) as f32 / 255.0 - 0.5,
            );
            ax = (ax - dir * ax.dot(dir)).normalize_or(dir.any_orthonormal_vector());
            let step = Quat::from_axis_angle(ax, BEAM_SWEEP / swarm_core::fx::BEAM_TICKS as f32);
            b.to = (from + step * (dir * len)).to_array();
        }
        shots.0.push(Capsule { from, to: Vec3::from(b.to), radius: b.radius });
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
    // The rounds in the air, as short bright bolts. Drawn here rather than in
    // their own pass because a tracer and a beam are the same picture problem:
    // a line in space that has to be the same width from anywhere, which is a
    // quad turned edge on to the eye.
    for t in &fx.tracers {
        let along = t.to - t.from;
        let at = t.from + along * t.t;
        let len = along.length() * TRACER_LEN;
        let back = along.normalize_or(Vec3::Z) * len;
        // Clipped at the muzzle, so a bolt grows out of the gun instead of
        // starting in front of it.
        let tail = t.from + along * (t.t - TRACER_LEN).max(0.0);
        let tail = if (at - tail).length() < len { tail } else { at - back };
        add_line(&mut pos, &mut col, &mut idx, eye, tail, at, TRACER_WIDTH, t.colour, 1.0);
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

// ------------------------------------------------------------- flame --

/// One engine flame, as a faceted cone.
///
/// GEOMETRY, not particles, which is what makes it read as Homeworld rather
/// than as a campfire: the silhouette is a polygon with hard edges, it is the
/// same shape from every angle, and it is exactly as long as the throttle
/// says. A particle plume says "something hot is happening back there"; a cone
/// says "this ship is under power, by this much, in this direction".
///
/// Six sides on purpose. Enough that it is a cone and few enough that the
/// facets show, which is the whole look.
#[allow(clippy::too_many_arguments)]
/// The stations along a flame: how wide it is there, how bright, and how
/// much of it survives. The last closes the tip.
///
/// Four stations are three BANDS, and a band is what makes this read as a
/// shape. Homeworld draws a flame as geometry rather than as a smear, and
/// what carries that is a hard boundary the eye can find: each band is one
/// flat colour across its whole width and steps at the ring, so the flame has
/// three parts a person could point at instead of a gradient.
const FLAME_BANDS: [(f32, f32, f32); 4] = [
    (1.00, 1.00, 1.00),
    (0.74, 0.66, 0.72),
    (0.40, 0.30, 0.34),
    (0.00, 0.08, 0.00),
];

/// How many facets round the flame. Eight, because six reads as a wedge from
/// abeam and twelve is a smooth horn again.
const FLAME_SIDES: usize = 8;

/// One engine flame: a stack of faceted frustums about `base` along `dir`.
///
/// The vertices are NOT shared between facets. A ring of six shared vertices
/// fanning to a tip is what the first cut drew, and a shared vertex is a
/// colour the rasteriser interpolates ACROSS the edge between two facets, so
/// every hard line in the mesh came out as a smooth ramp and the cone read as
/// a horn of smoke. Three vertices per triangle, all of them the band's own
/// colour, is flat shading, and flat shading is the whole of the look.
fn add_flame(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    base: Vec3,
    dir: Vec3,
    radius: f32,
    len: f32,
    tint: Vec3,
) {
    if len <= 0.0 || radius <= 0.0 {
        return;
    }
    let f = dir.normalize_or(Vec3::NEG_Z);
    let up = if f.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    // (a, b, f) is right handed, so a ring wound in increasing angle gives a
    // triangle whose right hand normal points OUTWARD, which is what the back
    // face cull needs. Built rather than asserted: `a x b == f` by
    // construction, since `b = f x a`.
    let a = f.cross(up).normalize_or(Vec3::X);
    let b = f.cross(a);

    let ring = |n: usize, r: f32, along: f32| -> Vec3 {
        let t = n as f32 / FLAME_SIDES as f32 * std::f32::consts::TAU;
        base + f * along + (a * t.cos() + b * t.sin()) * r
    };
    let mut tri = |p: [Vec3; 3], c: [f32; 4]| {
        let n = pos.len() as u32;
        for v in p {
            pos.push(v.to_array());
            col.push(c);
        }
        idx.extend_from_slice(&[n, n + 1, n + 2]);
    };

    for band in 0..FLAME_BANDS.len() - 1 {
        let (r0, _, _) = FLAME_BANDS[band];
        let (r1, _, _) = FLAME_BANDS[band + 1];
        let (z0, z1) = (
            band as f32 / (FLAME_BANDS.len() - 1) as f32 * len,
            (band + 1) as f32 / (FLAME_BANDS.len() - 1) as f32 * len,
        );
        // The band takes the MEAN of the two stations it lies between, so the
        // steps fall between bands and not inside one.
        let bright = 0.5 * (FLAME_BANDS[band].1 + FLAME_BANDS[band + 1].1);
        let alpha = 0.5 * (FLAME_BANDS[band].2 + FLAME_BANDS[band + 1].2);
        let c = [tint.x * bright, tint.y * bright, tint.z * bright, alpha];
        for n in 0..FLAME_SIDES {
            let m = (n + 1) % FLAME_SIDES;
            let p00 = ring(n, r0 * radius, z0);
            let p01 = ring(m, r0 * radius, z0);
            if r1 <= 0.0 {
                // The last band closes on the axis, so it is one triangle
                // rather than a quad with a degenerate edge in it.
                tri([p00, p01, base + f * z1], c);
                continue;
            }
            let p10 = ring(n, r1 * radius, z1);
            let p11 = ring(m, r1 * radius, z1);
            tri([p00, p01, p10], c);
            tri([p01, p11, p10], c);
        }
    }
}

/// Every engine on every ship and carrier, as one mesh rebuilt each frame.
///
/// There are dozens of engines and nine thousand motes, so the motes get
/// their glow in the shader and everything countable gets geometry. That is
/// the same split the whole design keeps: the ECS holds what there are dozens
/// of, and the GPU holds the rest.
fn draw_flames(
    tick: Res<Tick>,
    handle: Option<Res<FlameHandle>>,
    hulls: Query<(&Hull, &Transform)>,
    hives: Query<(&Hive, &Transform, &Hull)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(handle) = handle else { return };
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Two flames per engine: a short white hot core inside a longer coloured
    // one. One alone is a flat cutout of colour; nested, the core reads as
    // the part that is actually burning and the body as what it throws.
    let mut cone = |at: Vec3, dir: Vec3, size: f32, throttle: f32, hot: Vec3, cool: Vec3, seed: u32| {
        // A flicker, hashed off the engine and the tick rather than rolled,
        // so both seats and a re-watch see the same flame.
        let flick = 0.88 + 0.24 * (swarm_core::rng::hash_cell(seed ^ (tick.tick / 3)) & 0xFF) as f32 / 255.0;
        // Long and narrow rather than short and wide: a jet, not a bell of
        // flame. The first cut was as broad as it was long and read as a
        // paper cone stuck on the back of the ship.
        let len = size * (0.9 + 6.5 * throttle) * flick;
        add_flame(&mut pos, &mut col, &mut idx, at, dir, size * 0.92, len, cool);
        add_flame(&mut pos, &mut col, &mut idx, at, dir, size * 0.52, len * 0.40, hot);
    };

    for (hull, xf) in &hulls {
        if hull.dead_hull {
            continue;
        }
        let forward = xf.rotation * Vec3::Z;
        for e in &hull.engines {
            let throttle = throttle_of(hull, forward, e.out[2]);
            if throttle <= 0.01 {
                continue;
            }
            let at = xf.transform_point(Vec3::from(e.at));
            let dir = (xf.rotation * Vec3::from(e.out)).normalize_or(Vec3::NEG_Z);
            cone(
                at,
                dir,
                hull.model.cell * 2.4,
                throttle,
                Vec3::new(5.4, 4.3, 3.0),
                Vec3::new(3.0, 0.86, 0.14),
                e.cell ^ hull.seed,
            );
        }
    }
    for (hive, xf, hull) in &hives {
        if hull.dead_hull {
            continue;
        }
        // A carrier is always under way, and slowly: a fixed low throttle
        // rather than one read off its drift, which would be invisible.
        for e in &hive.engines {
            // NOT `* hive.scale`. The carrier's own Transform already carries
            // that scale, and `transform_point` applies it, so multiplying it
            // in here squared it: an engine a fifth of the way out from the
            // centre was drawn at a fifth SQUARED of the scaled radius, which
            // put every carrier's flames in open space several lengths off its
            // hull. They looked unaligned because they were not on the ship.
            let at = xf.transform_point(Vec3::from(e.at));
            let dir = (xf.rotation * Vec3::from(e.out)).normalize_or(Vec3::NEG_Z);
            cone(
                at,
                dir,
                hull.model.cell * hive.scale * 2.2,
                0.5,
                Vec3::new(3.4, 5.4, 6.0),
                Vec3::new(0.45, 2.6, 3.8),
                e.cell ^ hive.seed as u32,
            );
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
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

/// How hard one engine is burning, given which way it points.
///
/// A main engine burns to go FASTER and a retro burns to slow down, which is
/// what a retro is for. Both are propulsion and both wear the same surface, so
/// the only thing that tells them apart is which way their plume leaves: aft
/// for the drive, forward for the retro. Reading the throttle off the SPEED
/// rather than the acceleration lit both of them whenever the ship moved,
/// which is a ship fighting itself.
fn throttle_of(hull: &Hull, forward: Vec3, out_z: f32) -> f32 {
    if hull.dead_hull {
        return 0.0;
    }
    let cap = hull.model.radius() * HULL_ACCEL;
    let along = hull.accel.dot(forward) / cap.max(1e-4);
    if out_z < 0.0 {
        // The main drive. An idle that is not nought, because a ship with its
        // engines completely dark reads as one that has broken down.
        0.16 + 0.84 * along.clamp(0.0, 1.0)
    } else {
        // A retro, dark until something is being slowed.
        (-along).clamp(0.0, 1.0)
    }
}

/// The drive cells themselves brighten with the throttle, so the mouth of an
/// engine is hot before any flame comes out of it.
fn glow_engines(hulls: Query<(&Hull, &Transform)>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (hull, xf) in &hulls {
        // The main drive's own throttle: the surface is shared by every bell
        // on the ship and cannot be lit two ways at once.
        let t = throttle_of(hull, xf.rotation * Vec3::Z, -1.0);
        let Some(mat) = hull.surface_mats.get(SURF_DRIVE as usize) else { continue };
        if let Some(m) = materials.get_mut(mat) {
            m.emissive = LinearRgba::rgb(3.4 * t, 1.5 * t * t, 0.35 * t * t);
        }
    }
}

// ------------------------------------------------------------- flight --

/// What a hull can do, in hull radii a second. Slow on purpose: a capital
/// ship that darts is a fighter, and the whole point of moving one is that
/// the swarm has time to follow and you have time to watch it.
const HULL_SPEED: f32 = 1.15;
const HULL_ACCEL: f32 = 0.85;
/// Close enough to have arrived.
const ARRIVE: f32 = 0.35;

/// A move order being ISSUED: not the order itself, which lives on the hull,
/// but the thing the player is still pointing at.
///
/// Homeworld's own shape. The cursor picks a point on the horizontal plane
/// through the ship, and that alone can only ever name somewhere at the
/// ship's own height; holding shift lifts the target off that plane and draws
/// the line back down to it, which is what makes a flat screen able to say a
/// place in three dimensions at all.
#[derive(Resource, Default)]
struct NavOrder {
    active: bool,
    /// On the plane, at the ship's height when the order was opened.
    on_plane: Vec3,
    /// How far off that plane, positive up.
    lift: f32,
    plane_y: f32,
    /// Whether shift has been held at any point, so the vertical line is
    /// drawn even when the lift is momentarily nought.
    lifting: bool,
}

impl NavOrder {
    fn target(&self) -> Vec3 {
        self.on_plane + Vec3::Y * self.lift
    }
}

/// Homeworld's own move flow, which is a MODE rather than a drag.
///
/// Right button opens the disc, the cursor aims it on the plane through the
/// selected ships, shift lifts it off that plane, and a SECOND right button
/// commits it. Escape cancels the order and leaves the mode; escape again,
/// with no order open, is what opens the pause menu.
///
/// A press and drag is what this was, and it is the wrong shape for a
/// three dimensional order: holding a button while also moving the mouse to
/// pick a point and then holding shift to lift it off the plane is three
/// things one hand is doing at once. A mode costs one more click and lets the
/// player take as long as they like over the part that is actually hard.
///
/// It runs while PAUSED, deliberately. Giving orders with the world stopped is
/// the whole reason a pause key is worth having in an RTS.
fn nav_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    over_ui: Query<&Interaction>,
    mut order: ResMut<NavOrder>,
    mut hulls: Query<(&mut Hull, &Transform, Option<&Selected>)>,
) {
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    // Vertical mouse travel while shift is down, in pixels, which is the only
    // thing the mouse can say once the cursor has stopped meaning a place.
    let dy: f32 = motion.read().map(|m| m.delta.y).sum();

    let Ok(window) = windows.single() else { return };
    let Ok((cam, cam_xf)) = cams.single() else { return };
    if over_ui.iter().any(|i| *i != Interaction::None) {
        return;
    }

    // Whose order this is: everything selected, or the flagship if nothing is.
    let mut chosen: Vec<Vec3> = Vec::new();
    let mut radius = 1.0f32;
    for (hull, xf, sel) in &hulls {
        if sel.is_some() && !hull.dead_hull {
            chosen.push(xf.translation);
            radius = radius.max(hull.model.radius());
        }
    }
    if chosen.is_empty() {
        return;
    }
    let centre = chosen.iter().copied().sum::<Vec3>() / chosen.len() as f32;

    if buttons.just_pressed(MouseButton::Right) {
        if order.active {
            // The second press is the commit. Every selected ship gets the
            // same point offset by where it already stands relative to the
            // group, so a formation arrives as a formation instead of all
            // piling onto one coordinate.
            let to = order.target();
            for (mut hull, xf, sel) in &mut hulls {
                if sel.is_some() && !hull.dead_hull {
                    hull.order = Some(to + (xf.translation - centre));
                }
            }
            order.active = false;
            return;
        }
        order.active = true;
        order.lift = 0.0;
        order.lifting = false;
        order.plane_y = centre.y;
        order.on_plane = centre;
        return;
    }

    if keys.just_pressed(KeyCode::Escape) && order.active {
        // Cancelled. `toggle_pause` sees the order was open and leaves the
        // menu alone this frame, so one escape does one thing.
        order.active = false;
        return;
    }

    if !order.active {
        return;
    }

    if shift {
        order.lifting = true;
        // Up on the screen is up in the world. The scale is a fraction of the
        // ship's own size per pixel, so the reach of a drag is the same on
        // any hull and at any window size.
        order.lift -= dy * radius * 0.02;
    } else if let Some(cursor) = window.cursor_position() {
        // The cursor names a place on the plane, and only while shift is not
        // held: the two cannot both own the mouse.
        if let Ok(ray) = cam.viewport_to_world(cam_xf, cursor) {
            if let Some(d) = ray.intersect_plane(Vec3::new(0.0, order.plane_y, 0.0), InfinitePlane3d::new(Vec3::Y)) {
                order.on_plane = ray.get_point(d);
            }
        }
    }
}

/// A ship the player has picked. Only ever on the player's own.
#[derive(Component)]
struct Selected;

/// The band box, in screen pixels, while the left button is down.
#[derive(Resource, Default)]
struct Marquee {
    from: Option<Vec2>,
    to: Vec2,
}

/// Left button picks: a click takes one ship, a drag takes a box of them.
///
/// This is the button an RTS gives to selection, so the camera had to give it
/// up: orbit is the MIDDLE button now, with alt and left as an alias for a
/// mouse that has no middle. Holding shift adds to the selection rather than
/// replacing it, which is the one convention every RTS shares.
fn select_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    over_ui: Query<&Interaction>,
    mut marquee: ResMut<Marquee>,
    mut commands: Commands,
    ships: Query<(Entity, &Transform, Option<&Hull>, Option<&Fighter>), Or<(With<Hull>, With<Fighter>)>>,
    hives: Query<(), With<Hive>>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((cam, cam_xf)) = cams.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    if over_ui.iter().any(|i| *i != Interaction::None) {
        return;
    }
    if keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight) {
        return;
    }

    if buttons.just_pressed(MouseButton::Left) {
        marquee.from = Some(cursor);
        marquee.to = cursor;
    }
    if marquee.from.is_some() {
        marquee.to = cursor;
    }
    let Some(from) = marquee.from else { return };
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    marquee.from = None;

    let add = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if !add {
        for (e, _, _, _) in &ships {
            commands.entity(e).remove::<Selected>();
        }
    }
    let lo = from.min(marquee.to);
    let hi = from.max(marquee.to);
    // A click rather than a drag: take whatever is nearest the pointer instead
    // of whatever is inside a box a few pixels across.
    let click = (hi - lo).length() < 6.0;
    let mut best: Option<(f32, Entity)> = None;

    for (e, xf, hull, _) in &ships {
        // Carriers are hulls too. They are not yours and cannot be ordered.
        if hives.get(e).is_ok() {
            continue;
        }
        if hull.map(|h| h.dead_hull).unwrap_or(false) {
            continue;
        }
        let Ok(p) = cam.world_to_viewport(cam_xf, xf.translation) else { continue };
        if click {
            let d = p.distance(cursor);
            if d < 60.0 && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, e));
            }
        } else if p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y {
            commands.entity(e).insert(Selected);
        }
    }
    if let Some((_, e)) = best {
        commands.entity(e).insert(Selected);
    }
}

/// Fly the hull to wherever it was told, and turn it to face the way it is
/// going.
///
/// A real envelope rather than a lerp: it accelerates, it has a top speed, and
/// it slows into the arrival, so a move has weight and a player can see that
/// giving an order to a capital ship is a commitment.
fn fly_hull(
    time: Res<Time>,
    scene: Res<Scene>,
    lead: Res<Lead>,
    cfg: Res<SwarmConfig>,
    // WITHOUT a carrier. A mothership is a `Hull` now so that cells come off
    // it the way they come off a ship, and every system that takes hulls
    // therefore takes carriers too unless it says otherwise. This one would
    // have every carrier flying the flagship's own orders.
    mut hulls: Query<(&mut Hull, &mut Transform, Option<&Escort>), Without<Hive>>,
) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (mut hull, mut xf, escort) in &mut hulls {
        let radius = hull.model.radius();
        let (speed, accel, arrive) = (radius * HULL_SPEED, radius * HULL_ACCEL, radius * ARRIVE);
        let mut hurry = 1.0;
        let want = match escort {
            // An escort has no order of its own: its goal is a place in the
            // formation, which MOVES, so it is never reached and never
            // cleared. Matching the leader's velocity is what makes it hold
            // station rather than trail: steering alone would put it
            // permanently behind by however far it takes to close the gap.
            Some(e) => {
                let goal = lead.pos + lead.rot * e.station;
                let to = goal - xf.translation;
                let d = to.length();
                // A reinforcement ARRIVES: three times cruise while it is far
                // out, easing back to cruise over the last eight lengths, so
                // a wave called from off the map is on station in seconds
                // rather than in the minute a capital ship's cruise would
                // take. It is the same envelope, with the cap raised.
                hurry = 1.0 + 2.0 * (d / (radius * 8.0)).min(1.0);
                if d < arrive * 0.5 {
                    lead.vel
                } else {
                    lead.vel + to / d * speed * hurry * (d / (radius * 3.0)).min(1.0)
                }
            }
            None => match hull.order {
                Some(t) => {
                    let to = t - xf.translation;
                    let d = to.length();
                    if d < arrive {
                        hull.order = None;
                        Vec3::ZERO
                    } else {
                        // Slow into it: the speed asked for falls off over the
                        // last few lengths, so it settles rather than overshooting
                        // and hunting back and forth.
                        to / d * speed * (d / (radius * 4.0)).min(1.0)
                    }
                }
                None => Vec3::ZERO,
            },
        };
        let mut want = want;
        // ---- round the rocks ----
        //
        // A ship used to fly straight through an asteroid, which is the field
        // being scenery rather than terrain. It steers on the same distance
        // field the swarm does, at the ship's own scale: a rock inside the
        // keep-out radius pushes the ship out along the normal, harder the
        // closer it is.
        for r in &cfg.rocks {
            let off = xf.translation - r.truncate();
            let d = off.length();
            let keep = r.w + radius * ROCK_CLEAR;
            if d < keep && d > 1e-4 {
                let n = off / d;
                let bite = ((keep - d) / keep).clamp(0.0, 1.0);
                want += n * speed * bite * 3.0;
            }
        }

        let dv = want - hull.vel;
        let step = accel * hurry * dt;
        let was = hull.vel;
        hull.vel += if dv.length() > step { dv.normalize() * step } else { dv };
        hull.accel = if dt > 0.0 { (hull.vel - was) / dt } else { Vec3::ZERO };
        xf.translation += hull.vel * dt;

        // And never INSIDE a rock. Steering can be beaten: an order given
        // straight through an asteroid asks for exactly that, and a capital
        // ship has the momentum to win the argument. This is the guarantee,
        // and it costs nothing in the normal case.
        for r in &cfg.rocks {
            let off = xf.translation - r.truncate();
            let d = off.length();
            let keep = r.w + radius * 0.9;
            if d < keep && d > 1e-4 {
                let n = off / d;
                xf.translation = r.truncate() + n * keep;
                // Slide along it rather than stopping dead on it.
                let into = hull.vel.dot(n).min(0.0);
                hull.vel -= n * into;
            }
        }

        // Face the way it is going, eased, and only while it is going
        // anywhere: a ship at rest keeps the heading it stopped on.
        //
        // NEGATED, and this was a real bug rather than a taste. Bevy's
        // `forward` is -Z and `looking_to` aims that at the direction it is
        // given; a hull's bow is +Z, because the lattice runs stern to bow.
        // Aimed straight, the ship flew stern first with its main engines
        // leading, which read as a retro firing at full throttle while
        // accelerating and was what made the flames look wrong.
        if hull.vel.length() > radius * 0.05 {
            let want = Transform::from_translation(xf.translation).looking_to(-hull.vel.normalize(), Vec3::Y).rotation;
            xf.rotation = xf.rotation.slerp(want, (dt * 1.6).min(1.0));
        }
    }
}

/// The swarm wants the ship, so it has to be told where the ship IS.
///
/// Which is what makes moving it worth doing: the cloud is pulled along
/// behind, and a player who runs can watch the swarm string out.
fn publish_hull(
    lead_q: Query<(Entity, &Hull, &Transform), With<Flagship>>,
    // Every player hull, which is the flagship and the wing. NOT the carriers:
    // they are hulls too now, and a swarm that attacked its own motherships
    // would be a fight with one side in it.
    ships: Query<(&Hull, &Transform), Without<Hive>>,
    mut cfg: ResMut<SwarmConfig>,
    mut lead: ResMut<Lead>,
    mut was: Local<Option<Entity>>,
    mut last_n: Local<usize>,
) {
    // ---- what the swarm may attack ----
    //
    // A LIST, not a centre. The cloud used to chase one published position, so
    // it was always one animal on one ship: calling in reinforcements put five
    // frigates on the map and the swarm still sat on exactly one of them,
    // which is not what a swarm does and makes "divide it by where you put
    // your ships" impossible to express. Every live hull is a target now and a
    // mote picks one from its own seed.
    cfg.targets.clear();
    for (hull, xf) in &ships {
        if hull.dead_hull || cfg.targets.len() >= swarm::MAX_TARGETS {
            continue;
        }
        cfg.targets.push(xf.translation.extend(hull.model.radius()));
    }
    if *last_n != cfg.targets.len() {
        info!("the swarm has {} ships to divide between", cfg.targets.len());
        *last_n = cfg.targets.len();
    }

    // The FLAGSHIP, not whichever hull the query happened to yield last. With
    // one ship those were the same thing and this iterated; with a wing out,
    // the cloud would have chased whichever escort was stored last and the
    // formation would have tried to keep station on itself.
    // The flagship, for the camera, the nav disc and the formation. It is one
    // of the targets above and has no special standing to the swarm.
    let n = lead_q.iter().count();
    if n != 1 {
        if was.is_some() {
            warn!("the swarm has {n} flagships to chase, keeping the last target");
            *was = None;
        }
        return;
    }
    let Ok((e, hull, xf)) = lead_q.single() else { return };
    if *was != Some(e) {
        info!("the flagship is {e}");
        *was = Some(e);
    }
    cfg.hull_centre = xf.translation;
    cfg.hull_radius = hull.model.radius();
    lead.pos = xf.translation;
    lead.rot = xf.rotation;
    lead.vel = hull.vel;
}

/// Hand a freshly spawned hull the order that was waiting for it.
fn apply_nav_to(mut commands: Commands, mut q: Query<(Entity, &mut Hull, &NavTo)>) {
    for (e, mut hull, to) in &mut q {
        hull.order = Some(to.0);
        commands.entity(e).remove::<NavTo>();
    }
}

/// R calls in a wave: two more of the flagship's class, on the next free
/// stations of the wing.
///
/// Capped, because a formation is a picture and the tenth ship in it is a
/// ship nobody can see: `WING_MAX` is what the stations are laid out for.
fn call_reinforcements(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    scene: Res<Scene>,
    lead: Res<Lead>,
    flagship: Query<&Hull, With<Flagship>>,
    escorts: Query<(), With<Escort>>,
    button: Query<&Interaction, (Changed<Interaction>, With<CallButton>)>,
) {
    let clicked = button.iter().any(|i| *i == Interaction::Pressed);
    if !keys.just_pressed(KeyCode::KeyR) && !clicked {
        return;
    }
    let Ok(hull) = flagship.single() else { return };
    let radius = hull.model.radius();
    let out = escorts.iter().count() as u32;
    if out >= WING_MAX {
        info!("the wing is full at {WING_MAX}");
        return;
    }
    let call = WING_WAVE.min(WING_MAX - out);
    for n in out..out + call {
        call_one(&mut commands, &mut meshes, &mut materials, &tex, &scene.hull, radius, lead.pos, lead.rot, (scene.chewers / 3) as u32, n);
    }
    info!("{call} reinforcements inbound, {} in the wing (escorts, not flagships)", out + call);
}

/// One line as a quad turned edge on to the eye, which is the same trick the
/// beams use and for the same reason: it is the same width from anywhere.
#[allow(clippy::too_many_arguments)]
fn add_line(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    eye: Vec3,
    a: Vec3,
    b: Vec3,
    w: f32,
    c: [f32; 3],
    alpha: f32,
) {
    let dir = (b - a).normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let across = dir.cross(eye - (a + b) * 0.5).normalize_or(Vec3::Y.cross(dir).normalize_or(Vec3::X));
    let base = pos.len() as u32;
    for s in [-1.0f32, 1.0] {
        pos.push((a + across * w * s).to_array());
        pos.push((b + across * w * s).to_array());
        col.push([c[0], c[1], c[2], alpha]);
        col.push([c[0], c[1], c[2], alpha]);
    }
    idx.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
}

/// Draw the order: a disc on the plane, the line back down to it, and the
/// track from the ship.
fn draw_nav(
    order: Res<NavOrder>,
    handle: Option<Res<NavHandle>>,
    hulls: Query<(&Hull, &Transform), With<Flagship>>,
    cam: Query<&Transform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(handle) = handle else { return };
    let Ok(eye) = cam.single() else { return };
    let Ok((hull, hull_xf)) = hulls.single() else { return };
    let eye = eye.translation;
    let radius = hull.model.radius();

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    let green = [0.35, 2.6, 0.9];
    let (target, on_plane, show_disc) = match (order.active, hull.order) {
        // Being issued: the live one, disc and all.
        (true, _) => (order.target(), order.on_plane, true),
        // Committed and under way: the track and a ring at the far end, so a
        // player can see where a ship is going after they have let go.
        (false, Some(t)) => (t, Vec3::new(t.x, hull_xf.translation.y, t.z), true),
        (false, None) => (Vec3::ZERO, Vec3::ZERO, false),
    };

    if show_disc {
        // The disc: a flat ribbon in the horizontal plane, because that is
        // what it MEANS. Edge on from the side is correct and is exactly the
        // cue that tells a player the plane is a plane.
        const SEGMENTS: usize = 64;
        let r = radius * 1.6;
        for n in 0..SEGMENTS {
            let a0 = n as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let a1 = (n + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let p0 = on_plane + Vec3::new(a0.cos(), 0.0, a0.sin()) * r;
            let p1 = on_plane + Vec3::new(a1.cos(), 0.0, a1.sin()) * r;
            let base = pos.len() as u32;
            let w = radius * 0.03;
            for (p, a) in [(p0, a0), (p1, a1)] {
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                pos.push((p - out * w).to_array());
                pos.push((p + out * w).to_array());
                col.push([green[0], green[1], green[2], 0.85]);
                col.push([green[0], green[1], green[2], 0.85]);
            }
            idx.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
        }
        // Ticks round the rim, so the disc reads as an instrument and its
        // size is legible against the ship.
        for n in 0..8 {
            let a = n as f32 / 8.0 * std::f32::consts::TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            add_line(&mut pos, &mut col, &mut idx, eye, on_plane + out * r * 0.86, on_plane + out * r * 1.14, radius * 0.02, green, 0.7);
        }
        // The lift: straight up from the plane to where the ship is actually
        // being sent. This is the whole of what shift is for.
        if (target.y - on_plane.y).abs() > 1e-3 || order.lifting {
            add_line(&mut pos, &mut col, &mut idx, eye, on_plane, target, radius * 0.035, green, 0.9);
            // A cross at the far end, so the point itself has a mark.
            for d in [Vec3::X, Vec3::Z] {
                add_line(&mut pos, &mut col, &mut idx, eye, target - d * radius * 0.35, target + d * radius * 0.35, radius * 0.03, green, 0.95);
            }
        }
        // And the track from the ship, dimmer: where it is going FROM.
        add_line(&mut pos, &mut col, &mut idx, eye, hull_xf.translation, target, radius * 0.02, [0.2, 1.1, 0.5], 0.35);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
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
    /// The place the camera is looking at. The player's, not the ship's.
    target: Vec3,
    /// Easing onto the flagship because the player asked for it. Cleared when
    /// it arrives and cleared the instant the player pans, because a focus
    /// that fought the pan keys would be a camera arguing with its own user.
    follow: bool,
}

/// The camera's own controls, and nothing else touches them.
///
/// Left drag turns it, the wheel pulls it in and out, WASD and the arrows pan
/// the FOCUS across the plane the camera is looking along, and space snaps it
/// to the flagship. Right belongs to the nav order: the two cannot share a
/// button, and Homeworld gives the world to the right.
fn orbit_input(
    time: Res<Time>,
    scene: Res<Scene>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    hud: Res<Hud>,
    over_ui: Query<&Interaction>,
    mut q: Query<&mut Orbit>,
) {
    let Ok(mut o) = q.single_mut() else { return };
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };

    // The pointer over a button belongs to the BUTTON. `orbit_input` reads the
    // raw mouse and Bevy's UI does not consume it, so pressing Call
    // reinforcements used to drag the camera at the same time: every click on
    // the HUD threw the view sideways, which is most of what "the camera keeps
    // snapping" was. Drained rather than ignored, so the motion of a drag that
    // started on a button does not arrive in one lump when it leaves.
    let on_ui = over_ui.iter().any(|i| *i != Interaction::None);
    if on_ui || hud.paused {
        motion.clear();
        wheel.clear();
        return;
    }

    // ---- the drag, and why it used to throw the camera across the map ----
    //
    // Motion is DRAINED whenever a drag is not in progress, including on the
    // frame the button goes down. `MessageReader` keeps everything that
    // arrived since this system last read it, so a reader that only consumes
    // events while the button is held is a reader with a backlog: move the
    // mouse across the desk with the button up and the whole journey is
    // waiting, and it all applies on the first frame of the next drag.
    //
    // And a single event's delta is CLAMPED. The window handing back focus,
    // the pointer leaving and re-entering, or a compositor releasing a grab
    // all deliver one event carrying thousands of pixels. At 0.005 radians a
    // pixel that is several whole turns inside one frame, which is exactly
    // "the camera jumped to a random angle", and it happens at the edge of the
    // screen, which is why it seemed to depend on which way you had turned.
    // MIDDLE, or alt and left. The left button belongs to selection now,
    // which is what an RTS gives it; alt and left is the alias for a mouse
    // with no middle button.
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let dragging = buttons.pressed(MouseButton::Middle) || (alt && buttons.pressed(MouseButton::Left));
    let started = buttons.just_pressed(MouseButton::Middle) || (alt && buttons.just_pressed(MouseButton::Left));
    if !dragging || started {
        motion.clear();
    } else {
        for m in motion.read() {
            let d = m.delta.clamp(Vec2::splat(-MAX_DRAG), Vec2::splat(MAX_DRAG));
            o.yaw -= d.x * 0.005;
            o.pitch = (o.pitch + d.y * 0.005).clamp(-1.4, 1.4);
        }
    }
    // Wrapped, so a long session cannot walk the yaw out to where a float has
    // no precision left and the camera moves in visible steps.
    o.yaw = o.yaw.rem_euclid(std::f32::consts::TAU);

    // Zoom is MULTIPLICATIVE and goes through `exp`, which cannot return a
    // negative number however big the input is. That is the whole fix for the
    // snap: it was `dist * (1.0 - y * 0.08)`, and a wheel reporting PIXELS
    // rather than lines hands over a y of a hundred or more per notch, so the
    // factor came out at minus seven, the distance went negative, and the
    // clamp slammed the camera to its near stop. One notch the other way and
    // it slammed to the far one. A trackpad does this on every scroll.
    for w in wheel.read() {
        let step = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 40.0,
        };
        o.dist = (o.dist * (-step.clamp(-4.0, 4.0) * 0.12).exp()).clamp(2.0, 400.0);
    }

    // Panning is in the CAMERA's frame, not the world's: pressing left has to
    // move the view left whatever way the camera is turned, or the keys mean
    // something different at every heading and nobody can learn them.
    let fwd = Vec3::new(o.yaw.sin(), 0.0, o.yaw.cos());
    let right = Vec3::new(o.yaw.cos(), 0.0, -o.yaw.sin());
    let mut pan = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan -= fwd;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan += fwd;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan -= right;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan += right;
    }
    // And up and down, because this is a game in three dimensions and a
    // camera that could only pan on one plane could not be put above or below
    // a fight that is happening at an angle to it.
    if keys.pressed(KeyCode::KeyE) || keys.pressed(KeyCode::PageUp) {
        pan += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::PageDown) {
        pan -= Vec3::Y;
    }
    if pan != Vec3::ZERO {
        // Scaled by how far out the camera is, so one press covers the same
        // share of the screen at every zoom: panning at a hundred units with
        // a step tuned for ten is a camera that will not move.
        let step = o.dist * PAN_RATE * dt;
        o.target += pan.normalize() * step;
        // The player has taken the wheel, so the focus lets go.
        o.follow = false;
    }
    // F, not space. Space pauses, which is what an RTS does with it and what
    // makes "stop the world and give orders" possible at all.
    if keys.just_pressed(KeyCode::KeyF) {
        o.follow = true;
    }

    // Nothing leaves this function as a NaN. Every expression above is guarded
    // at the point it could go wrong, so this should never fire; it is here
    // because a NaN in the camera is not a wrong picture, it is EVERY picture
    // wrong from now on, since the bad value is stored and fed back in next
    // frame. A guard that can only ever be redundant is the right price for
    // that.
    if !o.yaw.is_finite() || !o.pitch.is_finite() || !o.dist.is_finite() || !o.target.is_finite() {
        warn!("camera went non finite, reset");
        *o = Orbit { yaw: 0.6, pitch: 0.38, dist: 40.0, target: Vec3::ZERO, follow: true };
    }
}

/// An RTS camera: the focus is a PLACE, and the angles are the player's.
///
/// It used to ease its focus onto the flagship every frame, which is a chase
/// camera wearing an orbit's controls. Two things are wrong with that on a
/// game about where you put your ships. You cannot look at anything except
/// the ship, so the swarm, the carriers and the rocks can only be seen by
/// flying to them; and the moment you give a move order the whole world
/// slides under you, which is the ship staying still and everything else
/// moving, exactly backwards from what an order is.
///
/// So the focus is a point in the world the player drives, and `Orbit.follow`
/// is how it gets to a ship: set by a key, eased in, and DROPPED the moment
/// the player pans. Focusing is a thing you ask for rather than a state you
/// are stuck in.
fn orbit_camera(
    time: Res<Time>,
    scene: Res<Scene>,
    hulls: Query<&Transform, (With<Flagship>, Without<Camera3d>)>,
    mut q: Query<(&mut Orbit, &mut Transform), With<Camera3d>>,
) {
    let dt = if scene.fixed_dt { 1.0 / 60.0 } else { time.delta_secs().min(swarm::STEP_CLAMP) };
    for (mut o, mut xf) in &mut q {
        if o.follow {
            if let Ok(hull) = hulls.single() {
                // Eased on `1 - exp(-k dt)` so the ease takes the same wall
                // time at twenty frames a second as at a hundred and twenty,
                // which is the rule redux-tribes' camera keeps.
                let k = 1.0 - (-3.4 * dt).exp();
                o.target = o.target.lerp(hull.translation, k);
                // And it LETS GO once it has arrived, so a focus is a move to
                // a place rather than a lock: the ship then flies out of the
                // middle of the view under its own power, which is what says
                // it is going somewhere.
                // Against the camera's own DISTANCE, not against how far the
                // ship happens to be from the world origin. The old test grew
                // its own threshold as the ship flew away from nothing in
                // particular, so a focus let go at a different gap depending
                // on where in the map it was asked for.
                if o.target.distance(hull.translation) < o.dist * 0.004 {
                    o.follow = false;
                }
            } else {
                o.follow = false;
            }
        }
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
    hulls: Query<&Hull>,
    fx: Res<LiveFx>,
    quads: Res<BeamQuads>,
    tex: Res<Textures>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut frames: Local<u32>,
    mut started: Local<Option<Instant>>,
    mut commands: Commands,
) {
    // Wall time, from an `Instant`, and NOT the sum of `Time::delta_secs()`.
    //
    // Bevy clamps the virtual delta at 250 ms so that one stalled frame
    // cannot make everything jump; the effect is that a frame slower than
    // that is REPORTED as 250 ms however long it really took. On a software
    // rasteriser that is exactly the range these runs live in, so a report
    // built out of deltas is a report that quietly stops counting at four
    // frames a second. It was also why the frame limiter looked broken when
    // it was working: capped at two a second, the sum still said four.
    let start = *started.get_or_insert_with(Instant::now);
    *frames += 1;
    if h.shot || *frames < h.frames {
        return;
    }
    let spent = start.elapsed().as_secs_f32();
    let Some(t) = target else { return };
    h.shot = true;
    let breaches: usize = hulls.iter().map(|x| x.breaches).sum();
    let dead: usize = hulls.iter().map(|x| x.damage.dead_count()).sum();
    println!(
        "headless: {} frames in {:.1}s ({:.1} ms/frame mean, wall clock), swarm ticks {}, chewed {} cells ({} breaches thrown)",
        *frames, spent, spent * 1000.0 / *frames as f32, clock.ticks, dead, breaches
    );
    println!(
        "fx: {} beams fired and {} flak bursts, {} beams live and {} quads on the last frame, {} blasts live, {} sparks queued",
        fx.fired, fx.flak, fx.beams.len(), quads.0, fx.blasts.len(), fx.sparked
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
