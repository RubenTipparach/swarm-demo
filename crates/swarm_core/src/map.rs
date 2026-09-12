//! The run: a branching chain of systems, generated from a seed.
//!
//! A node is a system you can jump to and a tag is what is in it. The map is
//! in the core because a run has to be the same run on two machines from the
//! same seed, and because what a map is allowed to look like is a rule
//! rather than a picture: every path has to reach the gate, and no path may
//! be a dead end, or a player picks a branch and the run stops.
//!
//! The tag BIASES a system rather than defining it. Every field carries some
//! of both flavours of rock, so no route can strand a run with no way to
//! make fuel: an ore system is one where the ore is worth the time, not one
//! where there is no ice.

use crate::rng::Rng;
use crate::rock::Flavour;

/// What a system holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tag {
    /// Where a run begins: thin, quiet, and the same every time, because a
    /// run needs one place to teach the loop.
    Start,
    Ore,
    Ice,
    /// A dead hull to cut, worth data and materials.
    Derelict,
    /// Somewhere to spend materials on something you did not research.
    Cache,
    /// Few carriers, thin rocks: a system you take to repair.
    Quiet,
    /// Carriers standing close from the first second.
    Nest,
    /// The way out of an act, and the last node of the run.
    Gate,
}

impl Tag {
    pub fn label(self) -> &'static str {
        match self {
            Tag::Start => "start",
            Tag::Ore => "ore",
            Tag::Ice => "ice",
            Tag::Derelict => "derelict",
            Tag::Cache => "cache",
            Tag::Quiet => "quiet",
            Tag::Nest => "nest",
            Tag::Gate => "gate",
        }
    }

    /// Which flavour the field leans toward. Both are always present.
    pub fn leans(self) -> Flavour {
        match self {
            Tag::Ice => Flavour::Ice,
            _ => Flavour::Ore,
        }
    }

    /// How many carriers this system's tide ends with, against the run's own
    /// base. A nest is where the run gets hard and a quiet system is where
    /// it lets you breathe.
    pub fn carriers(self, base: usize) -> usize {
        match self {
            Tag::Start => 1,
            Tag::Quiet | Tag::Cache => base.saturating_sub(1).max(1),
            Tag::Nest => base + 2,
            Tag::Gate => base + 3,
            _ => base,
        }
    }
}

/// One system on the map.
#[derive(Clone, Debug)]
pub struct Node {
    pub act: u8,
    /// How far into its act, so a screen can lay the map out in columns.
    pub depth: u8,
    pub tag: Tag,
    /// The seed this system's field is built from.
    pub seed: u64,
    /// Which nodes you may jump to from here. Empty only at the last gate.
    pub next: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct Map {
    pub nodes: Vec<Node>,
}

impl Map {
    pub fn node(&self, id: u16) -> &Node {
        &self.nodes[id as usize]
    }

    /// The last node, which is the run's own gate.
    pub fn last(&self) -> u16 {
        self.nodes.len() as u16 - 1
    }
}

/// The tags a middle column may draw from. Ore and ice are twice as likely
/// as the rest between them, because gathering is what a system is FOR and
/// the others are the texture round it.
const DRAW: [Tag; 8] = [
    Tag::Ore,
    Tag::Ice,
    Tag::Ore,
    Tag::Ice,
    Tag::Derelict,
    Tag::Cache,
    Tag::Quiet,
    Tag::Derelict,
];

/// Build a run: `acts` acts of `depth` ordinary systems each, every act
/// ending on a node nobody can route round.
pub fn generate(seed: u64, acts: u8, depth: u8) -> Map {
    let mut rng = Rng::new(seed);
    let mut nodes: Vec<Node> = Vec::new();
    // The opening system, alone, so every run starts in the same place.
    nodes.push(Node {
        act: 0,
        depth: 0,
        tag: Tag::Start,
        seed: rng.next_u64(),
        next: Vec::new(),
    });
    let mut previous: Vec<u16> = vec![0];
    for act in 0..acts.max(1) {
        for d in 0..depth.max(1) {
            // Two or three ways on, so a choice is a real one and the map
            // still fits a screen.
            let wide = 2 + (rng.unit() > 0.55) as usize;
            let mut column: Vec<u16> = Vec::new();
            for _ in 0..wide {
                let id = nodes.len() as u16;
                let tag = DRAW[rng.int(0, DRAW.len() as i32 - 1) as usize];
                nodes.push(Node {
                    act,
                    depth: d + 1,
                    tag,
                    seed: rng.next_u64(),
                    next: Vec::new(),
                });
                column.push(id);
            }
            // Ice is never more than one column away: a run that had to
            // cross three ore systems to find fuel would be a run the map
            // killed rather than the swarm.
            if !column.iter().any(|&c| nodes[c as usize].tag == Tag::Ice) {
                let pick = column[rng.int(0, column.len() as i32 - 1) as usize];
                nodes[pick as usize].tag = Tag::Ice;
            }
            link(&mut nodes, &previous, &column, &mut rng);
            previous = column;
        }
        // The act's own gate, which everything in the column before it leads
        // to: the last act ends the run and the others are nests.
        let id = nodes.len() as u16;
        let last_act = act + 1 == acts.max(1);
        nodes.push(Node {
            act,
            depth: depth.max(1) + 1,
            tag: if last_act { Tag::Gate } else { Tag::Nest },
            seed: rng.next_u64(),
            next: Vec::new(),
        });
        for &p in &previous {
            nodes[p as usize].next.push(id);
        }
        previous = vec![id];
    }
    Map { nodes }
}

/// Join one column to the next so that every node in the old column leads
/// somewhere and every node in the new one can be reached.
fn link(nodes: &mut [Node], from: &[u16], to: &[u16], rng: &mut Rng) {
    for (i, &f) in from.iter().enumerate() {
        // Straight on, plus sometimes a neighbour, so the map is a lattice
        // rather than a fan.
        let straight = to[i.min(to.len() - 1)];
        nodes[f as usize].next.push(straight);
        if to.len() > 1 && rng.unit() > 0.35 {
            let other = to[(i + 1) % to.len()];
            if other != straight {
                nodes[f as usize].next.push(other);
            }
        }
    }
    // Anything in the new column nobody reaches gets an edge from somebody.
    for &t in to {
        if from.iter().any(|&f| nodes[f as usize].next.contains(&t)) {
            continue;
        }
        let f = from[rng.int(0, from.len() as i32 - 1) as usize];
        nodes[f as usize].next.push(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reachable(map: &Map) -> Vec<bool> {
        let mut seen = vec![false; map.nodes.len()];
        let mut stack = vec![0u16];
        seen[0] = true;
        while let Some(n) = stack.pop() {
            for &k in &map.node(n).next {
                if !seen[k as usize] {
                    seen[k as usize] = true;
                    stack.push(k);
                }
            }
        }
        seen
    }

    #[test]
    fn every_system_is_reachable_and_only_the_gate_is_an_end() {
        for seed in 1..24u64 {
            let map = generate(seed, 3, 3);
            let seen = reachable(&map);
            assert!(seen.iter().all(|&s| s), "seed {seed} strands a system");
            let last = map.last();
            for (i, n) in map.nodes.iter().enumerate() {
                if i as u16 == last {
                    assert!(n.next.is_empty(), "the gate leads somewhere");
                } else {
                    assert!(!n.next.is_empty(), "seed {seed} node {i} is a dead end");
                }
            }
            assert_eq!(map.node(last).tag, Tag::Gate);
        }
    }

    #[test]
    fn every_path_ends_at_the_gate_and_crosses_every_act() {
        let map = generate(7, 3, 3);
        // Walk every path. The map is small enough to enumerate.
        let mut stack = vec![(0u16, 0usize)];
        let mut paths = 0;
        while let Some((n, steps)) = stack.pop() {
            assert!(steps < 40, "a path that long is a cycle");
            let node = map.node(n);
            if node.next.is_empty() {
                assert_eq!(node.tag, Tag::Gate);
                assert_eq!(node.act, 2, "a path skipped an act");
                paths += 1;
                continue;
            }
            for &k in &node.next {
                assert!(k > n, "an edge that goes backward is a cycle");
                stack.push((k, steps + 1));
            }
        }
        assert!(paths > 4, "only {paths} ways through");
    }

    #[test]
    fn ice_is_never_more_than_one_jump_away() {
        for seed in 1..24u64 {
            let map = generate(seed, 3, 3);
            let depth = map.nodes.iter().map(|n| (n.act, n.depth)).max().unwrap().1;
            for act in 0..3u8 {
                for d in 1..=depth {
                    let column: Vec<&Node> = map
                        .nodes
                        .iter()
                        .filter(|n| n.act == act && n.depth == d)
                        .collect();
                    if column.is_empty() || column[0].tag == Tag::Nest || column[0].tag == Tag::Gate
                    {
                        continue;
                    }
                    assert!(
                        column.iter().any(|n| n.tag == Tag::Ice),
                        "seed {seed} act {act} column {d} has no ice"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_seed_is_the_same_run() {
        let a = generate(99, 3, 3);
        let b = generate(99, 3, 3);
        assert_eq!(a.nodes.len(), b.nodes.len());
        for (x, y) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(x.tag, y.tag);
            assert_eq!(x.seed, y.seed);
            assert_eq!(x.next, y.next);
        }
        assert!(generate(100, 3, 3).nodes.len() > 3);
    }

    #[test]
    fn a_nest_is_harder_than_a_quiet_system() {
        assert!(Tag::Nest.carriers(6) > Tag::Ore.carriers(6));
        assert!(Tag::Quiet.carriers(6) < Tag::Ore.carriers(6));
        assert_eq!(Tag::Quiet.carriers(1), 1, "never nought carriers");
    }
}
