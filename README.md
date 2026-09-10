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

What is on screen: a stock redux-tribes hull meshed by brick, CPU chewers
eating it cell by cell and throwing chunks, the four alien archetypes, and the
swarm drawn instanced off the buffer a compute pass ticks. See `CLAUDE.md`
for the design and the rules.
