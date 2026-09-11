//! The front door: Skirmish, Sandbox, Campaign (not yet), Quit, over the
//! live sky.

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
            button(p, "Campaign  (not yet)", DIM, MenuAction::Campaign);
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
    mut form: ResMut<SetupForm>,
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
        // Greyed until the campaign lands: a button that does nothing says
        // so in its own label rather than by silence.
        MenuAction::Campaign => {}
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}
