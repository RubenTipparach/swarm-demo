#!/usr/bin/env python3
"""The aliens' skin: a chitin normal map.

    python3 tools/make_chitin_texture.py [--check]

Same contract as `make_surface_textures.py`, on the same `texkit`: a static
FILE with a committed generator, every dimension a power of two, `--check`
to catch drift, and no `sin` anywhere so the check is honest on another
machine. The hulls are riveted, corrugated and hexed because a navy fabricates
them; a mote GREW, so its surface is scales overlapping like roof tiles down
the body, each a dome with a crease at its root, wrinkles across them, and a
scatter of pores. Nothing here is straight, and that is the whole difference
from the plating it lands on.

Tiles once per cell, like the finishes, which on a drone at 0.0375 units a
cell reads as a fine skin rather than as a pattern.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from texkit import ROOT, clamp01, dome, emit, fbm, frac, normal_png, rng, smoothstep, value_noise  # noqa: E402

OUT = ROOT / "crates" / "swarm_app" / "assets" / "textures" / "alien_chitin_n.png"
SIZE = 128
STRENGTH = 3.4

# Scales per row across the tile, and rows down it. Odd rows are offset by
# half a scale, so each sits over the crease between two below it.
COLS = 5
ROWS = 6


def chitin() -> list:
    wrinkle = [value_noise(p, 0xC417 + i) for i, p in enumerate((9, 21, 43))]
    pores = value_noise(17, 0xC41A)
    r = rng(0xC41B)
    # Each scale's own size and lean, so no two are alike.
    sizes = [[0.92 + 0.16 * r() for _ in range(COLS)] for _ in range(ROWS)]
    leans = [[(r() - 0.5) * 0.18 for _ in range(COLS)] for _ in range(ROWS)]

    def at(x: float, y: float) -> float:
        # Which row we are in, and the row above it, which overlaps this one
        # by a third: a scale's crown is proud of the root of the one below.
        best = 0.0
        row_h = 1.0 / ROWS
        for dr in (0, 1):
            ry = y * ROWS + dr
            row = int(ry) % ROWS
            # The scale's root sits at the bottom of its row; the crown
            # reaches a third of the way into the row above.
            fy = frac(y * ROWS) + dr
            off = 0.5 if row % 2 else 0.0
            for dc in (-1, 0, 1):
                cx = x * COLS - off + dc
                col = int(cx + 0.5 + COLS) % COLS
                fx = frac(cx + 0.5) - 0.5
                size = sizes[row][col]
                lean = leans[row][col]
                # Distance from the scale's centre, measured in a squashed
                # frame so a scale is wider than it is tall, and leaning.
                u = (fx + lean * fy) / (0.62 * size)
                v = (fy - 0.55) / (0.80 * size)
                d = (u * u + v * v) ** 0.5
                h = dome(d)
                # A crease at the root: the height falls off fast in the
                # bottom fifth of the scale so the tile below shows.
                h *= smoothstep(-0.05, 0.28, fy)
                if h > best:
                    best = h
        # Wrinkles across the scales and a few pores sunk into them.
        w = (fbm(x, y, wrinkle) - 0.5) * 0.10
        p = pores(x, y)
        pit = smoothstep(0.74, 0.86, p) * 0.22
        return clamp01(0.15 + best * 0.72 + w - pit)

    return [at((x + 0.5) / SIZE, (y + 0.5) / SIZE) for y in range(SIZE) for x in range(SIZE)]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail if the file on disk differs from a fresh run")
    args = ap.parse_args()
    png = normal_png(chitin(), SIZE, SIZE, STRENGTH, True)
    return 0 if emit(OUT, png, args.check) else 1


if __name__ == "__main__":
    sys.exit(main())
