#!/usr/bin/env python3
"""The crystal a rock's fuel seam is made of: a colour map, a normal map and a
DEPTH map, so the faces of a cut seam are parallax mapped and read as looking
into the stone rather than as blue paint on it.

    python3 tools/make_crystal_texture.py            # write the three maps
    python3 tools/make_crystal_texture.py --check    # has either drifted?

Bevy does the parallax itself: `StandardMaterial::depth_map` walks the view
ray through the height field per fragment, which is the same trick the Godot
"parallax ice" shader does on a plane and is why this needs a texture rather
than a shader. The depth map is white at the BOTTOM and black at the top,
which is the height field inverted, and that convention is Bevy's rather than
a choice here.

WHAT MAKES IT READ AS OPAL rather than as blue glass, and all three are
needed:

- FACETS AT DIFFERENT DEPTHS. Two scales of jittered cell, a deep plateau per
  cell and a shallower one inside it, so the parallax has layers to move
  against. One depth everywhere is a flat surface with a texture on it, which
  is what a normal map alone already looked like.
- COLOUR PLAY PER FACET. Each cell takes its own hue off a short arc from
  green through cyan into violet, and the same map is the EMISSIVE, so a
  facet's own colour is what glows out of the shaft rather than one blue for
  the whole seam.
- A RIM. The distance to the cell's own edge darkens and steepens it, which is
  what gives a facet a border and stops the whole thing reading as noise.

No `sin` anywhere, the rule `texkit` states: this is a committed asset and
`--check` compares it byte for byte on another machine.
"""

import sys

from texkit import ROOT, clamp01, emit, encode_png, normal_png, rgb_png, rng, smoothstep

# A power of two, and no bigger: a depth map costs a texture lookup per
# parallax layer per fragment, and the docs are explicit that a larger one
# costs disproportionately. Four faces of a cut cell is a handful of pixels.
SIZE = 256

# The two cell scales, in cells across the tile. The coarse one is the
# facet a player sees and the fine one is what moves under it.
COARSE = 5
FINE = 11

# How deep each scale cuts, as a share of the height range. The coarse
# facets carry most of it, because the parallax reads the LARGEST step.
COARSE_DEPTH = 0.72
FINE_DEPTH = 0.28

# The arc a facet's hue is drawn from: sea green, through cyan, to violet.
# Three stops rather than two, because a straight line between two colours
# in RGB passes through grey in the middle and grey is the one thing opal
# never is.
HUES = [(0.24, 0.92, 0.62), (0.30, 0.80, 1.00), (0.62, 0.52, 1.00)]


def cells(period: int, seed: int):
    """A jittered grid, and the two nearest points of it to any sample.

    Periodic, because a tile meets itself: the grid wraps, so a cell on the
    right edge is the same cell as the one on the left. Answers the distance
    to the nearest point, the distance to the second nearest, and which cell
    the nearest belongs to, which is what a facet needs: the first two give
    the rim, the third gives it a colour and a depth of its own.
    """
    r = rng(seed)
    jitter = [[(r(), r()) for _ in range(period)] for _ in range(period)]

    def at(x: float, y: float):
        fx, fy = x * period, y * period
        cx, cy = int(fx), int(fy)
        best, second, who = 9.0, 9.0, 0
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                i, j = (cx + dx) % period, (cy + dy) % period
                jx, jy = jitter[j][i]
                # The point's place in the UNWRAPPED grid, so a sample near
                # an edge measures to the neighbour across the seam rather
                # than to the far side of the tile.
                px, py = cx + dx + jx, cy + dy + jy
                d = (px - fx) * (px - fx) + (py - fy) * (py - fy)
                if d < best:
                    second, best, who = best, d, j * period + i
                elif d < second:
                    second = d
        return best ** 0.5, second ** 0.5, who

    return at


def hue_of(t: float):
    """A point on the opal arc, in 0..1."""
    t = clamp01(t) * (len(HUES) - 1)
    i = min(int(t), len(HUES) - 2)
    f = t - i
    a, b = HUES[i], HUES[i + 1]
    return tuple(a[c] + (b[c] - a[c]) * f for c in range(3))


def build():
    """The height field and the colour field, in one pass, because a facet's
    colour is a function of the same cell its depth is."""
    coarse = cells(COARSE, 0x0BA1)
    fine = cells(FINE, 0x7EA5)
    tint = rng(0x5EED)
    # One hue per coarse cell, rolled once: a facet is one colour, and
    # rolling per pixel would be static rather than opal.
    hues = [tint() for _ in range(COARSE * COARSE)]
    height, colour = [], []
    for y in range(SIZE):
        for x in range(SIZE):
            u, v = (x + 0.5) / SIZE, (y + 0.5) / SIZE
            d1, d2, who = coarse(u, v)
            f1, f2, _ = fine(u, v)
            # The plateau: each cell sits at its own depth, and the RIM is
            # how near the sample is to the wall between two cells, which is
            # what `d2 - d1` measures. Squared, so a facet is flat in the
            # middle and turns hard at its edge.
            rim = smoothstep(0.0, 0.22, d2 - d1)
            plate = hues[who % len(hues)]
            deep = 0.25 + 0.75 * plate
            coarse_h = deep * rim * rim
            fine_h = smoothstep(0.0, 0.30, f2 - f1)
            # TURNED OVER: the facet interiors are the DEEP part and the
            # walls between them stand at the surface, which is what makes
            # the parallax read as looking into ice rather than as knobbly
            # stone standing proud of it. The first cut had it the other way
            # round and the maps say which immediately: a depth map whose
            # cell walls are white is one where the walls are the bottom.
            h = 1.0 - (COARSE_DEPTH * coarse_h + FINE_DEPTH * fine_h)
            height.append(clamp01(h))
            # And the colour, off the same cell. The DEEP part is darker,
            # which is what a lit body looks like from outside: what is near
            # the surface is what the light reaches.
            r, g, b = hue_of(plate)
            lift = 0.42 + 0.58 * h
            colour.append((r * lift, g * lift, b * lift))
    return height, colour


def main() -> int:
    check = "--check" in sys.argv
    height, colour = build()
    out = ROOT / "crates/swarm_app/assets/textures/surf"
    ok = emit(out / "crystal_c.png", rgb_png(colour, SIZE, SIZE), check)
    # Strength is in height units per texel, so it is the one number that
    # has to move if SIZE does.
    ok = emit(out / "crystal_n.png", normal_png(height, SIZE, SIZE, 5.0), check) and ok
    # White is the BOTTOM, which is Bevy's convention: a depth map is the
    # height field turned over.
    depth = bytearray()
    for h in height:
        v = int(255.0 * (1.0 - h) + 0.5)
        depth += bytes((v, v, v))
    ok = emit(out / "crystal_d.png", encode_png(SIZE, SIZE, bytes(depth)), check) and ok
    if not ok:
        return 1
    print(f"crystal: 3 maps of {SIZE}x{SIZE}" + (" unchanged" if check else " written"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
