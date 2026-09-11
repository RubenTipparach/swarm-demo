//! What makes one hull tougher than another under the current rules, read
//! off the hulls themselves: cells, plate, hit points, guns, engine and
//! attitude clusters, the reactor's size and how much plating stands between
//! it and OUTSIDE space.
//!
//! Written to answer "why are the Karisen ships stronger" with numbers rather
//! than a guess (`docs/proposals/menu-campaign-subsystems.md`), and kept
//! because the question will be asked of every class that is added.
//!
//! ```sh
//! cargo run --release -p swarm_core --example hull_stats -- assets/hulls
//! ```
use swarm_core::damage::hp_for;
use swarm_core::fx::{engines_of, guns_of, reactor_of};
use swarm_core::voxel::{mat, purpose, NEIGHBOURS};
use swarm_core::VoxelModel;

fn depth(m: &VoxelModel) -> Vec<u32> {
    let n = m.len();
    let mut d = vec![u32::MAX; n];
    let mut q = std::collections::VecDeque::new();
    // OUTSIDE space only: flood the empty cells from the lattice boundary, so
    // an internal void between the frame and a part does not count as a way in.
    for c in 0..n {
        let (i, j, k) = m.at(c);
        let edge = i == 0 || j == 0 || k == 0 || i == m.nx - 1 || j == m.ny - 1 || k == m.nz - 1;
        if m.grid[c] == mat::EMPTY && edge {
            d[c] = 0;
            q.push_back(c);
        }
    }
    while let Some(c) = q.pop_front() {
        let (i, j, k) = m.at(c);
        for (di, dj, dk) in NEIGHBOURS {
            let (ni, nj, nk) = (i as i32 + di, j as i32 + dj, k as i32 + dk);
            if !m.inside(ni, nj, nk) { continue; }
            let nb = m.index(ni as usize, nj as usize, nk as usize);
            if d[nb] != u32::MAX { continue; }
            if m.grid[nb] == mat::EMPTY {
                // through outside space for free, but never through a void inside the hull
                if d[c] == 0 { d[nb] = 0; q.push_back(nb); }
            } else {
                d[nb] = d[c] + 1;
                q.push_back(nb);
            }
        }
    }
    d
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or("assets/hulls".into());
    println!("{:<20} {:>6} {:>6} {:>6} {:>8} {:>5} {:>4} {:>4} {:>7} {:>5} {:>7} {:>7}",
        "hull", "cells", "plate", "mach", "hp", "guns", "eng", "att", "reactor", "depth", "radius", "volrad");
    for key in ["terran_corvette","terran_frigate","terran_destroyer","terran_cruiser","karisen_corvette","karisen_frigate","karisen_destroyer","karisen_cruiser","rogue_destroyer","benefactor_cruiser","civil_hauler"] {
        let bytes = std::fs::read(format!("{dir}/{key}.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let cells = m.solid_count();
        let plate = m.grid.iter().filter(|&&x| mat::is_armour(x)).count();
        let mach = cells - plate;
        let hp: f32 = m.grid.iter().map(|&x| hp_for(x)).sum();
        let guns = guns_of(&m).len();
        let eng = engines_of(&m).len();
        let att = m.purp.iter().zip(&m.grid).filter(|(&p, &g)| g != mat::EMPTY && p == purpose::ATTITUDE).count();
        let reactor = reactor_of(&m);
        let d = depth(&m);
        let rdepth = reactor.iter().map(|&c| d[c]).filter(|&x| x != u32::MAX).min().unwrap_or(0);
        println!("{:<20} {:>6} {:>6} {:>6} {:>8.0} {:>5} {:>4} {:>4} {:>7} {:>5} {:>7.2} {:>7.2}",
            key, cells, plate, mach, hp, guns, eng, att, reactor.len(), rdepth, m.radius(), m.volume_radius());
    }
}
