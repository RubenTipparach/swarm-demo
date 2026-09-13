//! The front door: Skirmish, Sandbox, Campaign, Quit, over the live sky.

use crate::*;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Skirmish,
    Sandbox,
    Campaign,
    Quit,
}

pub(crate) fn build_menu(mut commands: Commands) {
    commands
        .spawn((
            DespawnOnExit(AppState::Menu),
            panel(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(80.0),
                top: Val::Px(140.0),
                min_width: Val::Px(280.0),
                row_gap: Val::Px(8.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            text(p, "SWARM", 30.0, TEXT);
            text(p, "a real time space RTS against a cloud", 12.0, MUTED);
            p.spawn(Node {
                height: Val::Px(10.0),
                ..default()
            });
            button(p, "Skirmish", TEXT, MenuAction::Skirmish);
            button(p, "Sandbox", TEXT, MenuAction::Sandbox);
            button(p, "Campaign", TEXT, MenuAction::Campaign);
            button(p, "Quit", TEXT, MenuAction::Quit);
        });
    commands.spawn((
        DespawnOnExit(AppState::Menu),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            bottom: Val::Px(12.0),
            ..default()
        },
        Text::new("esc quits   the sky is live behind the menu"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(MUTED),
        Pickable::IGNORE,
    ));
}

/// A press on the menu, or Escape, which quits from here and nowhere else.
pub(crate) fn menu_input(
    presses: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    fleet: Res<Fleet>,
    mut form: ResMut<SetupForm>,
    mut run: ResMut<RunState>,
    mut scene: ResMut<SceneSpec>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let pressed = presses
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, a)| *a);
    let action = match pressed {
        Some(a) => a,
        None if keys.just_pressed(KeyCode::Escape) => MenuAction::Quit,
        None => return,
    };
    match action {
        MenuAction::Skirmish | MenuAction::Sandbox => {
            form.sandbox = action == MenuAction::Sandbox;
            next.set(AppState::Setup);
        }
        // The campaign is The Long Retreat, and its front door is the MAP
        // rather than the setup form: what a system holds is the node's own
        // answer (its tag decides the carriers, the rocks and which way the
        // field leans), so a form asking for hives and asteroids would be a
        // page of controls a run overwrites on the way in. The flagship is
        // the one thing a player does pick, and the form already carries it.
        MenuAction::Campaign => {
            let hull = fleet
                .0
                .get(form.flagship)
                .map(|h| h.key.clone())
                .unwrap_or_else(|| scene.hull.clone());
            // An LCG step off the seed the run is holding, so `--run-seed`
            // still decides the first campaign and a second press is a
            // different map rather than the same one again.
            let seed = run
                .seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *run = RunState::new(seed, &hull);
            run.write(&mut scene);
            info!("{}", run.brief());
            next.set(AppState::Map);
        }
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}
