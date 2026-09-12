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

## How the code is written

Ported from redux-tribes' `GUIDELINES.md` (section 5, reuse and SOLID) and
its `CLAUDE.md`, and adapted to Rust and Bevy. These are rules, and the
first three are checks: `.claude/skills/tidy/SKILL.md` runs them all.

- **A file is under 900 lines and a function under 100.** `python3
  tools/shape.py --check` fails on either. A file thousands of lines long is
  a file nobody can hold in their head and a function hundreds of lines long
  is a function nobody can test. The list it prints is work, never a reason
  to raise the limit.
- **rustfmt is the format**, `cargo fmt --all -- --check` in the suites. A
  formatting commit carries nothing else and goes in `.git-blame-ignore-revs`.
- **clippy is clean at `-D warnings` in the core**, and its count in the app
  never rises.
- **No em dashes or en dashes anywhere**, checked by
  `git ls-files -z | LC_ALL=C.UTF-8 xargs -0 grep -lP '[\x{2013}\x{2014}]'`.
  The locale goes on the grep, not on git: without it grep refuses the
  code points and the check passes by printing an error instead of a file.
- **Single responsibility.** A module owns one thing and its first line says
  what; a Bevy system does one thing and is named as a verb phrase
  (`fly_hull`, `draw_nav`), a component is a noun, a marker is an adjective.
  A function that needs a section comment inside it is two functions, and
  `#[allow(clippy::too_many_arguments)]` is the smell that says a struct is
  missing.
- **Divergent paths for like functionality are a defect.** Two places that
  need one behaviour call one function; a second caller that needs a
  variation parameterises the one implementation. The single allowed
  duplicate is the capsule test in `fx.rs` and `swarm.wgsl`, because the GPU
  cannot be asked, and it lives under the rule that the Rust one is the
  reference and the shader is its transcription.
- **Open for extension, closed for modification.** A new weapon is a shot
  kind and one match arm, a new mission is a row, a new class is a file in
  `assets/hulls`, and lists a player picks from are read off the manifest
  rather than typed. Tuning numbers and scenario contents are data, never
  inline in a system.
- **Liskov.** Anything standing in for a `Hull` keeps every invariant a hull
  has: a wreck is a hull with its cells, its damage grid and its bricks, which
  is why `remesh_dirty` cools its burns without knowing. A fighter is not one
  and no query pretends it is.
- **Interface segregation.** A query names exactly the components it reads,
  and its filters (`Without<Hive>`, `Without<Wreck>`, `Without<Turret>`) ARE
  the interface: they are what lets Bevy prove two systems disjoint, and a
  rule a system must not see ("skip a dead hull") is a marker and a filter,
  not an `if` in twelve systems. Resources stay narrow, one fact each.
- **Dependency inversion.** The core depends on nothing but `std`; the app
  depends on the core's public functions; that direction never reverses.
- **Rust, specifically.** `f32` wherever state lives. No `HashMap` in
  anything that will be hashed or replayed (the core has none; a render side
  material cache is fine). No `unwrap` past startup, and every `expect` says
  what was assumed. Every `pub fn` in the core has a doc comment and a test.
  A constant lives beside the one system that reads it, with its unit in the
  comment. Guard every expression at the point it can leave its domain: a NaN
  is not a wrong picture, it is every picture wrong from then on. `single()`
  only where exactly one can exist. Comments say WHY; the code says what.
- **A mockup before a large feature.** A new screen, a new mechanic's feel,
  a new hull, a scenario: rendered, linked, approved, then built. The move
  order was rebuilt as a three.js prototype after the first cut shipped four
  defects, and that is the rule now rather than the exception.
- **Measure, then decide.** Numbers in the commit message, a clock rather
  than the engine's delta, and never "faster" without a before and an after.
  The section of that name at the end of this file is the long form.
- **Before a push**: `/simplify` on the diff for reuse and altitude,
  `/code-review` for correctness, then the suites.

## The app is folders now, and the split was mechanical

`main.rs` was 5241 lines after the merge that preceded it. It is the
arguments, the `App` and the schedule now, and everything else is in the
folders the proposal drew: `app/` (the scene, the clocks, the headless
harness), `world/` (assets, backdrop, rocks), `ships/` (hull, spec, flight,
formation, damage, wreck, turrets, flames), `weapons/` (beams, flak),
`fighters/`, `hives/`, `controllers/` (selection, orders, camera), `ui/`
(hud, pause, bars) and `fx/` (nav). A folder's `mod.rs` names its files and
re-exports them `pub(crate)`; a file's first line says what it owns.

**Nothing was rewritten.** A script moved every top level item whole, with
the comments and attributes above it, into exactly one file, and refused to
run on an item that was not in its map. The only edits were `pub(crate)` on
every moved item, field and inherent method, and one rename: `Scene` is
`SceneSpec`, because `bevy::prelude` exports a `Scene` of its own, and two
glob imports supplying one name is an error the moment the name is used.
Every module opens with `use crate::*;`, so the import block is written once,
in `main.rs`, and a module sees exactly what the root sees. The script then
checked that every non blank line of the old file is in the new tree exactly
once, modulo the prefix and the rename, and it is.

**It is proved by the pictures.** Five headless scenes (the order, the
wreck, the beams, the battle and the wing) were rendered on the binary from
before and on the binary from after and compared with `tools/pngdiff.py`:

| scene | before vs after | the before binary vs itself |
| --- | --- | --- |
| order | 0.141% | 0.013%, 0.166%, 0.167% (three runs, every pair) |
| wreck | 0.032% | 2.069% (and 0.249% on the pre merge binary) |
| beams | 0.733% | 0.847% |
| battle | 0.429% | 0.418% |
| wing | 2.869% | 3.144% |

Every share is of pixels moved by more than 8 of 255. The wing is the noisy
one: three hundred and twenty frames of kills, with an order in it, and the
after picture against the before binary's OTHER run is 2.221%.

**The floor is two runs of the SAME binary, and it was not nought, for two
reasons it took the whole exercise to tell apart.** The first was the CPU:
three systems read the frame's own clock under `--fixed-dt` (the next
section), so the debris, the showcase and an order's ping stood somewhere
different every run, and the wreck scene's own floor on the before binary was
two percent. The second is the GPU, and it is intermittent: the spark and
shock rings are claimed with `atomicAdd`, so which deaths a full ring keeps
depends on the order the threads arrived in, and the swarm diverges from
there. Intermittent, because two runs on a quiet machine can arrive in the
same order, and then they agree to the BYTE: the split binary's battle
picture was byte identical to one of the before binary's, and its wing
picture to another, which is also what proves the split beyond any share.
With the clocks fixed, the wreck scene, which kills nothing after the
reactor goes, is byte identical run to run, and the scenes with kills differ
by 0.12% to 2.4% between two runs of the same binary. So a pair is judged
against that scene's OWN floor, `cmp` is the check only for a scene that has
one, and the HUD's frame counter, which reads real time, is about two
hundred pixels of any picture it is in. The whole branch, measured last, on
the final binary:

| scene | before vs final | the final binary vs itself |
| --- | --- | --- |
| order | 0.162% | 0.131% |
| wreck | 0.499% | 0, byte identical |
| beams | 0.638% | 0.123% |
| battle | 0.485% | 0.407% |
| wing | 3.164% | 2.411% |

The wreck's half a percent is real and is the fix: its debris flew fifteen
times too far per frame before. The battle's is the rocks turned through
`TAU` rather than 6.283. One trap on the way: the before pictures were being
taken off `target/release/swarm_app` while cargo was replacing it, so the
last of them came from the binary under test. Copy a binary aside before it
is the control for anything.

**What this stage did not do, on purpose.** The long functions are exactly
where they were, only in smaller files: `setup` (583 lines once rustfmt had
wrapped it), `go_critical` (299), `build_hud` (294), `spawn_ship` (205) and
the rest, seventeen over a hundred after the format against eleven before
it, and `swarm.rs` and the core's `fx.rs` are still one file each. They come
apart in the next stage, with the four `SystemSet`s and the explicit imports
that replace `use crate::*;` once each module knows what it reads.
`tools/shape.py --check` therefore still fails, on those, and the list it
prints is that stage's work.

**Then the format, on its own.** The tree had never been through rustfmt:
342 hunks at the default width, which is the width redux-tribes
keeps too. That commit carries nothing else and is in
`.git-blame-ignore-revs`, so `git blame` reads through it.

**And clippy.** The core is clean at `-D warnings`. Its one `allow` is the
star phase in `sky.rs`, which multiplies by 6.283 because sky.ts writes
6.283 and the port is line for line. The app's four deny by default errors
are gone: the rock rotations drew from `0.0..6.283` three times and draw from
`TAU` now, which is at most eighteen hundred thousandths of a radian on an
asteroid, and `ride_the_eye` computed a zero two ways and added it.

**One clock, asked in one place.** The fixed step was written out eight
times, the same three lines in eight systems, and three more places that
integrated something had never copied it: debris flew by the frame's own
delta, the showcase spun by it, and an order's ping aged on the real clock
whatever `--fixed-dt` said. On a software rasteriser a frame is a quarter
of a second, so a debris cube flew fifteen times too far per frame in
exactly the pictures the flag exists for, and a picture with an order in it
depended on how loaded the machine was. `SceneSpec::step` is the rule now,
and every system asks it. The lesson is the one the two clamps already
taught: a rule copied is a rule one copy will miss, and the copy that is
missing is the one nobody can grep for.

## A front door, a form, a verdict and a sandbox

The app used to boot straight into a fight with whatever the command line
said. It has states now, `AppState`: `Menu`, `Setup`, `Playing`, `Result`,
and `Boot`, which is the first frame and nothing else. Each screen is UI
built on the way in and dropped on the way out by its own `DespawnOnExit`;
the fight is spawned on the way INTO `Playing` and torn down on the way
into `Menu` or into the next `Playing`. A result keeps the field, because
the wreck the fight ended on is the picture that screen sits over.

**Boot exists because the initial state's `OnEnter` runs BEFORE `PreStartup`.**
A run that opened straight on the fight (every headless render does) spawned
its field before the textures had loaded or the camera existed, and the first
system to ask for `Textures` panicked. There is no startup schedule early
enough, so the app starts in `Boot`, `boot` in `PostStartup` sets the state the
arguments asked for, and the transition happens on the first frame after
startup has run. `--screen menu|setup|result` opens on a screen, `--play`
opens a window on the fight, and `--sandbox` opens on the playground.

**The backdrop is spawned once and the field is spawned per scene.** `setup`
came apart along that line: `spawn_backdrop` (the sky, the stars, the sun,
the planets, the camera and the meshes rebuilt every frame) runs at startup
and `mark_keep` tags everything then in the world with `Keep`; `spawn_field`
(the fleet, the showcase, the carriers, the rocks, what the swarm is told,
where the camera looks) runs on entering `Playing`. The teardown despawns
every top level entity with a `Transform` that is not kept and is not UI,
and the children go with their parents. A marker on the FEW things that
persist rather than on the many that do not, because the field is spawned
from a dozen places (a wave, a fighter, a wreck, a chunk) and a marker every
one of them had to remember is a marker one of them would forget. The swarm
is rebuilt too: `SwarmConfig.generation` moves with every field and the
render world compares it to the one its buffers were built for, so a new
fight starts with every mote inside its carrier again rather than carrying
the last cloud in.

**The setup is two columns of steppers**, the enemy on the left and your
side on the right, exactly the proposal's table: motherships, swarm (small,
medium, large: twenty, a hundred and three hundred thousand), launch delay,
where the carriers stand, asteroids, the seed; the flagship, its wing, its
fighters, and in the sandbox the target dummy's class. Every control is an
arrow pair, so it works without typing. The class list is read off the hull
files in `assets/hulls` rather than typed, navy by navy up its ladder and
then the civil trades, so a class added tomorrow is on the form tomorrow;
under the flagship the form says what it is made of (cells, guns, drives,
reactor), read off the hull once and remembered, so a pick is a decision and
not a name. The form is filled from the command line at start and kept
between visits, and Launch writes `SceneSpec` and enters `Playing`.

**The verdict is `judge`**: every carrier a wreck is a win, every player
hull a wreck is a loss, wrecks counting as neither, and a sandbox never
ends. It waits `VERDICT_GRACE` (four seconds) before the screen, so the
fireball and the wreck are seen before the word. The first cut had no grace,
and the wreck picture in the suites came out as a pristine frigate firing
beams: `--explode 90` forces every hull critical at tick ninety, which is a
victory, and the scene froze on the frame the pieces were spawned, before
any had drifted apart. A picture that looks wrong names a symptom.

**The sandbox is the skirmish with toggles, a target and a range.** Freeze
holds the cloud still while everything else runs (`SwarmConfig.frozen`, a
step of nought for the swarm's clock and the chewers standing down), and it
needed one fix in `swarm.wgsl`: the drag on a mote's speed was a flat 0.985
a pass whatever the step, so a cloud frozen for ten seconds restarted at a
crawl; it is `pow(0.985, dt * 60)` now, and a step of nought is no drag.
Slow motion is `time_scale`, a quarter, on `SceneSpec::step` and on the
swarm's clock alike, which is the one clock rule paying off. Invulnerable is
a flag on the flagship the chewers and the reactor rule both read. B is a
blast where the cursor points on the plane through the flagship, N a fresh
dummy. The keys are Z, X, V, B and N because F, R and space were taken.

**The range is a mode.** Arming a weapon (one to five, nought disarms, or
the panel) puts `OrderMode` in `Range`, so a left click is a shot and never
a selection, and Escape disarms before it can reach the pause menu, which
is the move order's own rule kept. The click is a ray from the camera into
the dummy's own lattice (`swarm_core::ray::march`, a cell walk, so a shot
into a crater lands on the crater's floor exactly where a chewer's bite
would), and what lands there is the weapon's:

| weapon | the cells | the shove |
| --- | --- | --- |
| beam | the ring of bites a beam takes off a carrier, held while the button is down | 0.02 |
| flak | a blast of cells at the point, as a burst does to a carrier | 0.25 |
| slug | a bored column seven cells deep along its own line | 0.6 |
| torpedo | flies from the flagship first, then a blast twice a flak's | 0.9 |
| bite | what a chewer does, one cell | 0 |

The shove is a change of velocity in units a second if it landed through
the centre of mass; the impulse is that times the mass. **The mass is the
core's** (`swarm_core::body`): every live cell a unit mass at its centre,
plus the inertia of its own cube, so the mass, the centre and the tensor are
three sums over the cells and a ship with its bow shot off balances further
aft and spins differently by construction. `Body::kick` is `dv = j / m` and
`dw = I^-1 (r x j)`, in the model's frame, so the tensor is never rotated:
the app takes the point and the impulse into the hull's frame and the two
velocities back out. The suite holds a block to the lattice's own inertia
formula, a hit through the centre to no turn, a hit at the rim to a turn
about the right axis, and half the cells gone to a centre that moved.

**A tumbling hull is placed from its centre of mass**, which is the wreck
lesson a second time: its cells are laid out about the lattice origin, which
is not where it balances, so the spin is kept about a pivot and the entity
placed from that every frame, or a ship kicked at the bow would orbit an
invisible point. It re-weighs itself after every hit. A little damping
stands in for the attitude thrusters, or a range target would spin for
ever, which is honest for vacuum and useless for a range. Measured: a slug
on a Karisen frigate (6486 cells) a sixth of a radius above its centre takes
nine cells off and leaves it turning at 14.8 degrees a second.

Two things the first cut got wrong, both caught by the scripted shot
(`--fire slug,30` lands one at tick thirty from the camera, so the tumble
can be photographed). The dummy came out as a second FLAGSHIP, because
`spawn_hull` marks any hull with no station as the flagship and selects it;
two flagships is a camera, a nav disc and a swarm target that all do nothing,
since `single()` fails on both. And the shot missed: it was aimed a bit
under half a radius above the centre, which is inside the LATTICE and above
the DECK, so the ray crossed empty cells and came out the other side. The
lattice is not the hull.

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

## The swarm shades itself, and a shadow is a property of the FIELD

Ten thousand motes lit by one key light are ten thousand equally bright
specks. The mote on the near face of a clump and the mote buried behind ten
thousand of its own kind came out the same colour, so a dense cloud read as a
flat sheet of them: it had no inside. What was missing is the one thing a
cloud is actually made of, which is that a cloud stops light.

**A mote cannot march toward the sun on its own account.** Sixteen steps each,
a million of them, is sixteen million samples a tick against a budget of
sixteen nanoseconds a mote: the same arithmetic that says a mote is not an
entity says a mote does not get a shadow ray of its own. So the GRID marches.
A 64^3 density field is cleared, counted into with one atomic per mote, and
marched toward the sun once a tick, and none of that moves when the swarm
grows: a quarter of a million cells at a thousand motes and at a million. A
mote pays one trilinear read to find out how dark it stands, and trilinear
because a cell is a few units and a mote is a fraction of one, so a nearest
read would fly the swarm through visible cubes of shade.

Three dispatches inside ONE compute pass, in that order, because each reads
what the one before it wrote and a write inside a pass is visible to the next
dispatch. The field is counted from where the motes are BEFORE they move and
read after they have: a tick of lag across a cell several units wide, which is
a fifth of a unit of travel and nothing anybody can see.

**Two numbers come out of it, because they are two different lights.** The sun
is one direction, so what stands in its way is the cloud along that line:
sixteen cells toward it, Beer's law on the counts. The sky and the bounce
arrive from everywhere, so what shuts them out is the cloud immediately round
the mote, its own cell and its six faces. Shading the ambient with the key's
own number would light a mote's dark side out of a cloud that has no light in
it at all.

Neither ever reaches nought (0.09 and 0.20), which is the ambient lesson from
further down this file twice over: a mote lit by nothing is the colour of the
gap between two stars, and a swarm whose middle is a hole is worse than one
with no shading in it.

**The thickness of the cloud is decided in exactly one place**, which is how
much of a cell's face one mote covers. That is what turns a count of motes
into an optical depth, and the app publishes it off the drone's own silhouette
rather than a shader tuning a number, so a bigger mote shadows more by itself.
`--thickness 0` is the flat lighting this replaced, which is what an A/B is
taken against.

**And the key is the SCENE's light now, read rather than copied.** It was a
constant in the shader, matched by hand to the vector `setup` aims the
directional light along. That was already one number written down in two
places, and shadowing makes it three, because the density field marches along
whatever the app publishes: a cloud shadowed from one side of the sky with its
highlight on the other is the one thing an eye will not forgive, and it is
exactly what copies drift into. The claim that the mote draw binds nothing but
the view and the chitin was the thing to check: it binds the mesh VIEW bind
group at nought, and `lights` is binding one of it, so the sun the scene is
actually lit by was there to be read for nothing the whole time. `SUN` in
`world/backdrop.rs` is the only place it is written now.

**A glow is EMISSIVE, and nothing in the cloud takes it away.** The lit cells
used to REPLACE the shaded body wherever they were over one, which made a glow
and a shadow two settings of one knob. With the cloud shading itself that is
backwards: the motes whose own lamps are worth looking at are the ones buried
deepest in it. So the fragment is two channels and the only question about any
term is which one it is in. Light that ARRIVED (key, fill, specular) is
attenuated by the field. Light a mote MAKES is added afterwards and attenuated
by nothing, because a lamp does not go out because the thing beside it is in
shadow. A mote in the dark heart of the swarm is a dark body with its drive
still lit, which is the picture.

**A WOUND is in that same channel, and that is the whole reason it can be
seen.** A mote is hurt rather than only alive or dead (`extra.x`, one down to
nought), and a hurt one BURNS as it turns for home. Shading that would be
exactly wrong: the swarm is thickest where the fighting is, so shading a wound
would put every bright one in the picture precisely where the cloud has already
put it out.

**And it burns ORANGE, not violet.** Violet was the first answer, on the
reasoning that violet is what a mote bleeds. That is the colour of the ANIMAL
rather than the colour of an injury, and laid over a body that is already
violet chitin it made a bug that happened to be a brighter purple: a player
reads that as another kind of mote, not as one they have just hit. Every other
damaged thing in this game goes orange, from a chewed frigate to a carrier
coming apart, so a damaged mote goes orange too. What stays violet is the GORE
that comes out when it bursts. Burning is what a hit DOES to it; bleeding is
what is inside it.

Two numbers make it read, and both were wrong in the first cut. It has to
clear the bloom threshold at the one hit a mote actually survives, which is
`SHOT_BITE` off a whole one, so the square the wound is scaled by is 0.36 and
nothing under about three ever blooms at all. And it PULSES, on a clock the
tick hands over in `shade.z` at the mote's own rate and phase: a steady glow is
a colour, a pulse is an injury, and a pulse is what carries at the one or two
pixels a mote is usually drawn at. `mote.wgsl` has no clock of its own, which
is why the beat comes down with the shading rather than being worked out in
the draw.

**And a kill FLASHES.** Five gore sparks and four pieces of debris was a puff:
at the size a mote is drawn, nine small additive particles read as a sparkle
rather than as a thing coming apart, which is why a volley into the cloud
looked like nothing was happening. Eleven of the spray now, and one big short
lived flash at the mote's own place, white hot and warm rather than violet,
because the flash is the moment of bursting rather than anything that came out
of it. The DEBRIS is deliberately not raised with them: that is the half that
lasts, and four a kill at a second and a half each over a swarm losing dozens
a second is the violet haze this file warned about once already. A kill is
bright and BRIEF.

The flash lives a third of a second and no less, and that is a trap in the
spark shader rather than a taste: a spark's colour is scaled by
`min(1, life * 3)` at birth, so a flash asked to live a tenth of a second is
born at a third of the brightness it was thrown with. It is brief because it
fades, not because it is cut short.

What it cost is a fifth `vec4` on the mote, on the same terms as the fourth.
It buys the only thing a shaded swarm cannot do without, which is somewhere to
put the answer: a mote cannot work its shadow out at draw time and cannot be
told it either, because nothing about a mote ever comes back to the CPU. The
mote buffer IS the instance buffer, so a field the tick fills is one the vertex
shader already has, for no upload and no pass.

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

## The swarm comes out of carriers, and you can move the ship

**Motes launch from motherships.** The swarm used to come back at a shell
round the target, which is a cloud that simply exists. Ten `Archetype::Mother`
hulls stand at fourteen to twenty two hull radii, and every mote carries the
index of the one it flies from in `state.z`: it appears at that carrier's
skin, is shoved out hard, and the appetite takes over once it is clear. A
mote that is killed goes back to its carrier and flies out again, so killing
fighters is holding a line and killing a carrier is winning ground.

The hive list in the params is compacted to the LIVE ones every frame, and a
mote takes its index modulo the length. So a carrier dying shortens the list
and every fighter that flew from it re-homes to whatever is left, and there is
no rule about it anywhere. An empty list is the win condition and needs no
rule either: `p.hives == 0` and a dead mote simply stays dead.

**Everything starts inside.** Every mote is created dead with a staggered
countdown over about eight seconds, so the first thing a player sees is ten
carriers streaming fighters rather than a cloud that was already there.

**Fighters have to be able to CROSS.** At the three to six units a second the
swarm held when it lived on a shell round the ship, a fighter launched from
seventy units out took twenty seconds to reach the fight and the cloud never
built. Eight to sixteen makes the transit about five seconds. The pull toward
the hull is also capped, or a fighter fifty units out accelerates at fifty and
arrives as a bullet: it is the cap that makes an approach read as a flight.

**A carrier is a siege.** `HIVE_HP` is twelve hundred, which is the better
part of a minute of concentrated fire from three beams. At a tenth of that
every mothership in the picture died inside two seconds and six simultaneous
fireballs turned the screen white, which is also why the green a carrier burns
is a MULTIPLIER on the ramp and not a term added to it: a constant added to
nine hundred additive sparks puts a floor under the whole burst.

**Moving the ship is Homeworld's own shape.** Right click opens an order on
the selection: the cursor picks a point on the horizontal plane through the
ships, and holding shift lifts the target off that plane and draws the right
angle triangle back down to it, which is what makes a flat screen able to name
a place in three dimensions at all. Left click commits. `fly_hull` is a real
envelope, not a lerp: it accelerates, it has a top speed, it slows into the
arrival and it turns to face the way it is going, so an order to a capital
ship has weight. The full flow, and what the first cut of it got wrong, is
under "The controls are an RTS's now".

`publish_hull` hands the hull's live position to the swarm every frame, which
is what makes moving it worth doing: the cloud is dragged along behind and a
player who runs can watch the swarm string out. `--move x,y,z` issues one
order at startup so a headless render can show it.

## The swarm makes RUNS, and a torus is what it did instead

**The cloud settled into a donut and that was two bugs wearing one shape.**
One standoff per mote plus one swirl AXIS for the whole swarm is a torus by
construction: every mote circulated about the same global up, so whatever the
standoffs were spread over, the cloud was a ring. Both halves are gone.

- **The swirl axis is the MOTE's now**, from its own seed, so the orbits sit
  at every inclination and the traffic is a sphere rather than a ring.
- **A mote makes ATTACK RUNS.** `state.w` is a run clock: positive is boring
  in, negative is breaking off, and it flips when the leg runs out. In is
  short and committed and presses to contact just off the plating; the break
  is longer and goes several lengths clear, because a pass that turned round
  the moment it arrived would never get far enough out for the next one to
  read as an approach.
- **And each one WEAVES**, a lateral oscillation about the third axis of its
  own frame at its own rate and phase. Without it a mote flies a clean arc,
  and ten thousand clean arcs read as a machine.

**The swarm bites, and you can see it.** A mote in contact throws sparks off
the plating, in the orange a hull burns rather than the green a mote bleeds.
The CPU is not told that it happened and does not need to be: the chewers are
what actually take cells off, and this is what says on screen that the cloud is
the reason. It is gated hard (about one in forty of the motes in contact, per
tick) because thousands are touching at once and the spark ring is shared with
everything that dies.

## Asteroids are drawn as lumps and navigated as spheres

`swarm_core::rock::generate` is a seeded asteroid: three octaves of value noise
sampled BY DIRECTION, so the radius varies with where you are looking from and
the silhouette is lumps and shelves. A threshold on distance alone gives a ball
and a ball is a planet. It carries ore on its own surface, which is the only
thing on a rock worth looking at twice, and the suite holds every seed to one
piece, clear of its own lattice wall, and actually different from the next
seed.

**The navigation shape is a sphere and that is not a shortcut, it is the
budget.** Every mote tests every rock every tick, so the distance to one has
to have a closed form. The sphere is INSCRIBED rather than circumscribed
(`ROCK_HULL` is 0.72 of the model's radius), because a sphere that contained a
lumpy rock would stand the swarm off well clear of the thin axes, and a cloud
swerving round empty space is worse than one clipping a corner.

The steering is the field's GRADIENT: push out along the normal, harder the
nearer the surface, squared so a mote well outside the margin is barely
deflected and one about to touch is turned hard. It also takes the INTO
component off the velocity a mote already has, or one arriving fast carries its
own momentum through the rock before the push can turn it. And after the
position is written there is a hard clamp out of any rock, because steering can
be beaten: a mote shoved by a blast arrives with more speed than a gradient
takes off it in one tick. The clamp costs nothing in the normal case and is the
only thing that makes "never inside a rock" a guarantee rather than a hope.

## The reactor is buried, and only the reactor kills the ship

**A share of the hull is a hit point bar with extra steps.** The old rule was
"a tenth of its cells gone", which does not care WHERE the damage landed: a
ship scoured evenly all over died exactly as fast as one drilled through the
middle, so aiming at anything bought nothing. redux-tribes' own rule is that
the layout IS the damage model, and this is that rule: the plating, the
machinery behind it, and finally the core, in the order they physically stand
in.

`fx::reactor_of` DERIVES the reactor, because there is nothing to read.
redux-tribes' eight purposes are propulsion, attitude, gun, ordnance, command,
crew, boarding and structure, with no reactor among them, and adding a ninth
would mean changing that project, which is reference only. So it is derived
from the one thing the owner said about it: it is buried. Depth is a multi
source breadth first search inward from every empty cell, so a cell's depth is
how many cells of solid material stand between it and the nearest gap; the
deepest cell is the most buried place in the ship by construction, and the
reactor is the ball of solid cells around it. Ties break toward the middle of
the lattice, so a long flat run of equally buried cells gets its reactor
amidships rather than at whichever end the scan reached first.

Half of it gone is what takes the ship (`REACTOR_LOSS`). Nothing else does, so
a hull can be shot to pieces everywhere else and keep flying, which is what
makes a wreck that is still fighting possible at all. The suite holds the
reactor to buried (no cell of it has a face open to space), solid, small, and
amidships: a reactor that came out at one end passes the first three and is
still a reactor anybody can shoot from the front.

**And the plating is worth a hundred times the bare material.** `ARMOUR` is a
multiplier on hit points rather than a divisor on the bite, so the heat ramp
and the crust still run off the same share of a cell's own maximum and a wound
looks exactly as it did. A siege, not a countdown.

## The ship puts up a squadron

A dozen fighters, and they are ENTITIES, for the reason the whole design turns
on: the ECS holds what there are dozens of. They share one mesh and one
material set between them, built once, so twelve fighters are twelve transforms
and one upload.

**They patrol rather than intercept, and that is not laziness.** The CPU cannot
see where a mote is, so there is nothing to intercept: a fighter picks a point
out on the sphere the swarm holds, flies to it, and picks another when it
arrives. Flying the circuit and firing into it is what a screen actually does,
and it puts the shots where the swarm has to come through.

Their gun needed nothing new. A burst is a `Blast`, which is a capsule of zero
length, which is the shape everything that kills a mote already is, so the
squadron added a weapon without adding a resolution path.

## A mothership is a SHIP, and it bleeds purple

Carriers used to be one mesh with a floating hit point number on it, so the
entire per cell model, the bricks, the four layer wound, the chunks that come
off and the re-mesh of only what changed, existed on one side of the battle.
There was never a reason for that beyond the order things were built in: a
carrier is a voxel model, and everything here works on a voxel model.

`spawn_ship` is `spawn_hull` with the model and its materials handed in, and a
carrier goes through it. It gets a damage grid at its own armour (eight, not
the player's hundred, or neither side could hurt the other and the battle would
never resolve), its bricks, its wounds and its chunks. `go_critical` takes it
the same way it takes a ship, on the same reactor rule, so `hives_critical` and
the hit point number are both gone.

**What it costs is that every system taking hulls now takes carriers.** Three
had to say otherwise: `fly_hull` would have every carrier flying the flagship's
orders, and `fire_guns` and `fire_flak` would have a mothership opening up with
the fleet's own guns on its own side. `Without<Hive>` on those three, and the
rest of the machinery was simply correct.

**A beam takes CELLS off.** It used to subtract from a number. The hit point is
taken into the carrier's own frame first (`bite` works in model coordinates and
a carrier is drawn scaled, turned, and a long way from the origin), and it
lands as a ring of bites rather than one, because a beam that took a single
cell off a mothership would need thousands of shots before the picture changed.

And it **bleeds purple**. The swarm is chitin over violet, so that is what
comes out of one: the death sparks, the beam splash and the carrier's own
bleeding all throw violet now. They used to throw green, which is the colour of
the lamps ON a mote rather than the colour of the animal, so a kill read as a
light going out instead of as a thing coming apart.

## Rounds fly, and the blast is where they land

A flak burst used to appear where it was going to go off, which is a gun with
no shell in it: the muzzle flashed, the target flashed, and nothing crossed the
gap between them. `Tracer` is that gap. A round travels over `TRACER_TICKS`, is
drawn as a short bright bolt (the same camera facing quad a beam is, for the
same reason), and the `Blast` is pushed when it ARRIVES rather than when the
trigger is pulled.

So what kills a mote is the shell reaching it, and a player can watch a burst
travel into the cloud and see the hole appear at the end of its own flight,
which is the whole reason for having a shell at all.

## Linking is the slow step, and it looked like a hang

A build that sits on `398/399: swarm_app(bin)` with no output is cargo waiting
for the LINKER, not a stall. A Bevy binary is a very large link: 124 MB before
anything is stripped, and a relink alone is about 27 seconds here with the
default linker. On Windows under `link.exe`, with Defender inspecting every
object file the linker opens, the same step runs into minutes.

Three things, in the order they are worth doing:

- **`strip = "true"` on the release profile.** 124 MB down to 83 MB, and a
  relink from 27 seconds to 21. The symbols were being written for nobody.
- **`rust-lld` on Windows**, in `.cargo/config.toml`. It ships with the Rust
  toolchain, so there is nothing to install, and it is dramatically faster on a
  link of this shape. The file says in place what to delete if it ever fails to
  resolve, because a config that breaks a build is worse than a slow one.
- **Exclude `target` from Defender.** Often the largest single difference on a
  Windows machine and it costs nothing.

Neither `lld` nor `mold` is configured for Linux or macOS on purpose: neither
ships with the toolchain, and a config that fails on a machine without them is
worse than a slower link on one that has neither.

## Armour went back to one, and the reactor rule is why that is safe

A hundred times the bare material made a ship that could not be hurt. What
actually needed fixing was never the plating: it was that losing a tenth of
your cells ANYWHERE blew the ship up, so the only way to survive was to not be
touched. `REACTOR_LOSS` fixed that on its own, and it is the rule that lets the
plating be soft again. A cell comes off in a few bites, the swarm visibly eats
a hull, and the ship keeps flying, because a hole in the plating is not a hole
in the reactor.

## A drive plumes from the MIDDLE of its bell

`clusters_of` put the muzzle at the single cell that reached furthest along the
cluster's axis. On a drive bell three cells square, every cell on the outer
face ties for that, so the first one found wins and it is a CORNER: every flame
was drawn half a bell up and half a bell across from its own engine, and on a
block of six bells that reads as the whole set being misaligned. The muzzle is
the cluster's midpoint carried out to its outer face now, and
`a_drive_plumes_from_the_centre_of_its_bell` builds exactly the shape that
exposed it.

## Ships steer round the rocks, beams sweep, and the dead are obstacles

**A ship used to fly straight through an asteroid**, which is the field being
scenery rather than terrain. Hulls steer on the same distance field the swarm
does, at their own scale, and there is a hard clamp after the position is
written: steering can be beaten, and an order given straight through a rock
asks for exactly that.

**A beam is SWEPT.** It used to be a fixed segment for its whole life, killing
whatever was on that line at the tick it went off and nothing after. The far
end walks across while the beam is alive, about an axis hashed off the beam
itself so two guns firing together do not scythe in step, and it is much wider:
a beam a couple of cells across cut a thread through the cloud and killed
almost nothing anybody could see. What the swarm is handed each tick is a
different segment, so the beam carves an arc and sets off a line of kills that
travels.

**And a mote that comes apart leaves a hole.** It writes its position and the
time into a ring of sixty four shocks, and every mote reads all of them every
tick: a sphere that opens on the same `sqrt` curve a blast uses and fades over
its life. The swarm opens where one died instead of closing straight over it.
Written by one thread and read by the others a tick later, which is the only
order available, and one tick of lag on a wave lasting most of a second is not
a thing anybody can see.

## A ship that dies leaves a WRECK

**The reactor took the whole ship, and three turrets hung in space where a
frigate had been.** `go_critical` blasted a sphere of one and a half radii,
which on any hull is every cell of it, and threw up to four hundred and fifty
single cell cubes: the picture after a death was a spray of dust and the
guns, which are children with meshes of their own that nothing had touched.
The owner caught both off one screenshot.

**A wreck is a few BIG pieces, and each piece is a hull.** The reactor takes
the ball around it (`HULL_HOLE`, well under the radius, or there is no
wreck), and `fx::shatter` breaks what is left along two planes through the
blast, hashed off the ship's seed: one near across the long axis, so a hull
breaks into a bow and a stern, one along it at a hashed roll, so each of
those breaks port from starboard or deck from keel, and then each sector's
connected runs of live cells. Two planes rather than three, because eight
pieces of a frigate are not giant and giant is the point. Anything under
`WRECK_MIN` cells is dust and is thrown as before. The suite holds a block
with a hole in it to three to eight pieces, every one connected, the biggest
a real share of the hull and never the whole of it, and the pieces and the
dust accounting for every live cell exactly once.

Each piece is spawned as a `Hull` of its own: a copy of the dead ship's model
and damage grid with every live cell that is not in the piece KILLED at the
blast tick, so its cut faces are wounds, white hot and cooling on the same
ramp as any bite, and `remesh_dirty` cools them without knowing it is looking
at a wreck. It is meshed once, over only the bricks the piece touches, and it
is `dead_hull` from birth, so everything that already skips a dead hull (the
swarm's targets, the guns, the bars, the orders, the chewers) never looks at
it; `fly_hull` says `Without<Wreck>` and `drift_wrecks` is its whole flight.
It SMOKES, and nothing was written for that: `vent_smoke` asks the damage
grid where a hit opened a face, and a wreck's cut is a wound like any other.
A piece tumbles about its OWN middle: its cells are laid out about the hull's
origin, which is not the piece's centre, so the tumble is kept as a pivot and
a rotation and the entity is placed from those every frame, because rotating
an off centre piece about its origin swings it round in an arc. The guns come
off whole, cut loose from the hull and thrown a little harder than a section.
And the ship that was is despawned: every cell of it is in a piece, in the
dust or in the fireball now. A minute later the pieces go too, because ten
carriers' wrecks are ten thousand bricks of mesh.

## The controls are an RTS's now

**Selection takes the left button, so the camera gave it up.** Orbit is the
MIDDLE button, with alt and left as the alias for a mouse that has none. A
click takes the nearest ship to the pointer, a drag takes a box of them, and
shift adds rather than replaces, which is the one convention every RTS shares.
Carriers are excluded even though they are hulls: they are not yours and cannot
be ordered.

**A move order is a MODE, not a drag, and it was prototyped before it was
written.** The first cut shipped four defects the owner found in one session,
so the flow was rebuilt as a three.js prototype and a design document, tested
in a browser until it was approved, and only then ported. What is here is that
prototype one to one. Right click opens the disc on the selection, the cursor
aims it on the plane through the ships, shift lifts it off that plane, and
LEFT click commits, which is Homeworld's own button and the one the first cut
got wrong. Holding a button while also moving the mouse to pick a point and
then holding shift to lift it is three things one hand is doing at once; a mode
costs one more click and lets the player take as long as they like over the
part that is actually hard. Every selected ship gets the commit point offset by
where it already stands relative to the group, so a formation arrives as a
formation instead of piling onto one coordinate. An escort that is given an
order of its own stops keeping station, because one flown to a point and then
straight back to its slot is an order that did nothing.

**`OrderMode` is the rule: a button press is read by exactly one system per
frame, and the mode decides which.** The first cut read every button in every
system, and every one of the four defects came from that.

- **Confirming the order CLEARED the selection.** The left press was the
  commit in `nav_input` and the start of a box in `select_input`, on the same
  frame, so the release then selected nothing. `select_input` acts only in
  `Idle` and `Box`, `nav_input` opens only from `Idle` and acts only in
  `Move`, and the three input systems run menu, box, order, each reading the
  mode the one before left.
- **A box that stayed open with no button down.** `select_input` returned
  early whenever the cursor was off the window, which is exactly where a drag
  that started near the edge ends, so the release was never seen. The box
  opens on the press and closes on the release, and the release is read off
  the BUTTON, never off the cursor; the cursor is tracked wherever it reports
  from and kept where it was last seen when it does not; and a window that
  loses focus drops the box outright, selecting nothing.
- **A bar on every ship says nothing about what is selected.** Bars and the
  cyan ring are on SELECTED ships only, which is how every RTS says a unit is
  selected. Three bar colours a player can name rather than a ramp.
- **A confirmed order did nothing anybody could see.** A gold ring pings open
  at the destination, the HUD says "2 ships under way" and fades, and an
  orange line and ring stand on each ship until it arrives: the order is
  visibly THERE.

**The disc's rim is AT THE CURSOR.** A large cyan disc on the plane through
the selection whose radius is the order's own distance, with an X across it,
so the small gold ring where the order lands sits ON the rim and the gold line
out to it is a radius; lifted, the vertical, the direct line and a red ring at
the raised point with the distance in red beside it, and the triangle's base
is the disc's radius. This is Homeworld's disc, which grows with the mouse.
The first port drew a fixed rim at `MOVE_RANGE` while the cursor named a point
a third of the way out, and the owner caught it off the screenshot: a disc
that does not reach the cursor says nothing about the order. It stops growing
at `MOVE_RANGE` radii of the biggest selected hull, and a point past that is
clamped to it. The elevation is read FROM THE CURSOR'S POSITION rather than from how far it
moved: the plane point holds still and the target sits on the vertical through
it at the closest point to the cursor's ray, `t = (b*e - d)/(1 - b*b)` with
`w = P - O`. Not `O - P`, which negates `t` and put the target below the plane
when the mouse went up, and was the prototype's own second bug. `--aim x,y,z`
opens the disc headless so the picture can be taken.

**Escape belongs to the ORDER first.** One escape cancels an open move, or an
open box, and does nothing else; the next, with nothing open, reaches the pause
menu. `toggle_pause` runs BEFORE the two input systems so it sees the mode the
key was pressed in and not the `Idle` they leave behind. Space is the plain
pause. And `nav_input` sits OUTSIDE the run gate with the camera and the
drawing, because giving orders with the world stopped is the entire reason a
pause key is worth having.

**And `--hud` PROVES the HUD rather than asserting it.** It did not, at first:
Bevy hands UI to whichever camera renders the primary window, a headless run
has no primary window, and every node was laid out and drawn to nothing. The
flag produced a screenshot with no HUD in it, which is the one outcome a flag
for showing the HUD must not have. `IsDefaultUiCamera` on the headless camera
is what fixed it, and `--paused` opens the menu so that can be photographed
too. Two glitches turned up the moment there was a picture to look at: a
`\u{25be}` caret the default font has no glyph for, which draws as a hollow
box, and a binding list where every line after the first was indented, because
a `\` string continuation keeps the leading whitespace of the next SOURCE line.
Neither would ever have been found by reading the code.

**The bars are UI nodes, not meshes**, and that is the right call for exactly
these two things: a health bar is a fixed number of pixels tall whatever the
range, and a band box is in screen space by definition. Anything that has to
hold its size in the WORLD stays a mesh, which is why the nav disc and the
beams are not here.

A bar reads the REACTOR, not the plating. Plating comes off and the ship keeps
flying; the reactor is the only thing that kills it, so it is the only honest
thing to put on a bar. Your ships only: a health bar over a carrier turns a
siege into a progress bar, and what tells you a carrier is hurt is that it is
bleeding and burning, which it already does.

**A fighter is worn down by WHERE it flies.** It cannot be shot by a named
mote, because no mote has a name on the CPU: they live in a buffer and never
come back. What is knowable is where the swarm holds, which is the ring round
each ship, and a fighter patrols inside that band on purpose. Attrition is
depth into the band, it mends once clear, and it comes apart when it runs out.
The squadron replaces losses one at a time on a timer, but launches WHOLE the
first time: pacing the first launch would mean half a minute before the wing
exists, which is a screen that arrives after the fight it was meant to screen.

**The ship dropdown despawns and respawns.** A ship here is its model, its
damage grid, its bricks, its materials, its turret children and its reactor,
and every one of those is derived from the class at spawn. There is no such
thing as changing the class of a hull that already exists, so it spawns a fresh
one through the same code the game starts with. The wing and the squadron go
with it, because an escort is a copy of the flagship's class and a fighter
flies off it.

## A turret is its own object, and it turns

A gun that swivels cannot be part of the mesh it is bolted to, so its cells are
lifted OUT of the hull's model before the bricks are built and given a child
entity pivoting on the cluster's own middle. `gun_clusters` returns what the
cluster walk already knew and used to throw away: the cells and the pivot.

**This is deliberately not redux-tribes' approach.** That project rewrites a
turret's quads inside the ship's own geometry every frame, for a reason that
does not apply here: its hulls carve holes through the same buffers, so a mount
with meshes of its own would mean the carve had to know which of four buffers a
quad lives in. Here a gun is three to a ship, nothing carves it, and a child
transform is free: the mesh is built once and only a rotation changes.

The aim is the SAME answer `fire_guns` uses, so the barrel and the beam agree.
A turret that pointed somewhere the beam did not come out of would be a
decoration rather than a gun. It eases on a slew cap and stands down to the
facing its own cluster looks out along when nothing is in reach.

Everything is in the HULL's frame: a child's rotation is relative to its
parent, so the target goes into that frame first and the rotation is then a
plain `looking_to` with no ship pose in it at all.

**And both companion queries need `Without<Turret>`.** Bevy proves two queries
disjoint from their FILTERS, not from what you know about the data: it cannot
tell that nothing is both a carrier and a turret, so a plain `&Transform` on
the carriers conflicts with the `&mut Transform` on the turrets and the app
panics at startup with B0001.

## A beam lasts a second, and that is what a sweep needs

`BEAM_TICKS` was nine, which is a flash: it lit, it killed whatever was on its
line at that instant, and it was gone before anything it set off could be
watched. Sixty is a second at sixty frames, and a sweep needs time to travel,
because the whole point of sweeping is the line of kills a player can follow.

That pushed straight through the shot cap. Seven ships with three guns each,
every beam alive for sixty ticks, plus the flak already running at two dozen
live bursts, goes well past thirty two, and what is past the cap is silently
TRUNCATED: a beam that draws and kills nothing. `MAX_SHOTS` is sixty four, and
the array in the shader is a hundred and twenty eight vectors, because the
capsules are stored in pairs.

## The swarm has a LIFE, and it is divided between ships

**One published centre was the wrong requirement, not a wrong implementation.**
The cloud chased `hull_centre`, a single position, so it could only ever be one
animal on one ship: calling in four reinforcements put five frigates on the map
and the swarm still sat on exactly one of them. "Position your ships to drive
or divide the swarm" is the premise of the whole game, and a single target
makes dividing it impossible to express at all.

Every live player hull is a target now (`SwarmConfig.targets`, capped at
`MAX_TARGETS`), and a mote picks one modulo the count from its own seed. That
makes the division STABLE, so a mote does not change its mind every tick, and
it means a ship dying shortens the list and its share of the cloud re-homes to
whatever is left. That is the rule the carriers already keep and it needs no
code of its own. Carriers are excluded, because they are hulls too now and a
swarm that attacked its own motherships would be a fight with one side in it.

**And a mote has four legs to its life rather than one.** It used to fly at the
ship for ever, with a clock flipping it between two standoffs, which is a cloud
and not an animal.

| leg | what it does |
| --- | --- |
| transit | crosses from its carrier to the ship it was given |
| circle | joins the RING round that ship |
| attack | leaves the ring, presses to contact, bites |
| return | goes home to its carrier, docks, and is put back together |

Which leg a mote is on decides exactly one number the steering reads: where it
wants to be. Everything else about the tick is unchanged, which is what keeps
four behaviours from being four code paths.

**A mote can be HURT now, and that is the only thing that sends one home.** A
shot takes `SHOT_BITE` off it instead of removing it: two hits kill and one
leaves it able to fly but not to fight, so at the end of a pass it breaks off
for its carrier, repairs, and comes back. That needed a fourth vector on the
mote (hit points, the leg, its timer and its place round the ring), which is
sixteen more bytes each and sixteen megabytes at a million. That is the price
of a mote that can be wounded rather than only alive or dead.

**A RING, not a shell, and the difference is one axis.** The swirl axis belongs
to the TARGET rather than to the mote: an axis per mote gives orbits at every
inclination, which is a sphere of traffic, fine for a cloud milling about and
not a formation. One axis per ship means every mote circling that ship goes
round the same way on the same plane, and two ships do not ring the same way
because the axis is hashed off which ship it is. A standoff alone would still
spread them over that sphere, so there is a term pulling each mote INTO its
ring's plane: that is what flattens the traffic into a ring somebody can see.

**And a mote comes apart.** On top of the spark burst it throws DEBRIS: bigger,
slower, longer lived and much dimmer, so what is left after the flash has gone
is pieces tumbling away rather than nothing at all. They are the same particles
as the flash and cost the same, which is the point: a mote that came apart into
real meshes would be nine thousand entities the moment a volley landed.

One WGSL trap on the way: `target` is a reserved keyword and the composer
refuses the name outright. The field is `ship`.

## Veins: a share of the swarm runs a ROUTE, and orbits were balls

A fifth of the motes do not go for the ship. They belong to a route between
two anchors, a rock and either another rock or the ship, and they ride a point
that slides along it.

**The first cut gave each of them its own standoff round one rock and its own
swirl axis, and that is a BALL.** A thousand orbits at every inclination is a
spherical shell by construction, which is the same mistake as the donut with
the axis freed instead of fixed: the swarm came out as a solid globe round
every asteroid. An ant does not orbit, it follows a path that other ants are
on. Every mote on a route is on the same line, which is what makes a line of
traffic, and the asteroid avoidance bends that line round anything standing in
it: a trail that weaves is a trail that met something.

It carries a TUBE rather than a line, a fixed offset per mote about the route,
so the traffic has a cross section a few motes wide instead of every one of
them trying to be at the same point. The route parameter lives in `state.w`,
which is the run clock for everything else: one field, two meanings, and
nothing reads the wrong one because `vein` is decided from the seed and never
changes. A vein gets no swirl at all, which is the term that spreads traffic
over a sphere: right for a cloud besieging a ship, wrong for a line of them
going somewhere.

## Three bugs that all looked like the swarm being out of sync

The report was that the cloud flew "in a different frame of reference" from the
world, that it flipped all at once, that it happened at certain camera angles,
and that it was not consistent. That is four symptoms of three separate causes,
and none of them was a sync problem.

**The mote shader read the mesh uniform at a hard coded index nought.** It
built its clip position with `mesh_position_local_to_clip(get_world_from_local(0u), ...)`.
`get_world_from_local` indexes Bevy's MESH INSTANCE buffer, which is built per
frame per view and holds every batched mesh in the scene. Slot nought is not
this entity: it is whichever mesh the batcher put first, and the batcher orders
by pipeline and by distance, so **the answer changes as the camera moves**. The
whole swarm was drawn in some other object's frame, every mote sharing the one
wrong matrix, so they all flipped together the moment the sort order changed.
Turning the camera to a certain angle is exactly what reorders the sort.

A mote position is already in world space, because the compute pass writes
world coordinates and the entity's transform is the identity, so there was
never a model matrix to look up. `view.clip_from_world` directly, which is what
the spark shader had been doing correctly the whole time. **The rule: if a
shader wants a matrix it is not using, that is not a spare argument, it is a
lookup that can be wrong.**

**The rock the swarm navigated was bigger than the rock that was drawn.**
`VoxelModel::radius` is the bounding sphere, measured to the furthest CORNER of
the furthest cell, so on a lump stretched half again on one axis it is set
entirely by that axis and stands well clear of the surface everywhere else.
Used as "how big is this", it put motes in orbit round a sphere with nothing in
it, which is what "they are orbiting nothing" was. `volume_radius` is the
radius of a sphere with the same VOLUME as the solid cells, which is the same
measure redux-tribes keeps beside its class table and for the same reason: a
radius that nothing links to the shape is a radius that disagrees with the
picture. Both are right answers to different questions, and the suite pins that
the volume one sits inside the bounding one and is not a token fraction of it.

**And there were two clocks.** The swarm's own clock clamped its step at a
twentieth of a second and every system on the CPU clamped at a quarter. On any
frame slower than fifty milliseconds the ship moved by the real elapsed time
and the cloud chasing it moved by at most a twentieth: the swarm fell behind
the world by the difference, every slow frame, and never caught up. Two clamps
is two clocks. `swarm::STEP_CLAMP` is the one number now, read by the swarm's
clock and by everything on the CPU that integrates anything.

## A guard that changed what it was guarding

The swarm settled into spheres with nothing inside them, sitting near the
asteroids but not on them, and it looked like a render offset. It was one line
of steering:

```wgsl
let inside = p.hull.w * 1.05 - dist;
if (inside > 0.0) { acc = acc - dir * inside * 40.0; }
```

`dist` and `dir` are relative to the mote's FOCUS. When every mote's focus was
the ship that line meant "do not fly through the hull" and was correct. The day
the focus became a variable, so a vein mote could aim at a point on a route,
the same line started meaning "do not approach your own destination": it pushed
a vein out to the SHIP's radius from wherever its route had reached, at up to a
hundred and forty against a pull of at most thirteen, so it could never get in.
Every vein settled onto a sphere three and a half units across, and the routes
run between rocks, so the spheres sat near the rocks and contained nothing.

It is measured against the hull now, explicitly, whatever the focus is. **A
guard written in terms of a variable that later gains a second meaning is a
guard that silently changes what it protects.** Nothing threw, nothing looked
wrong in the code, and the reviewer of that line would have had to remember
what `dir` had come to mean four screens further up.

The rock avoidance had the second half of the same shape. A radial push applied
OUTSIDE the surface is a force with nothing to spend itself on: it balances
against the pull toward the ship at some radius, and every mote that arrives is
held there, so traffic that was only meant to pass a rock built a standing
shell around it. Outside the surface the only correction now is to cancel the
part of the velocity going into the rock and keep the part going along it,
which is a mote sliding past an obstacle. A real push happens only once a mote
is actually inside, where there is something to be pushed out of.

## The camera jumped, and it was the input, not the camera

There is one camera and one system writes its transform. What threw it across
the map was how its input was read.

**Mouse motion was drained only while dragging.** `MessageReader` keeps
everything that arrived since the system last read it, and the drag loop only
consumed events while the button was held. Move the mouse across the desk with
the button up and the whole journey is waiting: it all applied on the first
frame of the next drag. Motion is cleared whenever a drag is not in progress,
including on the frame the button goes down.

**And a single event's delta is clamped.** The window handing back focus, the
pointer leaving and re-entering, or a compositor releasing a grab all deliver
one event carrying thousands of pixels. At 0.005 radians a pixel that is
several whole turns inside one frame, and it happens at the EDGE of the screen,
which is why it seemed to depend on which way you had turned.

**The wheel could make the distance negative.** It was `dist * (1.0 - y * 0.08)`,
and a wheel reporting PIXELS rather than lines hands over a y of a hundred or
more per notch, so the factor came out at minus seven, the distance went
negative and the clamp slammed the camera to its near stop. One notch the other
way and it slammed to the far one; a trackpad does this on every scroll. Zoom
goes through `exp` now, which cannot return a negative number however big the
input is, and the unit is read off the event rather than assumed.

**The pointer over a button belongs to the button.** Bevy's UI does not consume
the raw mouse, so pressing Call reinforcements dragged the camera at the same
time: every click on the HUD threw the view sideways.

**And the two systems had no order between them.** `orbit_input` and
`orbit_camera` were registered in separate `add_systems` calls, so Bevy was
free to run them either way round and could pick differently from one frame to
the next: a drag arrived a frame late on some frames and not others, which is
jitter that looks like a second camera fighting the first. There is only ever
one camera; there were two possible orders.

Nothing leaves `orbit_input` as a NaN either. Every expression is guarded where
it could go wrong, so the check can only ever be redundant, and it is there
because a NaN in the camera is not a wrong picture, it is every picture wrong
from now on: the bad value is stored and fed back in next frame.

## A frame counter, and a menu to turn it off

The counter is in the opposite corner from the controls so it never sits over
anything a player has to press, and it reads off `Time<Real>`.

**Not `Time`.** The virtual clock's delta is clamped at 250 ms so one stalled
frame cannot fling everything forward, which means a frame slower than that
reports as 250 ms however long it really took. That is the exact trap the frame
cap fell into once already, written up further down this file, and a counter
built on it would read a floor of four frames a second however bad things got.
It shows the frame TIME beside the rate, because a rate alone cannot be held
against a budget: 16.7 ms is a number somebody can compare to 60, and "59 fps"
is not.

Escape opens the pause menu, which carries the toggle and Resume. The menu sets
`SwarmConfig.paused` as well as its own flag, because the swarm lives in the
render world on the other side of an extract and does not see the HUD's
resource. Everything that MOVES is gated behind a run condition and everything
that only DRAWS keeps running: the beams, the nav disc and the flames are
rebuilt every frame from state, so skipping them empties their meshes and the
picture goes blank behind a menu that says Paused.

The overlay is hidden with `Display::None` and not `Visibility::Hidden`,
because a hidden node is still laid out and still picked: an invisible Resume
button would have gone on swallowing clicks in the middle of the screen the
whole time the game was running.

## The controls are on the screen

There were none. Every binding was a key somebody had to be told about, so
"where is my button to call in reinforcements" is the only question a player
could have had. A button that is only a key is a feature nobody can find, which
is redux-tribes' own lesson about move mode being buried in a rail.

The HUD is a button that calls the wave, a line saying what the wing is at, and
the bindings. The button and the R key are the same action asked for twice. It
is built only for a window: a headless run has no pointer, nobody to read a
label, and pays for every frame of it on a software rasteriser.

## There is no fog, and what looked like fog was the lighting

Nothing in this renderer fades with range. No shader reads a depth, there is no
fog component anywhere, and the far plane clips rather than blending. What read
as distance fog was two lights.

- **A green cubemap at 250 puts the sky's own colour on every surface in the
  scene.** A rock forty units out and the nebula behind it came out nearly the
  same green, so the rock's contrast against its background went to almost
  nothing. That is what aerial perspective IS, arrived at from the other
  direction, and it is exactly the thing space does not have: there is no
  medium between the camera and the rock. The environment light is 90 now.
- **A flat ambient is a grey floor under the whole picture.** An ambient term
  stands in for light bounced off air and ground and there is neither out here.
  It is 6 rather than 40, and not nought: zero puts a hull's shadowed flank at
  the same value as the gap between two stars, and a silhouette with no
  interior is a hole in the picture rather than a ship.

The general rule, and it is the one this file already keeps twice: **a picture
that looks wrong names a symptom, not a cause.** "Distance fog" is a real
effect with a real implementation, and looking for that implementation would
have found nothing for as long as anybody cared to look.

**The same trap caught the motherships.** They were "objectively glowing
green", and there was no green lamp: `tint_hives` set the material's EMISSIVE
to a green that grew with damage, and emissive applies over the whole material
rather than over the places that were hit. At 2.2 it did not mark a wound, it
turned the entire hull into a uniform green bulb the shape of a mothership. It
is a tenth of that now, and what actually says a carrier is hurt is that it
BURNS: sparks off its hull, thicker the worse it is, the same thing a chewed
frigate does. A fire has a place on the ship and a tint does not, which is why
one of them carries the information and the other only carried colour.

## An RTS camera: the focus is a place

It used to ease its focus onto the flagship every frame, which is a chase
camera wearing an orbit's controls, and two things are wrong with that in a
game about where you put your ships. You cannot look at anything except the
ship, so the swarm, the carriers and the rocks can only be seen by flying to
them. And the moment you give a move order the whole world slides under you,
which is the ship standing still and everything else moving, exactly backwards
from what an order is.

So the focus is a point in the world the player drives. WASD and the arrows pan
it, Q and E lift and drop it, because this is a game in three dimensions and a
camera that could only pan on one plane could not be put above a fight that is
happening at an angle to it. Panning is in the CAMERA's frame, not the world's,
or the keys mean something different at every heading and nobody can learn
them, and it is scaled by the camera's own distance so one press covers the
same share of the screen at every zoom.

Space snaps to the flagship, eased, and then **lets go**: a focus is a move to
a place rather than a lock, so the ship flies out of the middle of the view
under its own power, which is what says it is going somewhere. Any pan clears
it, because a focus that fought the pan keys would be a camera arguing with its
own user.

## Engines burn, and a flame is GEOMETRY

**A drive plumes on the throttle it is actually pulling.** `Hull.accel` is
last frame's change in velocity, and `throttle_of` resolves it against the
hull's forward axis: the main engines burn on acceleration over a low idle,
and the retros, which are propulsion cells forward of the middle, burn only on
deceleration. So a ship slowing into an arrival lights the guns at its bow
rather than the bells at its stern, which is what a Homeworld capital ship
does and what nothing else in the picture says is happening.

That needed the heading fixed first, and the bug is worth keeping. Bevy's
`forward` is `-Z` and `looking_to` aims THAT at whatever direction it is
given; a hull's bow is `+Z`, because the lattice runs stern to bow. Aimed
straight, the ship flew stern first with its main drives leading, so the
throttle logic was correct and the picture was a retro at full burn while
accelerating. `looking_to(-vel, Y)` is the fix, and the lesson is that an
engine's own axis convention is a fact to look up, not to assume.

**A flame is a stack of faceted frustums, and three things make it read.**

- **Flat facets, which means unshared vertices.** The first cut was a ring of
  six vertices fanning to a tip, with a little colour variation per vertex to
  suggest facets. A shared vertex is a colour the rasteriser interpolates
  ACROSS the edge between two facets, so every hard line in the mesh came out
  as a smooth ramp: the cone read as a horn of smoke. Three vertices per
  triangle, all of them their band's own colour, is flat shading, and flat
  shading is the whole of the look.
- **Bands, not a gradient.** `FLAME_BANDS` is four stations, so three bands,
  each one flat colour across its whole width, stepping at the ring. A player
  can point at three parts of a flame; nobody can point at a part of a ramp.
- **BACK FACE CULLED, which the nav disc is not.** Additive on a closed
  surface lays its colour down twice per ray, once on the way in and once on
  the way out, so the silhouette and the middle arrive the same brightness and
  the shape dissolves. This is redux-tribes' own reach shell lesson on a
  different mesh. Culled, a ray crosses one facet and that facet's flat colour
  is what arrives.

And the values came DOWN. `NeutralToneMapping` desaturates a highlight by
scaling every channel by `newPeak / peak`, so a flame authored at 7.0 red
against 1.5 blue is a flame that arrives white with a bloom halo round it,
which is the same pale plume by another route. The body is 3.0 red now and
only the nested core is allowed over four, which is what leaves a white hot
centre inside an orange jet instead of one white smear.

Carriers and the ship use the same builder, on the same engine cells: a
carrier's drives come off `engines_of` exactly as a frigate's do, because an
alien's propulsion cells carry `purpose::PROPULSION` too. The motes do NOT:
there are a million of them, so a fighter's glow is a term in `mote.wgsl`
scaled by its own speed, and geometry is for the dozens.

## A wing: the flagship, and what keeps station on it

**R calls in a wave.** Two more of the flagship's class per press, up to
`WING_MAX` of six, spawned well outside the formation and flying in past the
camera. `--reinforce N` does it at startup so a headless run can photograph
one.

**One hull became several, and `single()` is where that hurts.** Three
systems wanted THE ship rather than A ship: the nav disc, the camera and the
swarm's target. With one hull `single()` was right; with two it returns an
error and every one of them silently became "do nothing at all", which is a
ship that cannot be steered and a camera that stops following. A `Flagship`
marker is what those three ask for now, and `publish_hull` asks for it too
rather than iterating and keeping the last, which would have pointed the whole
swarm at whichever escort the query happened to yield last.

**The formation target is a RESOURCE, because Bevy will not lend it twice.**
`fly_hull` holds every hull's `Transform` mutably, so it cannot also read the
flagship's: the same component in the same system is refused. `Lead` carries
the flagship's pose and velocity, published a frame behind, which a formation
cannot see. A ship a sixtieth of a second stale is a ship a centimetre out of
place.

**An escort has no order, so its goal is never reached.** A station is an
offset in the FLAGSHIP's own frame, so the formation turns with the ship it is
flying beside instead of sliding round it, and the goal moves every frame.
Station keeping is the leader's velocity plus a steering term: steering alone
would leave an escort permanently behind by however far it takes to close the
gap. A reinforcement also ARRIVES, at three times cruise easing back over the
last eight lengths, because a capital ship's cruise would take a minute to
cross the gap it is called in over.

**And a wave is not free.** The GPU swarm knows one hull centre and chases the
flagship alone, so an escort with no chewers of its own is a ship that adds
guns and can never be hurt. An escort carries a third of the flagship's, which
is what makes calling one a decision.

**Two ships of a class have the same cells.** Every phase hashed off a cell
came out identical on every hull in the wing, so four frigates fired in one
volley, on the same tick, for ever, and their flames flickered in lockstep.
`Hull.seed` is per ship and is mixed into all of them.

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

**The ship shoots at two things and they are two weapons.** A BEAM goes for
the nearest carrier the gun can bear on, slowly, at long range, with a little
spread so some shots miss; a beam that reaches a carrier damages it and is CUT
at the hit point, which is redux-tribes' rule that the full range endpoint is
what a MISS looks like, and which also makes a carrier cover for the fighters
behind it. FLAK is point defence: a burst every few ticks out at the standoff
the swarm holds, walking round the hull.

A flak burst needed no new kind of anything, and that is the payoff of
resolving a shot as a volume: it is a `Blast`, which is a capsule of zero
length, which is the shape the shot path already carried. The shader kills
whatever is inside it and the CPU never learns where a mote was.

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

**A drone's stern is ONE light, inset.** It was three calls and six cells, two
of them `DRIVE_HOT`, the near white; against a swarm that shades itself that
made every bug a bank of headlamps seen from behind, and the darker the bodies
round it got the more a mote read as its own exhaust. What is left is the
centreline cell a row BELOW the middle, and it is WALLED IN: `shroud` fills
whatever is empty on the five faces that are not the aft one, so exactly one
face of the light is open and it is the one pointing the way the mote came
from. That is the whole of the difference between a lamp and an exhaust, and
it was not free by construction: a drive is the aftmost cell of its column, so
it is open aft AND open wherever the body's ellipsoid stopped short, which
measured two faces on a good seed and three on most.
`a_drones_drive_shows_one_face_and_it_faces_aft` holds it per seed, because
which side the ellipsoid falls short on moves with the radius it rolled.
It is two cells wide rather than one and that is the mirror and not a choice,
since anything on the centreline is a pair by construction; they touch, so
they read as one. The lancer and the chewer keep their hot drives, because
neither of them is what a hundred thousand of are on screen at once.

`GLOW`, which is what everything ALIVE about an alien is lit with, came down
from 0x9BFF4A to 0x6FB835 for the same reason. At the emissive a lit cell
carries it was a lamp rather than an eye once the shading round it went dark.

Their skin is `alien_chitin_n.png`, from `tools/make_chitin_texture.py` on
the same `texkit` the finishes use: scales overlapping like roof tiles, each
a dome with a crease at its root, wrinkles across them and pores sunk in, no
`sin` anywhere so `--check` holds on another machine. The showcase wears it
through `StandardMaterial`; the instanced motes wear it through their own
shader, which carries the mesh tangents at location 4 and a texture bind
group at 3, and that is why the instance attributes moved to 8 and 9: they
were at 3 and 4, which is fine for a cube and collides with the tangents the
moment the mesh has any.

**The motes are lit by the SUN, harshly, and their own shader does it.** A
mote is not a `StandardMaterial`: a million of them are one instanced draw
that binds the view and the chitin, so `mote.wgsl` lights them itself, off the
scene's own directional light: `lights` is binding one of the view bind group
that draw already binds, so a mote's lit side is the same side as a hull's
without a copy of the sun vector anywhere in the shader. The floor under it was cut
by sixty percent (0.22 to 0.088 ambient, 0.15 to 0.06 back fill) with the
direct term at a full one, and the scene's `AmbientLight` took the same cut
(6.0 to 2.4): a sun in vacuum makes a hard terminator and a nearly black far
side, and the grey lift both had before read as fog over the cloud. The
drive glow came DOWN at the same time, from 1.1 to 4.3 to 0.5 to 1.9 by
speed: at the old numbers every drive in the cloud was over the bloom
threshold at any speed, so a million bloom sources were the green haze the
swarm read as. A fighter at rest is an ember below the threshold now and only
one at full transit crosses it.

It is tiled at HALF the rate of a finish, a scale spanning two cells, in the
material's `uv_transform` and in the mote shader alike. At one scale a cell,
on a body a few cells across, the map was bound and loaded and read as grain:
the close up (`--target -1.4,-2.3,3.9 --zoom 1.05`, which is where the
showcase stands) is the check, because a normal map that reads as nothing is
the same failure as one that never loaded.

## The Long Retreat: a run is systems, and a system is a clock

The skirmish is a fight you win by killing every carrier. A RUN is not that.
You arrive in a system with a fleet and a bank, you have about six minutes
before the swarm's fleet does, and the way out is the jump drive: what you
take with you is what was standing inside the field when it went, and what
you leave is gone. Killing carriers is worth doing and is never a victory,
because the tide brings more.

**The rules are the core's and the ships are the app's.** `economy::yield_of`
says what a cell is worth when it comes off something, `tide` says how much
of the swarm is out at tick N, `map` is a run as a branching chain of systems
from a seed, and none of the three knows what a Bevy is. The app is what
flies the miner at the rock.

**A ROCK IS A HULL, and that is the whole of the mining model.** An asteroid
was always a voxel model with a damage grid; it just never went through the
ship builder, so it had no bricks and nothing could take a cell off it. It
goes through `spawn_ship` now and is born `inert`, which is `dead_hull` from
the first frame, so everything that already skips a dead hull skips it: the
swarm's targets, the guns, the bars, the orders, the chewers and the reactor
rule. That is most of the game for one flag, and it is why a miner's shaft
looks exactly like a chewed frigate. One damage pipeline, one four layer
wound, one re-mesh of only the bricks that changed.

`ShipSpec` is what made that affordable. `spawn_ship` was nine positional
arguments under an `allow(too_many_arguments)`, which this file calls the
smell that says a struct is missing, and a rock needed two more. `HiveSeat`
is the same answer for a carrier's place in the spiral, and both exist
because the day a rock needed to be a ship was the day the argument list had
to be paid for.

**A cut is the slug's bored column with a different thing on the end of it.**
`DamageGrid::bore` serves the range's slug, a miner's shaft and a salvager's
cut; `economy::yield_of` reads the cell that came off and answers what it was
worth. Only a seam cell is worth anything on a rock (ore to materials, ice to
volatiles, and a barren rock has none), and on a hull it is the machinery
that is worth DATA and the plating that is worth materials. A hold counts
every cell cut, spoil included, because a hold holds ROCK: that is what makes
the trip home a real cost and what the freighter is for.

**Where a cutter stands is written in the AVOIDANCE's terms.** `fly_hull`
holds every ship off a rock by the rock's own navigation sphere plus its own
radii, so a standoff written against the rock ALONE is a standoff the flight
rule can refuse, and then a miner hovers just outside its own reach for ever
with nothing in the log to say so. It was measured against the rock's
bounding radius first and worked by luck of two numbers.

**A shaft that breaks through makes the miner walk ROUND.** A rock is done
when it has nothing left in it, not when one face of it is used up. The first
cut of this went home the moment a bore came out the far side and left three
quarters of the field's ice in the ground, and what caught it was the run
report rather than the picture: a still frame of a rock cannot tell a miner
that gave up from a miner that is still working.

**The tide is three phases and the last one is the one you should have left
before.** Probes for two minutes (one carrier, far off), the swarm for four,
then the fleet, all at once. `carriers_at` is monotone and the app spawns the
difference, comparing against how many have EVER been spawned rather than how
many are alive: killing a carrier does not bring another, which is the whole
reason to shoot at them while you gather.

**The drive spools for thirty seconds and takes what is inside the field.**
Everything outside is left in the system, and what went is the fleet the next
system starts with. The fleet is despawned on the way out, because the result
screen sits over the field and a fleet still standing in it under the word
JUMPED is the one picture this must not leave behind.

**Fuel is not a second pile of materials.** A miner cuts ICE, and the tanker
turns one cell of it into one unit of fuel every half second, so a full tank
is half a minute of refining AFTER the ice is landed and mining the ice early
is worth doing. No tanker, no refining: a hold of volatiles with nothing to
process it is a hold of rock. It is a cadence and not a rate times the
frame's step, and that is a bug this had: at a fiftieth of a cell a frame,
`volatiles -= take.floor()` took nothing for ever while the fuel went up
anyway, so the tank filled itself out of a pile of ice that never shrank. A
rate in a resource counted in whole cells needs somewhere to keep the
remainder. A cadence needs none.

**The flagship carries its holes.** The field is built fresh every system, so
without that a run would arrive in a pristine ship however the last one went,
and nothing that happened in a system would cost anything. `scars_of` reads
the dead cells off the damage grid when it jumps and `apply_scars` puts them
back on the hull it arrives in, which needed one thing in the core: a scar is
dead and COLD. `heat_of` measures from a tick in THIS action and there is no
tick before nought, so a wound carried between systems and killed at tick
nought arrives white hot and burns for fifteen seconds. `DamageGrid::scar` is
a sentinel in `died_at` rather than an old number, and everything downstream
sees an ordinary dead cell: the four layer wound, the bricks, the vents and
the mesher are untouched.

**A run ends with the COMMAND SHIP, not with the last hull flying.** A
retreat whose flagship is a wreck is over however many miners are still
cutting, because the drive that carries them out was in it. And `judge`
stands down entirely once the drive has gone: the frame after a jump sees no
player hulls at all and called it a defeat, over the top of what the jump had
already written.

**Between two systems is one screen, because the two halves are one
decision.** What a node is worth depends on what you can afford to meet in
it, and what is worth buying depends on where you are going. The fleet on the
left, the yard in the middle, the branches on the right. Materials buy HULLS
(another escort, another support ship, the scars welded shut, by the cell);
data buys the RIGHT to buy (a role the yard has never built, or the next rung
of the flagship's own navy). That is the design's "equipment that unlocks
more research" said the one way that costs nothing to implement, and the
ladder is read off the fleet manifest rather than typed, so a class added
tomorrow is on it tomorrow. A row the bank cannot afford is DIM rather than
absent: what you could have had if you had mined one more rock is the
information the screen exists to give.

A purchase rebuilds the screen by hand rather than through the state machine,
because a yard whose prices and roster were painted once would go on offering
what it has already sold, and Bevy does not run `OnEnter` for a transition to
the state it is already in.

**The harness is two flags for the two things a player presses**, since a
headless run has no pointer: `--job TICK` is the right click that puts every
support ship to work, and `--jump TICK` is the button, with the fuel check
skipped because half an hour of mining is not a thing a headless run can
afford to render. `--onward` takes the map's first branch without being
pressed, so the system AFTER a jump can be photographed, which is the only
place a run's scars and roster show. The scripted jump arms whenever the
drive is idle rather than on tick nought, because the clock is advanced ahead
of it in the same frame and tick nought is never a tick anything there sees.

**And the report says what the loop DID.** `retreat:` counts the ore and the
ice left in the field and what is in the bank, because a picture of a rock
cannot tell a miner that arrived from a miner that never did, and the first
three runs of this were three different reasons for a bank that stayed at
nought.

Measured, headless, one miner on a field of eight rocks: the shaft reaches
the surface at tick 240, the hold fills at 156 cells, the miner unloads 27
volatiles at tick 860, and the ice in that rock goes 83 to 59. A run played
straight through with `--onward` crosses three systems in 420 frames and the
flagship arrives in the third with 257 cells still open.

## Suites

```sh
cargo test -p swarm_core                                   # 83, the core
cargo test -p swarm_app                                    # 5, the run's own economy
python3 tools/shape.py --check                             # no file over 900 lines, no function over 100
cargo fmt --all -- --check                                 # the format
cargo clippy -p swarm_core -- -D warnings                  # the core's lints
python3 tools/pngdiff.py before.png after.png --grid       # a refactor's pictures, against the scene's own floor
python3 tools/make_chitin_texture.py --check               # the chitin has not drifted
cargo run --release -p swarm_core --example hull_stats -- assets/hulls   # what makes a hull tough
cargo run --release -p swarm_core --example rock_stats                   # what a rock is worth, and how buried its ore is
cargo build --release -p swarm_app
./target/release/swarm_app --headless --motes 5000 --frames 60 --out shot.png
# A headless run defaults the launch delay to NOUGHT and a window defaults it
# to ten. The opening beat before the swarm arrives is for a player; a harness
# that waited ten seconds for its subject would spend every check rendering an
# empty sky. `--launch-delay N` sets it either way.
# --yaw and --pitch take the SAME tick from another angle, which is the only
# way to answer "it looks wrong at some angles": hold everything still and
# turn the camera.
./target/release/swarm_app --fps 60          # a window, capped; --fps 0 lifts the cap
./target/release/swarm_app --headless --motes 1 --chewers 0 --zoom 1.7 --out close.png
# The effects, aimed: one frame is one tick, so a shot can be taken AT a tick.
./target/release/swarm_app --headless --fixed-dt --motes 1 --chewers 80 --cadence 0 \
    --frames 260 --zoom 1.35 --out wound.png            # a hull burning
./target/release/swarm_app --headless --fixed-dt --motes 2600 --cadence 5 \
    --frames 140 --zoom 3.4 --out beams.png             # guns into the swarm
./target/release/swarm_app --headless --fixed-dt --motes 900 --explode 90 \
    --frames 93 --zoom 6.0 --out boom.png               # three ticks after the reactor
./target/release/swarm_app --headless --fixed-dt --motes 900 --explode 90 \
    --frames 300 --zoom 5.5 --out wreck.png             # the wreck, three and a half seconds on
./target/release/swarm_app --headless --fixed-dt --motes 800 --reinforce 4 \
    --move 30,3,-16 --chewers 0 --frames 320 --zoom 7 --out wing.png   # a wing on station
./target/release/swarm_app --headless --fixed-dt --motes 800 --chewers 0 --hud \
    --aim 40,14,-30 --frames 30 --zoom 30 --out order.png              # a move order being given
./target/release/swarm_app --headless --fixed-dt --motes 3000 --hives 3 --rocks 14 \
    --chewers 0 --frames 150 --zoom 5 --out battle.png     # the whole thing
# The screens, and the range: a scripted slug lands at tick thirty.
./target/release/swarm_app --headless --fixed-dt --motes 200 --frames 12 --screen menu --out menu.png
./target/release/swarm_app --headless --fixed-dt --motes 200 --frames 12 --screen setup --out setup.png
./target/release/swarm_app --headless --fixed-dt --motes 200 --frames 12 --screen result --out result.png
./target/release/swarm_app --headless --fixed-dt --sandbox --hud --fire slug,30 --motes 600 \
    --hives 2 --rocks 6 --chewers 0 --frames 150 --zoom 5 --out sandbox.png   # the range, two seconds on
# The Long Retreat. `--job` is the right click and `--jump` is the button,
# since a headless run has no pointer, and `--onward` takes the map's first
# branch so the system after a jump can be photographed.
./target/release/swarm_app --headless --fixed-dt --screen map --motes 200 --frames 12 --out map.png
./target/release/swarm_app --headless --fixed-dt --retreat --hud --job 40 --motes 600 \
    --frames 560 --target 2.2,7.1,-25.4 --zoom 2.6 --out mining.png   # a shaft being cut
./target/release/swarm_app --headless --fixed-dt --retreat --jump 200 --motes 900 \
    --chewers 90 --frames 240 --zoom 8 --out jump.png                 # the system, left
./target/release/swarm_app --headless --fixed-dt --retreat --jump 200 --onward --motes 900 \
    --chewers 90 --frames 420 --zoom 3.2 --out second.png             # and the scars it carried
for y in 0.0 1.6 3.1; do                                   # the same tick, three angles
  ./target/release/swarm_app --headless --fixed-dt --motes 4000 --frames 150 \
      --zoom 11 --yaw $y --out ang_$y.png
done
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

## A frame cap, and why the numbers before it were wrong

`--fps N` caps the frame loop, defaulting to 120; nought lifts it. Vsync is
not a cap, it is the MONITOR's cap: on a 144 or 240 hertz panel a scene this
cheap to simulate draws at the refresh rate and holds the GPU at full clock
the whole time, which is a hot room for frames nobody asked for.

It is a DEADLINE rather than a fixed sleep, so the cap does not drift, and a
frame that has already overrun resyncs the deadline to now rather than chasing
it: catching up means running the next few flat out, which is the thing the
cap exists to prevent. `sleep` and not a spin, because a spin paces better and
burns a core doing it, and burning a core is the problem.

**And it looked broken when it was working, which is the part worth keeping.**
Capped at two frames a second the report still said four. `Time` in Bevy
clamps the virtual delta at 250 ms so one stalled frame cannot make everything
jump, so a frame SLOWER than that is reported as 250 ms however long it really
took. The headless report was a sum of those deltas, which means it quietly
stopped counting at four frames a second, which is exactly the range a
software rasteriser lives in.

So every frame time this file recorded before the fix is a LOWER BOUND wherever
it approaches 250 ms, and the ones that sat just under it were the clamp rather
than a measurement. Re-measured off an `Instant`: the nine thousand mote scene
is **479 ms a frame**, not the 239.8 the delta sum reported. Exactly double,
because most of its frames were over the clamp.

The rule that comes out of it: **time a thing with a clock, not with the
engine's idea of how long a frame was allowed to be.** An engine's delta is
shaped for the simulation that reads it, and every shaping is a lie to a
harness.

## Measure, then decide

Numbers in the commit message. `wgpu` compute on this container, before any
of this was written: 1M motes at 8 stride sampled neighbours, 35.42 ms a tick
on four cores of llvmpipe, linear in the count and doubling from 8 neighbours
to 24. That is the arithmetic the whole design rests on and it was measured
before a line of it existed.
