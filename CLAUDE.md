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

## Hulls are the redux-tribes hulls

`assets/hulls/*.ftvx` are the twenty three stock hulls, exported from
redux-tribes' own rasteriser by `tools/export_hulls.mjs` and read by
`VoxelModel::from_ftvx`. Cells, materials, purposes, roles and colours are
that project's answers written down; nothing here interprets a design. They
are not mirrored and that is faithful (`assets/README.md` says why).

The mesher is `hull.ts` ported: a face wherever a solid cell meets one that is
not, INCLUDING the faces nothing outside can see, greedily merged into
rectangles. A frigate is 8938 cells and 2060 quads over 128 bricks. Meshing
the inside is what makes a hole read as a hull with a hole in it.

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

## Aliens are seeded, mirrored and connected

`alien::generate(archetype, seed)` builds a mote on a 16^3 lattice (24^3 for
a mother) from a seed and nothing else, mirrors it about x by copying one half
onto the other, and every limb is a walk of one face steps from a cell that is
already there, so it is connected by construction. The suite holds all four
archetypes to symmetric, one piece, at least two glow cells, and eight seeds
giving at least two bodies. An eye is a body cell relit on the crown of its
column, never a cell beside the head: the first cut put eyes in space and the
lancer came out in three pieces.

## Suites

```sh
cargo test -p swarm_core                                   # 23, the core
cargo build --release -p swarm_app
./target/release/swarm_app --headless --motes 5000 --frames 60 --out shot.png
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
