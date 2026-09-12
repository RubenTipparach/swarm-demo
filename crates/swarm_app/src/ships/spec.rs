//! Spawning a ship: the model, its materials, its bricks, its turrets, its
//! chewers and its reactor, from a class.

use crate::*;

/// Everything about a ship that is per SHIP rather than per class.
///
/// A struct rather than nine arguments, because the day a rock needed to be
/// a hull and a miner needed to be a ship that is not the flagship, the
/// argument list was already carrying an `allow(too_many_arguments)`, which
/// this file's own rules call the smell that says a struct is missing.
#[derive(Clone)]
pub(crate) struct ShipSpec {
    pub(crate) at: Transform,
    pub(crate) chewers: u32,
    pub(crate) seed: u32,
    /// A place in the flagship's formation, or nothing for a ship that flies
    /// its own orders.
    pub(crate) station: Option<Vec3>,
    pub(crate) armour: f32,
    /// The class this came from, when it is one of the fleet's own. A hull
    /// with a class and no station is the flagship, which is why a carrier
    /// and a rock both leave it empty.
    pub(crate) class: Option<String>,
    /// Not a ship anybody flies: a rock, or anything else that is only a
    /// voxel model with a damage grid on it. It is born `dead_hull`, so
    /// everything that already skips a dead hull skips it too, which is the
    /// swarm's targets, the guns, the bars, the orders, the chewers and the
    /// reactor rule. That is most of the game for one flag.
    pub(crate) inert: bool,
}

impl ShipSpec {
    pub(crate) fn at(at: Transform) -> ShipSpec {
        ShipSpec {
            at,
            chewers: 0,
            seed: 0,
            station: None,
            armour: ARMOUR,
            class: None,
            inert: false,
        }
    }

    pub(crate) fn chewers(mut self, n: u32) -> ShipSpec {
        self.chewers = n;
        self
    }

    pub(crate) fn seed(mut self, seed: u32) -> ShipSpec {
        self.seed = seed;
        self
    }

    pub(crate) fn station(mut self, station: Vec3) -> ShipSpec {
        self.station = Some(station);
        self
    }

    pub(crate) fn armour(mut self, armour: f32) -> ShipSpec {
        self.armour = armour;
        self
    }

    pub(crate) fn inert(mut self) -> ShipSpec {
        self.inert = true;
        self
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
pub(crate) fn spawn_hull(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    which: &str,
    spec: ShipSpec,
) -> (Entity, f32) {
    let model = load_hull(which);
    let surf = surface_materials(&model, tex, materials);
    let win = window_materials(&model, tex, materials);
    let spec = ShipSpec {
        class: Some(which.into()),
        ..spec
    };
    spawn_ship(commands, meshes, materials, tex, model, surf, win, spec)
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
pub(crate) fn spawn_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &Textures,
    model: VoxelModel,
    surface_mats: Vec<Handle<StandardMaterial>>,
    window_mats: Vec<Handle<StandardMaterial>>,
    spec: ShipSpec,
) -> (Entity, f32) {
    let ShipSpec {
        at,
        chewers,
        seed,
        station,
        armour,
        class,
        inert,
    } = spec;
    let log = class.as_deref();
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
    let engines: Vec<Drive> = engine_clusters(&model)
        .into_iter()
        .map(|(gun, _, cells)| Drive { gun, cells })
        .collect();
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
        bricks: (0..damage.brick_count())
            .map(|_| Brick::default())
            .collect(),
        chewers: Vec::new(),
        guns,
        engines,
        order: None,
        vel: Vec3::ZERO,
        accel: Vec3::ZERO,
        breaches: 0,
        last_heat_key: 0,
        cells,
        dead_hull: inert,
        reactor: reactor_of(&model),
        seed,
        invulnerable: false,
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
        let mut only =
            VoxelModel::new(hull.model.nx, hull.model.ny, hull.model.nz, hull.model.cell);
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
                Turret {
                    slot,
                    rest: Vec3::from(gun.out).normalize_or(Vec3::Z),
                },
                ChildOf(hull_entity),
            ));
        }
    }

    // Chewers come in from every SIDE, spread over the sphere by the same
    // golden angle spiral the carriers stand on: whatever the count, no two of
    // them end up on the same bearing and none of the hull is out of reach.
    //
    // They used to start on a random exposed quad, which is not a random part
    // of a ship at all. See `Chewer`: it ate every hull from the stern
    // forward, because the stern is where the small quads are.
    if chewers > 0 {
        hull.chewers = (0..chewers)
            .map(|n| {
                let t = (n as f32 + 0.5) / chewers as f32;
                let y = 1.0 - 2.0 * t;
                let r = (1.0 - y * y).max(0.0).sqrt();
                let a = n as f32 * 2.399_963_2;
                Chewer {
                    from: Vec3::new(r * a.cos(), y, r * a.sin()),
                    next: (n * 7) % 40,
                }
            })
            .collect();
    }

    if let Some(which) = log {
        let used: Vec<String> = (0..SURF_COUNT)
            .filter(|&s| {
                hull.bricks
                    .iter()
                    .any(|b| b.skin.get(s).and_then(|p| p.entity).is_some())
            })
            .map(|s| {
                format!(
                    "{s}:{}",
                    hull.model
                        .surfaces
                        .get(s)
                        .map(|x| x.finish.as_str())
                        .unwrap_or("?")
                )
            })
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
