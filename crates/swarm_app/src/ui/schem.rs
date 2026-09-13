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
///
/// **They are different SHAPES on purpose, and the mockup says why in its own
/// comment: the thumb is baked three by two rather than long, so a plan view
/// fills its frame with no band above and below to waste.** A rail row is
/// nearly square and a panel is a wide strip, and a plan letterboxed into the
/// wrong one of those is a ship drawn at a third of the room it has. The port
/// baked both at the panel's shape, which is what put a 3 by 1 picture in the
/// rail's 1.2 by 1 box.
pub(crate) const SCHEM: (u32, u32) = (256, 84);
pub(crate) const THUMB: (u32, u32) = (144, 96);

/// What shape a bake comes out, so a box drawn for one cannot squash it.
///
/// The rail drew a 3 by 1 picture in a 1.2 by 1 box and Bevy's `ImageNode`
/// stretches to fill, so every ship on it came out short and fat. The mockup
/// answers this with `object-fit: contain` and there is no such thing here,
/// so the NODE is fitted to the picture instead and this is the one place
/// either can be read off.
pub(crate) fn aspect(bake: (u32, u32)) -> f32 {
    bake.0 as f32 / bake.1.max(1) as f32
}

/// The top face of one column, as the greedy mesher would see it.
///
/// Everything that makes the mesher START A NEW QUAD, and nothing else: its
/// height, its colour and which surface it draws in. That is what turns the
/// edge pass into the mockup's own line network, which draws every greedy
/// quad's border: a boundary here is a boundary there, arrived at from the
/// other side.
///
/// The first cut carried the HEIGHT alone, so the only lines it could draw
/// were where the hull stepped. A Terran's deck is a flat plateau twelve cells
/// across and its livery paints four roles over it, so the mesher splits it
/// into four quads and the plan drew none of them: the hull came out as one
/// dark shape inside a bright rim, which is a silhouette rather than a plan.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Face {
    y: u16,
    /// The livery's own answer for this cell. A role boundary is a quad
    /// boundary, which is where most of a hull's panel detail comes from.
    colour: u32,
    surf: u8,
}

/// What a column of the lattice is made of, seen from above.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Top {
    Empty,
    /// Plating or frame: the body of the ship.
    Hull(Face),
    /// A part: what the plan picks out in the machinery's own colour.
    Subs(Face),
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
                let face = Face {
                    y: y as u16,
                    colour: model.colour[n],
                    surf: model.surf.get(n).copied().unwrap_or(0),
                };
                top[iz * h + ix] = if machinery(model.purp[n]) {
                    Top::Subs(face)
                } else {
                    Top::Hull(face)
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
    let (top, cw, ch, lo_z, lo_x) = plan_of(model);
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
    // Under two pixels a cell every pixel is a grid line, which is a wash
    // rather than a lattice.
    let grid = scale >= 2.0;
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
            // The FILL only. The lines are the mesh's own, drawn after this.
            let ink = if matches!(me, Top::Subs(_)) {
                plan.subs
            } else {
                plan.hull
            };
            over(&mut px, (y * iw + x) * 4, ink);
            // And the LATTICE over it, one line a cell, wherever this pixel
            // is the first of a new cell in either axis. That is the paper a
            // plan is drawn on: it gives the fill a scale a reader can count,
            // and it is what the prototype's own picture averages to once its
            // line network is denser than the pixels it is drawn at.
            if grid
                && (cell(iz - 1, ix) != me || cell(iz, ix - 1) != me || {
                    let fresh = |v: f32, o: f32| (v - o) / scale;
                    fresh(x as f32, ox).floor() != fresh(x as f32 - 1.0, ox).floor()
                        || fresh(y as f32, oy).floor() != fresh(y as f32 - 1.0, oy).floor()
                })
            {
                over(&mut px, (y * iw + x) * 4, plan.grid);
            }
        }
    }
    quad_lines(
        &mut px,
        iw,
        ih,
        model,
        plan,
        (ox, oy, scale),
        (&top, cw, ch, lo_z, lo_x),
    );
    rim(&mut px, iw, ih, plan.rim, 2);
    finish(px, w, h)
}

/// The greedy mesh's own line network, seen from above.
///
/// This is where a plan gets its panel detail, and deriving it from the top
/// face was the wrong source: a Terran's deck is one colour at one height over
/// its whole run, so a rule that drew a line where the column beside it
/// differed drew almost nothing, and the hull came out as a flat shape inside
/// a bright rim. The prototype draws `mesh.lin`, which is every greedy quad's
/// border in three dimensions, and projects it with the mesh; this is that,
/// arrived at from the quads themselves.
///
/// EVERY quad, not only the ones facing up: a side face is edge on from
/// straight above and projects to a line, which is exactly the step between
/// two courses of plating that a plan is supposed to show.
#[allow(clippy::too_many_arguments)]
fn quad_lines(
    px: &mut [u8],
    iw: usize,
    ih: usize,
    model: &VoxelModel,
    plan: Plan,
    fit: (f32, f32, f32),
    plan_map: (&[Top], usize, usize, usize, usize),
) {
    let (ox, oy, scale) = fit;
    let (top, _cw, ch, lo_z, lo_x) = plan_map;
    // Under a pixel a cell the network is denser than the picture can carry
    // and every line lands on its neighbour, which is a grey wash rather than
    // a plan. The thumb is baked above that, so this is a guard and not a
    // switch anything reaches in practice.
    if scale < 1.5 {
        return;
    }
    let surf = greedy_mesh(model, None);
    let half = |n: usize| n as f32 * 0.5;
    // `world` in the mesher is `(cell - half) * size`, so this undoes it and
    // lands back on the lattice the fill was drawn in.
    let to_px = |p: [f32; 3]| -> (f32, f32) {
        let lz = p[2] / model.cell + half(model.nz) - lo_z as f32;
        let lx = p[0] / model.cell + half(model.nx) - lo_x as f32;
        (ox + lz * scale, oy + lx * scale)
    };
    for md in &surf.skin {
        for q in 0..md.quads() {
            // UP facing only, which is the depth test doing its work in the
            // prototype: a camera straight overhead sees the faces whose
            // normal points at it and nothing buried under them. Drawing
            // every quad instead put the machinery's own borders over the
            // plating that covers it, so a frigate came out as an orange
            // wireframe of its own insides.
            if md.normals[q * 4][1] < 0.5 {
                continue;
            }
            let cell_of = md.quad_cell[q] as usize;
            // And it has to be the face a camera would actually REACH. The
            // mesher meshes the inside of the ship too, which is what makes a
            // hole read as a hull with a hole in it, so an up facing quad on
            // the floor of an internal cavity is a quad under the plating. The
            // top map IS the depth test: a quad is visible when its own cell
            // is the topmost solid one in its column.
            let (qx, qy, qz) = model.at(cell_of);
            let seen = match top.get((qz - lo_z) * ch + (qx - lo_x)) {
                Some(Top::Hull(f)) | Some(Top::Subs(f)) => f.y as usize == qy,
                _ => false,
            };
            if !seen {
                continue;
            }
            let ink = if machinery(model.purp[cell_of]) {
                plan.lsub
            } else {
                plan.line
            };
            let c: Vec<(f32, f32)> = (0..4).map(|n| to_px(md.positions[q * 4 + n])).collect();
            for n in 0..4 {
                segment(px, iw, ih, c[n], c[(n + 1) % 4], ink);
            }
        }
    }
}

/// One line into the bake, as the shortest walk between two pixels.
fn segment(px: &mut [u8], iw: usize, ih: usize, a: (f32, f32), b: (f32, f32), ink: Ink) {
    let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).round() as usize).max(1);
    for n in 0..=steps {
        let t = n as f32 / steps as f32;
        let x = (a.0 + (b.0 - a.0) * t).round();
        let y = (a.1 + (b.1 - a.1) * t).round();
        if x < 0.0 || y < 0.0 || x >= iw as f32 || y >= ih as f32 {
            continue;
        }
        over(px, (y as usize * iw + x as usize) * 4, ink);
    }
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
    /// The class each pair was baked for, in the order they were baked.
    pub(crate) keys: Vec<String>,
    pub(crate) big: Vec<Handle<Image>>,
    pub(crate) small: Vec<Handle<Image>>,
}

impl Schematics {
    pub(crate) fn of(&self, class: &str) -> Option<(Handle<Image>, Handle<Image>)> {
        let n = self.keys.iter().position(|k| k == class)?;
        Some((self.big.get(n)?.clone(), self.small.get(n)?.clone()))
    }
}

/// Bake the fleet. Called once, beside the frames, because both are pictures
/// the deck cannot be built without.
///
/// EVERY class on the manifest and not the dropdown's picked eight, because
/// the build menu offers what the yard can make and that is the whole fleet:
/// a row with no picture in it is a row a player cannot tell from the one
/// under it. Read off the same directory `Fleet::load` reads, so a class
/// added tomorrow has a schematic tomorrow.
pub(crate) fn bake_fleet(images: &mut Assets<Image>, plan: Plan) -> Schematics {
    let then = std::time::Instant::now();
    let mut out = Schematics::default();
    for entry in Fleet::load().0 {
        let model = load_hull(&entry.key);
        out.big
            .push(images.add(bake_plan(&model, plan, SCHEM.0, SCHEM.1)));
        out.small
            .push(images.add(bake_plan(&model, plan, THUMB.0, THUMB.1)));
        out.keys.push(entry.key);
    }
    info!(
        "schematics: {} classes baked in {:.0} ms",
        out.keys.len(),
        then.elapsed().as_secs_f32() * 1000.0
    );
    out
}
