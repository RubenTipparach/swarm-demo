//! The deck's line art: six class marks and one glyph per command.
//!
//! These are the mockup's own drawings, ported. A class is not a ship, so no
//! ship can stand for one: a carrier's picture on the capital button would say
//! that navy's carrier rather than capital. They are authored line art in the
//! same plan view the schematics are baked in, blocked out the way the lattice
//! is, carrying no faction paint and no livery.
//!
//! They are rasterised rather than drawn as an SVG, because Bevy's UI has no
//! vector layer. One tiny rasteriser and a table of primitives: a mark is data
//! here exactly as it is markup there, so adding one is a row.

use crate::*;

/// One primitive of a mark, in its own viewBox units.
///
/// Fills and strokes, which is all the mockup's symbols use: the class marks
/// are blocked shapes and the command glyphs are two pixel line art.
pub(crate) enum Ink2 {
    /// A filled box, with its own opacity so a mark can have depth.
    Rect(f32, f32, f32, f32, f32),
    /// A filled polygon.
    Poly(&'static [(f32, f32)], f32),
    /// A stroked polyline.
    Path(&'static [(f32, f32)]),
    /// A stroked circle.
    Ring(f32, f32, f32),
}

/// A mark: its viewBox and what is in it.
pub(crate) struct Mark {
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) parts: &'static [Ink2],
}

/// How thick a stroked glyph is, in viewBox units, as the mockup draws them.
const STROKE: f32 = 2.0;

/// How far a sample is from a segment.
fn to_seg(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len = vx * vx + vy * vy;
    let t = if len <= 1e-6 {
        0.0
    } else {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len).clamp(0.0, 1.0)
    };
    let (dx, dy) = (p.0 - (a.0 + vx * t), p.1 - (a.1 + vy * t));
    (dx * dx + dy * dy).sqrt()
}

/// Whether a sample is inside a polygon, by the crossing rule.
fn inside(p: (f32, f32), poly: &[(f32, f32)]) -> bool {
    let mut hit = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// What coverage a sample has, and at what opacity.
fn sample(mark: &Mark, p: (f32, f32)) -> f32 {
    let mut a: f32 = 0.0;
    for part in mark.parts {
        let hit = match part {
            Ink2::Rect(x, y, w, h, o) => {
                if p.0 >= *x && p.0 <= x + w && p.1 >= *y && p.1 <= y + h {
                    *o
                } else {
                    0.0
                }
            }
            Ink2::Poly(pts, o) => {
                if inside(p, pts) {
                    *o
                } else {
                    0.0
                }
            }
            Ink2::Path(pts) => {
                let near = pts
                    .windows(2)
                    .any(|w| to_seg(p, w[0], w[1]) <= STROKE * 0.5);
                if near {
                    1.0
                } else {
                    0.0
                }
            }
            Ink2::Ring(cx, cy, r) => {
                let d = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt();
                if (d - r).abs() <= STROKE * 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        };
        a = a.max(hit);
    }
    a
}

/// How many samples a pixel takes on each axis. Three is plenty for line art
/// at these sizes and the whole set costs under a millisecond.
const AA: u32 = 3;

/// Rasterise a mark at `w` by `h`, in one ink.
///
/// `currentColor` in the markup is the button's own state colour, and a
/// texture cannot be recoloured after the fact, so the ink is handed in and
/// the deck bakes the few it actually shows. Supersampled and boxed down,
/// which is what gives two pixel line art a soft edge at any size.
pub(crate) fn bake_mark(mark: &Mark, ink: Ink, w: u32, h: u32) -> Image {
    let (iw, ih) = (w as usize, h as usize);
    let mut px = vec![0u8; iw * ih * 4];
    // Fit the viewBox, keeping its aspect, so a forty by twenty four class
    // mark is not stretched into a square.
    let scale = (w as f32 / mark.w).min(h as f32 / mark.h);
    let (ox, oy) = (
        (w as f32 - mark.w * scale) * 0.5,
        (h as f32 - mark.h * scale) * 0.5,
    );
    for y in 0..ih {
        for x in 0..iw {
            let mut acc = 0.0;
            for sy in 0..AA {
                for sx in 0..AA {
                    let p = (
                        (x as f32 + (sx as f32 + 0.5) / AA as f32 - ox) / scale,
                        (y as f32 + (sy as f32 + 0.5) / AA as f32 - oy) / scale,
                    );
                    acc += sample(mark, p);
                }
            }
            let a = acc / (AA * AA) as f32;
            if a > 0.002 {
                let n = (y * iw + x) * 4;
                px[n] = ink.0;
                px[n + 1] = ink.1;
                px[n + 2] = ink.2;
                px[n + 3] = (a * ink.3 * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    let mut img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = ImageSampler::linear();
    img
}

/// Every mark the deck draws.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    Fighter,
    Corvette,
    Frigate,
    Capital,
    Utility,
    Platform,
    Move,
    Attack,
    Guard,
    Stop,
    Stance,
    Launch,
    Rally,
    Hyper,
    Scut,
    Speed,
    Gun,
    Armour,
    Ore,
    Crystal,
    Beam,
    Flak,
    Slug,
    Torpedo,
    Bite,
}

impl Glyph {
    pub(crate) const ALL: [Glyph; 25] = [
        Glyph::Fighter,
        Glyph::Corvette,
        Glyph::Frigate,
        Glyph::Capital,
        Glyph::Utility,
        Glyph::Platform,
        Glyph::Move,
        Glyph::Attack,
        Glyph::Guard,
        Glyph::Stop,
        Glyph::Stance,
        Glyph::Launch,
        Glyph::Rally,
        Glyph::Hyper,
        Glyph::Scut,
        Glyph::Speed,
        Glyph::Gun,
        Glyph::Armour,
        Glyph::Ore,
        Glyph::Crystal,
        Glyph::Beam,
        Glyph::Flak,
        Glyph::Slug,
        Glyph::Torpedo,
        Glyph::Bite,
    ];

    fn mark(self) -> &'static Mark {
        match self {
            Glyph::Fighter => &FIGHTER,
            Glyph::Corvette => &CORVETTE,
            Glyph::Frigate => &FRIGATE,
            Glyph::Capital => &CAPITAL,
            Glyph::Utility => &UTILITY,
            Glyph::Platform => &PLATFORM,
            Glyph::Move | Glyph::Speed => &MOVE,
            Glyph::Attack => &ATTACK,
            Glyph::Guard | Glyph::Armour => &GUARD,
            Glyph::Stop => &STOP,
            Glyph::Stance => &STANCE,
            Glyph::Launch => &LAUNCH,
            Glyph::Rally => &RALLY,
            Glyph::Hyper => &HYPER,
            Glyph::Scut => &SCUT,
            Glyph::Gun => &GUNMARK,
            Glyph::Ore => &ORE,
            Glyph::Crystal => &CRYSTAL,
            Glyph::Beam => &BEAM,
            Glyph::Flak => &FLAK,
            Glyph::Slug => &SLUG,
            Glyph::Torpedo => &TORPEDO,
            Glyph::Bite => &BITE,
        }
    }
}

/// The baked marks, in white.
///
/// One bake each and the ink is the NODE's: an `ImageNode` multiplies its
/// texture by its own colour, which is exactly what `currentColor` does in the
/// markup, so a mark tints with the state of the button it is on rather than
/// being baked once per colour it might ever wear.
#[derive(Resource, Default)]
pub(crate) struct Glyphs(pub(crate) Vec<Handle<Image>>);

impl Glyphs {
    pub(crate) fn of(&self, g: Glyph) -> Handle<Image> {
        let n = Glyph::ALL.iter().position(|x| *x == g).unwrap_or(0);
        self.0.get(n).cloned().unwrap_or_default()
    }
}

/// How big a mark is baked. Forty eight across holds the widest viewBox at
/// better than a pixel a unit, and every place one is shown is smaller.
const MARK_PX: u32 = 48;

pub(crate) fn bake_glyphs(images: &mut Assets<Image>) -> Glyphs {
    let white = Ink(255, 255, 255, 1.0);
    Glyphs(
        Glyph::ALL
            .iter()
            .map(|g| images.add(bake_mark(g.mark(), white, MARK_PX, MARK_PX)))
            .collect(),
    )
}

// The six class marks, blocked out the way the lattice is: a hull, its bridge
// and berths above and below, its drive block aft and its bells behind that.
static FIGHTER: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(37.0, 12.0), (25.0, 9.0), (25.0, 15.0)], 1.0),
        Ink2::Rect(17.0, 9.5, 9.0, 5.0, 1.0),
        Ink2::Poly(&[(20.0, 10.0), (8.0, 3.0), (4.0, 5.0), (16.0, 11.0)], 1.0),
        Ink2::Poly(&[(20.0, 14.0), (8.0, 21.0), (4.0, 19.0), (16.0, 13.0)], 1.0),
        Ink2::Rect(10.0, 10.5, 8.0, 3.0, 0.7),
        Ink2::Rect(4.5, 10.5, 5.0, 3.0, 0.45),
    ],
};

static CORVETTE: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(36.0, 12.0), (27.0, 8.0), (27.0, 16.0)], 1.0),
        Ink2::Rect(14.0, 7.5, 13.0, 9.0, 1.0),
        Ink2::Rect(16.0, 2.0, 9.0, 4.0, 0.8),
        Ink2::Rect(16.0, 18.0, 9.0, 4.0, 0.8),
        Ink2::Rect(8.0, 9.0, 6.0, 6.0, 0.75),
        Ink2::Rect(3.5, 9.5, 4.5, 2.0, 0.5),
        Ink2::Rect(3.5, 12.5, 4.5, 2.0, 0.5),
    ],
};

static FRIGATE: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Rect(30.0, 10.5, 8.0, 3.0, 1.0),
        Ink2::Poly(&[(30.0, 8.0), (24.0, 6.0), (24.0, 18.0), (30.0, 16.0)], 1.0),
        Ink2::Rect(12.0, 6.0, 12.0, 12.0, 1.0),
        Ink2::Rect(14.0, 2.0, 7.0, 4.0, 0.8),
        Ink2::Rect(14.0, 18.0, 7.0, 4.0, 0.8),
        Ink2::Rect(7.0, 7.5, 5.0, 9.0, 0.75),
        Ink2::Rect(2.5, 8.0, 4.5, 3.0, 0.5),
        Ink2::Rect(2.5, 13.0, 4.5, 3.0, 0.5),
    ],
};

static CAPITAL: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(38.0, 12.0), (31.0, 7.0), (31.0, 17.0)], 1.0),
        Ink2::Rect(18.0, 5.0, 13.0, 14.0, 1.0),
        Ink2::Rect(21.0, 1.0, 8.0, 4.0, 0.8),
        Ink2::Rect(21.0, 19.0, 8.0, 4.0, 0.8),
        Ink2::Rect(23.0, 9.0, 7.0, 6.0, 0.5),
        Ink2::Rect(10.0, 6.5, 8.0, 11.0, 0.8),
        Ink2::Rect(3.5, 6.0, 6.5, 3.4, 0.5),
        Ink2::Rect(3.5, 10.3, 6.5, 3.4, 0.5),
        Ink2::Rect(3.5, 14.6, 6.5, 3.4, 0.5),
    ],
};

static UTILITY: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Rect(33.0, 5.5, 4.5, 4.0, 1.0),
        Ink2::Rect(33.0, 14.5, 4.5, 4.0, 1.0),
        Ink2::Rect(28.0, 9.0, 6.0, 6.0, 0.8),
        Ink2::Rect(15.0, 5.0, 13.0, 14.0, 1.0),
        Ink2::Rect(18.0, 8.0, 7.0, 8.0, 0.45),
        Ink2::Rect(9.0, 7.5, 6.0, 9.0, 0.8),
        Ink2::Rect(3.5, 9.0, 5.5, 6.0, 0.5),
    ],
};

static PLATFORM: Mark = Mark {
    w: 40.0,
    h: 24.0,
    parts: &[
        Ink2::Rect(28.0, 10.5, 10.0, 3.0, 1.0),
        Ink2::Poly(
            &[
                (28.0, 6.0),
                (16.0, 6.0),
                (12.0, 12.0),
                (16.0, 18.0),
                (28.0, 18.0),
            ],
            1.0,
        ),
        Ink2::Rect(18.0, 9.0, 8.0, 6.0, 0.45),
        Ink2::Rect(16.0, 1.0, 5.0, 5.0, 0.7),
        Ink2::Rect(16.0, 18.0, 5.0, 5.0, 0.7),
        Ink2::Rect(5.5, 10.0, 6.5, 4.0, 0.6),
    ],
};

// And the commands, which are two pixel line art in a twenty four box.
static MOVE: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Path(&[(4.0, 20.0), (18.0, 6.0)]),
        Ink2::Path(&[(11.0, 6.0), (18.0, 6.0), (18.0, 13.0)]),
    ],
};

static ATTACK: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Ring(12.0, 12.0, 6.0),
        Ink2::Path(&[(12.0, 2.0), (12.0, 6.0)]),
        Ink2::Path(&[(12.0, 18.0), (12.0, 22.0)]),
        Ink2::Path(&[(2.0, 12.0), (6.0, 12.0)]),
        Ink2::Path(&[(18.0, 12.0), (22.0, 12.0)]),
    ],
};

static GUARD: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[Ink2::Path(&[
        (12.0, 3.0),
        (19.0, 6.0),
        (19.0, 12.0),
        (16.0, 17.0),
        (12.0, 21.0),
        (8.0, 17.0),
        (5.0, 12.0),
        (5.0, 6.0),
        (12.0, 3.0),
    ])],
};

static STOP: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[Ink2::Rect(6.0, 6.0, 12.0, 12.0, 1.0)],
};

static STANCE: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Path(&[(3.0, 18.0), (21.0, 18.0)]),
        Ink2::Path(&[(6.0, 18.0), (6.0, 9.0)]),
        Ink2::Path(&[(12.0, 18.0), (12.0, 5.0)]),
        Ink2::Path(&[(18.0, 18.0), (18.0, 12.0)]),
    ],
};

static LAUNCH: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Path(&[(12.0, 21.0), (12.0, 10.0)]),
        Ink2::Path(&[(7.0, 15.0), (12.0, 10.0), (17.0, 15.0)]),
        Ink2::Path(&[(4.0, 4.0), (20.0, 4.0)]),
    ],
};

static RALLY: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[Ink2::Path(&[
        (6.0, 21.0),
        (6.0, 4.0),
        (18.0, 8.0),
        (6.0, 12.0),
    ])],
};

static HYPER: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[Ink2::Path(&[
        (3.0, 12.0),
        (8.0, 12.0),
        (11.0, 5.0),
        (14.0, 19.0),
        (17.0, 12.0),
        (21.0, 12.0),
    ])],
};

static SCUT: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Ring(12.0, 12.0, 8.0),
        Ink2::Path(&[(6.5, 6.5), (17.5, 17.5)]),
    ],
};

static GUNMARK: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Ring(12.0, 12.0, 6.0),
        Ink2::Path(&[(12.0, 3.0), (12.0, 6.0)]),
    ],
};

static ORE: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(12.0, 2.0), (21.0, 7.0), (12.0, 12.0), (3.0, 7.0)], 1.0),
        Ink2::Poly(
            &[(12.0, 12.0), (21.0, 7.0), (21.0, 17.0), (12.0, 22.0)],
            0.6,
        ),
        Ink2::Poly(&[(12.0, 12.0), (12.0, 22.0), (3.0, 17.0), (3.0, 7.0)], 0.35),
    ],
};

static CRYSTAL: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(12.0, 1.0), (18.0, 8.0), (12.0, 23.0), (6.0, 8.0)], 0.32),
        Ink2::Poly(&[(12.0, 1.0), (18.0, 8.0), (12.0, 8.0)], 1.0),
        Ink2::Poly(&[(12.0, 1.0), (6.0, 8.0), (12.0, 8.0)], 0.72),
        Ink2::Poly(&[(12.0, 8.0), (18.0, 8.0), (12.0, 23.0)], 0.55),
    ],
};

// The five range weapons, in the same language, because a page of named
// buttons with no marks on it is the one page that would not match the rest.
static BEAM: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Path(&[(3.0, 20.0), (17.0, 6.0)]),
        Ink2::Ring(19.0, 4.0, 2.5),
    ],
};

static FLAK: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Path(&[(12.0, 4.0), (12.0, 20.0)]),
        Ink2::Path(&[(4.0, 12.0), (20.0, 12.0)]),
        Ink2::Path(&[(6.5, 6.5), (17.5, 17.5)]),
        Ink2::Path(&[(17.5, 6.5), (6.5, 17.5)]),
    ],
};

static SLUG: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(&[(12.0, 2.0), (15.0, 7.0), (9.0, 7.0)], 1.0),
        Ink2::Rect(9.5, 7.0, 5.0, 13.0, 1.0),
    ],
};

static TORPEDO: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Poly(
            &[
                (12.0, 2.0),
                (16.0, 9.0),
                (16.0, 18.0),
                (8.0, 18.0),
                (8.0, 9.0),
            ],
            1.0,
        ),
        Ink2::Poly(&[(8.0, 15.0), (4.0, 21.0), (8.0, 21.0)], 0.7),
        Ink2::Poly(&[(16.0, 15.0), (20.0, 21.0), (16.0, 21.0)], 0.7),
    ],
};

static BITE: Mark = Mark {
    w: 24.0,
    h: 24.0,
    parts: &[
        Ink2::Ring(12.0, 12.0, 7.0),
        Ink2::Poly(&[(12.0, 12.0), (21.0, 8.0), (21.0, 16.0)], 1.0),
    ],
};
