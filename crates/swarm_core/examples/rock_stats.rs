//! What a rock is worth, measured rather than guessed: how many cells of
//! stone and ore it carries, and how much of that ore has a face open to
//! space.
//!
//!     cargo run --release -p swarm_core --example rock_stats
//!
//! The design of mining rests on two numbers, and neither is a thing to
//! assume: a rock's ore is a share of it, and that ore is BURIED, so a miner
//! has to cut through stone to reach it exactly as the swarm cuts through
//! plating to reach a reactor.

use swarm_core::rock;
use swarm_core::voxel::mat;

fn main() {
    // The lattice and the seeds the app's own asteroid field uses.
    const N: usize = 22;
    println!(
        "{:>5}  {:>7}  {:>6}  {:>6}  {:>7}  {:>7}",
        "seed", "solid", "stone", "ore", "ore %", "exposed"
    );
    let (mut tot_solid, mut tot_ore, mut tot_open) = (0usize, 0usize, 0usize);
    let seeds: Vec<u64> = (700..714).collect();
    for &seed in &seeds {
        let m = rock::generate(N, 0.16, seed);
        let mut solid = 0;
        let mut ore = 0;
        let mut open = 0;
        for n in 0..m.len() {
            if m.grid[n] == mat::EMPTY {
                continue;
            }
            solid += 1;
            if m.grid[n] != mat::ACCENT {
                continue;
            }
            ore += 1;
            let (i, j, k) = m.at(n);
            let (i, j, k) = (i as i32, j as i32, k as i32);
            let face = [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ];
            if face
                .iter()
                .any(|(di, dj, dk)| m.get(i + di, j + dj, k + dk) == mat::EMPTY)
            {
                open += 1;
            }
        }
        tot_solid += solid;
        tot_ore += ore;
        tot_open += open;
        println!(
            "{seed:>5}  {solid:>7}  {:>6}  {ore:>6}  {:>6.1}%  {:>6.1}%",
            solid - ore,
            100.0 * ore as f32 / solid as f32,
            100.0 * open as f32 / ore.max(1) as f32
        );
    }
    let n = seeds.len();
    println!(
        "\n{n} rocks: {tot_solid} solid cells, {tot_ore} of ore ({:.1}%), {:.1}% of the ore has a face open to space",
        100.0 * tot_ore as f32 / tot_solid as f32,
        100.0 * tot_open as f32 / tot_ore.max(1) as f32
    );
    println!(
        "a rock averages {:.0} solid cells and {:.0} of ore",
        tot_solid as f32 / n as f32,
        tot_ore as f32 / n as f32
    );
}
