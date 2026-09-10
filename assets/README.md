# Assets

Everything here has a committed source, and nothing here was drawn by hand once.

## `hulls/*.ftvx`

The twenty three stock hulls of redux-tribes, as sparse voxel files (FTVX
v2): every cell's material, purpose, livery role, surface and resolved
colour, each surface's finish and PBR pair, and every window face, exactly as
that project's rasteriser and mesher answer them. Produced by
`tools/export_hulls.mjs` from a redux-tribes checkout, which it reads and
never writes (`cd tools && npm install` first). `manifest.json` lists each
hull's rung, cell size, cell count, hull.ts's own quad count and surface
groups, and its windows by kind. 2.2 MB for the fleet.

They are NOT mirrored, and that is faithful: redux-tribes' CLAUDE.md records
that its lattice centre falls on a cell boundary, so its shells come out a cell
skewed. The Terran frigate has 1406 of 8938 cells without a twin across the
keel plane; the civil boxship's solid cells average x = 16.5 on a 32 wide
lattice. A loader that corrected that would draw a different ship from the one
in redux-tribes' shipyard, so `swarm_core` reports it and leaves it.

## `crates/swarm_app/assets/textures/`

`ember.png`, the nine `surf/armour_*_n.png` finish normal maps and the
twenty seven `surf/window_*_{c,e,n}.png` decal maps (colour, emission,
normal, nine kinds), copied from redux-tribes `web/public/`. Their
generators are in `tools/` beside this file: `make_ember_texture.py`,
`make_surface_textures.py` and the `texkit.py` they share. The finishes and
the window maps are live: one material per surface per hull, one per window
kind.

`alien_chitin_n.png` is ours, from `tools/make_chitin_texture.py` on the same
`texkit`, and `--check` says whether it has drifted from its generator.
