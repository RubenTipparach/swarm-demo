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

mod app;
mod controllers;
mod fighters;
mod fx;
mod hives;
mod retreat;
mod ships;
mod swarm;
mod ui;
mod weapons;
mod world;

use app::*;
use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::{LoadState, RenderAssetUsages},
    camera::RenderTarget,
    ecs::system::SystemParam,
    image::{
        ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    },
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::{
            Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
            TextureViewDimension,
        },
        view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
    },
    sprite::{BorderRect, SliceScaleMode, TextureSlicer},
    ui::widget::NodeImageMode,
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use controllers::*;
use fighters::*;
use fx::*;
use hives::*;
use retreat::*;
use ships::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use swarm::{
    spawn_mote_mesh, spawn_spark_mesh, Capsule, FxTextures, Shots, SparkQueue, SwarmClock,
    SwarmConfig, SwarmPlugin, GRID,
};
use swarm_core::voxel::{mat, purpose, SURF_DRIVE};
use swarm_core::{
    alien::{generate, Archetype},
    body::Body,
    build::{self, Category, Module, Order, Slots, Task, Tier, Yard},
    damage::{chunk_for, wear_by_purpose, Breach, Chunk, DamageGrid, Vent, Wear},
    economy::{yield_of, Cube, Cut, Pack, Yield, DATA_CUBE, ORE_CUBE},
    formation::{self, Shape},
    fx::{
        blast_sparks, breach_sparks, engine_clusters, engines_of, gun_clusters, guns_of,
        muzzle_sparks, reactor_of, shatter, turret_split, Beam, Blast, Gun, Spark, SparkKind,
    },
    map::{self, Map, Node as MapNode, Tag},
    mesh::{greedy_mesh, mesh_region, srgb_to_linear, MeshData, Surfaces},
    rng::{drift_of, Rng},
    rock::{self, Flavour},
    sky::{bake_cubemap, starfield, to_half, SkyPreset},
    tide::{Phase, Tide},
    VoxelModel, SURF_COUNT,
};
use ui::*;
use weapons::*;
use world::*;

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
    wreck: u32,
    /// `--view mission|sensors|menu` opens one of the middle views at start,
    /// since a headless run has no pointer to press its tab with. The sensors
    /// manager is a camera MODE, so this is the only way to photograph what
    /// the eye does when it is asked for.
    view: String,
    /// `--panel build|research|launch` opens the side panel on one of its
    /// tabs at start, since a headless run has no pointer to press one with.
    /// Same rule as `--view`: a flag per thing a player presses, because a
    /// tab whose body nothing can photograph is a tab nobody can check.
    panel: String,
    /// `--build WHAT,TICK`: press one build row at that tick, so a hull coming
    /// off the queue can be photographed. WHAT is a class key or `fighter`.
    build: u32,
    build_what: String,
    /// Ticks between one gun firing and the next. Nought silences them.
    cadence: u32,
    /// How many motherships the swarm flies from.
    hives: usize,
    /// Issue one move order at startup, so a headless render can show the
    /// ship under way with its standing order drawn.
    order: Option<Vec3>,
    /// Open the move disc at startup, aimed at this point, so a headless
    /// render can show an order being GIVEN: the disc, the triangle, the
    /// label.
    aim: Option<Vec3>,
    /// Draw the four archetypes in a row beside the hull. Off by default now
    /// that the carriers are the aliens on show.
    showcase: bool,
    /// Frames a second, at most. Nought lifts the cap.
    fps: u32,
    /// How thick the swarm is to light, as a multiplier on how much of a
    /// cell one mote blocks. Nought is the flat lighting this replaced, which
    /// is what an A/B of the shading is taken against.
    thickness: f32,
    /// Advance exactly one tick a frame rather than by the wall clock.
    ///
    /// A software rasteriser draws at four frames a second, so a frame here
    /// is fourteen ticks and a screenshot cannot be aimed at one: the shot
    /// meant for the fireball arrives four hundred ticks after it went out.
    /// This is for the harness, and it is what makes a headless render a
    /// function of its frame count rather than of how fast the machine is.
    fixed_dt: bool,
    /// Which screen to open on: `menu`, `setup` or `result`. Implies
    /// `--hud`, because a screen is UI and UI needs the UI camera.
    screen: Option<String>,
    /// A playground rather than a fight: a target dummy, the range, the
    /// toggles, and no verdict.
    sandbox: bool,
    /// Straight into the fight from a window, the way the harness always is.
    play: bool,
    target_hull: String,
    seed: u64,
    stand: f32,
    /// One scripted shot on the range: `--fire slug,40` lands a slug on the
    /// dummy at tick forty, so a tumble can be photographed.
    fire: Option<(Weapon, u32)>,
    /// `--blast TICK`: one sandbox blast down the camera's own line at that
    /// tick, so what it does to a hull and to the cloud can be photographed.
    /// There is no cursor in a headless run, and the blast is aimed with one.
    blast: Option<u32>,
    /// A system of The Long Retreat: gather, hold, and jump out before the
    /// swarm's fleet arrives.
    retreat: bool,
    /// `--base` opens the skirmish's base building mode, which is the siege:
    /// a station, the rocks and a tide that never stops.
    base: bool,
    /// The run's seed, which is the whole map.
    run_seed: u64,
    /// `--job TICK` puts every support ship to work at that tick, so a
    /// headless run can photograph a shaft being cut.
    job: Option<u32>,
    /// Which support ships the fleet arrives with, by role: `--support
    /// miner,miner,salvager`. Empty means the run's own fleet.
    support: Vec<Role>,
    /// `--jump TICK` has the drive ready at that tick, so the jump and the
    /// screen after it can be photographed without mining for the fuel.
    jump: Option<u32>,
    /// And `--onward` goes straight on into the next system rather than
    /// waiting on the map for a branch to be pressed.
    onward: bool,
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
        wreck: 0,
        build: 0,
        build_what: String::new(),
        view: String::new(),
        panel: String::new(),
        cadence: 70,
        hives: 10,
        order: None,
        aim: None,
        showcase: false,
        fps: 120,
        thickness: 1.0,
        fixed_dt: false,
        screen: None,
        sandbox: false,
        play: false,
        target_hull: "karisen_frigate".into(),
        seed: 4242,
        stand: 1.0,
        fire: None,
        blast: None,
        retreat: false,
        base: false,
        run_seed: 0xB0A7,
        job: None,
        support: Vec::new(),
        jump: None,
        onward: false,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let next = || argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--headless" => a.headless = true,
            "--motes" => {
                a.motes = next().parse().expect("--motes N");
                i += 1;
            }
            "--hull" => {
                a.hull = next();
                i += 1;
            }
            "--out" => {
                a.out = next();
                i += 1;
            }
            "--frames" => {
                a.frames = next().parse().expect("--frames N");
                i += 1;
            }
            "--zoom" => {
                a.zoom = next().parse().expect("--zoom R");
                i += 1;
            }
            "--target" => {
                let s = next();
                let v: Vec<f32> = s
                    .split(',')
                    .map(|x| x.parse().expect("--target x,y,z"))
                    .collect();
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
            "--chewers" => {
                a.chewers = next().parse().expect("--chewers N");
                i += 1;
            }
            "--view" => {
                a.view = next();
                i += 1;
            }
            "--panel" => {
                a.panel = next();
                i += 1;
            }
            "--build" => {
                let arg = next();
                let (what, tick) = arg.split_once(',').expect("--build WHAT,TICK");
                a.build_what = what.to_string();
                a.build = tick.parse().expect("--build WHAT,TICK");
                i += 1;
            }
            "--wreck" => {
                a.wreck = next().parse().expect("--wreck TICK");
                i += 1;
            }
            "--explode" => {
                a.explode = next().parse().expect("--explode TICK");
                i += 1;
            }
            "--cadence" => {
                a.cadence = next().parse().expect("--cadence TICKS");
                i += 1;
            }
            "--fixed-dt" => a.fixed_dt = true,
            "--screen" => {
                a.screen = Some(next());
                a.hud = true;
                i += 1;
            }
            "--sandbox" => a.sandbox = true,
            "--retreat" => a.retreat = true,
            "--base" => a.base = true,
            "--run-seed" => {
                a.run_seed = next().parse().expect("--run-seed N");
                i += 1;
            }
            // `--job` is the right click that puts every support ship to
            // work, and BOTH modes with support ships have one now: it used
            // to turn a run on by itself, which made a base render into a
            // retreat with a jump drive and a fuel gauge on it. A flag that
            // quietly changes the mode is a flag that cannot photograph the
            // other one, so the mode is named on the command line and this
            // only says when the work starts.
            "--job" => {
                a.job = Some(next().parse().expect("--job TICK"));
                i += 1;
            }
            "--onward" => {
                a.onward = true;
                a.retreat = true;
            }
            "--jump" => {
                a.jump = Some(next().parse().expect("--jump TICK"));
                a.retreat = true;
                i += 1;
            }
            "--support" => {
                a.support = next()
                    .split(',')
                    .map(|r| {
                        Role::parse(r).unwrap_or_else(|| panic!("--support: no role called {r}"))
                    })
                    .collect();
                a.retreat = true;
                i += 1;
            }
            "--play" => a.play = true,
            "--target-hull" => {
                a.target_hull = next();
                i += 1;
            }
            "--seed" => {
                a.seed = next().parse().expect("--seed N");
                i += 1;
            }
            "--stand" => {
                a.stand = next().parse().expect("--stand R");
                i += 1;
            }
            "--fire" => {
                let s = next();
                let (w, at) = s.split_once(',').unwrap_or((&s, "40"));
                let w = Weapon::parse(w).expect("--fire beam|flak|slug|torpedo|bite[,TICK]");
                a.fire = Some((w, at.parse().expect("--fire WEAPON,TICK")));
                a.sandbox = true;
                i += 1;
            }
            "--blast" => {
                a.blast = Some(next().parse().expect("--blast TICK"));
                a.sandbox = true;
                i += 1;
            }
            "--hives" => {
                a.hives = next().parse().expect("--hives N");
                i += 1;
            }
            "--move" => {
                let v: Vec<f32> = next()
                    .split(',')
                    .map(|x| x.parse().expect("--move x,y,z"))
                    .collect();
                a.order = Some(Vec3::new(v[0], v[1], v[2]));
                i += 1;
            }
            "--aim" => {
                let v: Vec<f32> = next()
                    .split(',')
                    .map(|x| x.parse().expect("--aim x,y,z"))
                    .collect();
                a.aim = Some(Vec3::new(v[0], v[1], v[2]));
                i += 1;
            }
            "--showcase" => a.showcase = true,
            "--hud" => a.hud = true,
            "--yaw" => {
                a.yaw = next().parse().expect("--yaw RADIANS");
                i += 1;
            }
            "--pitch" => {
                a.pitch = next().parse().expect("--pitch RADIANS");
                i += 1;
            }
            "--paused" => {
                a.hud = true;
                a.paused = true;
            }
            "--reinforce" => {
                a.reinforce = next().parse().expect("--reinforce N");
                i += 1;
            }
            "--rocks" => {
                a.rocks = next().parse().expect("--rocks N");
                i += 1;
            }
            "--launch-delay" => {
                a.launch_delay = next().parse().expect("--launch-delay SECONDS");
                a.delay_set = true;
                i += 1;
            }
            "--fighters" => {
                a.fighters = next().parse().expect("--fighters N");
                i += 1;
            }
            "--fps" => {
                a.fps = next().parse().expect("--fps N, or 0 for no cap");
                i += 1;
            }
            "--thickness" => {
                a.thickness = next().parse().expect("--thickness R");
                i += 1;
            }
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
    // A window opens on the menu and a harness opens on its subject, unless
    // either says otherwise.
    let initial = match args.screen.as_deref() {
        Some("menu") => AppState::Menu,
        Some("setup") => AppState::Setup,
        Some("result") => AppState::Result,
        Some("map") => AppState::Map,
        Some(other) => panic!("--screen {other}: menu, setup or result"),
        None if args.headless || args.play => AppState::Playing,
        None => AppState::Menu,
    };
    let mut app = App::new();
    let fleet = Fleet::load();
    let scene = SceneSpec {
        hull: args.hull.clone(),
        // Empty until a base moves the form's pick into it: `wing_class`
        // falls back to the flagship's own class in every other mode.
        escort: String::new(),
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
        wreck: args.wreck,
        build: args.build,
        build_what: args.build_what.clone(),
        cadence: args.cadence,
        hives: args.hives.min(swarm::MAX_HIVES),
        order: args.order,
        aim: args.aim,
        showcase: args.showcase,
        thickness: args.thickness,
        fixed_dt: args.fixed_dt,
        motes: args.motes,
        sandbox: args.sandbox,
        base: args.base,
        time_scale: 1.0,
        seed: args.seed,
        stand: args.stand,
        target_hull: args.target_hull.clone(),
        retreat: args.retreat,
        tide: Tide::default(),
        support: Vec::new(),
        lean: Flavour::Ore,
    };
    let mut scene = scene;
    if scene.base {
        scene.open_base();
    }
    let scene = scene;
    let mut run = RunState::new(args.run_seed, &scene.hull);
    if !args.support.is_empty() {
        run.fleet = args.support.clone();
    }
    let run = run;
    let mut scene = scene;
    if args.retreat {
        run.write(&mut scene);
        info!("{}", run.brief());
    }
    let scene = scene;
    let form = SetupForm::from_scene(&scene, &fleet);
    // `--screen result` has no fight to report on, so it reports a made up
    // one: the screen is what is being photographed.
    let outcome = if args.screen.as_deref() == Some("result") {
        Outcome {
            won: true,
            ticks: 9060,
            ships_lost: 1,
            cells_lost: 3412,
            hives_killed: 3,
            hives: 3,
            ships: 3,
            decided: None,
            jumped: true,
            left: 0,
            escaped: false,
        }
    } else {
        Outcome::default()
    };
    let assets = AssetPlugin {
        file_path: ASSETS.into(),
        ..default()
    };
    if args.headless {
        app.add_plugins(
            DefaultPlugins
                .set(assets)
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(1)))
        .insert_resource(Headless {
            frames: args.frames,
            out: args.out.clone(),
            width: args.width,
            height: args.height,
            shot: false,
        })
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
        app.add_systems(Startup, load_skin);
        // The menu, then the box, then the order, and each reads the mode the
        // one before left. `nav_input` is in the main chain below because it
        // runs headless too; the `before` is what keeps a left press from
        // being a confirm AND the start of a box in the same frame.
        app.add_systems(
            Update,
            retreat_readouts.run_if(in_state(AppState::Playing).and(|s: Res<SceneSpec>| s.retreat)),
        );
        app.add_systems(OnEnter(AppState::Playing), (build_hud, build_deck))
            .add_systems(
                Update,
                (
                    (toggle_pause, select_input, sandbox_input, assign_work)
                        .chain()
                        .before(nav_input),
                    rebuild_input,
                    jump_input,
                    (range_input, sandbox_fire).after(nav_input),
                    sandbox_readouts,
                    adopt_dummies,
                    hud_feedback,
                    tick_fps,
                    hud_orders.after(nav_input),
                    draw_marquee,
                    draw_bars,
                    pick_hull,
                    quit_to_menu,
                )
                    .run_if(in_state(AppState::Playing)),
            )
            // The deck: its two inputs run before `nav_input` for the reason
            // every input here does, which is that a press is read by exactly
            // one system per frame and the mode decides which.
            .add_systems(
                Update,
                (
                    (deck_tabs, deck_commands).chain().before(nav_input),
                    // The HUD is authored at 1600 by 900 and scaled, so this
                    // runs before anything reads a layout.
                    scale_hud,
                    // Guard reads the left press, so it runs with the other
                    // input systems and after the cell that opens its mode.
                    guard_input.after(deck_commands),
                    // The build menu: what it says, then what a press on it
                    // does, in that order so a press names the row that was
                    // actually drawn.
                    (fill_build_list, build_input, show_panel_body).chain(),
                    // The sensors furniture DRAWS, so it sits outside the run
                    // gate with the nav disc: an overview with the world
                    // stopped is worth exactly as much as an order is.
                    draw_sensors,
                    place_sensor_marks,
                    slide_deck,
                    light_deck,
                    deck_readouts,
                    deck_bars,
                    deck_roster,
                    show_unit_shot,
                    pick_group,
                    deck_state.run_if(|s: Res<SceneSpec>| s.sandbox),
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
    if args.fps > 0 {
        app.insert_resource(FrameLimit::new(args.fps))
            .add_systems(Last, limit_frames);
    }
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(SwarmConfig {
            count: args.motes,
            fixed_dt: args.fixed_dt,
            ..default()
        })
        .insert_resource(scene)
        .init_resource::<Tick>()
        .init_state::<AppState>()
        .insert_resource(Initial(initial))
        .insert_resource(outcome)
        .insert_resource(form)
        .insert_resource(fleet)
        .init_resource::<HullFacts>()
        .insert_resource(run)
        .init_resource::<Bank>()
        .insert_resource(Script {
            job: args.job,
            jump: args.jump,
            onward: args.onward,
        })
        .init_resource::<JumpDrive>()
        .init_resource::<TideState>()
        .insert_resource(Sandbox {
            auto: args.fire,
            auto_blast: args.blast,
            ..default()
        })
        .init_resource::<Landed>()
        .init_resource::<ChunkMaterials>()
        .add_plugins(SwarmPlugin)
        .init_resource::<Shots>()
        .init_resource::<SparkQueue>()
        .init_resource::<LiveFx>()
        .init_resource::<NavOrder>()
        .init_resource::<NavAsk>()
        // The command bar's own state. Narrow resources, one fact each, which
        // is what keeps a cell from having to ask three systems what it is.
        .init_resource::<WorkAsk>()
        .init_resource::<FleetStance>()
        .init_resource::<Wing>()
        .init_resource::<Docked>()
        .init_resource::<Rally>()
        .insert_resource(Views {
            open: match args.view.as_str() {
                "mission" => Some(ViewTab::Mission),
                "sensors" => Some(ViewTab::Sensors),
                "menu" => Some(ViewTab::Menu),
                _ => None,
            },
            panel: match args.panel.as_str() {
                "research" => PanelTab::Research,
                "launch" => PanelTab::Launch,
                _ => PanelTab::Build,
            },
            ..default()
        })
        .init_resource::<Offers>()
        .init_resource::<Deck>()
        .init_resource::<OrderMode>()
        .init_resource::<Pings>()
        .init_resource::<Ack>()
        .init_resource::<BeamQuads>()
        .init_resource::<Lead>()
        .init_resource::<Marquee>()
        .init_resource::<BarPool>()
        .insert_resource(Hud {
            paused: args.paused,
            ..default()
        })
        .add_systems(Startup, (load_textures, spawn_backdrop).chain())
        .add_systems(PostStartup, (mark_keep, boot).chain())
        // The field is spawned on the way INTO a scene, after the last one
        // is gone, and the menu takes it down so the sky is all that is left
        // behind it. A result keeps it: the wreck the fight ended on is the
        // picture that screen sits over.
        .add_systems(
            OnEnter(AppState::Playing),
            (teardown_field, reset_run, reset_retreat, spawn_field).chain(),
        )
        .add_systems(OnEnter(AppState::Menu), teardown_field)
        .add_systems(OnExit(AppState::Playing), clear_bars)
        // The screens. Each is built on the way in and despawned on the way
        // out by its own `DespawnOnExit`, and reads its buttons only while it
        // is up.
        .add_systems(OnEnter(AppState::Menu), build_menu)
        .add_systems(OnEnter(AppState::Setup), build_setup)
        .add_systems(OnEnter(AppState::Result), build_result)
        .add_systems(OnEnter(AppState::Map), build_map)
        .add_systems(OnEnter(AppState::Result), script_onward)
        .add_systems(
            Update,
            (
                menu_input.run_if(in_state(AppState::Menu)),
                setup_input.run_if(in_state(AppState::Setup)),
                result_input.run_if(in_state(AppState::Result)),
                map_input.run_if(in_state(AppState::Map)),
                hover_buttons.run_if(not(in_state(AppState::Playing))),
            ),
        )
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
                nav_input.run_if(in_state(AppState::Playing)),
                (
                    advance_tick,
                    (
                        apply_nav_to,
                        // The yard: it is opened on the flagship, then
                        // ordered, then worked, so a job ordered this frame
                        // is advanced this frame rather than next.
                        (open_yards, call_reinforcements, script_build, work_yards).chain(),
                        fly_hull,
                        tumble,
                        publish_hull,
                    )
                        .chain(),
                    move_hives,
                    (fire_guns, fire_flak).chain(),
                    (launch_fighters, fly_fighters, fighters_fire, wear_fighters).chain(),
                    resolve_beams,
                    (
                        chew,
                        vent_smoke,
                        (script_wreck, go_critical).chain(),
                        bleed_hives,
                    ),
                    // The retreat: what the support ships are doing, what
                    // the tanker has refined, what the tide has brought in,
                    // and the drive, which is the only way out.
                    (
                        size_holds,
                        apply_scars,
                        price_jump,
                        haul_cargo,
                        tend_repairs,
                        script_jobs,
                        script_jump,
                        work_jobs,
                        // After the cutting rather than before it, so what a
                        // salvager took off this frame is in the run before
                        // anything can take the piece away.
                        record_salvage,
                        refine,
                        tide_carriers,
                        jump_spool,
                    )
                        .chain(),
                    judge,
                    (publish_hives, publish_field).chain(),
                    (fly_tracers, land_torpedoes).chain(),
                    age_fx,
                    fly_chunks,
                    drift_wrecks,
                    spin_showcase,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing).and(running)),
                // And everything that only DRAWS keeps running, or a paused
                // frame would show the last thing that was built rather than
                // the world as it stands: the beams, the nav disc and the
                // flames are all rebuilt every frame from state, so skipping
                // them empties their meshes and the picture goes blank behind
                // the menu.
                (
                    draw_beams,
                    draw_nav,
                    draw_flames,
                    // Beside the flame it belongs to, and in the DRAWING half
                    // for the same reason: a plume light is the flame's own
                    // fact and a paused frame still shows the flame.
                    light_plumes,
                    fade_flashes,
                    glow_engines,
                    aim_turrets,
                    hold_guard,
                    // An outline is what is SELECTED, which is a fact about
                    // the picture rather than about the world, so it keeps
                    // running with the paused frame like everything else here.
                    light_outline,
                ),
                remesh_dirty,
                (orbit_camera, ride_the_eye),
            )
                .chain(),
        )
        .run();
}
