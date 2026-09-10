# swarm-demo

A real time space RTS against an alien swarm, in the voxel design language of
[redux-tribes](https://github.com/RubenTipparach/redux-tribes). Homeworld's
verbs (a nav disc, shift for elevation, ships you position) against an enemy
that is a cloud rather than a fleet: it chews armour off your hulls cell by
cell, and you beat it by where you put your ships, not by what you click on.

No em dashes or en dashes anywhere, the same rule redux-tribes keeps.

## The two crates and the line between them

`crates/swarm_core` is the game's own reasoning and depends on nothing but
`std`: the voxel lattice, the greedy mesher, the damage grid and the alien
generator. `crates/swarm_app` is the Bevy harness: it draws what the core says,
ticks the swarm on the GPU, and collects input. This is redux-tribes' boundary
between `sim_core` and `web`, kept for the same reasons. A rule with one
implementation cannot be changed in only one of two places, a crate with no
engine in it is tested without one, and Bevy is 0.x: when the render graph
moves again, or the engine is swapped, the game is untouched.

The test is the same one: if two clients computed this differently, would the
match diverge? Then it belongs in the core.

## The swarm is a field, not a million entities

Do the arithmetic before adding anything per mote. Sixteen milliseconds over a
million motes is sixteen nanoseconds each for the whole frame. So:

- **Motes are never entities.** They are one storage buffer in the render
  world, made once from a seed, advanced by a compute pass before the cameras
  draw, and bound as the instance buffer of one draw. Nothing about a mote comes
  back to the CPU. `swarm.rs` is the whole of it.
- **The ECS holds what there are dozens of**: hulls, chewers, chunks, cameras.
  Broods and hives, when they come, are entities too, because there are
  hundreds of them and not a million.
- **The authoritative swarm will be a coarse continuum on the CPU** (density
  and pressure on a 64^3 grid, deterministic, hashable) and the million motes
  its picture. That is what makes lockstep possible with a GPU swarm, and it
  is not built yet. What is built is M0: motes steer on a pull toward the
  hull, a swirl and stride sampled separation, with the hull as a sphere.

## Hulls are the redux-tribes hulls, one to one

`assets/hulls/*.ftvx` are the twenty three stock hulls, exported from
redux-tribes' own rasteriser AND its own mesher by `tools/export_hulls.mjs`,
read by `VoxelModel::from_ftvx`. Cells, materials, purposes, roles, colours,
which of the fifteen surfaces each cell draws in, what each surface is made
of (finish, metalness, roughness, as `hullMaterials` builds them), and every
window face with its direction and decal kind are that project's answers
written down. Nothing here interprets a design: the export runs `hullMesh`
under node and reads the windows off its meshes and the surfaces off its
groups, and it checks the per cell surface it writes against every quad
hull.ts grouped (8126 of 8126 on the Terran frigate, and every hull). They
are not mirrored and that is faithful (`assets/README.md` says why).

The mesher is `hull.ts` ported: a face wherever a solid cell meets one that is
not, INCLUDING the faces nothing outside can see, greedily merged into
rectangles, keyed on colour AND surface so a material never straddles a
quad. A frigate is 8938 cells and 2118 quads over 128 bricks, in seven
surfaces. Meshing the inside is what makes a hole read as a hull with a hole
in it.

**A surface is a material is a draw call.** `Surfaces.skin[s]` is one mesh
per `SURF_*`, and the app makes one `StandardMaterial` per surface per hull:
the design's finish as its normal map (`armour_<finish>_n.png`, loaded
linear and repeating, `smooth` being none), the design's metalness and
roughness, vertex colours carrying the livery. Tangents come from the mesher
rather than from mikktspace: on an axis aligned quad the tangent is the
direction U grows in, and `tangents_follow_the_uvs_on_every_face` measures
that off the corners, because a rivet seated the wrong way up on half the
faces is what a guessed handedness looks like.

**A window is a hole in the plating.** The plate pass leaves the cell out and
the window quad is the only surface in that plane, one quad per cell and
never merged, each picking its slice of the decal's variant strip by a hash
of the cell (`hash_cell`, bit for bit `wound.ts`'s), with the hull's own
paint in its vertex colour so the frame round a pane is the plating it is
cut into. Three maps per kind, as `windowMaterial` builds them: colour
multiplies the paint down to glass, emission at 1.6 lights the panes that
are on and is the only part that survives with no light on it, the normal
seats the frame. `a_window_is_cut_out_of_the_plate` holds all of that, and
the Terran frigate's 292 (240 panes, 30 beacons, 13 bridge, 9 portholes) are
pinned against what hull.ts derived. A window on a cell that dies is gone:
its face is a wound now.

The headless run PROVES the textures rather than asserting them: every
finish, every window map and the chitin are checked for pixels after the
frames have run, the way `ftDebug.surfaces()` does it, because a normal map
that failed to decode is a hull in flat paint that looks exactly like a
finish never applied.

## Damage is per cell, and a hit re-meshes a brick

`DamageGrid` keeps hit points per cell. A bite lands on the nearest cell with
a face open to space, which is plate first and the machinery behind it once
the plate is gone; a cell that dies dirties its own 8^3 brick and the bricks
of its neighbours, whose faces toward it are exposed now; the renderer swaps
only those bricks' meshes. `remeshing_only_dirty_bricks_equals_a_full_remesh`
pins that the bricks add up to the whole.

A wound is TWO surfaces, from `wound.ts`: the plate still standing keeps the
hull's finish, and the faces a hit opened are the inside, drawn unlit on the
heat ramp, white hot through orange to char over `COOL_TICKS`. Char is nearly
black and not black, because flat black reads as a gap in the mesh.

A mote parked over one cell eats a CRATER, not a drill hole: the cell beside
the hole is nearer than the cell under it. `a_bite_lands_on_the_nearest_exposed_cell_and_chews_inward`
counts nine plate cells and one of machinery for forty bites. The CPU chewers
in the app follow their hole in, which is what drills.

**A wound is FOUR layers and each is a different thing**: the plate that
survived, the machinery the hole uncovered (lit, in the cell's own colour, on
the plate's own UVs), the burn over that machinery (unlit, on the ember
atlas, white hot through orange to char, alpha the CRUST so a third of it
stays once the fire is out), and the soot on the plating round the rim (two
rings at 0.78 and 0.34, on the ember atlas too, so both halves of a burn come
off one texture). `a_wound_is_four_layers_and_they_line_up` holds the first
three to the same faces quad for quad, and holds the soot OFF the faces that
already carry the burn: three decals deep on one plane is not a picture
anybody can read.

The burn leaves the greedy pass entirely, like a window, because each face
picks its own tile of the atlas by its own DEAD cell: a wound that took its
tile from the live cell would light a whole crater the same way. And the
material puts it on at 3.4, well over white, because the ramp is nought to
one by construction (it is a colour) and how bright that colour is laid on a
hull is the picture's business, not the ramp's.

`DamageGrid::vents` answers where smoke leaves, one per face a hit opened
rather than one per hole: a crater vents along its whole rim, and a single
plume from the middle would read as a chimney. Smoke is the one spark that
must NOT bloom, so it is thrown at 0.30, 0.20, 0.16 and nothing else here is
under one.

## Effects: what a shot is, and what comes off a thing that dies

**A shot is a VOLUME, and the swarm shader is what resolves it.** The CPU
cannot see where a mote is: they live in a storage buffer and never come
back. So nothing is resolved on the CPU and told to the swarm. `Shots` is a
list of capsules pushed into the params uniform every frame, and the tick
shader tests every mote against every one of them. A beam is a capsule from
muzzle to endpoint; a blast is a capsule of ZERO LENGTH whose radius the app
has already grown for this tick (`Blast::radius_at`, `sqrt` so it opens fast
and stops). One shape, so the shader has one test and no branch on kind.

That test is written twice, in `fx::Beam::kills` and in `swarm.wgsl`, and
that is the divergent path GUIDELINES 5.1 warns about. It is here on purpose
because the boundary is real, and the honest guard is that the Rust one is
the reference and is pinned against a brute force answer that walks the
segment (`the_capsule_test_agrees_with_walking_the_segment`, six thousand
points over three beams). The WGSL is a transcription of that same
expression. If it ever needs to change, change it in `fx.rs` first.

**Guns are read off the ship.** `fx::guns_of` clusters the cells the export
says are `SURF_WEAPON` and puts a muzzle at each cluster's outermost cell,
looking outward from the hull's own axis. The Terran frigate has three, which
is what its class table says it carries. Nothing about a gun is authored
beside the hull, so a class with no weapons has none rather than having some
invented for it.

**Sparks are one buffer with TWO writers.** The lower half of the ring is the
app's, written with `write_buffer` at a cursor `swarm.rs` keeps; the upper
half is the swarm shader's, claimed with one `atomicAdd` per burst so a
mote's sparks stay together and cannot interleave with another's half written
ones. Two regions, no contention. A compute pass integrates them and one
instanced draw of camera facing quads puts them on screen, additive and
unlit, because a spark is light rather than a surface.

Four things learned building it, each of which looked like something else:

- **`clear_spark_queue` ran in `Last` and drew nothing.** Extraction runs
  after the main schedule, so a queue emptied at the end of the frame is
  emptied before the render world has seen it. Every spark was counted, and
  every one was thrown away. It runs in `First` now. The reason it was found
  at all is that the headless report counts what was QUEUED, so "no sparks in
  the picture" and "no sparks asked for" could be told apart.
- **Additive is not the default.** The sorted phase hands out alpha blending,
  which is right for glass and exactly wrong for a spark: an alpha blended
  spark DARKENS whatever is behind it wherever its own colour is dimmer, so a
  burst over a lit hull came out as grey specks. The spark pipeline sets
  `One, One` itself.
- **The ember atlas is mostly char.** It is a burn seen on a hull, and the
  wound multiplies it by a heat ramp so the char is what shows once a hole has
  cooled. Multiplied into a spark it does the same thing, and a spark IS the
  molten part: sampling a random point of a mostly black tile put most of a
  burst out. Its LUMINANCE modulates over a floor instead, which keeps the
  mottling and never takes a spark below the colour it was thrown with.
- **A fireball is not a shell.** It was a sphere drawn additively and it came
  out as a solid orange disc with the wreck somewhere behind it: additive
  blending on a closed surface lays the same colour down twice per ray, going
  in and coming out, so a shell bright enough to read at its rim is opaque
  everywhere else. redux-tribes hit this on its movement envelope and answered
  it with a fresnel. The answer here is that a particle system already exists
  and an explosion is a great many burning pieces, which is a thing it can
  draw and a sphere is not.

**A beam is a strip of three quads turned edge on to the eye**, rebuilt on the
CPU every frame because there are a few dozen and they are a function of where
the camera is. Three rather than one so it has a soft edge: the outer columns
carry no alpha and the inner two carry all of it. `cull_mode: None`, because
which way the winding comes out depends on where the eye is and a culled beam
vanished over half the orbit.

**A hull that has lost a tenth of itself goes critical.** A share rather than
a count, because "enough" means something different on a corvette and a heavy
cruiser. The reactor takes a sphere of the ship at once (`blast_cells`, at the
FULL radius rather than staged over the two dozen ticks the fireball takes, or
the same bricks re-mesh twenty four times to no visible end), throws what it
took as debris up to a cap, sprays fourteen hundred sparks and a flash, and
pushes a capsule the swarm feels. What it does to the HULL is a smaller sphere
than what it does to the swarm: a reactor takes the ship it is in, and the
pressure wave goes further than the wreck does.

**`--fixed-dt` is how a screenshot is aimed.** A software rasteriser draws at
four frames a second, so a frame is fourteen ticks by the wall clock and the
shot meant for the fireball arrives four hundred ticks after it went out. With
it, one frame is one tick and a headless render is a function of its frame
count rather than of how fast the machine is.

## The sky is the archive's, baked once

`sky.rs` is `sky.ts`, which is `Procgen_Space_Skybox.shadergraph`: two layers
of turbulence at 3.4's octave counts, folded per octave and not after the
sum, a mask at 0.42, two colours and a seed per mission. Baked on the CPU at
launch into a `Rgba16Float` cubemap (6 faces of 256 in about 420 ms), because
the core has no renderer and a sky a test can measure is worth the wait:
`turbulence_is_dark_with_filaments` holds the port to the distribution sky.ts
measured (mean 0.345, p10 0.170), and the app logs what it baked (floor
0.0025, mean 0.012, max 0.070, gas over 18.6% of the sky). Half float, so no
step exists in the texture to magnify into a contour.

The stars are geometry, at the Voronoi feature points the shader used to test
rays against: 7187 points on a shell that rides the eye (`AtInfinity`),
because a point survives magnification and a texel does not. The same
cubemap lights the hulls through `GeneratedEnvironmentMapLight`, well under
the sky's own brightness: at the sky's level it washed a purple chitin grey.
The sun is a body at the key light's direction and the planets are lit
spheres in the 250 to 660 band with a shade colour so their dark side is
never black, as `backdrop.ts` has them.

`Skybox.brightness` is what texel 1.0 maps to in cd/m^2, and the first cut
had it at 1200 with the environment at 900: a uniform green sky with a grey
mother in it. The bake was right the whole time (the log said so), and the
lesson is the one redux-tribes keeps: log the numbers, because a picture
cannot tell a bake with no structure from a display that lifts its floor.

## Aliens are seeded, mirrored and connected, and wear chitin

`alien::generate(archetype, seed)` builds a mote on a 16^3 lattice (24^3 for
a mother) from a seed and nothing else, mirrors it about x by copying one half
onto the other, and every limb is a walk of one face steps from a cell that is
already there, so it is connected by construction. The suite holds all four
archetypes to symmetric, one piece, at least two glow cells, and eight seeds
giving at least two bodies. An eye is a body cell relit on the crown of its
column, never a cell beside the head: the first cut put eyes in space and the
lancer came out in three pieces.

Their skin is `alien_chitin_n.png`, from `tools/make_chitin_texture.py` on
the same `texkit` the finishes use: scales overlapping like roof tiles, each
a dome with a crease at its root, wrinkles across them and pores sunk in, no
`sin` anywhere so `--check` holds on another machine. The showcase wears it
through `StandardMaterial`; the instanced motes wear it through their own
shader, which carries the mesh tangents at location 4 and a texture bind
group at 3, and that is why the instance attributes moved to 8 and 9: they
were at 3 and 4, which is fine for a cube and collides with the tangents the
moment the mesh has any.

It is tiled at HALF the rate of a finish, a scale spanning two cells, in the
material's `uv_transform` and in the mote shader alike. At one scale a cell,
on a body a few cells across, the map was bound and loaded and read as grain:
the close up (`--target -1.4,-2.3,3.9 --zoom 1.05`, which is where the
showcase stands) is the check, because a normal map that reads as nothing is
the same failure as one that never loaded.

## Suites

```sh
cargo test -p swarm_core                                   # 42, the core
python3 tools/make_chitin_texture.py --check               # the chitin has not drifted
cargo build --release -p swarm_app
./target/release/swarm_app --headless --motes 5000 --frames 60 --out shot.png
./target/release/swarm_app --headless --motes 1 --chewers 0 --zoom 1.7 --out close.png
# The effects, aimed: one frame is one tick, so a shot can be taken AT a tick.
./target/release/swarm_app --headless --fixed-dt --motes 1 --chewers 80 --cadence 0 \
    --frames 260 --zoom 1.35 --out wound.png            # a hull burning
./target/release/swarm_app --headless --fixed-dt --motes 2600 --cadence 5 \
    --frames 140 --zoom 3.4 --out beams.png             # guns into the swarm
./target/release/swarm_app --headless --fixed-dt --motes 900 --explode 90 \
    --frames 93 --zoom 6.0 --out boom.png               # three ticks after the reactor
node tools/export_hulls.mjs ../redux-tribes assets/hulls   # re-export the fleet (needs npm install in tools/)
```

The headless run needs a Vulkan device. This was built in a container with no
GPU and `mesa-vulkan-drivers` installed for lavapipe, and it works there:
Bevy reports `llvmpipe | Vulkan | Cpu`, the compute pass ticks, the screenshot
comes back with the hull in the middle third. Numbers from lavapipe are not
GPU numbers: 5000 motes cost 188 ms a frame there, because a software
rasteriser pays for every triangle of every instanced drone. What lavapipe is
for is CORRECTNESS in CI and A/B of a change, never a frame rate.

And what it cannot catch: driver specific behaviour. Redux-tribes learned that
SwiftShader gives `pow` of a negative base a defined answer while a real driver
gives a NaN. Same trap here. Guard every expression at the point it can leave
its domain, and test invariants headless and performance on silicon.

## Measure, then decide

Numbers in the commit message. `wgpu` compute on this container, before any
of this was written: 1M motes at 8 stride sampled neighbours, 35.42 ms a tick
on four cores of llvmpipe, linear in the count and doubling from 8 neighbours
to 24. That is the arithmetic the whole design rests on and it was measured
before a line of it existed.
