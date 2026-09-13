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
use swarm_core::voxel::{depth_from_outside, mat, purpose};
use swarm_core::VoxelModel;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or("assets/hulls".into());
    println!(
        "{:<20} {:>6} {:>6} {:>6} {:>8} {:>5} {:>4} {:>4} {:>7} {:>5} {:>7} {:>7}",
        "hull",
        "cells",
        "plate",
        "mach",
        "hp",
        "guns",
        "eng",
        "att",
        "reactor",
        "depth",
        "radius",
        "volrad"
    );
    for key in [
        "terran_corvette",
        "terran_frigate",
        "terran_destroyer",
        "terran_cruiser",
        "karisen_corvette",
        "karisen_frigate",
        "karisen_destroyer",
        "karisen_cruiser",
        "rogue_destroyer",
        "benefactor_cruiser",
        "civil_hauler",
    ] {
        let bytes = std::fs::read(format!("{dir}/{key}.ftvx")).unwrap();
        let m = VoxelModel::from_ftvx(&bytes).unwrap();
        let cells = m.solid_count();
        let plate = m.grid.iter().filter(|&&x| mat::is_armour(x)).count();
        let mach = cells - plate;
        let hp: f32 = m.grid.iter().map(|&x| hp_for(x)).sum();
        let guns = guns_of(&m).len();
        let eng = engines_of(&m).len();
        let att = m
            .purp
            .iter()
            .zip(&m.grid)
            .filter(|(&p, &g)| g != mat::EMPTY && p == purpose::ATTITUDE)
            .count();
        let reactor = reactor_of(&m);
        let d = depth_from_outside(&m);
        let rdepth = reactor
            .iter()
            .map(|&c| d[c])
            .filter(|&x| x != u32::MAX)
            .min()
            .unwrap_or(0);
        println!(
            "{:<20} {:>6} {:>6} {:>6} {:>8.0} {:>5} {:>4} {:>4} {:>7} {:>5} {:>7.2} {:>7.2}",
            key,
            cells,
            plate,
            mach,
            hp,
            guns,
            eng,
            att,
            reactor.len(),
            rdepth,
            m.radius(),
            m.volume_radius()
        );
    }
}
