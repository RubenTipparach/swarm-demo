//! The HUD's palette and its nine sliced frames, baked at boot.
//!
//! Every panel, well and button on the command deck is ONE 48 pixel texture
//! with a 16 pixel slice, so a corner bracket is the same size on the resource
//! strip and on the build panel and nothing stretches that should not. The
//! mockup bakes these on a 2D canvas; this is that canvas, rasterised into an
//! `Image` and hung on an `ImageNode` with `ImageScaleMode::Sliced` and the
//! same four numbers.
//!
//! A skin is a ROW: it owns the palette the deck's colour roles resolve to and
//! the four frame descriptors. It owns nothing else, because the layout and
//! every rule about what a control DOES are the same whatever it is made of.

use crate::*;

/// The texture is 48 pixels square and the slice is 16, which is the mockup's
/// own pair and what makes a corner the same size at every panel width.
pub(crate) const SLICE_PX: u32 = 48;
pub(crate) const SLICE_IN: f32 = 16.0;

/// How long the bracket's arms are, in pixels, and how thick.
///
/// Twelve inside a sixteen pixel corner, so an arm never reaches the stretched
/// middle of the texture: a bracket that crossed the slice line would smear
/// along the edge of every panel wider than forty eight pixels.
const ARM: u32 = 12;
const ARM_W: u32 = 2;

/// A colour as the mockup writes one: sRGB bytes and straight alpha, because
/// that is the space a canvas composites in and the picture has to match.
#[derive(Clone, Copy)]
pub(crate) struct Ink(pub(crate) u8, pub(crate) u8, pub(crate) u8, pub(crate) f32);

impl Ink {
    pub(crate) const fn hex(v: u32) -> Ink {
        Ink(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
            1.0,
        )
    }

    pub(crate) const fn a(self, a: f32) -> Ink {
        Ink(self.0, self.1, self.2, a)
    }

    /// The same colour as a Bevy `Color`, for text and for anything tinted.
    pub(crate) fn col(self) -> Color {
        Color::srgba(
            self.0 as f32 / 255.0,
            self.1 as f32 / 255.0,
            self.2 as f32 / 255.0,
            self.3,
        )
    }
}

/// One frame: its fill, its hairline, an optional inner hairline and the
/// colour of the four corner brackets.
///
/// The fill is a vertical gradient when `grad` is set, because a button is lit
/// from above and a panel is not, and that is the whole of the difference
/// between the two descriptors.
#[derive(Clone, Copy)]
pub(crate) struct Slice {
    pub(crate) fill: Ink,
    pub(crate) grad: Option<Ink>,
    pub(crate) line: Ink,
    pub(crate) inner: Option<Ink>,
    pub(crate) corner: Ink,
}

/// Which frame a node wears. Four, and a fifth would be a frame nobody could
/// name: a panel, the well inside it, a button, and a button that is armed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Frame {
    Panel,
    Well,
    Btn,
    Hot,
}

/// The palette the deck's colour roles resolve to.
///
/// Named by ROLE rather than by hue, so a skin that paints its gold blue does
/// not leave the code calling it gold. The names are the mockup's own tokens.
#[derive(Clone, Copy)]
pub(crate) struct Tokens {
    pub(crate) ink: Ink,
    pub(crate) dim: Ink,
    pub(crate) faint: Ink,
    pub(crate) gold: Ink,
    pub(crate) cyan: Ink,
    pub(crate) teal: Ink,
    pub(crate) green: Ink,
    pub(crate) red: Ink,
    pub(crate) ore: Ink,
    pub(crate) cry: Ink,
    pub(crate) edge: Ink,
    pub(crate) edge_lit: Ink,
    pub(crate) row: Ink,
    pub(crate) row_hot: Ink,
    pub(crate) sky_ink: Ink,
}

/// The ink a SCHEMATIC is baked in.
///
/// A schematic is drawn in the HUD's own colours rather than the ship's,
/// because a cyan readout on a parchment panel is a picture somebody pasted
/// on rather than a page. Two materials and two lines: the hull and the
/// machinery the plan picks out inside it.
#[derive(Clone, Copy)]
pub(crate) struct Plan {
    pub(crate) hull: Ink,
    pub(crate) subs: Ink,
    pub(crate) line: Ink,
    pub(crate) lsub: Ink,
    pub(crate) rim: Ink,
}

/// The four frames a skin bakes, and the palette they are drawn beside.
pub(crate) struct SkinRow {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) tok: Tokens,
    pub(crate) panel: Slice,
    pub(crate) well: Slice,
    pub(crate) btn: Slice,
    pub(crate) hot: Slice,
    pub(crate) plan: Plan,
}

/// Fleet Command: the sci fi frame, a hairline box with an L at each corner,
/// which is what says "instrument" without drawing an instrument.
pub(crate) const COMMAND: SkinRow = SkinRow {
    id: "command",
    name: "Fleet Command",
    tok: Tokens {
        ink: Ink::hex(0xcfe2ee),
        dim: Ink::hex(0x7e94a6),
        faint: Ink::hex(0x4e6274),
        gold: Ink::hex(0xf5c542),
        cyan: Ink::hex(0x6ff0ff),
        teal: Ink::hex(0x49b6c8),
        green: Ink::hex(0x63d12b),
        red: Ink::hex(0xe8563c),
        ore: Ink::hex(0xe8a33c),
        cry: Ink::hex(0x58d8c4),
        edge: Ink::hex(0x1d3f52),
        edge_lit: Ink::hex(0x47869e),
        row: Ink::hex(0x091a25).a(0.85),
        row_hot: Ink::hex(0x122e3f).a(0.82),
        sky_ink: Ink::hex(0xcfe2ee),
    },
    panel: Slice {
        fill: Ink::hex(0x05101a).a(0.88),
        grad: None,
        line: Ink::hex(0x609ebc).a(0.42),
        inner: Some(Ink::hex(0x609ebc).a(0.16)),
        corner: Ink::hex(0xf5c542),
    },
    well: Slice {
        fill: Ink::hex(0x02090f).a(0.86),
        grad: None,
        line: Ink::hex(0x3c708e).a(0.36),
        inner: None,
        corner: Ink::hex(0x49b6c8).a(0.55),
    },
    btn: Slice {
        fill: Ink::hex(0x164056).a(0.90),
        grad: Some(Ink::hex(0x071823).a(0.90)),
        line: Ink::hex(0x78c8dc).a(0.50),
        inner: None,
        corner: Ink::hex(0x8ce0f4).a(0.75),
    },
    hot: Slice {
        fill: Ink::hex(0x604c0a).a(0.92),
        grad: Some(Ink::hex(0x1e1704).a(0.92)),
        line: Ink::hex(0xf5c542).a(0.70),
        inner: None,
        corner: Ink::hex(0xffd964),
    },
    plan: Plan {
        hull: Ink::hex(0x0d3a52),
        subs: Ink::hex(0x8e3d05),
        line: Ink::hex(0x3fc8e8),
        lsub: Ink::hex(0xff9a3c),
        rim: Ink::hex(0x7eeaff),
    },
};

/// The skins on offer. One row each, and the deck reads whichever is current.
pub(crate) const SKINS: [&SkinRow; 1] = [&COMMAND];

/// The baked frames, and which skin they came from.
///
/// A resource rather than four handles passed about, because "what the HUD is
/// made of" is one fact and every builder asks it.
#[derive(Resource)]
pub(crate) struct Skin {
    pub(crate) which: usize,
    pub(crate) panel: Handle<Image>,
    pub(crate) well: Handle<Image>,
    pub(crate) btn: Handle<Image>,
    pub(crate) hot: Handle<Image>,
}

impl Skin {
    pub(crate) fn row(&self) -> &'static SkinRow {
        SKINS[self.which.min(SKINS.len() - 1)]
    }

    pub(crate) fn tok(&self) -> Tokens {
        self.row().tok
    }

    pub(crate) fn tex(&self, f: Frame) -> Handle<Image> {
        match f {
            Frame::Panel => self.panel.clone(),
            Frame::Well => self.well.clone(),
            Frame::Btn => self.btn.clone(),
            Frame::Hot => self.hot.clone(),
        }
    }
}

/// Lay one colour over what is already in the buffer, source over, in sRGB.
///
/// The mockup composites on a canvas, which works in sRGB bytes, so this does
/// too: blending these in linear space would give a different picture from the
/// one that was approved.
pub(crate) fn over(dst: &mut [u8], at: usize, c: Ink) {
    let a = c.3.clamp(0.0, 1.0);
    let da = dst[at + 3] as f32 / 255.0;
    let out = a + da * (1.0 - a);
    for k in 0..3 {
        let s = [c.0, c.1, c.2][k] as f32;
        let d = dst[at + k] as f32;
        // Straight alpha, so the destination's own coverage weighs its colour.
        let v = if out <= 0.0 {
            0.0
        } else {
            (s * a + d * da * (1.0 - a)) / out
        };
        dst[at + k] = v.round().clamp(0.0, 255.0) as u8;
    }
    dst[at + 3] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Rasterise one frame's 48 pixel tile.
fn bake(s: Slice) -> Image {
    let n = SLICE_PX as usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        // A vertical gradient when the descriptor has one, which is what makes
        // a button look lit from above and a panel not.
        let c = match s.grad {
            Some(b) => {
                let t = y as f32 / (n - 1) as f32;
                let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
                Ink(
                    mix(s.fill.0, b.0),
                    mix(s.fill.1, b.1),
                    mix(s.fill.2, b.2),
                    s.fill.3 + (b.3 - s.fill.3) * t,
                )
            }
            None => s.fill,
        };
        for x in 0..n {
            over(&mut px, (y * n + x) * 4, c);
        }
    }
    let mut line = |x: usize, y: usize, c: Ink| over(&mut px, (y * n + x) * 4, c);
    // The outer hairline, then the inner one three pixels in when there is
    // one. Both are the canvas's own inset rectangles, on whole pixels.
    for i in 0..n {
        line(i, 0, s.line);
        line(i, n - 1, s.line);
        line(0, i, s.line);
        line(n - 1, i, s.line);
    }
    if let Some(c) = s.inner {
        for i in 3..n - 3 {
            line(i, 3, c);
            line(i, n - 4, c);
            line(3, i, c);
            line(n - 4, i, c);
        }
    }
    // And the four brackets: an L two pixels thick and twelve long, drawn from
    // each corner along both edges.
    for (cx, cy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let put = |px: &mut Vec<u8>, u: u32, v: u32| {
            let x = if cx == 0 { u } else { SLICE_PX - 1 - u } as usize;
            let y = if cy == 0 { v } else { SLICE_PX - 1 - v } as usize;
            over(px, (y * n + x) * 4, s.corner);
        };
        for u in 0..ARM {
            for v in 0..ARM_W {
                put(&mut px, u, v);
                put(&mut px, v, u);
            }
        }
    }
    let mut img = Image::new(
        Extent3d {
            width: SLICE_PX,
            height: SLICE_PX,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    // Nearest, because a nine slice stretches its middle and a linear filter
    // over a one pixel hairline is a hairline that fades out along the edge it
    // was drawn to mark.
    img.sampler = ImageSampler::nearest();
    img
}

/// Bake every frame of the current skin. Called at startup and again whenever
/// the skin changes, which is why it takes the row rather than reading one.
pub(crate) fn bake_skin(images: &mut Assets<Image>, which: usize) -> Skin {
    let row = SKINS[which.min(SKINS.len() - 1)];
    info!("HUD skin: {} ({})", row.name, row.id);
    Skin {
        which,
        panel: images.add(bake(row.panel)),
        well: images.add(bake(row.well)),
        btn: images.add(bake(row.btn)),
        hot: images.add(bake(row.hot)),
    }
}

/// Put the baked pictures in the world before anything asks for one: the
/// frames, the fleet's schematics and the marks.
///
/// All three at once, because they are one answer to "what is the deck made
/// of" and a deck built before any of them would be a deck with holes in it.
pub(crate) fn load_skin(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let skin = bake_skin(&mut images, 0);
    let fleet = bake_fleet(&mut images, skin.row().plan);
    commands.insert_resource(bake_glyphs(&mut images));
    commands.insert_resource(fleet);
    commands.insert_resource(skin);
}
