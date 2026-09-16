//! The command line: what a run can be asked for, and how that is read.
//!
//! Its own file because `main.rs` is the arguments, the `App` and the
//! schedule, and those are three things: the merge that brought main into
//! this branch put the file over this project's own nine hundred lines
//! (876 at the base, 895 on main, 892 here, 913 together), and the limit is
//! the work rather than a number to raise. The arguments are the third of
//! it that answers to nothing else in the file.

use crate::*;

pub(crate) struct Args {
    pub(crate) headless: bool,
    pub(crate) motes: u32,
    pub(crate) hull: String,
    pub(crate) out: String,
    pub(crate) frames: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) chewers: usize,
    /// How many reinforcements are already inbound when the app starts, so a
    /// headless run can photograph a wave flying in.
    pub(crate) reinforce: u32,
    /// How many asteroids to strew between the ship and the carriers.
    pub(crate) rocks: usize,
    /// How many fighters the ship puts up.
    pub(crate) fighters: u32,
    /// How long the carriers sit before the first mote comes out. Ten by
    /// default, and nought is what a headless render wants: a shot aimed at
    /// tick ninety cannot wait ten seconds for the swarm to exist.
    pub(crate) launch_delay: f32,
    /// Where the camera sits, in radians. Exposed so a headless run can take
    /// the SAME tick from two different angles and compare them: "it looks
    /// wrong at some angles" is a claim about the projection, and the only way
    /// to answer it is to hold everything else still and turn the camera.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    /// Ride a collector with the camera, since a craft that small crossing a
    /// field that wide cannot be named by a coordinate.
    pub(crate) watch_craft: bool,
    /// Draw the HUD in a headless run. It is a window's furniture and there is
    /// nobody to press it, but a screenshot is the only way to PROVE it draws
    /// rather than assert it, which is the rule the textures already keep.
    pub(crate) hud: bool,
    /// Start paused, with the menu open, for the same reason.
    pub(crate) paused: bool,
    /// Whether `--launch-delay` was actually given. A headless run wants the
    /// swarm to exist on frame one: it is a HARNESS, and a harness that waits
    /// ten seconds for its subject to appear is ten seconds of every check
    /// spent rendering an empty sky. So headless defaults the delay to nought
    /// and a window defaults it to ten, and this is how "the user asked for
    /// nought" is told apart from "nobody said".
    pub(crate) delay_set: bool,
    /// Camera distance in hull radii.
    pub(crate) zoom: f32,
    /// What the camera looks at, in world units. The hull's centre unless
    /// asked otherwise: the showcase aliens sit below and ahead of it.
    pub(crate) target: Vec3,
    /// Force the reactor at this tick. Nought leaves it to the hull's own
    /// state, which goes critical once enough of it is gone.
    pub(crate) explode: u32,
    pub(crate) wreck: u32,
    /// `--view mission|sensors|menu` opens one of the middle views at start,
    /// since a headless run has no pointer to press its tab with. The sensors
    /// manager is a camera MODE, so this is the only way to photograph what
    /// the eye does when it is asked for.
    pub(crate) view: String,
    /// `--panel build|research|launch` opens the side panel on one of its
    /// tabs at start, since a headless run has no pointer to press one with.
    /// Same rule as `--view`: a flag per thing a player presses, because a
    /// tab whose body nothing can photograph is a tab nobody can check.
    pub(crate) panel: String,
    /// `--build WHAT,TICK`: press one build row at that tick, so a hull coming
    /// off the queue can be photographed. WHAT is a class key or `fighter`.
    pub(crate) build: u32,
    pub(crate) build_what: String,
    /// Ticks between one gun firing and the next. Nought silences them.
    pub(crate) cadence: u32,
    /// How many motherships the swarm flies from.
    pub(crate) hives: usize,
    /// Issue one move order at startup, so a headless render can show the
    /// ship under way with its standing order drawn.
    pub(crate) order: Option<Vec3>,
    /// Open the move disc at startup, aimed at this point, so a headless
    /// render can show an order being GIVEN: the disc, the triangle, the
    /// label.
    pub(crate) aim: Option<Vec3>,
    /// Draw the four archetypes in a row beside the hull. Off by default now
    /// that the carriers are the aliens on show.
    pub(crate) showcase: bool,
    /// Frames a second, at most. Nought lifts the cap.
    pub(crate) fps: u32,
    /// How thick the swarm is to light, as a multiplier on how much of a
    /// cell one mote blocks. Nought is the flat lighting this replaced, which
    /// is what an A/B of the shading is taken against.
    pub(crate) thickness: f32,
    /// Advance exactly one tick a frame rather than by the wall clock.
    ///
    /// A software rasteriser draws at four frames a second, so a frame here
    /// is fourteen ticks and a screenshot cannot be aimed at one: the shot
    /// meant for the fireball arrives four hundred ticks after it went out.
    /// This is for the harness, and it is what makes a headless render a
    /// function of its frame count rather than of how fast the machine is.
    pub(crate) fixed_dt: bool,
    /// Which screen to open on: `menu`, `setup` or `result`. Implies
    /// `--hud`, because a screen is UI and UI needs the UI camera.
    pub(crate) screen: Option<String>,
    /// A playground rather than a fight: a target dummy, the range, the
    /// toggles, and no verdict.
    pub(crate) sandbox: bool,
    /// Straight into the fight from a window, the way the harness always is.
    pub(crate) play: bool,
    pub(crate) target_hull: String,
    pub(crate) seed: u64,
    pub(crate) stand: f32,
    /// One scripted shot on the range: `--fire slug,40` lands a slug on the
    /// dummy at tick forty, so a tumble can be photographed.
    pub(crate) fire: Option<(Weapon, u32)>,
    /// `--blast TICK`: one sandbox blast down the camera's own line at that
    /// tick, so what it does to a hull and to the cloud can be photographed.
    /// There is no cursor in a headless run, and the blast is aimed with one.
    pub(crate) blast: Option<u32>,
    /// A system of The Long Retreat: gather, hold, and jump out before the
    /// swarm's fleet arrives.
    pub(crate) retreat: bool,
    /// `--base` opens the skirmish's base building mode, which is the siege:
    /// a station, the rocks and a tide that never stops.
    pub(crate) base: bool,
    /// The run's seed, which is the whole map.
    pub(crate) run_seed: u64,
    /// `--job TICK` puts every support ship to work at that tick, so a
    /// headless run can photograph a shaft being cut.
    pub(crate) job: Option<u32>,
    /// Which support ships the fleet arrives with, by role: `--support
    /// miner,miner,salvager`. Empty means the run's own fleet.
    pub(crate) support: Vec<Role>,
    /// `--jump TICK` has the drive ready at that tick, so the jump and the
    /// screen after it can be photographed without mining for the fuel.
    pub(crate) jump: Option<u32>,
    /// And `--onward` goes straight on into the next system rather than
    /// waiting on the map for a branch to be pressed.
    pub(crate) onward: bool,
}

pub(crate) fn parse_args() -> Args {
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
        watch_craft: false,
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
            "--watch-craft" => a.watch_craft = true,
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
