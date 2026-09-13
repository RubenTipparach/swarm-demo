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
        FrameLimit {
            budget: Duration::from_secs_f64(1.0 / fps as f64),
            next: Instant::now(),
        }
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
    /// `--wreck TICK` takes the reactor out of ONE escort at that tick, so a
    /// headless run has a wreck for a salvager to work. It kills the reactor
    /// cells rather than exploding the ship itself, so the death goes through
    /// the same `go_critical` rule every other one does.
    pub(crate) wreck: u32,
    /// `--build WHAT,TICK` presses one build row at that tick, since a
    /// headless run has no pointer. It goes through `order_one` exactly as the
    /// button does, so what is photographed is the mechanic and not a second
    /// path to it. WHAT is a class key or `fighter`, because a frigate is
    /// fifty seconds of queue and a fighter is two and a half: the cheap one
    /// is what lets a run photograph a hull ARRIVING rather than only a bar.
    pub(crate) build: u32,
    pub(crate) build_what: String,
    pub(crate) cadence: u32,
    pub(crate) hives: usize,
    pub(crate) order: Option<Vec3>,
    pub(crate) aim: Option<Vec3>,
    pub(crate) showcase: bool,
    pub(crate) thickness: f32,
    pub(crate) fixed_dt: bool,
    /// How many motes the swarm has. The render world builds its buffers
    /// from this on the first frame of a scene.
    pub(crate) motes: u32,
    /// A playground rather than a fight: a target dummy, the range, the
    /// toggles, and no verdict.
    pub(crate) sandbox: bool,
    /// A SIEGE rather than a battle scenario: a station you build from, rocks
    /// to mine, and carriers arriving on a clock that never stops. The two
    /// skirmish modes are the same field with two things to do on it, which
    /// is why this is a flag on the scene rather than a second scene: the
    /// swarm, the rocks, the yard and the tide are one set of machinery and
    /// what changes is which of them the mode turns on.
    pub(crate) base: bool,
    /// One for real time, a quarter for slow motion. Read by the CPU's step
    /// and copied to the swarm's clock, so both halves slow together.
    pub(crate) time_scale: f32,
    /// What the rocks are laid out from.
    pub(crate) seed: u64,
    /// A multiplier on where the carriers stand: close, normal, far.
    pub(crate) stand: f32,
    /// The class the sandbox's target dummy is.
    pub(crate) target_hull: String,
    /// A system of The Long Retreat rather than a skirmish: the carriers
    /// arrive on a tide, the rocks are worth cutting, and the way out is the
    /// jump drive rather than killing everything.
    pub(crate) retreat: bool,
    pub(crate) tide: Tide,
    /// Which support ships arrived with the fleet.
    pub(crate) support: Vec<Role>,
    /// Which flavour this field's rocks lean toward. Both are always there:
    /// a system with no ice is a system that strands a run.
    pub(crate) lean: Flavour,
}

/// Sixty a second, accumulated from wall time and clamped, so the chewers eat
/// at one rate whatever the frame rate is doing.
#[derive(Resource, Default)]
pub(crate) struct Tick {
    pub(crate) tick: u32,
    pub(crate) acc: f32,
}

impl Tick {
    /// Whether a scripted moment has arrived, once.
    ///
    /// The harness's own clock rule, in one place, because it has already
    /// been got wrong twice by being written out. `== tick.tick` is right
    /// only under `--fixed-dt`, where one frame is one tick; on a real clock
    /// the tick jumps by whatever the frame took, so a mark can fall in a
    /// gap between two frames and the thing it was meant to trigger simply
    /// never happens, with nothing in the log to say why. At or past the
    /// mark, and the caller's `fired` is what makes it once: a `Local<bool>`
    /// on the system, so nothing global remembers a scene that has been torn
    /// down.
    ///
    /// This is the one clock lesson a third time: a rule copied is a rule
    /// one copy will miss, and the copy that is missing is the one nobody
    /// can grep for.
    pub(crate) fn cue(&self, at: Option<u32>, fired: &mut bool) -> bool {
        if *fired || at.is_none_or(|mark| self.tick < mark) {
            return false;
        }
        *fired = true;
        true
    }
}

impl SceneSpec {
    /// Whether the swarm arrives on a CLOCK rather than all at once, and
    /// therefore whether killing every carrier can be a victory.
    ///
    /// A retreat and the base are the same siege: probes, then the swarm,
    /// then the fleet, and one more carrier for ever after that. What differs
    /// is the way out, and only one of them has one. Written once because
    /// three systems ask it (what the field opens with, what the tide spawns,
    /// and what `judge` calls a win) and a copy in each is the copy one of
    /// them would have differently.
    pub(crate) fn sieged(&self) -> bool {
        self.retreat || self.base
    }

    /// Turn the base building mode ON, which is everything a siege needs that
    /// a battle scenario does not.
    ///
    /// One implementation and two callers (the setup form's Launch and the
    /// `--base` flag), because a mode set up one way on the form and another
    /// way on the command line is a mode the harness cannot photograph.
    ///
    /// Three things, and each is the reason one of the other modes exists.
    /// The carriers arrive on the TIDE, so the motherships row stops meaning
    /// "what stands there at tick nought" and starts meaning what the fleet
    /// phase brings. There are ROCKS, because a base with nothing to mine is
    /// a build menu with an empty bank behind it. And the opening pair of
    /// support ships is a miner and a tanker, which is the run's own, because
    /// what fills the bank is a cutter and what turns crystal into fuel is a
    /// tanker and neither is any use alone.
    pub(crate) fn open_base(&mut self) {
        self.tide = Tide {
            carriers: self.hives.max(1),
            ..Tide::default()
        };
        self.rocks = self.rocks.max(BASE_ROCKS);
        self.support = vec![Role::Miner, Role::Tanker];
    }

    /// One frame's worth of time for everything on the CPU that integrates
    /// anything: a sixtieth under `--fixed-dt`, so a headless render is a
    /// function of its frame count and not of the machine, and the frame's
    /// own delta otherwise, clamped at the swarm's clamp because two clamps
    /// are two clocks. Every system asks this; none writes the rule itself.
    pub(crate) fn step<T: Default>(&self, time: &Time<T>) -> f32 {
        let dt = if self.fixed_dt {
            1.0 / 60.0
        } else {
            time.delta_secs().min(swarm::STEP_CLAMP)
        };
        dt * self.time_scale
    }
}

pub(crate) fn advance_tick(time: Res<Time>, scene: Res<SceneSpec>, mut t: ResMut<Tick>) {
    // Under a fixed step this adds exactly a sixtieth and takes exactly one
    // off, so it is one tick a frame with nothing accumulating.
    t.acc += scene.step(&time);
    while t.acc >= 1.0 / 60.0 {
        t.acc -= 1.0 / 60.0;
        t.tick += 1;
    }
}

/// The fight, from the scene: the flagship and its wave, the showcase, the
/// carriers, the rocks, what the swarm is told, and where the camera looks.
/// Runs on entering `Playing`, after the last field has been torn down.
pub(crate) fn spawn_field(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cfg: ResMut<SwarmConfig>,
    scene: Res<SceneSpec>,
    tex: Res<Textures>,
    mut cams: Query<(&mut Orbit, &mut Transform), With<Camera3d>>,
) {
    let radius = spawn_fleet(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene,
        &mut cfg,
    );
    spawn_showcase(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene,
        radius,
    );
    spawn_hives(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene,
        radius,
    );
    spawn_rocks(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene,
        &mut cfg,
        radius,
    );
    spawn_support(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &scene,
        radius,
    );
    if scene.sandbox {
        spawn_dummy(
            &mut commands,
            &mut meshes,
            &mut materials,
            &tex,
            &scene,
            radius,
        );
    }
    brief_swarm(&mut cfg, &scene);
    aim_camera(&mut cams, &scene, radius);
}

/// The flagship, its order and its wave. Returns the flagship's radius, which
/// everything else in the field is laid out against.
fn spawn_fleet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    cfg: &mut SwarmConfig,
) -> f32 {
    // ---- the flagship ----
    let (lead, radius) = spawn_hull(
        commands,
        meshes,
        materials,
        tex,
        &scene.hull,
        ShipSpec::at(Transform::IDENTITY).chewers(scene.chewers as u32),
    );
    cfg.hull_radius = radius;
    if let Some(order) = scene.order {
        commands.entity(lead).insert(NavTo(order));
    }

    // A wave already on its way in, so a headless run can photograph one.
    for n in 0..scene.reinforce {
        call_one(
            commands,
            meshes,
            materials,
            tex,
            &scene.hull,
            Wave {
                radius,
                lead_pos: Vec3::ZERO,
                lead_rot: Quat::IDENTITY,
                chewers: (scene.chewers / 3) as u32,
                n,
                shape: Shape::default(),
            },
        );
    }

    radius
}

/// One of each archetype, big, in chitin, when the scene asks for them.
fn spawn_showcase(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    radius: f32,
) {
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
                Transform::from_xyz(-4.5 + n as f32 * 3.0, -radius * 0.75, radius * 1.15)
                    .with_scale(Vec3::splat(scale)),
                Showcase,
            ));
        }
    }
}

/// The carriers, on a spiral round the flagship at the stand the scene says.
fn spawn_hives(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    radius: f32,
) {
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
    // A retreat's carriers arrive on the tide rather than all at once, so
    // the field starts with whatever the first tick of it calls for.
    let now = if scene.sieged() {
        scene.tide.carriers_at(0)
    } else {
        scene.hives
    };
    for n in 0..now {
        spawn_hive(
            commands,
            meshes,
            materials,
            tex,
            HiveSeat {
                n,
                of: scene.hives.max(1),
                radius,
                stand: scene.stand,
            },
        );
    }
    // WITH the stand in it. It was `radius * HIVE_NEAR` and `radius *
    // HIVE_FAR`, which is where the carriers stand at a stand of one and
    // nowhere else, so the line printed the same two numbers whatever
    // `--stand` was set to and a scene with the carriers brought right in
    // reported them at their usual distance. A log that does not move when
    // the setting moves is worse than no log: it is the thing you check the
    // setting against.
    info!(
        "{} motherships at {:.1} to {:.1} units, radius {:.2} each",
        scene.hives,
        radius * HIVE_NEAR * scene.stand,
        radius * HIVE_FAR * scene.stand,
        want
    );
}

/// Which carrier of how many, and the two numbers that decide where one
/// stands. A struct rather than four more arguments, for the reason this
/// project states everywhere it says `too_many_arguments` is a smell.
pub(crate) struct HiveSeat {
    /// Which of the spiral's places this one takes, and how many places
    /// there are: a tide's carriers arrive one at a time and each has to
    /// land on its own place rather than on the first.
    pub(crate) n: usize,
    pub(crate) of: usize,
    /// The ship it is besieging, which is what everything about a carrier is
    /// measured in: how big it is and how far off it stands.
    pub(crate) radius: f32,
    pub(crate) stand: f32,
}

/// One carrier, seated on the golden angle spiral round the ship it is
/// besieging.
///
/// Factored out of the field the day the tide arrived: a carrier that could
/// only be made while a system was being built is a carrier that cannot
/// arrive two minutes into one.
pub(crate) fn spawn_hive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    seat: HiveSeat,
) {
    let HiveSeat {
        n,
        of,
        radius,
        stand,
    } = seat;
    let want = radius * HIVE_RADIUS;
    let m = generate(Archetype::Mother, 900 + n as u64);
    let scale = want / m.radius();
    // A golden angle spiral over the sphere, which spreads n points
    // evenly without any two ending up in the same place, whatever n is.
    let t = (n as f32 + 0.5) / of as f32;
    let y = 1.0 - 2.0 * t;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let a = n as f32 * 2.399_963_2;
    let dir = Vec3::new(r * a.cos(), y * 0.45, r * a.sin()).normalize();
    let at = dir * radius * stand * (HIVE_NEAR + (HIVE_FAR - HIVE_NEAR) * ((n % 3) as f32 / 2.0));
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
    let engines: Vec<Drive> = engine_clusters(&m)
        .into_iter()
        .map(|(gun, _, cells)| Drive { gun, cells })
        .collect();
    let at_xf = Transform::from_translation(at).with_scale(Vec3::splat(scale));
    // No armour multiplier: the hundred is what makes the PLAYER's ship a
    // siege, and putting it on the carriers too would mean neither side
    // could hurt the other and the battle would never resolve.
    let (e, _) = spawn_ship(
        commands,
        meshes,
        materials,
        tex,
        m,
        surf,
        Vec::new(),
        ShipSpec::at(at_xf)
            .seed(0x5EED ^ (n as u32).wrapping_mul(0x9E37))
            .armour(HIVE_ARMOUR),
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

/// The asteroid field, and the spheres the swarm is told about.
#[allow(clippy::too_many_arguments)]
fn spawn_rocks(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    scene: &SceneSpec,
    cfg: &mut SwarmConfig,
    radius: f32,
) {
    // ---- the asteroid field ----
    //
    // Drawn as voxel lumps and NAVIGATED as spheres, and the gap between
    // those two is deliberate. A mote pays for every rock every tick, so the
    // navigation shape has to have a closed form; the sphere is inscribed
    // rather than circumscribed and the margin in the shader stands the swarm
    // off outside it, so what a player sees is a cloud flowing round a rock
    // and not a cloud flowing round an invisible ball.
    let mut rocks: Vec<Vec4> = Vec::new();
    let mut rng = Rng::new(scene.seed);
    for n in 0..scene.rocks.min(swarm::MAX_ROCKS) {
        // A skirmish's rocks lean to ore, which is what they have always
        // been; a retreat's field leans the way its node says. Either way
        // both seams are in every rock, because the crystal grows on the
        // ore: the lean is how much of it there is, never whether.
        let flavour = if scene.retreat {
            flavour_at(n, scene.lean)
        } else {
            Flavour::Ore
        };
        let m = rock::generate_of(
            ROCK_LATTICE,
            radius * ROCK_CELL,
            scene.seed.wrapping_add(700 + n as u64),
            flavour,
        );
        let rr = m.volume_radius();
        let count = |what: u8| (0..m.len()).filter(|&c| m.grid[c] == what).count() as u32;
        let (ore, crystal) = (count(swarm_core::mat::ACCENT), count(swarm_core::mat::GLOW));
        // Strewn between the ship and the carriers, off the plane, so they
        // are cover on the way out rather than scenery at the edge.
        let t = (n as f32 + 0.5) / scene.rocks.max(1) as f32;
        let a = n as f32 * 2.399_963_2;
        let out = radius * (ROCK_NEAR + (ROCK_FAR - ROCK_NEAR) * t);
        let at = Vec3::new(
            a.cos() * out,
            (rng.range(-1.0, 1.0)) * radius * 2.6,
            a.sin() * out,
        );
        // One material per surface, so the seam keeps its own finish instead
        // of wearing the stone's, and through the SHIP builder, so the rock
        // has a damage grid and bricks and a miner has something to cut.
        let mut mats = surface_materials(&m, tex, materials);
        // And the crystal gets the one material in the game with depth in
        // it. Overridden here rather than described in the core, the same
        // way a carrier's drives are: what a surface is made OF is the
        // model's business and how it is drawn is the picture's.
        mats[rock::CRYSTAL_SURF as usize] = crystal_material(tex, materials);
        let xf = Transform::from_translation(at).with_rotation(Quat::from_euler(
            EulerRot::YXZ,
            rng.range(0.0, std::f32::consts::TAU),
            rng.range(0.0, std::f32::consts::TAU),
            rng.range(0.0, std::f32::consts::TAU),
        ));
        let (rock, _) = spawn_ship(
            commands,
            meshes,
            materials,
            tex,
            m,
            mats,
            Vec::new(),
            ShipSpec::at(xf).seed(0x0CE4 ^ n as u32).inert(),
        );
        commands.entity(rock).insert(Rock { ore, crystal });
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
}

/// What the swarm is told about this scene that is not a position: how many
/// motes, how thick they are against the light, when they launch, and that
/// this is a NEW swarm, so the render world rebuilds its buffers rather than
/// carrying the last fight's cloud into this one.
fn brief_swarm(cfg: &mut SwarmConfig, scene: &SceneSpec) {
    cfg.launch_delay = scene.launch_delay;
    cfg.count = scene.motes;
    cfg.generation += 1;
    // The swarm's body: a drone, drawn once per mote off the GPU buffer.
    let drone = generate(Archetype::Drone, 1);
    // And how much of a cell's face one of them covers, which is the only
    // place the thickness of the cloud is decided: it is what turns a count of
    // motes in a cell of the density field into an optical depth. A disc the
    // size of the drone's own silhouette, halved, because a drone is limbs and
    // the gaps between them rather than a ball.
    let area =
        std::f32::consts::PI * drone.radius() * drone.radius() * 0.5 * scene.thickness.max(0.0);
    cfg.sun = SUN.normalize().extend(area);
    info!(
        "a mote covers {:.3} square units of the light, thickness {:.2}",
        area, scene.thickness
    );
}

/// The camera, framed on the hull from ahead and above, OUTSIDE the swarm,
/// whose standoff reaches about two radii.
fn aim_camera(
    cams: &mut Query<(&mut Orbit, &mut Transform), With<Camera3d>>,
    scene: &SceneSpec,
    radius: f32,
) {
    let dist = radius * scene.zoom;
    // A sandbox looks between the flagship and the dummy.
    let target = if scene.sandbox && scene.target == Vec3::ZERO {
        dummy_station(radius) * 0.5
    } else {
        scene.target
    };
    for (mut o, mut xf) in cams.iter_mut() {
        *o = Orbit {
            yaw: scene.yaw,
            pitch: scene.pitch,
            dist,
            // Opens where the player's own zoom is: the sensors view is the
            // only thing that moves it, and it eases.
            eye: dist,
            target,
            follow: false,
        };
        let eye = target
            + Vec3::new(
                o.yaw.sin() * o.pitch.cos(),
                o.pitch.sin(),
                o.yaw.cos() * o.pitch.cos(),
            ) * dist;
        *xf = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    }
}

/// The fewest asteroids a base is opened with.
///
/// A base is held by what it can build and what it builds is paid for out of
/// the ground, so a field thinner than this is a mode whose economy runs out
/// before its first carrier arrives. Eight is the run's own quiet system.
pub(crate) const BASE_ROCKS: usize = 8;
