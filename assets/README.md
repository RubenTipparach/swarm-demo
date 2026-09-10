# Assets

Everything here has a committed source, and nothing here was drawn by hand once.

## `hulls/*.ftvx`

The twenty three stock hulls of redux-tribes, as sparse voxel files: every
cell's material, purpose, livery role and resolved colour, exactly as that
project's rasteriser answers them. Produced by `tools/export_hulls.mjs` from a
redux-tribes checkout, which it reads and never writes. `manifest.json` lists
each hull's rung, cell size and cell count. 2.1 MB for the fleet.

They are NOT mirrored, and that is faithful: redux-tribes' CLAUDE.md records
that its lattice centre falls on a cell boundary, so its shells come out a cell
skewed. The Terran frigate has 1406 of 8938 cells without a twin across the
keel plane; the civil boxship's solid cells average x = 16.5 on a 32 wide
lattice. A loader that corrected that would draw a different ship from the one
in redux-tribes' shipyard, so `swarm_core` reports it and leaves it.

## `crates/swarm_app/assets/textures/`

`ember.png` and the nine `surf/armour_*_n.png` normal maps, copied from
redux-tribes `web/public/`. Their generators are in `tools/` beside this file:
`make_ember_texture.py`, `make_surface_textures.py` and the `texkit.py` they
share. Not wired into a material yet: the hull is drawn with vertex colours and
a plain PBR surface, and these are here so the finish work has its inputs.
