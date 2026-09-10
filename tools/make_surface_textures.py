#!/usr/bin/env python3
"""Candidate surface finishes and window decals for a hull, for review.

    python3 tools/make_surface_textures.py [--check]

WHAT THIS IS FOR. A hull is a lattice of cells and every cell face is drawn as a
flat rectangle of one colour. That reads at map range and it is bare up close,
which is exactly where the console now takes a player: the ship data inspector
zooms in on one hull. Two things fix that without touching a single rule:

  a NORMAL MAP per palette colour, so a player picks what their armour is MADE
  of (riveted panel, corrugation, hex ablative, greebles) and every cell wearing
  that colour is lit as though it had that surface, and

  a WINDOW DECAL per cell, painted on in the shipyard, which adds a recess to
  the normals, takes the hull's paint off the glass, and puts a glow behind it.
  Three maps, and the middle one is the one that makes an unlit window read as
  a window rather than as a rectangle of hull.

Neither changes an outcome, so neither crosses the boundary: meshes and
materials are the client's business (CLAUDE.md, the boundary). What a player
picked is stored with the design beside `paint`, and like `paint` it is not part
of what the core is told and not part of the hash.

WHY A FILE PER FINISH AND NOT ONE ATLAS. The ember texture is an atlas because
its tiles are picked per face and never repeat. A finish is the opposite: it has
to TILE, once per cell, across a greedy quad that may be twenty cells wide, and
repeat wrapping repeats the whole image rather than one tile of it. Separate
textures also cost nothing in draw calls here, because a design wears one finish
per colour and realistically one or two colours.

WHERE THEY LIVE. `web/public/surf/` holds the PNGs, because they ship: the
client's build copies everything in `public/`. The mockup gets the same bytes
as data URIs in `textures.js`, because GUIDELINES 2.1 gives it no network at
all and that includes an `<img src>` to a file beside it. One home for the
pixels and one generator for both, so there is nothing to keep in step.

STOPGAP, STILL. Per GUIDELINES 4 the tool for a texture is Material Maker,
which answers 403 on every download path from this sandbox, so these should be
re-authored as `.ptex` graphs when a session can install it.

NO `sin` ANYWHERE, for the reason `texkit.py` explains at length: `--check`
compares bytes on a machine that is not this one.
"""

import argparse
import hashlib
import sys
from base64 import b64encode
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from texkit import (   # noqa: E402
    ROOT, clamp01, dome, emit, fbm, frac, normal_png, rgb_png, rng, smoothstep,
    tri, value_noise, wrap_delta,
)

# The PNGs SHIP, so they live where the client's build copies from, and the
# mockup gets the same bytes as data URIs because it takes no network. One home
# for the pixels: two copies of a binary in a repo is two things to keep in
# step and no way to notice when they part.
OUT = ROOT / "web" / "public" / "surf"
MOCKUP = ROOT / "mockups" / "surface-finishes"
SIZE = 128


# ------------------------------------------------------- armour finishes --
#
# Every one of these returns a height field: a flat list of SIZE * SIZE values
# in roughly 0 to 1, which `normal_png` differentiates into a normal map. They
# are authored as HEIGHT rather than as normals directly because a height field
# is a thing a person can reason about, and because the two derivatives then
# cannot disagree with each other.

def field(fn) -> list:
    """Sample a height function over the tile."""
    return [fn((x + 0.5) / SIZE, (y + 0.5) / SIZE)
            for y in range(SIZE) for x in range(SIZE)]


def f_smooth() -> list:
    """No finish. The off state, so the palette has one."""
    return [0.0] * (SIZE * SIZE)


def f_plate() -> list:
    """A panel with a seam round it and rivets inside the seam.

    The default a warship wants: it says "this is armour plate" at a glance and
    it puts a line on every cell boundary, which is what makes a voxel hull read
    as built rather than as a solid lump.
    """
    grain = [value_noise(p, 0x51A7E + i) for i, p in enumerate((16, 32))]
    rivets = []
    n = 5
    for i in range(n):
        t = (i + 0.5) / n
        rivets += [(t, 0.085), (t, 0.915), (0.085, t), (0.915, t)]

    def h(x, y):
        d = min(x, 1 - x, y, 1 - y)
        v = smoothstep(0.0, 0.052, d) * 0.62      # seam groove at the edge
        v += smoothstep(0.052, 0.16, d) * 0.10    # and a shallow crown inside
        for (cx, cy) in rivets:
            dx, dy = wrap_delta(cx, x), wrap_delta(cy, y)
            r = (dx * dx + dy * dy) ** 0.5
            if r < 0.028:
                v += dome(r / 0.028) * 0.30
        return v + 0.045 * (fbm(x, y, grain) - 0.5)
    return field(h)


def f_ribbed() -> list:
    """Corrugation. Structural, cheap to read, and it gives a hull a direction."""
    grain = [value_noise(24, 0x0BB1)]

    def h(x, y):
        v = smoothstep(0.30, 0.92, tri(y * 5.0)) * 0.85
        d = min(x, 1 - x, y, 1 - y)
        v *= smoothstep(0.0, 0.04, d)             # the corrugation stops at the seam
        return v + 0.035 * (fbm(x, y, grain) - 0.5)
    return field(h)


def _voronoi(cols: int, rows: int, jitter: float, seed: int):
    """Nearest and second nearest centre on a staggered lattice.

    Staggered because a Voronoi of a staggered lattice is a field of hexagons,
    which is what ablative tiling looks like, and jittering the same lattice
    turns it into cracked plating without a second implementation.
    """
    r = rng(seed)
    jx = [[(r() - 0.5) * jitter / cols for _ in range(cols)] for _ in range(rows)]
    jy = [[(r() - 0.5) * jitter / rows for _ in range(cols)] for _ in range(rows)]

    def centre(c: int, rw: int):
        cc, rr = c % cols, rw % rows
        return ((c + 0.5 * (rw & 1) + 0.5) / cols + jx[rr][cc],
                (rw + 0.5) / rows + jy[rr][cc])

    def at(x: float, y: float):
        rw0 = int(y * rows)
        c0 = int(x * cols)
        d1 = d2 = 9.0
        for dr in (-1, 0, 1):
            for dc in (-2, -1, 0, 1, 2):
                cx, cy = centre(c0 + dc, rw0 + dr)
                dx, dy = wrap_delta(cx, x), wrap_delta(cy, y)
                d = (dx * dx + dy * dy) ** 0.5
                if d < d1:
                    d2, d1 = d1, d
                elif d < d2:
                    d2 = d
        return d1, d2

    return at


def f_hex() -> list:
    """Ablative tiling. The one that most says "this ship expects to be shot"."""
    vor = _voronoi(4, 5, 0.0, 0x11E1)

    def h(x, y):
        d1, d2 = vor(x, y)
        edge = smoothstep(0.0, 0.030, d2 - d1)     # groove along every seam
        return edge * (0.62 + 0.24 * smoothstep(0.10, 0.0, d1))
    return field(h)


def f_cracked() -> list:
    """The same lattice, thrown out of true: irregular plating, welded up."""
    vor = _voronoi(5, 6, 0.85, 0x3C4A)
    grain = [value_noise(p, 0x77D + i) for i, p in enumerate((12, 28))]

    def h(x, y):
        d1, d2 = vor(x, y)
        edge = smoothstep(0.0, 0.024, d2 - d1)
        weld = smoothstep(0.030, 0.014, d2 - d1) * 0.22   # a proud bead in the seam
        return edge * 0.60 + weld + 0.06 * (fbm(x, y, grain) - 0.5)
    return field(h)


def f_tread() -> list:
    """Crossed grip bars. Deck plate, and it catches a light beautifully."""
    def bar(u, v):
        ridge = smoothstep(0.34, 0.95, tri(v))
        gap = smoothstep(0.06, 0.20, tri(u * 0.5))
        return ridge * gap

    def h(x, y):
        a = bar((x + y) * 4.0, (x - y) * 4.0)
        b = bar((x - y) * 4.0 + 0.5, (x + y) * 4.0 + 0.5)
        return (a if a > b else b) * 0.80
    return field(h)


def f_greeble() -> list:
    """Kit bashed boxes. The look every model maker glued on to sell scale.

    Many small boxes rather than a few big ones, and panel lines between them.
    A normal map only shows an EDGE, so a handful of wide flat lids is a nearly
    blank tile: what makes greebling read is edge density, not box count.
    """
    r = rng(0x6EEB)
    boxes = []
    for _ in range(30):
        w = 0.030 + r() * 0.085
        t = 0.030 + r() * 0.070
        boxes.append((r(), r(), w, t, 0.22 + r() * 0.70))
    grain = [value_noise(20, 0x6EEC)]

    def h(x, y):
        v = 0.10
        # Panel lines under everything, so the flat ground is not featureless.
        v -= smoothstep(0.045, 0.0, tri(x * 3.0) * 0.5) * 0.10
        v -= smoothstep(0.045, 0.0, tri(y * 2.0) * 0.5) * 0.10
        for (cx, cy, w, t, hgt) in boxes:
            dx, dy = abs(wrap_delta(cx, x)), abs(wrap_delta(cy, y))
            if dx < w and dy < t:
                # A chamfer rather than a cliff, or the normals are a hard edge
                # one texel wide and alias into nothing at distance.
                e = min(smoothstep(0.0, 0.014, w - dx), smoothstep(0.0, 0.014, t - dy))
                b = hgt * e
                if b > v:
                    v = b
        return v + 0.04 * (fbm(x, y, grain) - 0.5)
    return field(h)


def f_weave() -> list:
    """Woven composite: strips over and under, with the gaps between them.

    The gap is the whole thing. A first cut had the strips meeting edge to edge,
    which is not a weave but a checkerboard of raised squares: what says woven
    is seeing one strip pass BEHIND another and the ground showing between them.
    """
    n = 4.0
    band = 0.78          # how much of each period the strip occupies

    def strip(t: float) -> float:
        """Height across one strip, 0 in the gap either side of it."""
        k = abs(2.0 * frac(t) - 1.0) / band
        return 0.0 if k >= 1.0 else dome(k)

    def h(x, y):
        warp = strip(x * n)                       # runs along y
        weft = strip(y * n)                       # runs along x
        over_warp = ((int(x * n) + int(y * n)) & 1) == 0
        if warp <= 0.0 and weft <= 0.0:
            return 0.0                            # the ground, seen through
        if over_warp:
            return warp * 0.88 + weft * 0.30
        return weft * 0.88 + warp * 0.30
    return field(h)


def f_battered() -> list:
    """Dents, scoring and a couple of deep gouges. A hull with a history."""
    dents = [value_noise(p, 0xBA77 + i) for i, p in enumerate((6, 13, 26))]
    r = rng(0xBA78)
    gouges = [(r(), r(), r() - 0.5, r() - 0.5) for _ in range(3)]

    def h(x, y):
        v = 0.55 + 0.30 * (fbm(x, y, dents) - 0.5)
        for (ax, ay, dx, dy) in gouges:
            ln = (dx * dx + dy * dy) ** 0.5
            if ln < 1e-6:
                continue
            ux, uy = dx / ln, dy / ln
            px, py = wrap_delta(ax, x), wrap_delta(ay, y)
            t = px * ux + py * uy
            t = -0.14 if t < -0.14 else (0.14 if t > 0.14 else t)
            ox, oy = px - ux * t, py - uy * t
            d = (ox * ox + oy * oy) ** 0.5
            v -= smoothstep(0.030, 0.0, d) * 0.42
        return v
    return field(h)



def f_crate() -> list:
    """Deep vertical corrugation between a top and a bottom rail.

    A shipping container, which is the one surface a civil hull is mostly made
    of. `ribbed` is a structural rib and this is not the same thing: a
    container's corrugation is deeper, squarer, and stops dead at a rail top
    and bottom, and that stop is most of what makes the shape read as a box
    rather than as a panel.
    """
    grime = value_noise(19, 0xC2A7)

    def h(x, y):
        rail = 0.0
        # The two rails, flat and proud, with a chamfer into the corrugation.
        if y < 0.11:
            rail = smoothstep(0.11, 0.085, y)
        elif y > 0.89:
            rail = smoothstep(0.89, 0.915, y)
        # A square wave along x, chamfered, so the normal map has a face to
        # catch a light on rather than one hard edge.
        w = tri(x * 8.0)
        corr = smoothstep(0.30, 0.44, w) * 0.72
        v = 0.30 + 0.62 * rail + corr * (1.0 - rail)
        return v + 0.035 * (grime(x * 3.0, y * 3.0) - 0.5)
    return field(h)

ARMOUR = [
    ("smooth",   "Smooth",       "No finish. Bare plate, and the off state.",              f_smooth,   0.0),
    ("plate",    "Riveted",      "Panel seams with rivets. The default a warship wants.",  f_plate,   34.0),
    ("ribbed",   "Corrugated",   "Structural ribbing. Gives a hull a grain.",              f_ribbed,  30.0),
    ("hex",      "Ablative",     "Hex tiling. Says the ship expects to be shot at.",       f_hex,     32.0),
    ("cracked",  "Patched",      "Irregular plates, welded. A hull with a past.",          f_cracked, 30.0),
    ("tread",    "Grip deck",    "Crossed bars. Catches a raking light.",                  f_tread,   26.0),
    ("greeble",  "Greebled",     "Kit bashed boxes. Scale, the way a model maker sells it.", f_greeble, 26.0),
    ("weave",    "Composite",    "Woven strips, over and under.",                          f_weave,   24.0),
    ("battered", "Battered",     "Dents and gouges. Hard use, before a shot lands.",        f_battered, 26.0),
    ("crate",    "Container",    "Deep corrugation between two rails. A box, not a panel.", f_crate,   32.0),
]


# --------------------------------------------------------- window decals --
#
# A decal comes in two halves: a height field that recesses the glass behind a
# frame, and an emission that says what colour the light is.
#
# It tiles like everything else, and it costs nothing to let it: every shape
# here is inset from the tile border with flat hull around it, so the seam is
# already invisible, and one wrap mode for every texture beats two. It also
# means two window cells side by side merge into ONE quad carrying two windows,
# which is what a row of portholes should be.
#
# The albedo is untouched on purpose. A cell keeps the armour colour a player
# painted it, and the window is a recess and a glow ON that colour, which is
# what makes a window painted onto a green hull look like a window in a green
# hull rather than a sticker.

WARM = (1.00, 0.86, 0.62)     # lived in: galley light, crew spaces
DEEP = (1.00, 0.62, 0.26)     # the falloff at the edge of a warm pane
COOL = (0.80, 0.92, 1.00)     # instrument light, which is what a bridge is
AMBER = (1.00, 0.72, 0.20)    # running lights and bay markers

# How dark the GLASS is, as a multiplier on whatever the hull is painted.
#
# This is the map the first two cuts did not have, and not having it is why a
# window with its light off looked like no window at all. Emission only ADDS:
# it can put a glow on a pane and it cannot take the hull's paint off one, so
# an unlit pane rendered as plain armour in the armour's own colour, which on a
# blue hull is a blue rectangle indistinguishable from the plating around it.
# A window is dark glass. It multiplies the paint down to nearly nothing and
# THEN the emission adds whatever is on inside, which is also why a lit pane
# now reads in its own colour instead of the hull's colour plus a wash.
GLASS = 0.06


def pane_lights(n: int, seed: int):
    """How many of `n` panes are on, dim or off, for one variant.

    Three states rather than a continuum, because at the size a cell is drawn
    the eye reads on, half and off and nothing between them. Deterministic from
    the seed, so a regeneration is bit identical and the same hull looks the
    same on both screens.
    """
    r = rng(seed)
    out = []
    for _ in range(n):
        x = r()
        out.append(1.0 if x > 0.58 else (0.55 if x > 0.34 else 0.0))
    return tuple(out)


def lerp3(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)


def _panes(spec):
    """Shared shape for every rectangular window: a list of (x0, y0, x1, y1, lit).

    `lit` is 0 to 1, and it is deliberately not 1 everywhere. A row of identical
    panes reads as a texture; one dark pane and one dim one reads as a ship with
    somebody aboard, and it costs nothing.

    But a dark pane is not an ABSENT one. The first cut set the fourth cabin to
    exactly zero, meaning nobody home, and at the size a cell is drawn that is
    a hole rather than a window: the frame is in the normal map but the eye is
    reading the emission, and black emission next to three lit panes looks like
    a window somebody forgot to draw. An unlit pane is still glass, and glass
    with no light behind it reflects the sky, so it keeps a little cool
    emission instead of none.
    """
    def height(x, y):
        v = 0.0
        for (x0, y0, x1, y1, _lit) in spec:
            if x0 - 0.045 < x < x1 + 0.045 and y0 - 0.045 < y < y1 + 0.045:
                # Frame proud of the hull, glass sunk behind it.
                v = max(v, 0.85)
            if x0 < x < x1 and y0 < y < y1:
                inset = min(x - x0, x1 - x, y - y0, y1 - y)
                v = min(v, 0.85 - smoothstep(0.0, 0.020, inset) * 0.85)
        return v

    def emission(x, y, hot, cold):
        for (x0, y0, x1, y1, lit) in spec:
            if x0 < x < x1 and y0 < y < y1:
                inset = min(x - x0, x1 - x, y - y0, y1 - y)
                k = smoothstep(0.0, 0.026, inset)
                # A pane with nothing on behind it emits nothing. It is still
                # obviously a window, because the glass map has taken the
                # hull's paint off it and left it black.
                return tuple(v * lit * (0.35 + 0.65 * k)
                             for v in lerp3(cold, hot, k))
        return (0.0, 0.0, 0.0)

    def glass(x, y):
        for (x0, y0, x1, y1, _lit) in spec:
            if x0 < x < x1 and y0 < y < y1:
                inset = min(x - x0, x1 - x, y - y0, y1 - y)
                return smoothstep(0.0, 0.012, inset)
        return 0.0

    return height, emission, glass


def w_porthole(v=0):
    """One round window. The unit a habitation deck is made of."""
    def height(x, y):
        dx, dy = x - 0.5, y - 0.5
        r = (dx * dx + dy * dy) ** 0.5
        frame = smoothstep(0.345, 0.300, r) * 0.9
        glass = smoothstep(0.268, 0.238, r) * 0.9
        return frame - glass

    def emission(x, y):
        dx, dy = x - 0.5, y - 0.5
        r = (dx * dx + dy * dy) ** 0.5
        if r >= 0.252:
            return (0.0, 0.0, 0.0)
        # Bright almost to the rim, then a quick falloff. A slow radial ramp
        # from the centre reads as a lit BALL rather than as a lit room.
        k = smoothstep(0.252, 0.205, r)
        return tuple(v * (0.30 + 0.70 * k) for v in lerp3(DEEP, WARM, k))

    def glass(x, y):
        dx, dy = x - 0.5, y - 0.5
        r = (dx * dx + dy * dy) ** 0.5
        return smoothstep(0.262, 0.248, r)
    return height, emission, glass


def w_panes(v=0):
    """Four square windows. A cabin block."""
    g, lit = 0.055, pane_lights(4, 0xCAB19 + v * 7919)
    spec, i = [], 0
    for by in (0.5 + g, 0.5 - g - 0.30):
        for bx in (0.5 + g, 0.5 - g - 0.30):
            spec.append((bx, by, bx + 0.30, by + 0.30, lit[i]))
            i += 1
    height, emission, glass = _panes(spec)
    return height, lambda x, y: emission(x, y, WARM, DEEP), glass


def w_strip(v=0):
    """A viewport band with mullions. A promenade, or a long corridor."""
    spec, n = [], 4
    lit = pane_lights(n, 0x57317 + v * 7919)
    for i in range(n):
        x0 = 0.10 + i * (0.80 / n)
        spec.append((x0 + 0.014, 0.40, x0 + 0.80 / n - 0.014, 0.60, lit[i]))
    height, emission, glass = _panes(spec)
    return height, lambda x, y: emission(x, y, WARM, DEEP), glass


def w_bridge(v=0):
    """One wide canted viewport. Where somebody is flying this thing."""
    def shape(x, y):
        # Canted: the top edge runs wider than the bottom one.
        half = 0.16 + 0.20 * smoothstep(0.30, 0.68, y)
        return abs(x - 0.5) < half and 0.32 < y < 0.68

    def height(x, y):
        if shape(x, y):
            d = min(0.68 - y, y - 0.32, 0.02 + 0.5 - abs(x - 0.5))
            return 0.85 - smoothstep(0.0, 0.028, d) * 0.85
        half = 0.16 + 0.20 * smoothstep(0.30, 0.68, y)
        near = abs(x - 0.5) < half + 0.05 and 0.28 < y < 0.72
        return 0.85 if near else 0.0

    def emission(x, y):
        if not shape(x, y):
            return (0.0, 0.0, 0.0)
        d = min(0.68 - y, y - 0.32)
        k = smoothstep(0.0, 0.05, d)
        return tuple(v * (0.45 + 0.55 * k) for v in lerp3(WARM, COOL, k))

    def glass(x, y):
        return 1.0 if shape(x, y) else 0.0
    return height, emission, glass


def w_beacons(v=0):
    """Three running lights. Not a window: the thing that makes a hull read as
    crewed at a range where a window is one pixel."""
    pts = [(0.5, 0.22), (0.5, 0.50), (0.5, 0.78)]

    def height(x, y):
        v = 0.0
        for (cx, cy) in pts:
            dx, dy = x - cx, y - cy
            r = (dx * dx + dy * dy) ** 0.5
            if r < 0.075:
                v = max(v, dome(r / 0.075) * 0.75)
        return v

    def emission(x, y):
        for (cx, cy) in pts:
            dx, dy = x - cx, y - cy
            r = (dx * dx + dy * dy) ** 0.5
            if r < 0.062:
                k = smoothstep(0.062, 0.0, r)
                return tuple(v * (0.25 + 0.75 * k) for v in lerp3(AMBER, (1, 1, 0.92), k))
        return (0.0, 0.0, 0.0)

    def glass(x, y):
        out = 0.0
        for (cx, cy) in pts:
            dx, dy = x - cx, y - cy
            r = (dx * dx + dy * dy) ** 0.5
            out = max(out, smoothstep(0.068, 0.056, r))
        return out
    return height, emission, glass


def w_hangar(v=0):
    """A bay mouth: deeply recessed, lit from inside, with a lip round it."""
    x0, x1, y0, y1 = 0.14, 0.86, 0.22, 0.78

    def height(x, y):
        if x0 < x < x1 and y0 < y < y1:
            inset = min(x - x0, x1 - x, y - y0, y1 - y)
            return 0.9 - smoothstep(0.0, 0.045, inset) * 0.9
        if x0 - 0.06 < x < x1 + 0.06 and y0 - 0.06 < y < y1 + 0.06:
            return 0.9
        return 0.35

    def emission(x, y):
        if not (x0 < x < x1 and y0 < y < y1):
            return (0.0, 0.0, 0.0)
        inset = min(x - x0, x1 - x, y - y0, y1 - y)
        k = smoothstep(0.0, 0.12, inset)
        # Floor lit: brighter low in the opening, because the deck is what a
        # bay light actually falls on.
        floor = smoothstep(0.70, 0.24, y)
        c = lerp3(DEEP, (1.0, 0.93, 0.80), k)
        return tuple(v * (0.20 + 0.80 * k) * (0.55 + 0.45 * floor) for v in c)

    def glass(x, y):
        if not (x0 < x < x1 and y0 < y < y1):
            return 0.0
        inset = min(x - x0, x1 - x, y - y0, y1 - y)
        return smoothstep(0.0, 0.012, inset)
    return height, emission, glass



def w_cargo(v=0):
    """A container door: two leaves, a centre seam, latch rods and a placard.

    Not a window at all in the sense the others are, and that is the point. A
    civil hull is mostly boxes, and what tells a viewer they are boxes is the
    DOOR on the end of one: a seam down the middle, four vertical rods, and a
    lit placard where the manifest goes. The placard is the only part that
    emits, so a rack of these reads as cargo with one light on it rather than
    as a wall of windows.
    """
    lit = pane_lights(1, 0x3A11D + v * 7919)[0]
    rods = (0.24, 0.40, 0.60, 0.76)
    # Which corner the placard sits in, so a stack of containers is not a
    # stack of one container.
    px = 0.14 + 0.52 * ((v % 2))
    py = 0.60 if (v // 2) % 2 == 0 else 0.24

    def door(x, y):
        return 0.08 < x < 0.92 and 0.10 < y < 0.90

    def height(x, y):
        if not door(x, y):
            return 0.86
        inset = min(x - 0.08, 0.92 - x, y - 0.10, 0.90 - y)
        v0 = 0.86 - smoothstep(0.0, 0.030, inset) * 0.50
        # The seam down the middle, cut into both leaves.
        v0 -= smoothstep(0.020, 0.0, abs(x - 0.5)) * 0.34
        # Latch rods, standing proud.
        for r in rods:
            v0 += smoothstep(0.016, 0.008, abs(x - r)) * 0.30
        # The placard plate.
        if px < x < px + 0.20 and py < y < py + 0.14:
            v0 = 0.72
        return v0

    def emission(x, y):
        if not (px < x < px + 0.20 and py < y < py + 0.14):
            return (0.0, 0.0, 0.0)
        inset = min(x - px, px + 0.20 - x, y - py, py + 0.14 - y)
        k = smoothstep(0.0, 0.018, inset)
        return tuple(c * lit * (0.30 + 0.70 * k) for c in lerp3(DEEP, AMBER, k))

    def glass(x, y):
        if not (px < x < px + 0.20 and py < y < py + 0.14):
            return 0.0
        inset = min(x - px, px + 0.20 - x, y - py, py + 0.14 - y)
        return smoothstep(0.0, 0.010, inset)
    return height, emission, glass


def w_promenade(v=0):
    """A passenger deck: one long band of glass, close mullioned, warm.

    `strip` is four panes across a corridor. This is a liner's promenade, so it
    is twice as many and twice as tall, and almost all of it is lit: what makes
    a passenger ship read as one from across a battlefield is that every deck
    is on when a warship beside it is dark.
    """
    n = 8
    lit = pane_lights(n, 0x7DE41 + v * 7919)
    spec = []
    for i in range(n):
        x0 = 0.06 + i * (0.88 / n)
        # A liner is lit: three quarters of a dim pane is still a lit deck.
        spec.append((x0 + 0.010, 0.30, x0 + 0.88 / n - 0.010, 0.70,
                     0.55 + 0.45 * lit[i]))
    height, emission, glass = _panes(spec)
    return height, lambda x, y: emission(x, y, WARM, DEEP), glass


def w_louvre(v=0):
    """A radiator face: horizontal slats over a hot gap.

    A tank and a reactor housing are not rooms and should not wear panes, but
    a blank wall on a civil hull is the thing that made a freighter read as a
    grey brick. Slats give the surface a direction and the gaps between them
    glow, which is what a heat exchanger looks like from outside.
    """
    n = 7

    def slat(y):
        return tri(y * n)

    def height(x, y):
        if not (0.10 < x < 0.90 and 0.08 < y < 0.92):
            return 0.80
        t = slat(y)
        return 0.30 + 0.55 * smoothstep(0.28, 0.46, t)

    def emission(x, y):
        if not (0.10 < x < 0.90 and 0.08 < y < 0.92):
            return (0.0, 0.0, 0.0)
        t = slat(y)
        k = smoothstep(0.30, 0.06, t)
        if k <= 0.0:
            return (0.0, 0.0, 0.0)
        edge = smoothstep(0.10, 0.16, x) * smoothstep(0.90, 0.84, x)
        return tuple(c * 0.55 * k * edge for c in lerp3(DEEP, AMBER, k))

    def glass(x, y):
        if not (0.10 < x < 0.90 and 0.08 < y < 0.92):
            return 0.0
        return smoothstep(0.30, 0.10, slat(y))
    return height, emission, glass

# The last number is how many VARIANTS the decal carries, side by side in one
# texture.
#
# A run of cabins with the same panes lit in every one of them reads as a
# repeating texture rather than as a ship with people in it, which is the same
# thing sixteen ember tiles exist to avoid. There is no per cell attribute to
# hang a choice on here, and there does not need to be: neighbouring window
# cells merge into ONE quad whose UV runs 0 to N in cells, so a texture holding
# V patterns side by side and sampled at 1/V cycles through them, one per cell,
# for nothing but the width. Seven and five rather than four and six, because a
# period sharing a factor with a run lines the same pattern up under the same
# part of the ship every time round.
WINDOWS = [
    ("porthole", "Porthole",  "One round window. The unit a habitation deck is made of.", w_porthole, 1),
    ("panes",    "Cabins",    "Four panes, lit differently down the run.",                w_panes,    7),
    ("strip",    "Promenade", "A viewport band with mullions.",                           w_strip,    5),
    ("bridge",   "Bridge",    "A canted viewport, lit by instruments.",                   w_bridge,   1),
    ("beacons",  "Beacons",   "Running lights. Reads as crewed at map range.",            w_beacons,  1),
    ("hangar",   "Bay mouth", "A recessed opening, lit from the deck inside.",            w_hangar,   1),
    ("cargo",    "Container", "Door leaves, latch rods and a lit manifest placard.",       w_cargo,    4),
    ("promenade", "Liner deck", "Eight panes of a passenger promenade, mostly lit.",       w_promenade, 4),
    ("louvre",   "Radiator",  "Slats over a hot gap. What a tank wears instead of panes.", w_louvre,   1),
]

WINDOW_STRENGTH = 26.0


# ----------------------------------------------------------- environment --
#
# What a metal has to reflect.
#
# A metal with no environment is BLACK: metalness says "this surface shows you
# what is around it", and around it is nothing until something is there. So the
# moment a palette colour carries metalness, the scene needs one of these, and
# it may as well be a file with a generator like every other texture here
# rather than a gradient built at load time (GUIDELINES 3).
#
# Equirectangular, and authored directly in that space: u is the way round, v
# is up. That means no trigonometry, which is what keeps `--check` honest on a
# machine that is not this one.
#
# Low frequency on purpose. PMREM blurs this into roughness levels before
# anything reflects it, so fine detail is detail that gets averaged away; what
# survives and what actually matters is the big split between a lit sky above
# and a dark ground below. The two ends are the same colours the battlefield's
# hemisphere light already uses, so a reflection agrees with the lighting
# rather than arguing with it.
ENV_W, ENV_H = 512, 256
ENV_SKY = (0.373, 0.498, 0.627)     # 0x5f7fa0, the hemisphere light's sky
ENV_MID = (0.098, 0.129, 0.180)
ENV_GROUND = (0.039, 0.055, 0.078)  # 0x0a0e14, its ground and the background
# A cool wash across the sky, so a curved hull does not reflect one flat tone.
ENV_WASH = (0.16, 0.30, 0.42)


def env_png() -> bytes:
    """The battlefield's surroundings, as one equirectangular strip."""
    cloud = [value_noise(p, 0xE0A5 + i) for i, p in enumerate((3, 6))]
    grain = value_noise(12, 0xE0A7)
    out = []
    for y in range(ENV_H):
        # 0 at the top of the sphere, 1 at the bottom.
        t = (y + 0.5) / ENV_H
        for x in range(ENV_W):
            u = (x + 0.5) / ENV_W
            base = (lerp3(ENV_SKY, ENV_MID, smoothstep(0.0, 0.55, t))
                    if t < 0.55
                    else lerp3(ENV_MID, ENV_GROUND, smoothstep(0.55, 1.0, t)))
            # A slow drift round the sphere, strongest up top where a hull's
            # shoulders catch it.
            k = fbm(u, t, cloud) * (1.0 - smoothstep(0.25, 0.85, t))
            c = lerp3(base, ENV_WASH, 0.35 * k)
            # A little tooth, so a mirror surface is not a flat field.
            g = (grain(u, t) - 0.5) * 0.03
            out.append((c[0] + g, c[1] + g, c[2] + g))
    return rgb_png(out, ENV_W, ENV_H)


# ------------------------------------------------------------------ main --

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="fail if anything on disk differs from a fresh run")
    args = ap.parse_args()

    files: list = []      # (relative path, bytes)

    for key, _label, _blurb, build, strength in ARMOUR:
        h = build()
        files.append((f"armour_{key}_n.png", normal_png(h, SIZE, SIZE, strength, True)))

    for key, _label, _blurb, build, variants in WINDOWS:
        made = [build(v) for v in range(variants)]
        wide = SIZE * variants
        h, e, c = [], [], []
        for y in range(SIZE):
            for x in range(wide):
                # Which variant this column is in, and where inside it.
                height, emission, glass = made[x // SIZE]
                u = ((x % SIZE) + 0.5) / SIZE
                w = (y + 0.5) / SIZE
                h.append(height(u, w))
                e.append(emission(u, w))
                # The glass map, as a MULTIPLIER on whatever the hull is
                # painted: white where the hull shows through and near black
                # on the glass.
                g = glass(u, w)
                t = 1.0 + (GLASS - 1.0) * g
                c.append((t, t, t))
        files.append((f"window_{key}_n.png",
                      normal_png(h, wide, SIZE, WINDOW_STRENGTH, True)))
        files.append((f"window_{key}_e.png", rgb_png(e, wide, SIZE)))
        files.append((f"window_{key}_c.png", rgb_png(c, wide, SIZE)))

    files.append(("env.png", env_png()))

    # The same bytes again as data URIs, because GUIDELINES 2.1 gives a mockup
    # no network at all and that includes a file sitting next to it.
    lines = ["// GENERATED by tools/make_surface_textures.py. Do not edit.",
             "//",
             "// The PNGs in tex/ as data URIs, because a mockup takes no network",
             "// (GUIDELINES 2.1) and an <img src> to a sibling file is a request.",
             "window.FT_TEX = {"]
    for path, png in files:
        name = Path(path).stem
        lines.append(f"  {name}: 'data:image/png;base64,{b64encode(png).decode()}',")
    lines.append("};")
    js = ("\n".join(lines) + "\n").encode()

    ok = True
    for path, data in files:
        target = OUT / path
        if args.check:
            ok = emit(target, data, True) and ok
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    bundle = MOCKUP / "textures.js"
    if args.check:
        ok = emit(bundle, js, True) and ok
    else:
        bundle.parent.mkdir(parents=True, exist_ok=True)
        bundle.write_bytes(js)

    total = sum(len(d) for _p, d in files)
    if args.check:
        if not ok:
            return 1
        print(f"surface textures match their generator ({len(files)} files)")
        return 0

    print(f"wrote {len(files)} files to {OUT}")
    print(f"  {len(ARMOUR)} armour finishes, {len(WINDOWS)} window decals, "
          f"{SIZE}x{SIZE} each")
    print(f"  {total} bytes of PNG, {len(js)} bytes of data URI bundle")
    for path, data in files:
        print(f"    {len(data):7d}  {path}  "
              f"{hashlib.sha256(data).hexdigest()[:8]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
