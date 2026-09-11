# swarm-demo

A real time space RTS against an alien swarm, in the voxel language of
[redux-tribes](https://github.com/RubenTipparach/redux-tribes). Bevy 0.18.

Right button opens a move order: the cursor picks a point on the plane
through the ship, hold shift to lift it off that plane, release to commit.
Left drag orbits, wheel zooms, **WASD and the arrows pan** the camera, Q and E
lift and drop it, space snaps it to your ship. **R, or the button on screen,
calls in reinforcements**, two per press up to a wing of six, which fly in from
off the map and keep station on you. Your ship also puts up a squadron of
fighters of its own.

The carriers hold for ten seconds before the first fighter comes out, so there
is a beat before the swarm arrives. `--launch-delay 0` removes it.

```sh
cargo test -p swarm_core                           # the engine-free core
cargo run --release -p swarm_app                   # a window: drag to orbit, wheel to zoom
cargo run --release -p swarm_app -- --fps 60      # frames are capped at 120 by default
cargo run --release -p swarm_app -- --headless \
    --motes 5000 --frames 60 --out shot.png        # no window: render, screenshot, exit
cargo run --release -p swarm_app -- --reinforce 4  # start with a wing already inbound
cargo run --release -p swarm_app -- --thickness 0  # the swarm with its self shadowing off
```

## Building and running

One script per shell, the same flags on all three platforms. They check the
toolchain and the platform's own prerequisites before they build, because a
missing alsa header or a missing link.exe fails as a linker error that never
names the package.

```sh
./run.sh                                      # Linux, macOS: build release, open a window
./run.sh --headless -- --motes 5000 --frames 60 --out shot.png
./run.sh --test                               # the suites, then build and run
./run.sh --debug --build-only                 # the dev profile, no run
```

```powershell
.\run.ps1                              # Windows: the same script, the same flags
.\run.ps1 --headless -- --motes 5000 --frames 60 --out shot.png
```

`--triple <target>` cross compiles and does not run. Everything after `--`
goes to `swarm_app` itself, and on Windows, where PowerShell eats the `--`,
the first flag the script does not recognise starts the passthrough anyway.
`run.sh -h` prints the rest. `run.bat` is a plain double-click launcher for
Windows: it just builds the release binary and runs it, no flags.

A built binary is not relocatable yet: `HULLS` and `ASSETS` in
`crates/swarm_app/src/main.rs` are `CARGO_MANIFEST_DIR` paths baked in at
compile time, so it reads its hulls and shaders out of this source tree and
not out of a folder beside itself. Building on each platform is what these
scripts do; shipping one is a change to those two constants.

![M0](docs/m0.png)

What is on screen: a stock redux-tribes hull, drawn as redux-tribes draws it
(one material per surface with its finish normal map, windows cut into the
plating wearing their decals), meshed by brick, CPU chewers eating it cell by
cell and throwing chunks, the four alien archetypes in chitin, the swarm
drawn instanced off the buffer a compute pass ticks, and the archive's sky
baked at launch. The swarm SHADES ITSELF: a density grid is counted and
marched toward the sun once a tick, so a mote buried in a thick cloud is dark
and one on the near face of it is not, while anything a mote lights itself
with stays lit wherever it is standing. See `CLAUDE.md` for the design and the
rules.

![Terran frigate](docs/terran_close.png)
![Karisen cruiser](docs/karisen_close.png)

Effects: a hull burning where the swarm has chewed it, guns raking the cloud,
a reactor going three ticks in, the engines burning on the throttle they are
actually pulling, and a wing of reinforcements on station.

![Ten carriers](docs/fx_hives.png)
![The nav disc](docs/fx_nav.png)
![A burning hull](docs/fx_wound.png)
![Beams](docs/fx_beams.png)
![A reactor going](docs/fx_boom.png)
![Geometric flames](docs/fx_flame.png)
![A wing on station](docs/fx_wing.png)
![The whole battle](docs/fx_battle.png)
