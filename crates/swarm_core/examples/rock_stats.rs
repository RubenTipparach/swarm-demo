//! What a rock is worth, measured rather than guessed: how many cells of
//! stone, ore and crystal it carries, and how much of either seam has a face
//! open to space.
//!
//!     cargo run --release -p swarm_core --example rock_stats
//!
//! The design of mining rests on two numbers, and neither is a thing to
//! assume: a rock's seams are a share of it, and they are BURIED, so a miner
//! has to cut through stone to reach them exactly as the swarm cuts through
//! plating to reach a reactor. The third number is what the crystal rule
//! bought: a rock leaning to ore still carries fuel, so one rock is one
//! decision rather than two.

use swarm_core::rock::{self, Flavour};
use swarm_core::voxel::{mat, NEIGHBOURS};

/// What one rock is made of.
#[derive(Default)]
struct Cut {
    solid: usize,
    ore: usize,
    crystal: usize,
    open: usize,
}

fn cut_of(m: &swarm_core::voxel::VoxelModel) -> Cut {
    let mut c = Cut::default();
    for n in 0..m.len() {
        if m.grid[n] == mat::EMPTY {
            continue;
        }
        c.solid += 1;
        match m.grid[n] {
            mat::ACCENT => c.ore += 1,
            mat::GLOW => c.crystal += 1,
            _ => continue,
        }
        let (i, j, k) = m.at(n);
        let (i, j, k) = (i as i32, j as i32, k as i32);
        if NEIGHBOURS
            .iter()
            .any(|(di, dj, dk)| m.get(i + di, j + dj, k + dk) == mat::EMPTY)
        {
            c.open += 1;
        }
    }
    c
}

fn main() {
    // The lattice and the seeds the app's own asteroid field uses.
    const N: usize = 22;
    println!(
        "{:>5}  {:>7}  {:>6}  {:>6}  {:>8}  {:>7}  {:>7}",
        "seed", "solid", "stone", "ore", "crystal", "seam %", "exposed"
    );
    let mut tot = Cut::default();
    let seeds: Vec<u64> = (700..714).collect();
    for &seed in &seeds {
        let c = cut_of(&rock::generate(N, 0.16, seed));
        let seam = c.ore + c.crystal;
        println!(
            "{seed:>5}  {:>7}  {:>6}  {:>6}  {:>8}  {:>6.1}%  {:>6.1}%",
            c.solid,
            c.solid - seam,
            c.ore,
            c.crystal,
            100.0 * seam as f32 / c.solid as f32,
            100.0 * c.open as f32 / seam.max(1) as f32
        );
        tot.solid += c.solid;
        tot.ore += c.ore;
        tot.crystal += c.crystal;
        tot.open += c.open;
    }
    let n = seeds.len();
    let seam = tot.ore + tot.crystal;
    println!(
        "\n{n} rocks: {} solid cells, {} of ore and {} of crystal ({:.1}%), {:.1}% of the seam has a face open to space",
        tot.solid,
        tot.ore,
        tot.crystal,
        100.0 * seam as f32 / tot.solid as f32,
        100.0 * tot.open as f32 / seam.max(1) as f32
    );
    println!(
        "a rock averages {:.0} solid cells, {:.0} of ore and {:.0} of crystal",
        tot.solid as f32 / n as f32,
        tot.ore as f32 / n as f32,
        tot.crystal as f32 / n as f32
    );
    // And what the lean is worth, which is the only thing a node's tag moves.
    let rich: usize = seeds
        .iter()
        .map(|&s| cut_of(&rock::generate_of(N, 0.16, s, Flavour::Crystal)).crystal)
        .sum();
    println!(
        "a field that leans to crystal carries {rich} of it against {}, on the same ore",
        tot.crystal
    );
}
