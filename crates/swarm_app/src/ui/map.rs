//! Between two systems: what the fleet is, what the bank buys, and which way
//! it jumps next.
//!
//! One screen and not two, because the two halves are one decision. What a
//! node is worth depends on what you can afford to meet in it, and what is
//! worth buying depends on where you are going: a nest is not the place to
//! arrive having just spent the last of the materials on research.

use crate::*;

/// One branch out of the node the fleet is standing in.
#[derive(Component, Clone, Copy)]
pub(crate) struct GoTo(pub(crate) u16);

/// Back to the front door, giving the run up.
#[derive(Component)]
pub(crate) struct AbandonButton;

/// The screen's root, so a purchase can take it down and put it up again.
#[derive(Component)]
pub(crate) struct MapScreen;

pub(crate) fn build_map(mut commands: Commands, run: Res<RunState>, fleet: Res<Fleet>) {
    map_ui(&mut commands, &run, &fleet);
}

/// The screen itself, as a plain function rather than a system, because it
/// is built twice: on the way in, and again after a purchase. A yard whose
/// prices and roster were painted once would go on offering what it has
/// already sold.
fn map_ui(commands: &mut Commands, run: &RunState, fleet: &Fleet) {
    let offers = offers(run, fleet);
    commands
        .spawn((
            DespawnOnExit(AppState::Map),
            MapScreen,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                // Top aligned rather than centred: three columns of
                // different lengths centred on one another read as three
                // floating boxes, and these are one screen.
                align_items: AlignItems::FlexStart,
                justify_content: JustifyContent::Center,
                padding: UiRect::top(Val::Px(70.0)),
                column_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.03, 0.05, 0.86)),
        ))
        .with_children(|p| {
            p.spawn(panel(Node {
                width: Val::Px(330.0),
                ..default()
            }))
            .with_children(|p| fleet_column(p, run));
            p.spawn(panel(Node {
                width: Val::Px(390.0),
                ..default()
            }))
            .with_children(|p| refit_column(p, run, &offers));
            p.spawn(panel(Node {
                width: Val::Px(330.0),
                ..default()
            }))
            .with_children(|p| jump_column(p, run));
        });
}

/// What came through the last jump. A roster rather than a count, because
/// what a run HAS is the only thing that says what the next system will be
/// like, and "3 ships" says nothing about whether one of them refines fuel.
fn fleet_column(p: &mut ChildSpawnerCommands, run: &RunState) {
    text(p, "THE FLEET", 18.0, GOLD_TEXT);
    text(p, &run.brief(), 12.0, MUTED);
    p.spawn(Node {
        height: Val::Px(6.0),
        ..default()
    });
    row(p, "flagship", |r| {
        text(r, &hull_label(&run.flagship), 14.0, TEXT);
    });
    row(p, "escorts", |r| {
        text(r, &run.escorts.to_string(), 14.0, TEXT);
    });
    row(p, "berths", |r| {
        let (used, all) = (berthed(run), berths(run));
        let c = if used >= all { RED_TEXT } else { TEXT };
        text(r, &format!("{used} of {all}"), 14.0, c);
    });
    row(p, "hull damage", |r| {
        let (s, c) = match run.scars.len() {
            0 => ("sound".to_string(), GREEN),
            n => (format!("{n} cells open"), RED_TEXT),
        };
        text(r, &s, 14.0, c);
    });
    for role in ALL_ROLES {
        let n = run.fleet.iter().filter(|&&r| r == role).count();
        if n > 0 {
            row(p, role.label(), |r| {
                text(r, &n.to_string(), 14.0, TEXT);
            });
        }
    }
    p.spawn(Node {
        height: Val::Px(6.0),
        ..default()
    });
    row(p, "materials", |r| {
        text(r, &run.bank.materials.to_string(), 14.0, TEXT);
    });
    row(p, "volatiles", |r| {
        text(r, &run.bank.volatiles.to_string(), 14.0, TEXT);
    });
    row(p, "jump fuel", |r| {
        text(r, &format!("{:.0}", run.bank.fuel), 14.0, TEXT);
    });
    row(p, "data", |r| {
        text(r, &run.bank.data.to_string(), 14.0, TEXT);
    });
}

/// The yard. Every offer is a row, and a row a run cannot afford is DIM
/// rather than absent: what you could have had if you had mined one more
/// rock is the information the screen exists to give.
fn refit_column(p: &mut ChildSpawnerCommands, run: &RunState, offers: &[Buy]) {
    text(p, "THE YARD", 18.0, GOLD_TEXT);
    text(
        p,
        "materials buy hulls, data buys the right to build them, freighters buy the room",
        12.0,
        MUTED,
    );
    p.spawn(Node {
        height: Val::Px(6.0),
        ..default()
    });
    for buy in offers {
        let (m, d) = buy.price(run);
        let afford = run.bank.materials >= m && run.bank.data >= d;
        let price = match (m, d) {
            (0, d) => format!("{d} data"),
            (m, 0) => format!("{m} mat"),
            (m, d) => format!("{m} mat, {d} data"),
        };
        button(
            p,
            &format!("{}   ({price})", buy.label(run)),
            if afford { TEXT } else { DIM },
            buy.clone(),
        );
    }
}

/// Where next. One button per branch, and what a tag MEANS under it, because
/// a name that has to be learned by dying in it is a name that teaches
/// nothing the first time.
fn jump_column(p: &mut ChildSpawnerCommands, run: &RunState) {
    text(p, "WHERE NEXT", 18.0, GOLD_TEXT);
    text(p, "every path crosses every act", 12.0, MUTED);
    p.spawn(Node {
        height: Val::Px(6.0),
        ..default()
    });
    for &to in &run.node().next {
        let node = run.map.node(to);
        button(p, node.tag.label(), GOLD_TEXT, GoTo(to));
        text(p, tag_says(node.tag), 12.0, MUTED);
    }
    p.spawn(Node {
        height: Val::Px(10.0),
        ..default()
    });
    button(p, "Abandon the run", MUTED, AbandonButton);
}

/// What a node's tag is worth knowing before you jump into it.
fn tag_says(tag: Tag) -> &'static str {
    match tag {
        Tag::Start => "where the retreat began",
        Tag::Quiet => "thin rocks, and a slow tide",
        Tag::Ore => "metal, and the swarm knows it",
        Tag::Crystal => "crystal in every seam: fuel for two jumps",
        Tag::Derelict => "a dead hull: data, and metal, for a salvager",
        Tag::Cache => "a yard, for what you never researched",
        Tag::Nest => "carriers standing close from the first second",
        Tag::Gate => "the way out of the act",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn map_input(
    picks: Query<(&Interaction, &GoTo), Changed<Interaction>>,
    buys: Query<(&Interaction, &Buy), Changed<Interaction>>,
    abandon: Query<&Interaction, (Changed<Interaction>, With<AbandonButton>)>,
    screen: Query<Entity, With<MapScreen>>,
    fleet: Res<Fleet>,
    mut commands: Commands,
    mut run: ResMut<RunState>,
    mut ack: ResMut<Ack>,
    mut next: ResMut<NextState<AppState>>,
) {
    for (i, buy) in &buys {
        if *i != Interaction::Pressed {
            continue;
        }
        let buy = buy.clone();
        ack.text = if buy.take(&mut run) {
            format!("{}: done", buy.label(&run))
        } else {
            format!("{}: not enough in the bank", buy.label(&run))
        };
        ack.left = ACK_LIFE;
        for e in &screen {
            commands.entity(e).despawn();
        }
        map_ui(&mut commands, &run, &fleet);
        return;
    }
    for (i, to) in &picks {
        if *i == Interaction::Pressed {
            run.go(to.0);
            next.set(AppState::Playing);
            return;
        }
    }
    if pressed(&abandon) {
        next.set(AppState::Menu);
    }
}
