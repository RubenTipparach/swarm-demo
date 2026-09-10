#!/usr/bin/env python3
"""Generate the ember texture the torn edge of a hull is drawn with.

    python3 tools/make_ember_texture.py [--check]

GUIDELINES rule 3: a texture is a static FILE with a committed generator, never
something built at runtime and never drawn by hand once. So this script is the
source and `web/public/ember.png` is the product, and `--check` fails if the two
have drifted.

WHAT IT IS. Sixteen 64 x 64 tiles in a 256 x 256 atlas, every dimension a power
of two (rule 3 again). Each tile is a patch of hull burning through:

  char    near black crust, most of what is left when it has cooled
  melt    clustered patches of molten metal, orange through yellow
  cores   white hot centres, which are what makes it read as molten
  cracks  fissures that run hot through whatever they cross
  embers  a few bright specks, because a real burn is not tidy

SIXTEEN of them because one is worse than none. A wound is hundreds of cell
faces a tenth of a unit across, and the same tile on all of them reads as a
repeating pattern rather than as burning: each face picks a tile by a hash of
its own cell, so neighbours differ.

HOW IT IS USED. `MeshBasicMaterial` multiplies `map` by the vertex colour. The
map is this file and carries WHERE the metal is hot and what colour that is;
the vertex colour is the heat ramp in `wound.ts` and carries how hot this cell
is right now, starting at white so a fresh wound shows the gradient as authored
and cooling the whole thing toward char.

That split is load bearing, and it took two wrong cuts to find. The first tries
were neutral greys with the hue left to the ramp, and a grey times an orange is
only ever a darker orange: the white hot cores that make a burn look molten are
unreachable that way. The second problem was scale. A cell face is a few pixels
across at the range anyone looks at a wound, so a fine bright web over a dark
ground samples as the dark ground almost every time and the texture multiplied
the glow out altogether. Whatever is drawn here has to survive being three
pixels across, which is why every octave is coarse and why the molten share is
held near half.

WHY THIS IS A SCRIPT AND SHOULD NOT STAY ONE. GUIDELINES 4 wants textures out of
Material Maker (https://www.materialmaker.org/), which is node based, runs
headless and exports the whole PBR set from one graph. It could not be fetched
from the sandbox this was written in: every GitHub release download path
answered 403 through the agent proxy. So this file holds the same contract a
real export would, and the first session that can install Material Maker should
replace it with a `.ptex` graph committed beside the PNG.

The noise, the PNG encoder and the drift check live in `texkit.py`, because a
second generator wanted all three and two copies of a thing is one copy too many
(GUIDELINES 5.1).
"""

import argparse
import hashlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from texkit import ROOT, encode_png, fbm, value_noise   # noqa: E402

OUT = ROOT / "web" / "public" / "ember.png"

TILE = 64
GRID = 4
SIZE = TILE * GRID

# The gradient a burn actually runs through, coldest first.
#
# Authored as COLOUR, not as a grey. The first two cuts of this file were
# neutral and let the heat ramp supply the hue, and that can never reach the
# reference: multiplying a grey by orange only makes darker orange, so the
# white hot cores that make a wound read as molten are unreachable by
# construction. The texture carries where the metal is hot and what colour that
# is; the vertex ramp carries how hot this cell is NOW, as a multiplier that
# starts at white and cools the whole thing down.
LAVA = (
    (0.00, (0.045, 0.040, 0.045)),   # char, near black
    (0.34, (0.180, 0.075, 0.050)),   # scorched
    (0.55, (0.720, 0.170, 0.030)),   # deep red
    (0.74, (1.000, 0.470, 0.055)),   # orange
    (0.88, (1.000, 0.790, 0.240)),   # yellow
    (1.00, (1.000, 0.960, 0.820)),   # white hot
)

# Where the melt starts and where it is fully molten. Wide, because a cell face
# is a few pixels across at the range a wound is looked at and a narrow band
# aliases into its own average.
MELT_LO, MELT_HI = 0.34, 0.72
# Fissures through the crust, hotter than what is around them.
CRACK_EDGE = 0.84
# How much of a face the hot material takes. Under about a third and a wound
# samples as char at this size and goes out, which is exactly what the first
# cut did on a ship.
TARGET_HOT = 0.52


def lava(t: float):
    """The gradient, sampled. `t` is how hot this pixel is, 0 to 1."""
    t = min(1.0, max(0.0, t))
    for i in range(1, len(LAVA)):
        lo_t, lo_c = LAVA[i - 1]
        hi_t, hi_c = LAVA[i]
        if t <= hi_t or i == len(LAVA) - 1:
            span = hi_t - lo_t
            k = 0.0 if span <= 0 else (t - lo_t) / span
            k = min(1.0, max(0.0, k))
            return tuple(lo_c[j] + (hi_c[j] - lo_c[j]) * k for j in range(3))
    return LAVA[-1][1]


def tile_heat(index: int) -> list:
    """One tile as a list of TILE*TILE heats in 0..1, before the gradient."""
    seed = 0x9E3779B9 ^ (index * 2654435761)
    # Coarse on purpose, all of it: a cell face is a few pixels across at the
    # range a wound is looked at, so detail finer than this is detail that
    # aliases into an average and takes the effect with it.
    melt_oct = [value_noise(p, seed + i * 31) for i, p in enumerate((3, 6, 12))]
    crack_oct = [value_noise(p, seed + 97 + i * 17) for i, p in enumerate((4, 8))]
    speck = value_noise(24, seed + 999)

    out = []
    for py in range(TILE):
        for px in range(TILE):
            x, y = px / TILE, py / TILE
            # Clustered, not even: a burn is patches of molten metal with char
            # between them, which is what the blotches of the low octaves give.
            n = fbm(x, y, melt_oct)
            if n <= MELT_LO:
                t = 0.0
            elif n >= MELT_HI:
                t = 1.0
            else:
                k = (n - MELT_LO) / (MELT_HI - MELT_LO)
                t = k * k * (3 - 2 * k)

            # Fissures, which run hot through whatever they cross.
            ridge = 1.0 - abs(2.0 * fbm(x, y, crack_oct) - 1.0)
            if ridge > CRACK_EDGE:
                k = (ridge - CRACK_EDGE) / (1.0 - CRACK_EDGE)
                t = max(t, 0.55 + 0.45 * k * k * (3 - 2 * k))

            # Embers sitting in the char, because a real burn is not tidy.
            s = speck(x, y)
            if s > 0.94:
                t = max(t, 0.62 + (s - 0.94) * 6.0)
            out.append(min(1.0, t))
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="fail if the committed PNG differs from a fresh one")
    args = ap.parse_args()

    tiles = [tile_heat(i) for i in range(GRID * GRID)]

    # Hold the share of the face that is actually hot. Too little and a wound
    # samples as char at the size a cell face is drawn and goes out; too much
    # and there is no char left for the melt to read against.
    hot = sum(1 for t in tiles for v in t if v > 0.45) / (len(tiles) * TILE * TILE)
    bias = TARGET_HOT - hot

    rows = bytearray()
    for y in range(SIZE):
        ty, iy = divmod(y, TILE)
        for x in range(SIZE):
            tx, ix = divmod(x, TILE)
            t = tiles[ty * GRID + tx][iy * TILE + ix]
            r, g, b = lava(min(1.0, max(0.0, t + bias * (1.0 - abs(2 * t - 1)))))
            rows += bytes((int(255 * r), int(255 * g), int(255 * b)))

    png = encode_png(SIZE, SIZE, bytes(rows))

    if args.check:
        if not OUT.exists():
            print(f"{OUT} is missing; run this script without --check")
            return 1
        have = OUT.read_bytes()
        if have != png:
            print(f"{OUT} has drifted from {Path(__file__).name}; regenerate it")
            return 1
        print(f"ember.png matches its generator ({len(png)} bytes)")
        return 0

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(png)
    print(f"wrote {OUT} ({SIZE}x{SIZE}, {GRID * GRID} tiles, {len(png)} bytes, "
          f"{hot * 100:.0f}% molten, sha {hashlib.sha256(png).hexdigest()[:12]})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
