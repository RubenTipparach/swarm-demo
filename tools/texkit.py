#!/usr/bin/env python3
"""What every texture generator here shares: noise, a PNG encoder, a normal
map encoder, and the drift check.

GUIDELINES 5.1: divergent paths for like functionality are a defect. The ember
texture arrived with its own value noise and its own PNG writer, and the moment
a second generator wanted either one there were two copies to keep in step. So
they live here and both scripts call them.

WHY THERE IS NO `sin` IN THIS FILE. A generated asset is committed and CI runs
`--check`, which compares the file on disk against a fresh generation BYTE FOR
BYTE on a different machine. `sin`, `cos` and friends lower to intrinsics that
differ in the last bits between platforms, so one of those in a height field is
a check that fails on somebody else's runner for no reason anybody can see. Add,
subtract, multiply, divide and `sqrt` are all exactly specified by IEEE-754, so
everything here is built out of those: ridges are triangle waves, curves are
polynomials, and circles use `sqrt`. It is the same rule the simulation keeps
(CLAUDE.md, physics and determinism) for the same reason, applied to a build
product rather than to a turn.

No third party imaging library: the PNG encoder is thirty lines and the
alternative is a dependency in a repo that has none.
"""

import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


# ------------------------------------------------------------------ noise --

def rng(seed: int):
    """A tiny deterministic generator, so a regeneration is bit identical."""
    state = seed & 0xFFFFFFFF

    def nxt() -> float:
        nonlocal state
        state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
        return state / 0x100000000

    return nxt


def value_noise(period: int, seed: int):
    """Periodic value noise on a `period` x `period` lattice.

    Periodic so a tile meets itself without a seam, which matters because a
    face may be seen next to another face carrying the same tile.
    """
    r = rng(seed)
    lattice = [[r() for _ in range(period)] for _ in range(period)]

    def at(x: float, y: float) -> float:
        fx, fy = x * period, y * period
        x0, y0 = int(math.floor(fx)), int(math.floor(fy))
        tx, ty = fx - x0, fy - y0
        # Smoothstep, so the lattice does not show as a grid of diamonds.
        tx = tx * tx * (3 - 2 * tx)
        ty = ty * ty * (3 - 2 * ty)
        a = lattice[y0 % period][x0 % period]
        b = lattice[y0 % period][(x0 + 1) % period]
        c = lattice[(y0 + 1) % period][x0 % period]
        d = lattice[(y0 + 1) % period][(x0 + 1) % period]
        return (a * (1 - tx) + b * tx) * (1 - ty) + (c * (1 - tx) + d * tx) * ty

    return at


def fbm(x: float, y: float, octaves) -> float:
    total, amp, norm = 0.0, 1.0, 0.0
    for oct_at in octaves:
        total += amp * oct_at(x, y)
        norm += amp
        amp *= 0.5
    return total / norm


# ------------------------------------------------------------------ maths --

def clamp01(t: float) -> float:
    return 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)


def smoothstep(a: float, b: float, t: float) -> float:
    """The usual one, and it works with a > b for a falling edge."""
    if a == b:
        return 0.0 if t < a else 1.0
    k = (t - a) / (b - a)
    k = 0.0 if k < 0.0 else (1.0 if k > 1.0 else k)
    return k * k * (3 - 2 * k)


def frac(t: float) -> float:
    return t - math.floor(t)


def tri(t: float) -> float:
    """Triangle wave of period 1, 0 at the ends and 1 in the middle.

    This is what a ridge is made of here instead of a sine: exactly
    representable, and identical on every machine.
    """
    f = frac(t)
    return 2.0 * f if f < 0.5 else 2.0 * (1.0 - f)


def dome(t: float) -> float:
    """A hemisphere over 0..1: 1 at the centre, 0 at the rim.

    `sqrt` is the one transcendental looking thing allowed, because IEEE-754
    specifies it exactly and it is therefore portable.
    """
    if t >= 1.0:
        return 0.0
    return (1.0 - t * t) ** 0.5


def wrap_delta(a: float, b: float) -> float:
    """b - a on a tile that repeats, taking the short way round."""
    d = b - a
    while d > 0.5:
        d -= 1.0
    while d < -0.5:
        d += 1.0
    return d


# ------------------------------------------------------------------- png ---

def encode_png(width: int, height: int, rgb: bytes) -> bytes:
    """`rgb` is width * height * 3 bytes, row major, top row first."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)                      # filter 0, none
        raw += rgb[y * width * 3:(y + 1) * width * 3]

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))


def emit(out: Path, png: bytes, check: bool) -> bool:
    """Write the file, or in check mode report whether it has drifted.

    Returns True when everything is as it should be.
    """
    if check:
        if not out.exists():
            print(f"{out} is missing; run this script without --check")
            return False
        if out.read_bytes() != png:
            print(f"{out} has drifted from its generator; regenerate it")
            return False
        return True
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(png)
    return True


# --------------------------------------------------------------- normals ---

def normal_png(height_field, w: int, h: int, strength: float, tiling: bool = True) -> bytes:
    """A height field as a tangent space normal map.

    `height_field` is a flat list of w * h heights in roughly 0..1. Not square,
    because a decal that wants several variants holds them side by side and
    lets repeat wrapping cycle through them.

    Green is +Y up, which is the OpenGL convention and the one three.js reads,
    so a ridge lit from above looks lit from above rather than inside out.

    Sampling wraps for a tiling texture and clamps for a decal, because a decal
    is one per cell and wrapping its edge would pull the far side of the window
    into the frame.
    """
    def at(ix: int, iy: int) -> float:
        if tiling:
            return height_field[(iy % h) * w + (ix % w)]
        ix = 0 if ix < 0 else (w - 1 if ix >= w else ix)
        iy = 0 if iy < 0 else (h - 1 if iy >= h else iy)
        return height_field[iy * w + ix]

    out = bytearray()
    for y in range(h):
        for x in range(w):
            # Central differences. The step is one texel, so `strength` is in
            # height units per texel and a bigger texture is not automatically
            # a bumpier one.
            dx = (at(x + 1, y) - at(x - 1, y)) * 0.5 * strength
            dy = (at(x, y + 1) - at(x, y - 1)) * 0.5 * strength
            # The surface is (x, y, h), so its normal is (-dh/dx, -dh/dy, 1).
            # y is flipped because image rows run down and the map's y runs up.
            nx, ny, nz = -dx, dy, 1.0
            inv = 1.0 / ((nx * nx + ny * ny + nz * nz) ** 0.5)
            nx, ny, nz = nx * inv, ny * inv, nz * inv
            out += bytes((
                int(255.0 * (nx * 0.5 + 0.5) + 0.5),
                int(255.0 * (ny * 0.5 + 0.5) + 0.5),
                int(255.0 * (nz * 0.5 + 0.5) + 0.5)))
    return encode_png(w, h, bytes(out))


def rgb_png(colour_field, w: int, h: int) -> bytes:
    """A flat list of w * h (r, g, b) triples in 0..1 as a PNG."""
    out = bytearray()
    for r, g, b in colour_field:
        out += bytes((
            int(255.0 * (0.0 if r < 0 else (1.0 if r > 1 else r)) + 0.5),
            int(255.0 * (0.0 if g < 0 else (1.0 if g > 1 else g)) + 0.5),
            int(255.0 * (0.0 if b < 0 else (1.0 if b > 1 else b)) + 0.5)))
    return encode_png(w, h, bytes(out))
