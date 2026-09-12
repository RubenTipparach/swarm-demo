//! Every ship in the game as geometry a browser can draw, for the record
//! keeping doc (`docs/SHIPS.md` names where it is published).
//!
//! The doc is the one place the whole fleet is listed, and a list of ships
//! nobody can look at is a table of names. So this writes what the game
//! itself meshes: the same `greedy_mesh` the battlefield draws, the same
//! cells, the same colours, for the twenty three stock hulls AND the four
//! alien archetypes, which are generated rather than authored and so exist
//! nowhere a person can open.
//!
//! What it writes is one `.json` per ship and one `index.json` beside them.
//! A quad rather than two triangles, because every face this mesher makes is
//! an axis aligned rectangle and four corners is what it already has; a u16
//! per coordinate over the model's own bounding box, because a lattice is at
//! most 128 cells across and a sixteenth of a cell is far finer than anything
//! a viewer can show.
//!
//! The quads are packed BINARY and then base64 inside a JSON envelope, which
//! reads as a strange thing to do until you try to publish them: a static
//! host serves standard web media types and a private format is not one, so
//! the choice is JSON or nothing. As numbers in a JSON array the same
//! geometry is three times the size and parses an order slower; as base64 it
//! is a third larger than the bytes and `atob` is one call.
//!
//! ```sh
//! cargo run --release -p swarm_core --example ship_export -- assets/hulls out/
//! ```

use swarm_core::alien::{self, Archetype};
use swarm_core::damage::hp_for;
use swarm_core::fx::{engines_of, guns_of, reactor_of};
use swarm_core::mesh::{greedy_mesh, MeshData};
use swarm_core::voxel::{depth_from_outside, mat, purpose};
use swarm_core::VoxelModel;

/// Bytes per quad in a `.shp`: twelve u16 of corner, one of face, three of
/// colour and one of what the face IS.
const QUAD_BYTES: usize = 29;

/// Base64, written out because the core depends on nothing but `std` and
/// this is the one place anything needs it.
fn base64(bytes: &[u8]) -> String {
    const SET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(SET[((n >> (18 - i * 6)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A face's own axis and sign, so a viewer can light a quad without storing
/// three floats of normal per corner. Every face this mesher makes is axis
/// aligned, which is what makes one byte enough.
fn axis_of(n: [f32; 3]) -> u8 {
    match (n[0] > 0.5, n[0] < -0.5, n[1] > 0.5, n[1] < -0.5, n[2] > 0.5) {
        (true, _, _, _, _) => 0,
        (_, true, _, _, _) => 1,
        (_, _, true, _, _) => 2,
        (_, _, _, true, _) => 3,
        (_, _, _, _, true) => 4,
        _ => 5,
    }
}

/// One layer of a ship's mesh, and whether it is a window.
///
/// The flag is what lets a viewer light the panes and nothing else. It is a
/// property of the LAYER rather than of a quad's colour, because a window
/// carries the hull's own paint in its vertex colour on purpose: the frame
/// round a pane is the plating it was cut into.
struct Layer<'a> {
    mesh: &'a MeshData,
    window: bool,
}

/// What a ship comes out as: the header a viewer needs and the quads.
fn encode(layers: &[Layer]) -> Vec<u8> {
    let quads: usize = layers.iter().map(|l| l.mesh.quads()).sum();
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for l in layers {
        for p in &l.mesh.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
    }
    // One scale for all three axes, so nothing is stretched by the encoding
    // and a viewer needs no per axis divisor. A model with no quads at all
    // would divide by nought, which is a NaN in every vertex from then on.
    let span = (0..3).fold(1e-6f32, |s, a| s.max(hi[a] - lo[a]));
    let mut out = Vec::with_capacity(28 + quads * QUAD_BYTES);
    out.extend_from_slice(b"SHIP");
    out.extend_from_slice(&1u32.to_le_bytes());
    for v in &lo {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&span.to_le_bytes());
    out.extend_from_slice(&(quads as u32).to_le_bytes());
    for l in layers {
        let m = l.mesh;
        for q in 0..m.quads() {
            for c in 0..4 {
                let p = m.positions[q * 4 + c];
                for a in 0..3 {
                    let t = ((p[a] - lo[a]) / span * 65535.0).clamp(0.0, 65535.0);
                    out.extend_from_slice(&(t as u16).to_le_bytes());
                }
            }
            out.push(axis_of(m.normals[q * 4]));
            let col = m.colours[q * 4];
            for v in col.iter().take(3) {
                out.push((v.clamp(0.0, 1.0) * 255.0) as u8);
            }
            out.push(u8::from(l.window));
        }
    }
    out
}

/// What a ship is PAINTED, as the one colour that covers most of it.
///
/// Measured off the mesh rather than named beside the class, so a navy's
/// swatch in the doc is that navy's own plating and cannot drift from it.
/// By quad AREA and not by quad count, because the greedy mesher leaves a
/// flank as one big rectangle and a greebled stern as dozens of small ones:
/// counting quads would paint every ship the colour of its machinery.
fn paint(m: &MeshData) -> u32 {
    let mut seen: Vec<(u32, f32)> = Vec::new();
    for q in 0..m.quads() {
        let p = &m.positions[q * 4..q * 4 + 4];
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
        let area = (e1[0] * e1[0] + e1[1] * e1[1] + e1[2] * e1[2]).sqrt()
            * (e2[0] * e2[0] + e2[1] * e2[1] + e2[2] * e2[2]).sqrt();
        let c = m.colours[q * 4];
        let key = (0..3).fold(0u32, |a, i| {
            (a << 8) | ((c[i].clamp(0.0, 1.0) * 255.0) as u32)
        });
        match seen.iter_mut().find(|(k, _)| *k == key) {
            Some((_, a)) => *a += area,
            None => seen.push((key, area)),
        }
    }
    seen.into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
        .unwrap_or(0)
}

/// Everything the doc says about one ship, measured off the model rather
/// than typed beside it.
fn stats(key: &str, m: &VoxelModel, skin: &MeshData, quads: usize) -> String {
    let cells = m.solid_count();
    let plate = m.grid.iter().filter(|&&x| mat::is_armour(x)).count();
    let hp: f32 = m.grid.iter().map(|&x| hp_for(x)).sum();
    let att = m
        .purp
        .iter()
        .zip(&m.grid)
        .filter(|(&p, &g)| g != mat::EMPTY && p == purpose::ATTITUDE)
        .count();
    let reactor = reactor_of(m);
    let d = depth_from_outside(m);
    let buried = reactor
        .iter()
        .map(|&c| d[c])
        .filter(|&x| x != u32::MAX)
        .min()
        .unwrap_or(0);
    format!(
        r##"{{"key":"{key}","cells":{cells},"plate":{plate},"machinery":{},"hp":{:.0},"guns":{},"engines":{},"attitude":{att},"reactor":{},"buried":{buried},"radius":{:.2},"volumeRadius":{:.2},"cell":{},"quads":{quads},"paint":"#{:06X}","lattice":[{},{},{}]}}"##,
        cells - plate,
        hp,
        guns_of(m).len(),
        engines_of(m).len(),
        reactor.len(),
        m.radius(),
        m.volume_radius(),
        m.cell,
        paint(skin),
        m.nx,
        m.ny,
        m.nz,
    )
}

/// Mesh a ship, write its geometry, and answer its row of the index.
fn ship(key: &str, m: &VoxelModel, out: &str) -> String {
    let s = greedy_mesh(m, None);
    let skin = s.skin_all();
    let mut layers = vec![Layer {
        mesh: &skin,
        window: false,
    }];
    for w in &s.windows {
        layers.push(Layer {
            mesh: w,
            window: true,
        });
    }
    let bytes = encode(&layers);
    let quads: usize = layers.iter().map(|l| l.mesh.quads()).sum();
    let json = format!("{{\"quads\":{quads},\"b64\":\"{}\"}}\n", base64(&bytes));
    std::fs::write(format!("{out}/{key}.json"), &json).expect("a ship file can be written");
    println!("{key}: {quads} quads, {} bytes packed", bytes.len());
    stats(key, m, &skin, quads)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or("assets/hulls".into());
    let out = args.next().unwrap_or("out".into());
    std::fs::create_dir_all(&out).expect("the output directory can be made");
    let mut rows = Vec::new();
    // The fleet, read off the manifest rather than typed, so a class added
    // tomorrow is in the doc tomorrow. This is the same rule the setup form
    // keeps for its dropdown.
    let mut keys: Vec<String> = std::fs::read_dir(&dir)
        .expect("the hull directory can be read")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".ftvx").map(str::to_string)
        })
        .collect();
    keys.sort();
    for key in &keys {
        let bytes = std::fs::read(format!("{dir}/{key}.ftvx")).expect("a hull file can be read");
        let m = VoxelModel::from_ftvx(&bytes).expect("a hull file parses");
        rows.push(ship(key, &m, &out));
    }
    // And the aliens, which are GENERATED: there is no file for a drone, so
    // the doc is the only place anybody can look at one without running the
    // game. A fixed seed, because the doc has to show the same bug twice.
    for arch in Archetype::ALL {
        let m = alien::generate(arch, 1);
        rows.push(ship(arch.name(), &m, &out));
    }
    let index = format!("[\n  {}\n]\n", rows.join(",\n  "));
    std::fs::write(format!("{out}/index.json"), index).expect("the index can be written");
    println!("{} ships written to {out}", rows.len());
}
