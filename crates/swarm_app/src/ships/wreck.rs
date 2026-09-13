//! A reactor going up, and what is left: big pieces, dust and freed turrets.

use crate::*;

#[derive(Component)]
pub(crate) struct Debris {
    pub(crate) vel: Vec3,
    pub(crate) born: u32,
}

/// A piece of a ship that died: a hull section, or a gun that came off whole.
///
/// It drifts and tumbles about its OWN middle. A section's cells are laid out
/// about the hull's origin, which is not the section's centre, so the tumble
/// is kept as a pivot and a rotation and the entity is placed from those every
/// frame: rotating the entity about its origin would swing an off centre
/// piece round in an arc.
#[derive(Component)]
pub(crate) struct Wreck {
    /// Where the piece's middle is, in the world.
    pub(crate) pivot: Vec3,
    /// Where that middle is in the piece's own frame.
    pub(crate) centroid: Vec3,
    pub(crate) vel: Vec3,
    /// Axis times rate, in radians a second.
    pub(crate) spin: Vec3,
    pub(crate) born: u32,
}

/// What a reactor takes of its own hull, in hull radii: the hole in the
/// middle of the wreck. Well under the radius, or there is no wreck: at
/// 0.38 the hole was forty percent of a frigate's cells and the pieces were
/// stubs, at 0.3 it is about a fifth and the pieces are the ship in quarters.
pub(crate) const HULL_HOLE: f32 = 0.3;

/// Fewer cells than this is dust, not a piece.
pub(crate) const WRECK_MIN: usize = 30;

/// How fast a piece leaves the blast, in hull radii a second. Slow: a hull
/// section is heavy, and the dust that flies past it is what says so.
pub(crate) const WRECK_SPEED: f32 = 0.35;

/// How fast a piece tumbles, in radians a second, least and most.
pub(crate) const WRECK_SPIN: (f32, f32) = (0.25, 0.6);

/// How long a piece drifts before it is taken away, in ticks: a minute. Not
/// for ever, because ten carriers' wrecks are ten thousand bricks of mesh.
pub(crate) const WRECK_TICKS: u32 = 3600;

/// The wreck drifts. A piece keeps the way it was thrown and turns about its
/// own middle, and after a minute it is gone unless somebody is working it.
///
/// A piece ages out because ten carriers' wrecks are ten thousand bricks of
/// mesh, and that is a cost argument rather than a rule about wrecks. A
/// piece a salvager is standing on is one a player has DECIDED to keep, and
/// taking it away mid cut is the harness deleting the thing under test:
/// measured, two salvagers strip about thirty cells a second off a wreck,
/// and half a frigate is four and a half thousand of them. A minute is not
/// enough for any tier of a rebuild, and nothing in the picture would have
/// said why.
pub(crate) fn drift_wrecks(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    jobs: Query<&Job>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Wreck)>,
) {
    let dt = scene.step(&time);
    for (e, mut xf, mut w) in &mut q {
        let step = w.vel * dt;
        w.pivot += step;
        xf.rotation = (Quat::from_scaled_axis(w.spin * dt) * xf.rotation).normalize();
        xf.translation = w.pivot - xf.rotation * (w.centroid * xf.scale);
        if tick.tick.saturating_sub(w.born) <= WRECK_TICKS {
            continue;
        }
        if jobs.iter().any(|j| matches!(j, Job::Work(t) if *t == e)) {
            continue;
        }
        commands.entity(e).despawn();
    }
}

pub(crate) fn fly_chunks(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    tick: Res<Tick>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &Debris)>,
) {
    let dt = scene.step(&time);
    for (e, mut xf, d) in &mut q {
        xf.translation += d.vel * dt;
        xf.rotate_local_x(dt * 3.0);
        if tick.tick.saturating_sub(d.born) > 240 {
            commands.entity(e).despawn();
        }
    }
}

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
pub(crate) const REACTOR_LOSS: f32 = 0.50;

/// An escort of the fleet's, which is a hull that is neither a support ship
/// nor a carrier. The filter IS the interface, as every query here.
pub(crate) type LiveEscort = (With<Escort>, Without<Support>, Without<Hive>);

/// Take the reactor out of ONE escort, for a harness that needs a wreck.
///
/// The headless equivalent of the one thing a player cannot script: a ship
/// dying. `--explode` takes every hull at once, which is a picture of a
/// fireball and no fleet left to salvage with, so this kills one escort's
/// reactor cells and lets `go_critical` take the ship on its OWN rule the
/// tick after. One death path, not two: the wreck a salvager works is the
/// wreck the swarm would have made.
pub(crate) fn script_wreck(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    mut hulls: Query<&mut Hull, LiveEscort>,
    mut fired: Local<bool>,
) {
    if scene.wreck == 0 || !tick.cue(Some(scene.wreck), &mut fired) {
        return;
    }
    let Some(mut hull) = hulls.iter_mut().find(|h| !h.dead_hull) else {
        return;
    };
    let hull = &mut *hull;
    let core = hull.reactor.clone();
    for c in core {
        hull.damage.kill(c, tick.tick);
    }
    info!(
        "the harness took an escort's reactor out at tick {}",
        tick.tick
    );
}

pub(crate) fn go_critical(
    tick: Res<Tick>,
    scene: Res<SceneSpec>,
    mut hulls: Query<(Entity, &mut Hull, &Transform)>,
    mut fx: ResMut<LiveFx>,
    mut sparks: ResMut<SparkQueue>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_mats: ResMut<ChunkMaterials>,
    turrets: Query<(Entity, &GlobalTransform, &ChildOf), With<Turret>>,
    hives: Query<(), With<Hive>>,
    mut outcome: ResMut<Outcome>,
) {
    for (entity, mut hull, xf) in &mut hulls {
        let hull = &mut *hull;
        if hull.dead_hull || hull.invulnerable {
            continue;
        }
        // Only the reactor counts. A hull can be shot to pieces everywhere
        // else and keep flying, which is what makes a wreck that is still
        // fighting possible at all.
        let core = hull.reactor.len();
        let gone = hull
            .reactor
            .iter()
            .filter(|&&c| hull.damage.is_dead(c))
            .count();
        let share = if core == 0 {
            0.0
        } else {
            gone as f32 / core as f32
        };
        let forced = scene.explode > 0 && tick.tick >= scene.explode;
        if !forced && share < REACTOR_LOSS {
            continue;
        }
        hull.dead_hull = true;
        // The verdict's ledger: a player hull gone, and every cell it had
        // lost by then, because the wreck it becomes is not asked again.
        if !hives.contains(entity) {
            outcome.ships_lost += 1;
            outcome.cells_lost += hull.damage.dead_count();
        }

        let radius = hull.model.radius();
        let centre = xf.translation;
        let blast = Blast {
            at: centre.to_array(),
            radius: radius * 1.8,
            born: tick.tick,
        };
        // What it does to the HULL is a much smaller sphere than what it does
        // to the swarm: a reactor takes the ball around it, and the pressure
        // wave goes a long way further than the hole does.
        let hull_blast = Blast {
            at: [0.0; 3],
            radius: radius * HULL_HOLE,
            born: tick.tick,
        };
        info!(
            "hull went critical at tick {} ({:.0}% of its {} reactor cells gone{}): blast radius {:.2}",
            tick.tick,
            share * 100.0,
            core,
            if forced { ", forced" } else { "" },
            blast.radius
        );

        // ---- the hull: a hole where the reactor was, and the rest in PIECES ----
        //
        // It used to take a sphere of one and a half radii, which on any hull
        // is the whole ship: the picture after a reactor went was a spray of
        // single cells and three turrets hanging in space where a frigate had
        // been. A ship that dies leaves a WRECK. The reactor takes the ball
        // around it, and what is left is broken along two planes through the
        // blast into big pieces (`fx::shatter`), each of which is a hull of
        // its own from here on: the same cells, the same damage, the same
        // materials, meshed once with its cut faces white hot and cooling on
        // the same ramp as any wound, tumbling away from the blast. The guns
        // come off whole, because a turret is a piece too.
        let started = Instant::now();
        let mut breaches = hull.damage.blast_cells(&hull.model, &hull_blast, tick.tick);
        let broken = shatter(
            &hull.model,
            &hull.damage,
            [0.0; 3],
            hull.seed ^ tick.tick,
            WRECK_MIN,
        );
        // Whatever is too small to be a piece is dust, and dust is thrown.
        for &c in &broken.dust {
            if let Some(b) = hull.damage.kill(c, tick.tick) {
                breaches.push(b);
            }
        }
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
                    materials.add(StandardMaterial {
                        base_color: Color::srgb(r, g, bl),
                        perceptual_roughness: 0.8,
                        ..default()
                    })
                })
                .clone();
            let origin = xf.transform_point(Vec3::from(ch.origin));
            let away = (origin - centre).normalize_or_zero();
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(origin),
                Debris {
                    vel: Vec3::from(ch.velocity) + away * radius * 1.6,
                    born: ch.born,
                },
            ));
        }

        // The pieces. Each is a hull: a copy of this one's model and damage
        // with every live cell that is not in the piece killed at this tick,
        // so its cut faces are wounds and burn like any other, meshed once
        // over only the bricks the piece touches. Dead from birth, so nothing
        // that skips a dead hull (the swarm, the guns, the bars, the orders)
        // ever looks at it, and `remesh_dirty` cools its burns exactly as it
        // cools a ship's.
        let mut sizes = Vec::new();
        for (i, piece) in broken.pieces.iter().enumerate() {
            let mut member = vec![false; hull.model.len()];
            for &c in piece {
                member[c] = true;
            }
            let mut damage = hull.damage.clone();
            for c in 0..hull.model.len() {
                if !member[c] && hull.model.grid[c] != mat::EMPTY && !damage.is_dead(c) {
                    damage.kill(c, tick.tick);
                }
            }
            damage.take_dirty();
            let centroid = piece
                .iter()
                .map(|&c| Vec3::from(hull.model.centre_of(c)))
                .sum::<Vec3>()
                / piece.len() as f32;
            let mut wreck = Hull {
                // A piece remembers what ship it came off, which is what a
                // salvager is working toward: you cannot rebuild a frigate
                // out of wreckage that has forgotten it was one.
                class: hull.class.clone(),
                model: hull.model.clone(),
                bricks: (0..damage.brick_count())
                    .map(|_| Brick::default())
                    .collect(),
                damage,
                surface_mats: hull.surface_mats.clone(),
                window_mats: hull.window_mats.clone(),
                inner_mat: hull.inner_mat.clone(),
                wound_mat: hull.wound_mat.clone(),
                scorch_mat: hull.scorch_mat.clone(),
                chewers: Vec::new(),
                guns: Vec::new(),
                engines: Vec::new(),
                order: None,
                vel: Vec3::ZERO,
                accel: Vec3::ZERO,
                breaches: 0,
                last_heat_key: 0,
                cells: piece.len(),
                dead_hull: true,
                reactor: Vec::new(),
                seed: hull.seed ^ (i as u32 + 1),
                invulnerable: false,
            };
            let wreck_e = commands.spawn((*xf, Visibility::default())).id();
            let mut touched = vec![false; wreck.damage.brick_count()];
            for &c in piece {
                touched[wreck.damage.brick_of(c)] = true;
            }
            for b in 0..wreck.damage.brick_count() {
                if !touched[b] {
                    continue;
                }
                let (lo, hi) = wreck.damage.brick_bounds(b);
                let s = mesh_region(&wreck.model, Some((&wreck.damage, tick.tick)), lo, hi);
                place_brick(&mut commands, &mut meshes, &mut wreck, b, &s, wreck_e);
            }
            let pivot = xf.transform_point(centroid);
            let away = (pivot - centre).normalize_or(Vec3::Y);
            let mut rng =
                Rng::new(((hull.seed as u64) << 16) ^ (i as u64) ^ ((tick.tick as u64) << 32));
            let axis = Vec3::new(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            )
            .normalize_or(Vec3::X);
            let jitter = Vec3::new(
                rng.range(-0.08, 0.08),
                rng.range(-0.08, 0.08),
                rng.range(-0.08, 0.08),
            );
            sizes.push(piece.len());
            // What it came off, for the salvager. Only a ship of the
            // fleet's: a carrier has no class and cannot be rebuilt, which
            // is the rule falling out of the data rather than being written.
            if let Some(class) = hull.class.clone() {
                commands.entity(wreck_e).insert(Remains {
                    ship: hull.seed,
                    class,
                    whole: hull.cells as u32,
                    got: 0,
                    banked: 0,
                });
            }
            commands.entity(wreck_e).insert((
                wreck,
                Wreck {
                    pivot,
                    centroid,
                    vel: hull.vel + (away * WRECK_SPEED + jitter) * radius,
                    spin: axis * rng.range(WRECK_SPIN.0, WRECK_SPIN.1),
                    born: tick.tick,
                },
            ));
        }

        // The guns come off whole. A turret is its own child with its own
        // mesh, so it is cut loose from the hull, put where it was, and thrown
        // a little harder than a section: it is lighter.
        let mut guns = 0u32;
        for (t, gxf, child_of) in &turrets {
            if child_of.parent() != entity {
                continue;
            }
            let (scale, rotation, translation) = gxf.to_scale_rotation_translation();
            let away = (translation - centre).normalize_or(Vec3::Y);
            let mut rng = Rng::new(
                ((hull.seed as u64) << 16) ^ (0x77 + guns as u64) ^ ((tick.tick as u64) << 32),
            );
            let axis = Vec3::new(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            )
            .normalize_or(Vec3::X);
            commands
                .entity(t)
                .remove::<ChildOf>()
                .remove::<Turret>()
                .insert((
                    Transform {
                        translation,
                        rotation,
                        scale,
                    },
                    Wreck {
                        pivot: translation,
                        centroid: Vec3::ZERO,
                        vel: hull.vel + away * radius * WRECK_SPEED * 1.6,
                        spin: axis * rng.range(WRECK_SPIN.1, WRECK_SPIN.1 * 2.5),
                        born: tick.tick,
                    },
                ));
            guns += 1;
        }
        info!(
            "the wreck is {} pieces of {:?} cells, {guns} guns and {} dust, built in {:.1} ms",
            sizes.len(),
            sizes,
            broken.dust.len(),
            started.elapsed().as_secs_f32() * 1000.0
        );

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
        blast_sparks(
            tick.tick ^ 0xF1A5,
            centre.to_array(),
            blast.radius * 0.22,
            26,
            &mut flash,
        );
        for f in &mut flash {
            f.size *= 5.0;
            f.life = 0.10 + f.life * 0.06;
            f.colour = [9.0, 6.0, 3.2];
            f.kind = SparkKind::Blast;
        }
        sparks.extend(flash);
        // And it LIGHTS what is standing round it. Fourteen hundred additive
        // sparks are a great many bright pixels and not one lumen: a reactor
        // going off a length from a carrier left that carrier exactly the
        // colour it was on the frame before, which is a fireball painted on
        // the picture rather than one happening in it.
        flash_light(
            &mut commands,
            centre,
            blast.radius,
            REACTOR_LUMENS,
            tick.tick,
        );
        fx.blasts.push(blast);
        // And the hull that was is gone. Every cell of it is in a piece, in
        // the dust or in the fireball now, and its bricks go with it.
        commands.entity(entity).despawn();
    }
}
