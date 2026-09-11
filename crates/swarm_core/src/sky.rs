//! The sky: a nebula baked into a cubemap, and the stars as points.
//!
//! Ported from redux-tribes `sky.ts`, which is itself the archive's
//! `Procgen_Space_Skybox.shadergraph` down to the octave counts: two layers of
//! turbulence with independent octaves, frequency and amplitude, a mask that
//! passes about a fifth of the sky, and per mission recolouring that varies
//! two colours and nothing else.
//!
//! Baked, because two layers of fBm per pixel of a full sky sixty times a
//! second is not a thing a small GPU does; baked on the CPU here rather than
//! by a render pass, because the core has no renderer and a sky that a test
//! can measure is worth the second at launch. Value noise, folded per octave:
//! folding the finished sum was the trap sky.ts fell into first, because
//! eight octaves of value noise concentrate hard round 0.5 and `1 - |2n - 1|`
//! is maximal there, so that version painted the whole sky. Per octave it is
//! dark nearly everywhere with filaments where the octaves agree, and the
//! test below holds the port to the distribution sky.ts measured.
//!
//! The stars are NOT in the texture. A cube face is 512 texels across 90
//! degrees and a star is a point, and no texture survives being magnified
//! three times; sky.ts learned that and moved them to geometry, at the
//! Voronoi feature points the shader used to measure rays against. Same here.

/// Two colours and a seed: what a mission's sky is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPreset {
    /// The nebula, linear RGB.
    pub a: [f32; 3],
    /// The ground it sits over, near black in every archived mission.
    pub b: [f32; 3],
    /// Reseeds the whole sky. `Fractal_offset` in the original.
    pub seed: [f32; 3],
}

fn srgb_hex(hex: u32) -> [f32; 3] {
    let c = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [c((hex >> 16) & 0xFF), c((hex >> 8) & 0xFF), c(hex & 0xFF)]
}

impl SkyPreset {
    /// `space_mission_4`: green over near black purple, the archived
    /// Skirmish scene's own sky.
    pub fn skirmish() -> Self {
        SkyPreset { a: srgb_hex(0x00714b), b: srgb_hex(0x0a0616), seed: [0.0, 0.0, 0.0] }
    }
    pub fn duel() -> Self {
        SkyPreset { a: srgb_hex(0x1d4d86), b: srgb_hex(0x05070f), seed: [11.3, 4.1, 27.7] }
    }
    pub fn binary() -> Self {
        SkyPreset { a: srgb_hex(0x7a3560), b: srgb_hex(0x0b0512), seed: [17.5, 28.3, 2.2] }
    }
}

// 3.4's own numbers.
const OCT_1: u32 = 8;
const FREQ_1: f32 = 1.5;
const AMP_1: f32 = 1.0;
const OCT_2: u32 = 5;
const FREQ_2: f32 = 3.0;
const AMP_2: f32 = 0.5;
const FUZZ: f32 = 0.3;
/// Where the mask sits. 0.42 passes about a fifth of the sky.
const HIGH: f32 = 0.42;
/// How far the nebula may climb above the ground colour.
const NEB_GAIN: f32 = 0.55;
const REMAP_IN: [f32; 2] = [0.41, 2.24];
const REMAP_OUT: [f32; 2] = [0.0, 1.84];

#[inline]
fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// `hash13` from the shader: no `sin`, so it is the same on every machine.
fn hash13(p: [f32; 3]) -> f32 {
    let mut p = [fract(p[0] * 0.1031), fract(p[1] * 0.1031), fract(p[2] * 0.1031)];
    let d = p[0] * (p[1] + 33.33) + p[1] * (p[2] + 33.33) + p[2] * (p[0] + 33.33);
    p[0] += d;
    p[1] += d;
    p[2] += d;
    fract((p[0] + p[1]) * p[2])
}

#[inline]
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Value noise, smoothstepped.
fn noise3(p: [f32; 3]) -> f32 {
    let i = [p[0].floor(), p[1].floor(), p[2].floor()];
    let f = [p[0] - i[0], p[1] - i[1], p[2] - i[2]];
    let f = [f[0] * f[0] * (3.0 - 2.0 * f[0]), f[1] * f[1] * (3.0 - 2.0 * f[1]), f[2] * f[2] * (3.0 - 2.0 * f[2])];
    let h = |dx: f32, dy: f32, dz: f32| hash13([i[0] + dx, i[1] + dy, i[2] + dz]);
    let n00 = mix(h(0.0, 0.0, 0.0), h(1.0, 0.0, 0.0), f[0]);
    let n10 = mix(h(0.0, 1.0, 0.0), h(1.0, 1.0, 0.0), f[0]);
    let n01 = mix(h(0.0, 0.0, 1.0), h(1.0, 0.0, 1.0), f[0]);
    let n11 = mix(h(0.0, 1.0, 1.0), h(1.0, 1.0, 1.0), f[0]);
    mix(mix(n00, n10, f[1]), mix(n01, n11, f[1]), f[2])
}

/// Turbulence: octaves of noise, each folded about zero BEFORE it is summed.
pub fn turb(p: [f32; 3], octaves: u32, freq: f32, amp: f32) -> f32 {
    let (mut sum, mut norm, mut f, mut a) = (0.0, 0.0, freq, amp);
    for _ in 0..octaves {
        sum += a * (2.0 * noise3([p[0] * f, p[1] * f, p[2] * f]) - 1.0).abs();
        norm += a;
        f *= 2.0;
        a *= 0.5;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn remap(v: f32, i: [f32; 2], o: [f32; 2]) -> f32 {
    let t = (v - i[0]) / (i[1] - i[0]).max(1e-5);
    o[0] + t.clamp(0.0, 1.0) * (o[1] - o[0])
}

/// The sky in one direction, linear RGB. The shader's `main`, minus the stars.
pub fn sky_at(preset: &SkyPreset, dir: [f32; 3]) -> [f32; 3] {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
    let d = [dir[0] / len, dir[1] / len, dir[2] / len];
    let p = [d[0] + preset.seed[0], d[1] + preset.seed[1], d[2] + preset.seed[2]];
    let t1 = turb(p, OCT_1, FREQ_1, AMP_1);
    let t2 = turb([p[0] + 17.0, p[1] + 17.0, p[2] + 17.0], OCT_2, FREQ_2, AMP_2);
    let mask = smoothstep(HIGH - FUZZ * 0.5, HIGH + FUZZ * 0.5, t1 * 0.65 + t2 * 0.35);
    let density = remap(t1 * 1.4 + t2 * 0.8, REMAP_IN, REMAP_OUT);
    let gas = (mask * density).clamp(0.0, 1.0) * NEB_GAIN;
    let tint = 0.22 * smoothstep(0.72, 1.0, t2) * density * NEB_GAIN;
    std::array::from_fn(|c| preset.b[c] + preset.a[c] * gas + preset.a[c] * tint)
}

/// The direction a texel of a cube face looks in, wgpu's face order (+x -x
/// +y -y +z -z), `u` and `v` in 0..1 with `v` down the image.
pub fn cube_dir(face: usize, u: f32, v: f32) -> [f32; 3] {
    let a = 2.0 * u - 1.0;
    let b = 2.0 * v - 1.0;
    match face {
        0 => [1.0, -b, -a],
        1 => [-1.0, -b, a],
        2 => [a, 1.0, b],
        3 => [a, -1.0, -b],
        4 => [a, -b, 1.0],
        _ => [-a, -b, -1.0],
    }
}

/// Bake the six faces, `size` texels a side, as linear RGB rows of texels in
/// face order. `size * size * 6` colours.
pub fn bake_cubemap(preset: &SkyPreset, size: usize) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(size * size * 6);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let u = (x as f32 + 0.5) / size as f32;
                let v = (y as f32 + 0.5) / size as f32;
                out.push(sky_at(preset, cube_dir(face, u, v)));
            }
        }
    }
    out
}

/// A float as the sixteen bit float a `Rgba16Float` texture stores, rounded
/// to nearest even, for a sky that must not band: sixteen bits of float has
/// enough resolution in the dark that no step exists to magnify.
pub fn to_half(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7F_FFFF;
    if exp == 0xFF {
        return sign | 0x7C00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let round = if rem > mid || (rem == mid && (half & 1) == 1) { 1 } else { 0 };
        return sign | (half + round) as u16;
    }
    let mut half = ((e as u32) << 10) | (mant >> 13);
    let rem = mant & 0x1FFF;
    if rem > 0x1000 || (rem == 0x1000 && (half & 1) == 1) {
        half += 1;
    }
    sign | half as u16
}

/// One star: a direction, a size in pixels, a tint, and a twinkle phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    pub dir: [f32; 3],
    pub size: f32,
    pub tint: [f32; 3],
    pub phase: f32,
}

/// The lattice density the feature points are drawn from: about 4 pi 24^2 =
/// 7238 cells on the shell.
const STAR_LATTICE: i32 = 24;

/// `hash33` from the shader. This one keeps its `sin`, as sky.ts does: it
/// decides where a dot goes and nothing that crosses a boundary.
fn hash33(x: f32, y: f32, z: f32) -> [f32; 3] {
    let d1 = x * 127.1 + y * 311.7 + z * 74.7;
    let d2 = x * 269.5 + y * 183.3 + z * 246.1;
    let d3 = x * 113.5 + y * 271.9 + z * 124.6;
    let f = |v: f32| fract(v.sin() * 43758.547);
    [f(d1), f(d2), f(d3)]
}

// 6.283 rather than TAU because sky.ts writes 6.283, and the port is line for
// line: a star's phase off the archive's own number is the archive's star.
/// The star field for a sky: the Voronoi feature points on the unit shell.
#[allow(clippy::approx_constant)]
pub fn starfield(preset: &SkyPreset) -> Vec<Star> {
    let mut out = Vec::new();
    let d = STAR_LATTICE;
    let (lo, hi) = (d as f32 - 0.5, d as f32 + 0.5);
    let s = preset.seed;
    for i in -d - 1..=d + 1 {
        for j in -d - 1..=d + 1 {
            for k in -d - 1..=d + 1 {
                let h = hash33(i as f32 + s[0], j as f32 + s[1], k as f32 + s[2]);
                let (fx, fy, fz) = (i as f32 + h[0], j as f32 + h[1], k as f32 + h[2]);
                let len = (fx * fx + fy * fy + fz * fz).sqrt();
                if !(lo..=hi).contains(&len) {
                    continue;
                }
                let b = hash33(i as f32 * 3.1 + 5.0, j as f32 * 3.1 + 5.0, k as f32 * 3.1 + 5.0)[0];
                let bright = b.powi(7);
                let c = hash33(i as f32 * 1.7 + 9.0, j as f32 * 1.7 + 9.0, k as f32 * 1.7 + 9.0)[0];
                let warm = 0.45 + 0.55 * bright;
                out.push(Star {
                    dir: [fx / len, fy / len, fz / len],
                    size: 1.0 + bright * 2.6,
                    tint: [
                        (0.74 + 0.26 * c) * warm + (1.0 - warm) * 0.30,
                        (0.80 + 0.18 * c) * warm + (1.0 - warm) * 0.34,
                        (1.00 - 0.16 * c) * warm + (1.0 - warm) * 0.42,
                    ],
                    phase: hash33(i as f32 + 31.0, j as f32 + 31.0, k as f32 + 31.0)[0] * 6.283,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sky.ts measured its turbulence at mean 0.345, p10 0.170 over the sky.
    /// The port has to come out in the same place, or it is not the same sky.
    #[test]
    fn turbulence_is_dark_with_filaments() {
        let preset = SkyPreset::skirmish();
        let mut v: Vec<f32> = Vec::new();
        for face in 0..6 {
            for y in 0..32 {
                for x in 0..32 {
                    let d = cube_dir(face, (x as f32 + 0.5) / 32.0, (y as f32 + 0.5) / 32.0);
                    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    let p = [d[0] / l + preset.seed[0], d[1] / l + preset.seed[1], d[2] / l + preset.seed[2]];
                    v.push(turb(p, OCT_1, FREQ_1, AMP_1));
                }
            }
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let p10 = v[v.len() / 10];
        eprintln!("turbulence: mean {mean:.3}, p10 {p10:.3}");
        assert!((0.30..0.40).contains(&mean), "mean {mean}");
        assert!((0.12..0.22).contains(&p10), "p10 {p10}");
    }

    #[test]
    fn the_mask_passes_about_a_fifth_of_the_sky() {
        let preset = SkyPreset::skirmish();
        let (mut lit, mut n) = (0, 0);
        for face in 0..6 {
            for y in 0..24 {
                for x in 0..24 {
                    let c = sky_at(&preset, cube_dir(face, (x as f32 + 0.5) / 24.0, (y as f32 + 0.5) / 24.0));
                    n += 1;
                    if c[1] > preset.b[1] + 0.02 {
                        lit += 1;
                    }
                }
            }
        }
        let share = lit as f32 / n as f32;
        eprintln!("nebula covers {:.1}% of the sky", share * 100.0);
        assert!((0.08..0.40).contains(&share), "{share}");
    }

    #[test]
    fn cube_directions_point_out_of_their_faces() {
        for face in 0..6 {
            let d = cube_dir(face, 0.5, 0.5);
            let axis = face / 2;
            let sign = if face % 2 == 0 { 1.0 } else { -1.0 };
            assert_eq!(d[axis], sign);
            for a in 0..3 {
                if a != axis {
                    assert!(d[a].abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn half_floats_round_trip_what_a_sky_needs() {
        let back = |h: u16| -> f32 {
            let s = ((h >> 15) & 1) as u32;
            let e = ((h >> 10) & 0x1F) as i32;
            let m = (h & 0x3FF) as u32;
            let v = if e == 0 {
                m as f32 * 2f32.powi(-24)
            } else {
                (1.0 + m as f32 / 1024.0) * 2f32.powi(e - 15)
            };
            if s == 1 { -v } else { v }
        };
        for &x in &[0.0f32, 1.0, 0.5, 0.0123, 0.00012, 1.5, 65504.0, -2.0, 0.333] {
            let h = to_half(x);
            let r = back(h);
            assert!((r - x).abs() <= x.abs() * 1e-3 + 1e-7, "{x} -> {h:#x} -> {r}");
        }
        assert_eq!(to_half(1.0), 0x3C00);
        assert_eq!(to_half(f32::INFINITY), 0x7C00);
        assert_eq!(to_half(1e6), 0x7C00, "overflow is infinity");
        // The darkest sky value is representable, which is the point.
        let dark = SkyPreset::skirmish().b[2];
        assert!(back(to_half(dark)) > 0.0);
    }

    #[test]
    fn the_stars_are_one_shell_and_the_same_every_time() {
        let a = starfield(&SkyPreset::skirmish());
        let b = starfield(&SkyPreset::skirmish());
        assert_eq!(a, b);
        assert!((6500..8000).contains(&a.len()), "{} stars", a.len());
        for s in &a {
            let l = (s.dir[0].powi(2) + s.dir[1].powi(2) + s.dir[2].powi(2)).sqrt();
            assert!((l - 1.0).abs() < 1e-5);
            assert!(s.size >= 1.0 && s.size <= 3.6);
        }
        let bright = a.iter().filter(|s| s.size > 2.5).count();
        assert!(bright > 10 && bright < a.len() / 10, "{bright} bright of {}", a.len());
        assert_ne!(starfield(&SkyPreset::duel()), a, "a seed reshapes the field");
    }
}
