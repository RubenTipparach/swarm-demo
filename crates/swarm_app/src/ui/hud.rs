//! The HUD: the call button, the bindings, the frame counter, the ship
//! dropdown and what an order says on screen.

use crate::*;

/// The on screen controls.
///
/// There were none at all: every binding in the game was a key somebody had to
/// be told about, so "where is my button to call in reinforcements" is the
/// only question a player could possibly have. A button that is only a key is
/// a feature nobody can find, which is the rule redux-tribes keeps about move
/// mode being buried in a rail.
///
/// It is a real button as well as a label. Clicking it calls the wave, and the
/// key still works, because the two are the same action asked for twice.
#[derive(Component)]
pub(crate) struct CallButton;

/// The line under the button, which says what the wing is doing.
#[derive(Component)]
pub(crate) struct CallLabel;

/// What the HUD is showing and whether the game is running.
///
/// One resource rather than a flag on each widget, because "is the game
/// paused" is a fact about the SESSION and three different things need to
/// agree on it: the swarm's clock, every gameplay system, and the menu that
/// says so on screen.
#[derive(Resource)]
pub(crate) struct Hud {
    pub(crate) show_fps: bool,
    pub(crate) show_bars: bool,
    pub(crate) paused: bool,
    /// A smoothed frame rate, in frames a second.
    pub(crate) fps: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Hud {
            show_fps: true,
            show_bars: true,
            paused: false,
            fps: 0.0,
        }
    }
}

/// The frame rate in the corner.
#[derive(Component)]
pub(crate) struct FpsText;

/// The row in the menu that turns the counter off and on.
#[derive(Component)]
pub(crate) struct FpsToggle;

/// And the one for the bars over your ships.
#[derive(Component)]
pub(crate) struct BarToggle;

/// And the one that puts you back in the game.
#[derive(Component)]
pub(crate) struct ResumeButton;

/// Is the game running? Everything that moves asks this.
pub(crate) fn running(hud: Res<Hud>) -> bool {
    !hud.paused
}

/// The counter itself, off the REAL clock.
///
/// `Time` is the virtual clock and Bevy clamps its delta at 250 ms so one
/// stalled frame cannot fling everything forward, which means a frame slower
/// than that reports as 250 ms however long it really took. That is exactly
/// the trap the frame cap fell into once already, and an FPS counter built on
/// it would read a floor of four however bad things got. `Time<Real>` is the
/// wall clock and is the only honest input here.
///
/// Smoothed on a time constant rather than over a fixed number of frames, so
/// the number settles at the same rate whatever the frame rate is.
pub(crate) fn tick_fps(
    real: Res<Time<Real>>,
    mut hud: ResMut<Hud>,
    mut text: Query<(&mut Text, &mut Node), With<FpsText>>,
) {
    let dt = real.delta_secs();
    if dt > 0.0 {
        let now = 1.0 / dt;
        let k = 1.0 - (-dt / FPS_SMOOTH).exp();
        hud.fps = if hud.fps <= 0.0 {
            now
        } else {
            hud.fps + (now - hud.fps) * k
        };
    }
    let Ok((mut t, mut n)) = text.single_mut() else {
        return;
    };
    n.display = if hud.show_fps {
        Display::Flex
    } else {
        Display::None
    };
    if !hud.show_fps {
        return;
    }
    // The frame TIME beside it, because a frame rate alone cannot be compared
    // against a budget: sixteen point seven milliseconds is a number somebody
    // can hold against sixty, and "59 fps" is not.
    let want = format!("{:.0} fps   {:.1} ms", hud.fps, 1000.0 / hud.fps.max(1e-3));
    if t.0 != want {
        t.0 = want;
    }
}

/// How long the frame rate takes to settle, in seconds.
pub(crate) const FPS_SMOOTH: f32 = 0.4;

/// The hulls the dropdown offers, one per navy and per rung, so a player can
/// see what the ladder actually looks like without editing a command line.
///
/// A picked subset rather than all twenty three: the point is to try DIFFERENT
/// ships, and four corvettes from four navies tell you less than a corvette, a
/// frigate, a destroyer and a cruiser do.
pub(crate) const PICKABLE: [&str; 8] = [
    "terran_frigate",
    "terran_destroyer",
    "terran_cruiser",
    "karisen_frigate",
    "karisen_cruiser",
    "rogue_destroyer",
    "benefactor_cruiser",
    "civil_hauler",
];

/// One row of the ship dropdown.
#[derive(Component)]
pub(crate) struct HullPick(pub(crate) usize);

/// The list itself, shown and hidden by its `display`.
#[derive(Component)]
pub(crate) struct HullMenu;

/// The button that opens it.
#[derive(Component)]
pub(crate) struct HullButton;

/// Swap the flagship for another class.
///
/// It DESPAWNS and respawns rather than editing the hull in place, because a
/// ship here is its model, its damage grid, its bricks, its materials, its
/// turret children and its reactor: every one of those is derived from the
/// class at spawn, and there is no such thing as changing the class of a hull
/// that already exists. Spawning a fresh one is the same code the game starts
/// with, which is the only version of it worth having.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pick_hull(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<Textures>,
    mut scene: ResMut<SceneSpec>,
    mut cfg: ResMut<SwarmConfig>,
    picks: Query<(&Interaction, &HullPick), Changed<Interaction>>,
    opener: Query<&Interaction, (Changed<Interaction>, With<HullButton>)>,
    mut menu: Query<&mut Node, With<HullMenu>>,
    old: Query<Entity, (With<Flagship>, Without<Hive>)>,
    escorts: Query<Entity, With<Escort>>,
    wing: Query<Entity, With<Fighter>>,
) {
    if opener.iter().any(|i| *i == Interaction::Pressed) {
        for mut n in &mut menu {
            n.display = if n.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    let Some((_, pick)) = picks.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let Some(&which) = PICKABLE.get(pick.0) else {
        return;
    };
    for mut n in &mut menu {
        n.display = Display::None;
    }
    if which == scene.hull {
        return;
    }
    // The wing and the squadron go with it: an escort is a copy of the
    // flagship's class and a fighter flies off it, so leaving either behind
    // would be a formation of the ship you just replaced.
    for e in old.iter().chain(escorts.iter()).chain(wing.iter()) {
        commands.entity(e).despawn();
    }
    scene.hull = which.into();
    let (_, radius) = spawn_hull(
        &mut commands,
        &mut meshes,
        &mut materials,
        &tex,
        which,
        ShipSpec::at(Transform::IDENTITY).chewers(scene.chewers as u32),
    );
    cfg.hull_radius = radius;
    info!("flagship is a {which} now, radius {radius:.2}");
}

/// A set the whole HUD hangs off, so a headless run can skip it.
pub(crate) fn build_hud(mut commands: Commands) {
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.55)),
                BackgroundColor(Color::srgba(0.04, 0.10, 0.16, 0.80)),
                CallButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Call reinforcements  (R)"),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.80, 0.94, 1.0)),
                    Pickable::IGNORE,
                ));
            });
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.70, 0.82, 0.92, 0.85)),
                CallLabel,
                Pickable::IGNORE,
            ));
            p.spawn((
                // `concat!` of separate literals rather than one string with
                // backslash continuations: a continuation keeps the leading
                // whitespace of the next SOURCE line, so every line after the
                // first came out indented by however far the code was.
                Text::new(concat!(
                    "left drag select   middle drag orbit   WASD pan   Q E up down   F focus\n",
                    "right button opens the move disc, left click confirms, esc cancels\n",
                    "shift lifts the target off the plane   space pauses   esc opens the menu",
                )),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.62, 0.74, 0.86, 0.72)),
                Pickable::IGNORE,
            ));
            // What the mouse does RIGHT NOW, which changes with the mode, and
            // the acknowledgement of the last order, which fades.
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgba(0.55, 0.92, 0.95, 0.95)),
                ModeText,
                Pickable::IGNORE,
            ));
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.86, 0.45, 0.0)),
                AckText,
                Pickable::IGNORE,
            ));
        });

    // The distance beside a lifted target. Placed by `hud_orders`.
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.40, 0.34)),
        DistLabel,
        Pickable::IGNORE,
    ));

    // The band box. One node, moved and resized in screen pixels.
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.50, 0.95, 1.0, 0.90)),
        BackgroundColor(Color::srgba(0.30, 0.75, 1.0, 0.10)),
        MarqueeBox,
        Pickable::IGNORE,
    ));

    // The ship picker, top left, with its list folded away under it.
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.55)),
                BackgroundColor(Color::srgba(0.04, 0.10, 0.16, 0.85)),
                HullButton,
            ))
            .with_children(|b| {
                b.spawn((
                    // ASCII. The default font has no U+25BE and a missing
                    // glyph draws as a hollow box, which reads as a bug in the
                    // button rather than as a caret.
                    Text::new("Ship  v"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.80, 0.94, 1.0)),
                    Pickable::IGNORE,
                ));
            });
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    ..default()
                },
                HullMenu,
            ))
            .with_children(|list| {
                for (n, name) in PICKABLE.iter().enumerate() {
                    list.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.25)),
                        BackgroundColor(Color::srgba(0.03, 0.08, 0.13, 0.95)),
                        HullPick(n),
                    ))
                    .with_children(|t| {
                        t.spawn((
                            Text::new(name.replace('_', " ")),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.78, 0.90, 1.0)),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        });

    // The counter, in the opposite corner from the controls so it never sits
    // over anything a player has to press.
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(12.0),
            ..default()
        },
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(0.72, 0.86, 0.98, 0.80)),
        FpsText,
        Pickable::IGNORE,
    ));

    // The pause overlay. Built once and hidden by its own `display` rather
    // than spawned and despawned, so the buttons keep their identity and
    // nothing has to rebuild a menu on the frame somebody pressed escape.
    //
    // `Display::None` and not `Visibility::Hidden`: a hidden node is still
    // laid out and still picked, so an invisible Resume button would have gone
    // on swallowing clicks in the middle of the screen the whole time the game
    // was running.
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.04, 0.72)),
            PauseMenu,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::axes(Val::Px(26.0), Val::Px(22.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    min_width: Val::Px(260.0),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.45)),
                BackgroundColor(Color::srgba(0.03, 0.07, 0.12, 0.95)),
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new("Paused"),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.95, 1.0)),
                    Pickable::IGNORE,
                ));
                for (marker, label) in [
                    ("fps", "FPS counter: on"),
                    ("bars", "Health bars: on"),
                    ("resume", "Resume  (esc)"),
                    ("quit", "Quit to menu"),
                ] {
                    let mut b = c.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(9.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.40)),
                        BackgroundColor(Color::srgba(0.05, 0.12, 0.18, 0.90)),
                    ));
                    if marker == "fps" {
                        b.insert(FpsToggle);
                    } else if marker == "bars" {
                        b.insert(BarToggle);
                    } else if marker == "quit" {
                        b.insert(QuitButton);
                    } else {
                        b.insert(ResumeButton);
                    }
                    b.with_children(|t| {
                        t.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.80, 0.94, 1.0)),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        });
}

/// The button's own colours, and what it says the wing is at.
pub(crate) fn hud_feedback(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
    toggled: Query<&Interaction, (Changed<Interaction>, With<FpsToggle>)>,
    toggle_text: Query<&Children, With<FpsToggle>>,
    bars_toggled: Query<&Interaction, (Changed<Interaction>, With<BarToggle>)>,
    bars_text: Query<&Children, With<BarToggle>>,
    mut texts: Query<&mut Text, Without<CallLabel>>,
    escorts: Query<(), With<Escort>>,
    mut label: Query<&mut Text, With<CallLabel>>,
    mut hud: ResMut<Hud>,
) {
    if bars_toggled.iter().any(|i| *i == Interaction::Pressed) {
        hud.show_bars = !hud.show_bars;
        let want = if hud.show_bars {
            "Health bars: on"
        } else {
            "Health bars: off"
        };
        for kids in &bars_text {
            for k in kids.iter() {
                if let Ok(mut t) = texts.get_mut(k) {
                    t.0 = want.into();
                }
            }
        }
    }
    if toggled.iter().any(|i| *i == Interaction::Pressed) {
        hud.show_fps = !hud.show_fps;
        // The row says what it will DO next time, which means it has to say
        // what the counter is doing now. A toggle whose label never changes is
        // a control nobody can read the state of, which is the rail rule
        // redux-tribes keeps.
        let want = if hud.show_fps {
            "FPS counter: on"
        } else {
            "FPS counter: off"
        };
        for kids in &toggle_text {
            for k in kids.iter() {
                if let Ok(mut t) = texts.get_mut(k) {
                    t.0 = want.into();
                }
            }
        }
    }
    for (i, mut bg) in &mut buttons {
        bg.0 = match i {
            Interaction::Pressed => Color::srgba(0.16, 0.38, 0.52, 0.95),
            Interaction::Hovered => Color::srgba(0.09, 0.22, 0.32, 0.90),
            Interaction::None => Color::srgba(0.04, 0.10, 0.16, 0.80),
        };
    }
    let out = escorts.iter().count();
    if let Ok(mut t) = label.single_mut() {
        let want = if out >= WING_MAX as usize {
            format!("wing full: {out} of {WING_MAX}")
        } else {
            format!("wing {out} of {WING_MAX}, {WING_WAVE} per call")
        };
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// The line under the buttons that says what the mouse does right now.
#[derive(Component)]
pub(crate) struct ModeText;

/// The line that acknowledges an order and fades.
#[derive(Component)]
pub(crate) struct AckText;

/// The distance beside a lifted target, red, following it on screen.
#[derive(Component)]
pub(crate) struct DistLabel;

/// What the HUD says about the order flow: the mode line, the
/// acknowledgement, and the distance label on a lifted target.
///
/// The label is UI text projected from the raised point every frame rather
/// than a mesh, because a number has to stay the same size on screen at any
/// zoom and a mesh would not. It is shown only while the target is off the
/// plane, because on the plane the gold ring already says where, and a label
/// on every aim is clutter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hud_orders(
    mode: Res<OrderMode>,
    order: Res<NavOrder>,
    ack: Res<Ack>,
    sb: Res<Sandbox>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut mode_text: Query<&mut Text, (With<ModeText>, Without<AckText>, Without<DistLabel>)>,
    mut ack_text: Query<
        (&mut Text, &mut TextColor),
        (With<AckText>, Without<ModeText>, Without<DistLabel>),
    >,
    mut dist: Query<(&mut Node, &mut Text), (With<DistLabel>, Without<ModeText>, Without<AckText>)>,
) {
    if let Ok(mut t) = mode_text.single_mut() {
        let armed = format!(
            "{} armed: click a cell on the target   esc disarms",
            sb.weapon.map_or("nothing", Weapon::label)
        );
        let want = match *mode {
            OrderMode::Idle => "left drag selects   right click opens a move order",
            OrderMode::Box => "release to select what is inside the box",
            OrderMode::Move => "aim on the plane   hold shift to raise or lower   left click confirms   esc cancels",
            OrderMode::Range => &armed,
        };
        if t.0 != want {
            t.0 = want.into();
        }
    }
    if let Ok((mut t, mut c)) = ack_text.single_mut() {
        if t.0 != ack.text {
            t.0 = ack.text.clone();
        }
        c.0 = Color::srgba(1.0, 0.86, 0.45, (ack.left / ACK_LIFE).clamp(0.0, 1.0));
    }
    let Ok((mut node, mut text)) = dist.single_mut() else {
        return;
    };
    let Ok((cam, cam_xf)) = cams.single() else {
        return;
    };
    let shown = *mode == OrderMode::Move && order.lifted();
    let at = if shown {
        cam.world_to_viewport(cam_xf, order.target()).ok()
    } else {
        None
    };
    match at {
        Some(p) => {
            node.display = Display::Flex;
            node.left = Val::Px(p.x + 14.0);
            node.top = Val::Px(p.y - 8.0);
            let want = format!("{:.1} u", order.target().distance(order.anchor));
            if text.0 != want {
                text.0 = want;
            }
        }
        None => node.display = Display::None,
    }
}
