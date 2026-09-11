# Menus, a campaign, subsystems, a playground, and a swarm that glows

Status: PROPOSAL. Nothing in here is built. Every section ends with what it
would take and what has to be decided; the build follows the approved list
and nothing else.

The six asks, in the order they were given:

1. A main menu, and a skirmish setup: how many alien motherships against
   which ships on your side.
2. Campaign scenarios, starting with a first encounter that is one ship
   against one carrier and escalating to more swarm.
3. Why do the Karisen ships seem stronger, when they should share the Terran
   frigate's hull values?
4. A playable ship: every subsystem can be destroyed, no engines means no
   moving, no guns means no shooting.
5. Playground missions, starting with freezing the swarm so a beam's carve
   through it can be seen.
6. A damaged swarm creature glows hot yellow and pops when it has had enough:
   explosives and projectiles kill outright, beams wound.

## 1. The Karisen question, measured first

The rules are identical for every class. A cell's hit points come from what it
is MADE OF (`hp_for`: plate 100, frame 60, casing 50, machinery 40, a glow
cell 30) times `ARMOUR`, which is 1.0 for every player hull; the reactor is
derived the same way on every hull (`reactor_of`, the ball around the most
buried cell) and half of it dead is what kills the ship (`REACTOR_LOSS`); the
chewers on a flagship are `--chewers` (48) whatever its class. Nothing in the
tables says "Karisen".

`cargo run --release -p swarm_core --example hull_stats` reads the answer off
the hulls themselves:

| hull | cells | plate | hit points | guns | engine clusters | reactor cells | plating over the reactor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| terran_frigate | 8938 | 7085 | 791k | 3 | 8 | 176 | 2 cells |
| karisen_frigate | 7029 | 5366 | 611k | 3 | 6 | 174 | 2 cells |
| terran_destroyer | 9688 | 6818 | 810k | 5 | 6 | 176 | 2 |
| karisen_destroyer | 7312 | 5131 | 612k | 4 | 5 | 177 | 1 |
| terran_cruiser | 11993 | 7901 | 972k | 8 | 6 | 179 | 2 |
| karisen_cruiser | 7876 | 5224 | 641k | 6 | 6 | 176 | 3 |

A Karisen frigate has 21 percent fewer cells and 23 percent less total hit
points than the Terran, the same three guns, and a reactor of the same size
under the same two courses of plating. By the numbers it is the WEAKER ship.
Under an identical chew, headless, both frigates with 120 chewers and no guns
firing:

| hull | chewers | ticks | cells eaten | share of the hull | went critical |
| --- | ---: | ---: | ---: | ---: | --- |
| terran_frigate | 120 | 1500 | 2622 | 29% | no |
| karisen_frigate | 120 | 1500 | 2612 | 37% | no |

The chewers eat the SAME number of cells a tick on either hull (a bite is a
bite), so the smaller ship loses a bigger share of itself in the same time:
the Karisen frigate is a third gone where the Terran is under a third. Neither
reached its reactor in twenty five seconds, which says how long a siege is at
this chew rate and nothing about a difference between the two. If a Karisen
seemed to last longer in play, it was not its hull values.

What a player would feel as "stronger" is not in the tables and is worth
knowing anyway:

- **Where the chewers land.** A chewer stands on a random exposed cell and
  drills straight in. The Terran is a wide flat section, so its reactor sits
  two cells under a deck twelve cells across and a chewer on the deck is over
  the core; the Karisen is a diamond on a keel edge, so most of its skin is
  flank, and a drill from a flank runs the long way to the centreline. Same
  rule, different geometry, and that is the redux-tribes premise (the layout
  IS the damage model) doing exactly what it should.
- **The same 48 chewers on any hull.** A chewer count is per ship rather than
  per cell, so a small hull is eaten faster relative to its size. If a class
  should be worth more chewers, that is the swarm's rule to change, not the
  hull's.
- **A real defect the table exposed.** The reactor is meant to be buried and
  the suite proves it on a synthetic block, but on the stock hulls its
  shallowest cell sits ONE to THREE cells under outside space, and on both
  corvettes it touches it: `reactor_of` measures depth against every empty
  cell including the voids INSIDE a hull between its frame and its parts, so
  "most buried" is buried from a corridor, and the ball around that cell
  reaches the skin. The fix is to measure depth from outside space only (a
  flood fill from the lattice wall, as the mesher's outside test already
  does) and to keep the ball inside a minimum depth; the test then runs on
  the real hulls rather than a block. This is in the build list below because
  it changes how every fight ends.

## 2. Main menu and skirmish setup

The app boots straight into a fight with whatever the command line said, and
the only way to fight something else is to restart it. That was right for a
harness. A game has a front door.

**States.** `AppState`: `Menu`, `Setup`, `Playing`, `Result`. Bevy's `States`
and `OnEnter` / `OnExit` build and tear down each screen's UI; the fight is
spawned on entering `Playing` from the `Scene` resource, which already IS the
whole specification of a battle (hull, chewers, hives, motes, rocks, fighters,
launch delay). The setup screen writes `Scene`; the command line still writes
it directly, so every headless render works unchanged.

**Main menu**: Campaign, Skirmish, Playground, Quit. The settings the pause
menu already carries (frame counter, bars) stay there.

**Skirmish setup**, one screen, two columns:

| the enemy | your side |
| --- | --- |
| motherships: 1 to 10 (default 3) | flagship: any of the 23 hulls, grouped by navy and rung |
| swarm: small, medium, large (20k, 100k, 300k motes) | wing: 0 to 6 escorts of the flagship's class |
| launch delay: 0 to 20 seconds | fighters: 0 to 24 |
| carriers stand: close, normal, far (`HIVE_NEAR` / `HIVE_FAR`) | |
| the field: asteroids 0 to 30, and a seed | |

Launch spawns exactly what the screen says. Escape from Setup goes back to
the menu.

**A result**, because a win has no screen today: "an empty hive list is the
win condition and needs no rule" is true of the swarm and false of the game.
`Playing` ends when every carrier is a wreck (win) or every player hull is
(loss), wrecks not counting as ships; the `Result` screen says which, how long
it took, and what it cost (ships lost, cells lost). Motes killed would need
one number read back from the GPU once a second: a count, not a position, so
it does not break the rule that nothing about a mote comes back to the CPU. It
is optional and listed as such.

Cost: a session for the states and the two screens, in the HUD's own style,
with the ship list built from `assets/hulls/manifest.json` rather than typed.

## 3. Campaign

Scenarios are DATA, one table in `scenario.rs`: name, a short briefing, the
player's fleet (flagship class, escorts, fighters), motherships, motes, rocks,
launch delay, chewers per ship, how close the carriers stand. Adding a mission
is adding a row. Progress (the furthest mission won) is one small file in the
platform's data directory, so the campaign survives a restart and a rebuild.

| mission | your side | the swarm | the field | what it teaches |
| --- | --- | --- | --- | --- |
| 1. First encounter | one Terran frigate | 1 carrier, 15k motes, 10 s before launch | open space | select, move, and watch the guns work a carrier down |
| 2. Picket | frigate and 2 corvettes | 2 carriers, 40k | 6 rocks | the swarm divides between ships: where you put the escorts decides who gets chewed |
| 3. The rock field | destroyer and 2 frigates | 4 carriers, 100k, veins between the rocks | 14 rocks | rocks are cover and the veins are the swarm's roads |
| 4. Breakthrough | heavy cruiser and a wing of 4 | 6 carriers standing close, 200k | 20 rocks | a siege from inside the ring, reinforcements as a decision |
| 5. Siege | heavy cruiser and a wing of 6 | 10 carriers, 300k, carriers replenish faster | 24 rocks | everything at once |

Escalation is five knobs, not five rules: carriers, motes, chewers per ship,
launch delay, and how close the carriers stand. Every mission gets a headless
render in `docs/` so its picture is proved rather than described.

Cost: half a session for the table, the progress file and the mission select
screen (a list under the Campaign entry, locked past the furthest win); the
missions themselves are tuning, done with the fixed step headless so a
mission's difficulty is a number in a log and not a feeling.

## 4. A playable ship: subsystems that can be destroyed

**What is true today.** Only the reactor matters: half of it dead kills the
ship, and nothing else on a hull does anything when it dies. The turrets are
INVULNERABLE, because their cells are lifted out of the hull's damage grid to
be drawn as children, so a bite that lands on a gun lands on nothing. The
engines plume whatever state they are in.

**The rule to build.** A subsystem is a cluster of cells that already exists,
read off the purposes redux-tribes exported with every cell: each gun cluster
(`gun_clusters`), each drive cluster (`engine_clusters`), the attitude cells
(`purpose::ATTITUDE`), the bridge (`purpose::COMMAND`). Its health is the share
of its cells still alive, and it is OFFLINE below a threshold. This is the
same rule redux-tribes keeps ("a volume is offline before it is gone"), on
the cells this game already has, with one core function
`fx::subsystems_of(model)` and one method `Subsystem::share(damage)` that the
tests pin.

| subsystem | when it is offline | what that does |
| --- | --- | --- |
| a gun | half its cells dead | that turret stops firing and stops aiming (it droops to rest); the others carry on |
| the drives | thrust is the share of live drive cells, so a ship with half its bells gone accelerates at half; at nought it COASTS: no thrust, no braking, a move order is refused with "engines out" | the flames of a dead cluster do not draw |
| the thrusters | turn rate is the share of live attitude cells; at nought the heading is fixed | a ship that can still burn along a heading it cannot change, which is Homeworld's drifting hulk |
| the bridge | half dead: guns fire at half cadence (no fire control) | optional; the cheapest thing that makes the bridge worth protecting |
| the reactor | as now | the ship dies and leaves its wreck |

**The turrets have to be hittable for any of this to matter.** The fix is one
line of intent: a turret's cells stay in the damage grid and are only left out
of the hull's MESH. Bites, beams and blasts then find them as they find any
cell, and the turret child re-meshes from its live cells when its bricks are
dirty, the same way a brick does, so a chewed gun is visibly chewed and a dead
one is a stump. `fire_guns` and `aim_turrets` ask the slot's share.

**Feedback**, or none of it exists for the player: the selected ship's bar
gains four pips (drives, thrusters, guns, reactor) in the bar's three colours,
the mode line says "engines out" when a move order is refused, and a ship that
cannot turn draws its order line from where it will actually go.

Cost: a session. The core half is small and tested; the app half touches
`spawn_ship`, `fly_hull`, `fire_guns`, `aim_turrets`, `draw_flames` and the
bars, each in one place.

## 5. Playground

A third menu entry that is a set of toggles on a skirmish rather than a
scenario. Every one of these is a question about the game that a fight cannot
hold still long enough to answer.

- **Freeze the swarm.** The swarm's own clock already stops when the game
  pauses, and the compute pass still runs with a step of nought, so a frozen
  cloud that still dies to shots is `SwarmConfig.paused` without the CPU's
  pause: the ships fly, the guns fire, and the carve a beam cuts stays cut
  where a player can look at it. One catch to fix on the way: the tick decays
  a mote's speed by 0.985 every pass whatever the step is, so a cloud frozen
  for ten seconds restarts at a crawl; the decay has to be raised to the step.
- **A target dummy**: a hull that never moves and never fires, to shoot at.
- **Slow motion**: a time scale of a quarter on the swarm and the ships alike
  (one number, since they share `STEP_CLAMP`).
- **A blast at the cursor**: a debug shot with a key, to see what a flak burst
  does to a frozen cloud from any angle.
- **An invulnerable flagship**, to watch the swarm at work without the fight
  ending.
- **Any hull, any fleet**: the skirmish setup's own controls.

Cost: small. Freeze and slow motion are two flags the config already nearly
has; the dummy and the debug shot are a spawn and a key.

## 6. The swarm glows hot, then pops

**What is true today.** Any shot takes `SHOT_BITE` (0.6) off a mote; two hits
kill; under half it breaks off for its carrier and repairs. There is no
difference between a beam and a burst, and a wounded mote looks exactly like a
whole one.

**The rule.** Two kinds of harm, which the shot list already distinguishes by
shape (a beam is a capsule with length, a blast is one without):

- **Blasts kill outright.** Flak, the fighters' guns, a reactor's wave: a mote
  inside one is gone that tick. Explosives and projectiles kill.
- **Beams wound.** A mote inside a beam takes `BEAM_BITE` (0.25 proposed) per
  TICK it is inside, so a mote the sweep crosses is hurt and one that dives
  through the beam is hurt more, and a mote that stays in it for four ticks
  pops. A wounded mote still breaks off for home under half, glowing all the
  way, and cools as it repairs at the carrier.
- **The glow.** Hit points already ride in the mote's `extra.x` and already
  reach the draw as an instance attribute, so the change is in `mote.wgsl`
  alone: the body colour mixes toward a hot yellow by `(1 - hp)` squared, over
  the bloom threshold below about 0.4, so a hurt mote reads as an ember from
  across the field. The drive glow is untouched.
- **The pop** is the death burst that exists: the sparks, the debris and the
  shock wave the neighbours feel.

The kill test lives in `fx.rs` first and the WGSL transcribes it, as now; the
one new core test is the rule itself: a blast kills a live mote in one tick and
a beam takes four.

Cost: small. A dozen lines in the shader and the reference, one test, and a
headless render with the swarm frozen to prove the carve, which is what the
playground's freeze is for.

## 7. The order to build it in

1. **Subsystems** (section 4) and the **reactor burial fix** (section 1),
   because every scenario is a ship that can be disabled and a swarm that has
   to reach the core.
2. **The swarm's glow and pop** (section 6), small and self contained.
3. **The playground** (section 5), because the freeze is how the swarm changes
   above are proved.
4. **Menu, skirmish setup and the result screen** (section 2).
5. **The campaign** (section 3), which is data once the screens exist.

Each step is a pull request with its headless pictures and the numbers in the
commit message, as the rest of the project is.

## 8. Decisions that are yours

- **The offline threshold**: half a subsystem's cells (proposed), or
  redux-tribes' fifth? Half means a gun keeps firing through a lot of damage;
  a fifth means the first good chew silences it.
- **Should a dead bridge do anything?** Half cadence is proposed. Strike it
  if it is one rule too many.
- **Campaign fleets**: fixed per mission (proposed), or the player picks a
  flagship within the mission's rung?
- **The progress file**: yes (proposed), or keep the campaign stateless?
- **The kill counter readback**: one integer a second from the GPU, for the
  result screen and the playground's readout. Allowed, or not worth the
  exception?
- **Time to critical is what "difficulty" means.** Tune every mission by that
  number, measured headless, rather than by feel. Yes?
