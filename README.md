# swarm-demo

A real time space RTS against an alien swarm, in the voxel language of
[redux-tribes](https://github.com/RubenTipparach/redux-tribes). Bevy 0.18.

```sh
cargo test -p swarm_core                           # the engine-free core
cargo run --release -p swarm_app                   # a window: drag to orbit, wheel to zoom
cargo run --release -p swarm_app -- --headless \
    --motes 5000 --frames 60 --out shot.png        # no window: render, screenshot, exit
```

![M0](docs/m0.png)

What is on screen: a stock redux-tribes hull, drawn as redux-tribes draws it
(one material per surface with its finish normal map, windows cut into the
plating wearing their decals), meshed by brick, CPU chewers eating it cell by
cell and throwing chunks, the four alien archetypes in chitin, the swarm
drawn instanced off the buffer a compute pass ticks, and the archive's sky
baked at launch. See `CLAUDE.md` for the design and the rules.

![Terran frigate](docs/terran_close.png)
![Karisen cruiser](docs/karisen_close.png)

Effects: a hull burning where the swarm has chewed it, guns raking the cloud,
and a reactor going three ticks in.

![A burning hull](docs/fx_wound.png)
![Beams](docs/fx_beams.png)
![A reactor going](docs/fx_boom.png)
