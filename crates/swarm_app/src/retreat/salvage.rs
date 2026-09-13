//! A dead ship, and what a salvager can do about it.
//!
//! Cutting a wreck already pays: the cubes that come off it are materials
//! and data like any other cut, which is LIQUIDATION and is what happens if
//! nobody decides otherwise. What this adds is the other half, which is that
//! the parts a salvager recovers are parts OF something, and enough of them
//! is a ship again.
//!
//! Three tiers, and they are the owner's:
//!
//! - **Seven tenths recovered** and it can be rebuilt where it lies, in the
//!   field, for materials and research.
//! - **Half to seven tenths** and it is still a ship, but only a yard can
//!   put it back, and it costs a great deal more research: what is missing
//!   has to be worked out rather than found.
//! - **Under half** and it is scrap. There is no price that rebuilds it,
//!   which is what stops a run treating every loss as a delay.

use crate::*;

/// One piece of a dead ship, and what has been taken off it.
///
/// The RECOVERY lives on the piece rather than on the run, because that is
/// where it happens; `record_salvage` is what carries it across, and it
/// carries the DIFFERENCE each frame so a piece that ages out and despawns
/// does not take what it gave back with it.
#[derive(Component)]
pub(crate) struct Remains {
    /// The ship it came off, which is that ship's own seed: a hull is given
    /// one per ship at birth and nothing else shares it.
    pub(crate) ship: u32,
    pub(crate) class: String,
    /// Cells the whole ship had, before any of this.
    pub(crate) whole: u32,
    /// Cells a salvager has cut off this piece, and how many of those the
    /// run has already been told about.
    pub(crate) got: u32,
    pub(crate) banked: u32,
}

/// A dead ship of the fleet's, as the run remembers it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hulk {
    pub(crate) ship: u32,
    pub(crate) class: String,
    pub(crate) whole: u32,
    pub(crate) got: u32,
}

impl Hulk {
    /// How much of it is back, as a share of what it was.
    pub(crate) fn share(&self) -> f32 {
        if self.whole == 0 {
            0.0
        } else {
            (self.got as f32 / self.whole as f32).min(1.0)
        }
    }

    /// What it would take to put it back together, as (materials, data), or
    /// nothing at all when too little of it was found.
    ///
    /// The materials are what a hull costs either way. What moves is the
    /// RESEARCH: a ship recovered nearly whole is reassembled, and one
    /// recovered half is half worked out from first principles, which is
    /// what research is for and why the price is in it.
    pub(crate) fn rebuild_price(&self) -> Option<(u32, u32)> {
        let share = self.share();
        if share < REBUILD_FLOOR {
            return None;
        }
        if share >= REBUILD_FIELD {
            Some((REBUILD_COST, REBUILD_DATA))
        } else {
            Some((REBUILD_COST, REBUILD_DATA * REBUILD_PENALTY))
        }
    }

    /// Whether it can be put back where it lies, rather than only at a yard.
    pub(crate) fn in_field(&self) -> bool {
        self.share() >= REBUILD_FIELD
    }
}

/// The three shares the owner set, and what a rebuild costs at each.
///
/// The materials are an escort's own price, because that is what it is: a
/// hull of that class, flying. The data is what tells the two tiers apart,
/// at three times over for a ship that has to be worked out rather than
/// reassembled.
pub(crate) const REBUILD_FLOOR: f32 = 0.5;
pub(crate) const REBUILD_FIELD: f32 = 0.7;
pub(crate) const REBUILD_COST: u32 = ESCORT_COST;
pub(crate) const REBUILD_DATA: u32 = DATA_CUBE;
pub(crate) const REBUILD_PENALTY: u32 = 3;

/// Carry what the salvagers have cut into the run's own memory of the ship.
///
/// Every frame rather than on a change, because a piece that drifts out of
/// the world takes its own count with it: what is carried across is the
/// DIFFERENCE since last time, so the total only ever goes up and a wreck
/// that ages out is a wreck nobody can recover any more of.
pub(crate) fn record_salvage(mut pieces: Query<&mut Remains>, mut run: ResMut<RunState>) {
    for mut piece in &mut pieces {
        let owed = piece.got.saturating_sub(piece.banked);
        if owed == 0 {
            continue;
        }
        piece.banked = piece.got;
        match run.hulks.iter_mut().find(|h| h.ship == piece.ship) {
            Some(hulk) => hulk.got += owed,
            None => run.hulks.push(Hulk {
                ship: piece.ship,
                class: piece.class.clone(),
                whole: piece.whole,
                got: owed,
            }),
        }
    }
}

/// The panel's own rebuild button, and what it would put back.
#[derive(Component)]
pub(crate) struct RebuildButton;

/// Put a hulk back together where it lies.
///
/// The field tier, which is the one the owner drew the line at seven tenths
/// for: enough of the ship came back that it can be reassembled rather than
/// worked out, and a yard is not needed to do it. It flies in as an escort
/// through the same call a reinforcement does, because that is what it is.
///
/// The bank pays either way, so a rebuild in the field competes with the
/// jump it is delaying: the materials are the same materials.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_input(
    keys: Res<ButtonInput<KeyCode>>,
    presses: Query<&Interaction, (Changed<Interaction>, With<RebuildButton>)>,
    scene: Res<SceneSpec>,
    cfg: Res<SwarmConfig>,
    lead: Res<Lead>,
    wing: Res<Wing>,
    tex: Res<Textures>,
    escorts: Query<(), (With<Escort>, Without<Support>)>,
    mut run: ResMut<RunState>,
    mut bank: ResMut<Bank>,
    mut ack: ResMut<Ack>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !scene.retreat || !(keys.just_pressed(KeyCode::KeyB) || pressed(&presses)) {
        return;
    }
    // The best hulk there is enough of, because a player pressing one button
    // means the obvious one: most recovered first.
    let mut ready: Vec<&Hulk> = run.hulks.iter().filter(|h| h.in_field()).collect();
    ready.sort_by(|a, b| b.share().total_cmp(&a.share()));
    let Some(hulk) = ready.first().map(|h| (*h).clone()) else {
        return;
    };
    let Some((m, d)) = hulk.rebuild_price() else {
        return;
    };
    if bank.materials < m || bank.data < d {
        ack.text = format!(
            "the {} needs {m} materials and {d} data to rebuild",
            hull_label(&hulk.class)
        );
        ack.left = ACK_LIFE * 2.0;
        return;
    }
    bank.materials -= m;
    bank.data -= d;
    run.hulks.retain(|h| h.ship != hulk.ship);
    run.escorts += 1;
    call_one(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        &hulk.class,
        Wave {
            radius: cfg.hull_radius,
            lead_pos: lead.pos,
            lead_rot: lead.rot,
            chewers: (scene.chewers / 3) as u32,
            n: escorts.iter().count() as u32,
            shape: wing.0,
        },
    );
    ack.text = format!("the {} is under way again", hull_label(&hulk.class));
    ack.left = ACK_LIFE * 2.0;
    info!(
        "rebuilt the {} from {:.0}% of its wreck",
        hulk.class,
        hulk.share() * 100.0
    );
}
