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

**Moving the ship is Homeworld's own shape.** The right button opens an order:
the cursor picks a point on the horizontal plane through the ship, and holding
shift lifts the target off that plane and draws the line back down to it,
which is what makes a flat screen able to name a place in three dimensions at
all. Release commits. `fly_hull` is a real envelope, not a lerp: it
accelerates, it has a top speed, it slows into the arrival and it turns to
face the way it is going, so an order to a capital ship has weight. Left drag
is the camera, which now follows the ship on `1 - exp(-k dt)` so the ease
takes the same wall time at any frame rate.

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

## Veins: a share of the swarm winds round the rocks

A fifth of the motes do not go for the ship. They take an asteroid as their
focus instead, at a standoff of a little over its own radius, and they do not
make runs: they HOLD, and they run fast along their own orbit.

It is one hash and one select, and nothing else in the tick knows which kind a
mote is. That is the point: a vein is not a second behaviour, it is the same
behaviour pointed at something else. Combined with the per mote swirl axis it
draws a braid winding round the rock rather than a shell round it, because a
few hundred near circular orbits at every inclination is what a braid is.

Two things had to be told about it. A vein's OWN rock pushes it far less than
the others do, or the avoidance term would shove the ribbon straight off the
thing it is wound round, since the standoff is inside the margin by design. And
a vein does not bite the hull, because it is nowhere near it.

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
cargo test -p swarm_core                                   # 51, the core
python3 tools/make_chitin_texture.py --check               # the chitin has not drifted
cargo build --release -p swarm_app
./target/release/swarm_app --headless --motes 5000 --frames 60 --launch-delay 0 --out shot.png
# --launch-delay 0 on every headless shot that wants a swarm in it: the
# carriers hold for ten seconds by default, and a render aimed at tick ninety
# cannot wait for that.
./target/release/swarm_app --fps 60          # a window, capped; --fps 0 lifts the cap
./target/release/swarm_app --headless --motes 1 --chewers 0 --zoom 1.7 --out close.png
# The effects, aimed: one frame is one tick, so a shot can be taken AT a tick.
./target/release/swarm_app --headless --fixed-dt --motes 1 --chewers 80 --cadence 0 \
    --frames 260 --zoom 1.35 --out wound.png            # a hull burning
./target/release/swarm_app --headless --fixed-dt --motes 2600 --cadence 5 \
    --frames 140 --zoom 3.4 --out beams.png             # guns into the swarm
./target/release/swarm_app --headless --fixed-dt --motes 900 --explode 90 \
    --frames 93 --zoom 6.0 --out boom.png               # three ticks after the reactor
./target/release/swarm_app --headless --fixed-dt --motes 800 --reinforce 4 \
    --move 30,3,-16 --chewers 0 --frames 320 --zoom 7 --out wing.png   # a wing on station
./target/release/swarm_app --headless --fixed-dt --motes 3000 --hives 3 --rocks 14 \
    --launch-delay 0 --chewers 0 --frames 150 --zoom 5 --out battle.png  # the whole thing
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
