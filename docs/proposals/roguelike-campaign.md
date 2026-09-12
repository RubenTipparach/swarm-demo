# The Long Retreat: a roguelike campaign, an economy, and the ships that gather it

The designed version of this document, which is the one to read:
https://claude.ai/code/artifact/807d3960-3307-4154-85ff-1527b0e1f790

Nothing here is built. It follows `menu-campaign-subsystems.md` and replaces
that document's section 3, the five mission campaign, with a run. Numbers
marked MEASURED came off the code.

The premise in one line: **you need fuel to leave and materials to grow, they
are in the same rocks, and the clock is the swarm.**

## 0. Why this shape

The skirmish is a battle with a win condition, and five battles in a row with
a number saved between them is a level select. What makes a run worth
replaying is that the fleet you finish with is the fleet your decisions built,
and every decision cost something you could have spent elsewhere.

So the campaign is a RUN: a chain of systems, one command ship that must
survive all of them, a fleet that accumulates damage and history, and a
currency you are always short of. The swarm is already the right antagonist:
it is a tide rather than an army, it does not garrison and does not
negotiate, it arrives. A game about staying exactly as long as you dare is
what this swarm is best at.

## 1. What it is built out of, which is mostly what exists

| exists today | becomes |
| --- | --- |
| seven civil hulls | six of them ARE the support fleet: miner, tanker, hauler, lighter, boxship, liner |
| ore in a rock | what a miner cuts for: a separate material, on its own surface, already buried |
| `DamageGrid::bite` and `bore` | the mining cut and the salvage cut: chewing with the sign flipped |
| wrecks as hulls | salvage sites, with cells that already know what they were for |
| four navy ladders | the four command ships, and the warships you research |
| `AppState`, `SceneSpec`, the setup screen | a system is a `SceneSpec` plus a tide; the map and the refit are two more states |
| the rigid body and the range | a mining cut shoves a rock and a salvage cut turns a wreck, for free |
| carriers, motes, veins, chewers | unchanged. The fight is the fight |
| nothing yet | cargo and yields in the core, jobs for support ships, the tide, the run state, three screens |

**Mining is a cut, and that is measured.** `cargo run --release -p swarm_core
--example rock_stats` over the fourteen seeds the asteroid field actually
uses:

```
14 rocks: 14214 solid cells, 445 of ore (3.1%),
4.9% of the ore has a face open to space
a rock averages 1015 solid cells and 32 of ore
```

Ninety five percent of a rock's ore is BURIED. A miner cannot scrape a
surface, it has to cut a shaft, which takes time and holds it still in one
place, which is exactly where the swarm wants it. The economy's pressure is a
consequence of the rock generator written months ago, not a rule somebody
added afterwards.

## 2. The run

Ten to twelve systems in three acts, an hour to ninety minutes. You start with
a command ship, one escort and one miner. The map branches two or three ways
and every node carries a tag: ore, ice, derelict, cache, quiet, nest. You see
the next node's tag, and the survey ship shows you one further, which is the
strategic half of why it is worth an escort it cannot pay for.

- **The command ship carries everything**: its damage, its research, its
  cargo, its upgrades. It is the run.
- **Ships carry their damage.** A frigate that left with a hole arrives with
  the hole, and nothing repairs itself, which is what makes the repair column
  a real competitor to the research column.
- **Loss is permanent.** A miner eaten in system four is gone until you build
  another one out of materials.

A run ends when the command ship's reactor goes, which is the rule the game
already has for every hull, or less cleanly when you are in a system with no
fuel, no tanker and the fleet on top of you: a death you can see coming three
minutes out, which is the best kind.

Across runs the proposal is deliberately thin: **blueprints you finished
researching become starting options**, at a materials cost. No stat creep.
What you keep is knowledge, which is the only thing a fleeing fleet plausibly
keeps.

## 3. A system, and the tide

| phase | when | what it is |
| --- | --- | --- |
| Probes | 0:00 to 2:00 | one carrier far off, a trickle. Mine freely |
| Swarm | 2:00 to 6:00 | carriers launch, veins form between the rocks. Support ships need escort |
| Fleet | 6:00 on | the rest of the carriers arrive at once. Staying is a decision, not a mistake |

Nothing in the tide is a new fight: it is the skirmish already in the game, on
the knobs it already has. The fleet arriving does not end the system, it makes
every further minute expensive.

**Leaving is a spool, not a button.** The command ship spools its drive for
thirty seconds; everything inside the jump field, a sphere round it, goes.
**Everything outside is left in the system.** That one rule is where the design
earns its keep: at 6:40 the fleet is in, the tanker is full, and your miner is
ninety seconds away at the far rock with a hold you need. You start the spool
or you do not.

## 4. Three resources

| resource | from | buys | carried by |
| --- | --- | --- | --- |
| Materials | ore cells cut out of rocks; armour and structure cells cut out of wrecks | repairs, ships, refits, building a blueprint | miner and freighter, unloaded to the command ship |
| Jump fuel | volatile cells cut out of ice rocks, refined over time | the jump out, priced by the hop | the tanker, which refines and holds it |
| Data | surveys, scanning a live carrier, part cells off wrecks | research: blueprints and equipment | the command ship |

**Yield comes from the cell, not from a table beside it.** Every cell already
knows its material and its purpose, which is how a wound draws machinery
behind plating. Salvage reads the same fact: plating is materials, a drive or
gun or bridge cell is a part and a sample, chitin is the only route to alien
equipment.

**One damage pipeline, three uses.** The swarm's bite, the range's slug and
the miner's cutter are the same function with different numbers, which is the
project's own rule rather than a saving. A rock a miner has worked looks like
a rock the swarm has chewed, because it is the same code taking the same cells
off.

The anchor: a field holds 445 cells of ore, so stripping a whole system is 445
materials and you will never do it. Every price is set against that number,
headless, the way mission difficulty already is.

## 5. The support fleet

| ship | hull | cells | job | what kills it |
| --- | --- | --- | --- | --- |
| Miner | `civil_miner` | 8519 | cuts ore and ice out of rocks | holding still beside a rock the veins run past |
| Tanker | `civil_tanker` | 11814 | refines volatiles, holds the jump fuel | being the slowest thing on the map |
| Salvager | `civil_hauler` | 9854 | cuts wrecks for materials, parts and data | working a dead carrier while the live ones watch |
| Survey ship | `civil_lighter` | 4984 | reveals rocks, finds derelicts and the jump point | being alone, far out, by definition |
| Freighter | `freighter` | 10580 | shuttles cargo so the miner never leaves its shaft | the run between the rock and the command ship |
| Tender | `civil_liner` | 10898 | repairs hulls in the field, slowly, for materials | whatever it was repairing |

**The miner** holds station off a face and cuts: ore goes into the hold, stone
is spoil thrown as chunks. Because the ore is buried, a cut is a SHAFT, and
the rock ends the system with a hole you can see from across the map. Breaking
off and coming back resumes the same shaft, because the hole is state on the
rock rather than progress on the miner. Its hold is a tenth of its own cells,
so the constraint is never the hold: it is minutes, and the escort you can
spare.

**The tanker** refines volatiles at a fixed rate, so fuel arrives on a curve
that starts when the first ice is aboard: send the miner to the ice early and
your jump window opens early. That is what stops fuel being a second pile of
materials. It is the fattest, slowest hull you own and it cannot defend
itself.

**The salvager** is the same cutter pointed at a wreck, and what it gets
depends on what it cuts. A carrier you killed leaves the richest wreck in the
game, sitting in a system you were about to leave: that is what makes the
fleet phase a temptation rather than a countdown.

**The survey ship** is fast and unarmed. A system arrives unknown, and which
rocks hold ore, which hold ice and which are barren is hidden until something
looks.

**The freighter and the tender** are mid-run purchases that buy throughput
rather than capability: the freighter roughly doubles a system's take for the
price of one more thing to escort, and the tender turns a between-systems
repair cost into an in-system one.

**How you command them: no new verbs.** Select a miner, right click a rock.
The order mode already casts a ray and already knows what it hit, because the
range's weapons land on a clicked cell. A rock under the cursor with a miner
selected is a mine order, a wreck with a salvager selected is a salvage order,
anything else is the move order that exists.

## 6. Four command ships

The command ship is your reactor, refinery, research bay and jump drive, and
the one hull whose loss ends the run.

| faction | hull | cells | doctrine | the run it wants |
| --- | --- | --- | --- | --- |
| Terran | `terran_cruiser` | 11993 | the line: most plating, best flak, extra beams | stay into the fleet phase and take the salvage |
| Karisen | `karisen_cruiser` | 7876 | reach: longest guns, best sensors | kill carriers before they close, leave early |
| Rogue | `rogue_cruiser` | 7503 | scavenger: a third of its mass is boarding gear, so salvage yields more and repairs cost less | live off wrecks, fly a fleet of patched ruins |
| Benefactor | `benefactor_cruiser` | 9738 | engineer: fastest refining, cheapest research, longest jump | skip nodes, tech up, win the last act |

Four numbers each, not four code paths: multipliers on salvage yield, refine
rate, research cost and jump range, plus the hull they already are. A faction
is a row.

## 7. Between systems: the refit

One screen, four columns, all competing for the same materials.

| column | what it does | costs |
| --- | --- | --- |
| Repair | cells back on a hull, priced per cell; scrapping returns a share | materials |
| Build | anything researched: a warship, a support ship, a replacement | materials |
| Research | blueprints and equipment, offered by what you have found | data |
| Buy | at a cache or trader: equipment, a hull nobody researched, a whole ship | materials |

**Equipment unlocks research, not the other way round.** Each research entry
has a prerequisite you have to FIND: an alien drive core cut out of a carrier,
a Benefactor targeting set bought at a cache, a Karisen missile rack salvaged
off a derelict. Holding the thing is what puts the research on the list, so
two runs with the same faction diverge on what the map gave you, and salvage
and caches matter for a reason that is not just numbers.

Research is data in a table: cost, prerequisite item, and what it unlocks,
which is either a hull key (the build column can then spawn it, because a
class is already a file in `assets/hulls`) or a modifier. Adding research is
adding a row.

## 8. Where the five missions went

They become the node archetypes the map is built out of, which is what they
were describing anyway.

| was | becomes |
| --- | --- |
| 1. First encounter | the opening system of act one, fixed every run, because a run needs one place to teach the loop |
| 2. Picket | the quiet node: a system you take to repair rather than to grow |
| 3. The rock field | the ore and ice nodes, where the veins between rocks are the danger |
| 4. Breakthrough | the nest: carriers standing close from the first second, the jump point behind them |
| 5. Siege | the act boss and the gate: everything at once, and no leaving until it is dead |

Escalation stays five knobs rather than five rules: carriers, motes, chewers,
launch delay, how close the carriers stand.

## 9. The order to build it in

Each stage is playable on its own and provable with a headless picture.

1. **Cargo and the cut.** Yields in the core, a hold on a hull, a miner that
   cuts a rock and unloads. Rocks become damageable, which is the path the
   range's target dummy already proved. Playable: one miner, one rock, a
   number going up.
2. **The tide and the jump.** The three phase clock as a scenario field, the
   fuel store, the spool, the jump field and what it leaves behind. Playable:
   a whole system, start to jump, with no map round it.
3. **The rest of the support fleet.** Ice rocks and the tanker, the salvager
   and wreck yields, the survey ship and hidden rocks, the freighter's ferry.
4. **The run.** Run state, the map screen, node types, the refit's repair and
   build columns, persistence. Playable: a full run of dumb systems.
5. **Research and equipment.** The data currency, the table, prerequisites,
   caches. Playable: two runs that differ.
6. **The four command ships** and their four numbers each, and the act bosses.
7. **Tuning**, headless, against the 445 ore cells a system holds.

The boundary holds throughout: what a cut yields, what a jump costs and what
research unlocks are data and pure functions in `swarm_core`; the ships, the
orders and the screens stay in the app.

## 10. Decisions that are yours

- **Does losing the tanker strand you?** Total loss is the better disaster;
  a small reserve on the command ship keeps a bad system from ending a good
  run. Proposed: a reserve worth one short hop.
- **How long is the tide?** Proposed six minutes to the fleet, eight to ten
  for a full system.
- **How long is a run?** Proposed ten to twelve systems, sixty to ninety
  minutes.
- **Do you keep anything between runs?** Proposed: finished blueprints become
  starting options, nothing else.
- **Is the jump field forgiving?** Proposed: ships outside are lost outright.
  The softer rule is that they arrive damaged one system later.
- **Can you refuse to jump?** Proposed yes, and if staying is too strong the
  fleet keeps escalating until it is not.
- **Is the tender worth existing?** It is the seventh civil hull and the only
  one with no job. Cut it if in-system repair is one system too many.
- **Three currencies or two?** Data could be materials by another name. Three
  is proposed so research does not compete with repair, or nobody researches.
- **Do support ships carry guns?** Proposed no, except a light point defence
  the Rogue can research. Unarmed support is what makes escorting a decision.
