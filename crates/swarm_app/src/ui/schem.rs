//! A schematic is a PLAN of a hull, baked off its own cells.
//!
//! Every picture of a ship on the deck is generated rather than drawn, which
//! is the mockup's own rule: one bake per class, shown at two sizes, so
//! nothing can drift between the rail, the list and the panel. The mockup
//! renders the greedy mesh under an orthographic camera and reads the pixels
//! back; there is no camera to spare here and none is needed, because a plan
//! view of an axis aligned lattice IS a projection the CPU can write down.
//!
//! Straight down, with the ship's LENGTH across the frame. A ship seen from
//! above is its layout, which is the thing the panel is asking a player to
//! read, and it is the one view every hull in a fleet can be compared in.

use crate::*;

/// The two sizes every hull is baked at, in pixels.
///
/// Two bakes rather than one scaled, because this is blocked art off a
/// lattice: a big bake shown small is a cell and a half per pixel, which is
/// the detail averaging itself away. The rail asks for the small one and the
/// panel for the large. Both are baked ABOVE the box they are shown in, at
/// better than a pixel a cell, or the edge pass has nothing to draw on and a
/// schematic comes out as a silhouette with a rim round it.
pub(crate) const SCHEM: (u32, u32) = (256, 84);
pub(crate) const THUMB: (u32, u32) = (176, 58);

/// What a column of the lattice is made of, seen from above.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Top {
    Empty,
    /// Plating or frame: the body of the ship.
    Hull(u16),
    /// A part: what the plan picks out in the machinery's own colour.
    Subs(u16),
}

impl Top {
    fn filled(self) -> bool {
        self != Top::Empty
    }
}

/// Whether a purpose is MACHINERY rather than the hull it is buried in.
///
/// Straight off the export's own byte: structure and nothing are the ship,
/// and the other seven are parts. A schematic that painted the frame as
/// machinery would be a ship with no body in it.
fn machinery(purp: u8) -> bool {
    !matches!(
        purp,
        swarm_core::voxel::purpose::NONE | swarm_core::voxel::purpose::STRUCTURE
    )
}

/// The topmost live cell of every column, over the model's own solid box.
fn plan_of(model: &VoxelModel) -> (Vec<Top>, usize, usize, usize, usize) {
    let (mut lo_z, mut hi_z, mut lo_x, mut hi_x) = (usize::MAX, 0usize, usize::MAX, 0usize);
    for n in 0..model.grid.len() {
        if model.grid[n] == mat::EMPTY {
            continue;
        }
        let (x, _, z) = model.at(n);
        lo_z = lo_z.min(z);
        hi_z = hi_z.max(z);
        lo_x = lo_x.min(x);
        hi_x = hi_x.max(x);
    }
    if lo_z == usize::MAX {
        return (Vec::new(), 0, 0, 0, 0);
    }
    let (w, h) = (hi_z - lo_z + 1, hi_x - lo_x + 1);
    let mut top = vec![Top::Empty; w * h];
    for iz in 0..w {
        for ix in 0..h {
            let (z, x) = (lo_z + iz, lo_x + ix);
            for y in (0..model.ny).rev() {
                let n = model.index(x, y, z);
                if model.grid[n] == mat::EMPTY {
                    continue;
                }
                // The HEIGHT rides in the cell, because that is what gives a
                // schematic its panel lines: a flat plate is one height over
                // its whole run and draws no line inside itself, and greebled
                // work steps at every cell and is full of them.
                top[iz * h + ix] = if machinery(model.purp[n]) {
                    Top::Subs(y as u16)
                } else {
                    Top::Hull(y as u16)
                };
                break;
            }
        }
    }
    (top, w, h, lo_z, lo_x)
}

/// Grow a rim outward from any opaque pixel.
///
/// This is the one honest way to get a hard outline off a voxel silhouette in
/// a BAKE: a shader would want the depth buffer, and a bake already has the
/// pixels in hand. Two rings, the second at a third of the first, so the edge
/// has a falloff rather than a second hard line.
fn rim(px: &mut [u8], w: usize, h: usize, ink: Ink, rings: usize) {
    let mut cur: Vec<bool> = (0..w * h).map(|i| px[i * 4 + 3] > 8).collect();
    for pass in 0..rings {
        let mut next = cur.clone();
        let a = if pass == 0 { 1.0 } else { 0.34 };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if cur[i] {
                    continue;
                }
                let near = (x > 0 && cur[i - 1])
                    || (x + 1 < w && cur[i + 1])
                    || (y > 0 && cur[i - w])
                    || (y + 1 < h && cur[i + w]);
                if near {
                    over(px, i * 4, ink.a(a));
                    next[i] = true;
                }
            }
        }
        cur = next;
    }
}

/// Bake one hull's plan at `w` by `h`.
pub(crate) fn bake_plan(model: &VoxelModel, plan: Plan, w: u32, h: u32) -> Image {
    let (top, cw, ch, _, _) = plan_of(model);
    let (iw, ih) = (w as usize, h as usize);
    let mut px = vec![0u8; iw * ih * 4];
    if cw == 0 {
        return finish(px, w, h);
    }
    // Fit the model's own EXTENTS rather than its bounding sphere: a plan of a
    // frigate is long and thin, and a sphere that contains it is set by the
    // length alone, which drew every ship at a third of the room it had.
    let pad = 3.0;
    let scale = ((iw as f32 - pad * 2.0) / cw as f32).min((ih as f32 - pad * 2.0) / ch as f32);
    let (ox, oy) = (
        (iw as f32 - cw as f32 * scale) * 0.5,
        (ih as f32 - ch as f32 * scale) * 0.5,
    );
    let cell = |iz: i32, ix: i32| -> Top {
        if iz < 0 || ix < 0 || iz as usize >= cw || ix as usize >= ch {
            Top::Empty
        } else {
            top[iz as usize * ch + ix as usize]
        }
    };
    for y in 0..ih {
        for x in 0..iw {
            let iz = ((x as f32 - ox) / scale).floor() as i32;
            let ix = ((y as f32 - oy) / scale).floor() as i32;
            let me = cell(iz, ix);
            if !me.filled() {
                continue;
            }
            let subs = matches!(me, Top::Subs(_));
            // A border wherever the column beside this one is a different
            // material or stands at a different height, which is every greedy
            // quad's own edge arrived at from the other side.
            let edge = scale >= 1.5
                && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dz, dx)| cell(iz + dz, ix + dx) != me);
            let ink = match (subs, edge) {
                (true, true) => plan.lsub,
                (true, false) => plan.subs,
                (false, true) => plan.line,
                (false, false) => plan.hull,
            };
            over(&mut px, (y * iw + x) * 4, ink);
        }
    }
    rim(&mut px, iw, ih, plan.rim, 2);
    finish(px, w, h)
}

fn finish(px: Vec<u8>, w: u32, h: u32) -> Image {
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

/// Every class's two pictures, baked once.
///
/// One rule, and it holds everywhere: a ship is shown by its own schematic, in
/// the rail, in the list and on the panel, so three places cannot disagree
/// about what a hull looks like.
#[derive(Resource, Default)]
pub(crate) struct Schematics {
    pub(crate) big: Vec<Handle<Image>>,
    pub(crate) small: Vec<Handle<Image>>,
}

impl Schematics {
    pub(crate) fn of(&self, class: &str) -> Option<(Handle<Image>, Handle<Image>)> {
        let n = PICKABLE.iter().position(|p| *p == class)?;
        Some((self.big.get(n)?.clone(), self.small.get(n)?.clone()))
    }
}

/// Bake the fleet. Called once, beside the frames, because both are pictures
/// the deck cannot be built without.
pub(crate) fn bake_fleet(images: &mut Assets<Image>, plan: Plan) -> Schematics {
    let mut out = Schematics::default();
    for class in PICKABLE {
        let model = load_hull(class);
        out.big
            .push(images.add(bake_plan(&model, plan, SCHEM.0, SCHEM.1)));
        out.small
            .push(images.add(bake_plan(&model, plan, THUMB.0, THUMB.1)));
    }
    out
}
