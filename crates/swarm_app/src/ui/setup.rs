//! The setup: two columns of steppers, the enemy on the left and your side
//! on the right, which write the scene and launch it. The sandbox opens the
//! same screen with one more row, the target dummy's class.

use crate::*;

/// What the screen is holding. Kept between visits, so Again and Back do
/// not lose the last choice, and filled from the command line at start so
/// `--hull karisen_frigate` is what the form opens on.
#[derive(Resource)]
pub(crate) struct SetupForm {
    pub(crate) sandbox: bool,
    /// Which of the skirmish's two modes: a battle scenario, or the base.
    pub(crate) base: bool,
    pub(crate) hives: usize,
    /// Small, medium, large: an index into `SWARMS`.
    pub(crate) swarm: usize,
    pub(crate) delay: f32,
    /// Close, normal, far: an index into `STANDS`.
    pub(crate) stand: usize,
    pub(crate) rocks: usize,
    pub(crate) seed: u64,
    /// Indices into the fleet.
    pub(crate) flagship: usize,
    pub(crate) target: usize,
    pub(crate) escorts: u32,
    pub(crate) fighters: u32,
}

pub(crate) const SWARMS: [(&str, u32); 3] = [
    ("small  20k", 20_000),
    ("medium  100k", 100_000),
    ("large  300k", 300_000),
];
pub(crate) const STANDS: [(&str, f32); 3] = [("close", 0.7), ("normal", 1.0), ("far", 1.4)];

impl SetupForm {
    pub(crate) fn from_scene(scene: &SceneSpec, fleet: &Fleet) -> SetupForm {
        let nearest = |list: &[f32], v: f32| {
            list.iter()
                .enumerate()
                .min_by(|a, b| (a.1 - v).abs().total_cmp(&(b.1 - v).abs()))
                .map_or(1, |(i, _)| i)
        };
        SetupForm {
            sandbox: scene.sandbox,
            base: scene.base,
            hives: scene.hives.clamp(1, swarm::MAX_HIVES),
            swarm: nearest(&SWARMS.map(|s| s.1 as f32), scene.motes as f32),
            delay: scene.launch_delay.clamp(0.0, 20.0),
            stand: nearest(&STANDS.map(|s| s.1), scene.stand),
            rocks: scene.rocks.min(swarm::MAX_ROCKS),
            seed: scene.seed,
            flagship: fleet.index_of(&scene.hull),
            target: fleet.index_of(&scene.target_hull),
            escorts: scene.reinforce.min(WING_MAX),
            fighters: scene.fighters.min(24),
        }
    }

    /// Write the scene the form describes. What the form does not cover
    /// (the camera, the chewers, the cadence) stays as the arguments left it.
    pub(crate) fn apply(&self, scene: &mut SceneSpec, fleet: &Fleet) {
        scene.sandbox = self.sandbox;
        scene.base = self.base && !self.sandbox;
        if scene.base {
            scene.open_base();
        } else {
            scene.support = Vec::new();
        }
        scene.hives = self.hives;
        scene.motes = SWARMS[self.swarm].1;
        scene.launch_delay = self.delay;
        scene.stand = STANDS[self.stand].1;
        scene.rocks = self.rocks;
        scene.seed = self.seed;
        scene.hull = fleet.0[self.flagship].key.clone();
        scene.target_hull = fleet.0[self.target].key.clone();
        scene.reinforce = self.escorts;
        scene.fighters = self.fighters;
        scene.showcase = self.sandbox;
        scene.time_scale = 1.0;
        scene.order = None;
        scene.aim = None;
        scene.explode = 0;
    }

    /// One step of a field, wrapping where the list is a ring and clamping
    /// where it is a range.
    pub(crate) fn step(&mut self, field: Field, delta: i32, fleet: &Fleet) {
        let ring = |v: usize, n: usize| (v as i32 + delta).rem_euclid(n as i32) as usize;
        let clamp = |v: i32, lo: i32, hi: i32| (v + delta).clamp(lo, hi);
        match field {
            Field::Hives => {
                self.hives = clamp(self.hives as i32, 1, swarm::MAX_HIVES as i32) as usize
            }
            Field::Swarm => self.swarm = ring(self.swarm, SWARMS.len()),
            Field::Delay => self.delay = clamp(self.delay as i32, 0, 20) as f32,
            Field::Stand => self.stand = ring(self.stand, STANDS.len()),
            Field::Rocks => {
                self.rocks = clamp(self.rocks as i32, 0, swarm::MAX_ROCKS as i32) as usize
            }
            Field::Seed => self.seed = (self.seed as i64 + delta as i64).max(0) as u64,
            Field::Flagship => self.flagship = ring(self.flagship, fleet.0.len().max(1)),
            Field::Target => self.target = ring(self.target, fleet.0.len().max(1)),
            Field::Escorts => self.escorts = clamp(self.escorts as i32, 0, WING_MAX as i32) as u32,
            Field::Fighters => self.fighters = clamp(self.fighters as i32, 0, 24) as u32,
            Field::Mode => self.base = !self.base,
            Field::ModeSays => {}
            Field::Facts => {}
        }
    }

    /// What a field reads as.
    pub(crate) fn show(&self, field: Field, fleet: &Fleet, facts: &HullFacts) -> String {
        let name = |i: usize| fleet.0.get(i).map_or("?".to_string(), |h| h.label.clone());
        match field {
            Field::Hives => self.hives.to_string(),
            Field::Swarm => SWARMS[self.swarm].0.to_string(),
            Field::Delay => format!("{:.0} s", self.delay),
            Field::Stand => STANDS[self.stand].0.to_string(),
            Field::Rocks => self.rocks.to_string(),
            Field::Seed => self.seed.to_string(),
            Field::Flagship => name(self.flagship),
            Field::Target => name(self.target),
            Field::Escorts => match self.escorts {
                0 => "none".into(),
                1 => "1 escort".into(),
                n => format!("{n} escorts"),
            },
            Field::Fighters => self.fighters.to_string(),
            Field::Mode => if self.base {
                "base building"
            } else {
                "battle scenario"
            }
            .to_string(),
            Field::ModeSays => if self.base {
                "hold a station against a tide that never stops: mine, build, research"
            } else {
                "destroy every mothership"
            }
            .to_string(),
            Field::Facts => facts.line(fleet.0.get(self.flagship).map_or("", |h| &h.key)),
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Field {
    /// Which of the two skirmish modes. A ring of two, so either arrow is the
    /// other one: a mode is a choice between named things rather than a
    /// number, and the arrows are what the whole form is built out of.
    Mode,
    /// The line under the mode: what winning it is. A mode is two words and
    /// what those words COST a player is a sentence, which is the flagship's
    /// own facts line arrived at for a different pick.
    ModeSays,
    Hives,
    Swarm,
    Delay,
    Stand,
    Rocks,
    Seed,
    Flagship,
    Target,
    Escorts,
    Fighters,
    /// The line under the flagship: cells, guns, drives, reactor.
    Facts,
}

/// One of the two arrows on a row.
#[derive(Component)]
pub(crate) struct Stepper(pub(crate) Field, pub(crate) i32);

/// The value between them.
#[derive(Component)]
pub(crate) struct Readout(pub(crate) Field);

#[derive(Component)]
pub(crate) struct LaunchButton;

#[derive(Component)]
pub(crate) struct BackButton;

/// What a class is made of, read off its hull once and remembered: the
/// setup shows it under the flagship so a pick is a decision and not a name.
#[derive(Resource, Default)]
pub(crate) struct HullFacts(pub(crate) HashMap<String, String>);

impl HullFacts {
    pub(crate) fn line(&self, key: &str) -> String {
        self.0.get(key).cloned().unwrap_or_default()
    }

    pub(crate) fn learn(&mut self, key: &str) {
        if key.is_empty() || self.0.contains_key(key) {
            return;
        }
        let m = load_hull(key);
        let line = format!(
            "{} cells, {} guns, {} drives, reactor {} cells",
            m.solid_count(),
            guns_of(&m).len(),
            engines_of(&m).len(),
            reactor_of(&m).len()
        );
        self.0.insert(key.to_string(), line);
    }
}

fn stepper_row(p: &mut ChildSpawnerCommands, label: &str, field: Field) {
    row(p, label, |r| {
        button(r, "<", CYAN_TEXT, Stepper(field, -1));
        r.spawn((
            Node {
                min_width: Val::Px(120.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|v| {
            v.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(TEXT),
                Readout(field),
                Pickable::IGNORE,
            ));
        });
        button(r, ">", CYAN_TEXT, Stepper(field, 1));
    });
}

/// A column of the form: the two panels either side.
fn column(left: bool) -> impl Bundle {
    panel(Node {
        position_type: PositionType::Absolute,
        top: Val::Px(40.0),
        left: if left { Val::Px(40.0) } else { Val::Auto },
        right: if left { Val::Auto } else { Val::Px(40.0) },
        width: Val::Px(400.0),
        ..default()
    })
}

/// What the swarm brings.
fn enemy_column(commands: &mut Commands) {
    commands
        .spawn((DespawnOnExit(AppState::Setup), column(true)))
        .with_children(|p| {
            text(p, "THE ENEMY", 12.0, MUTED);
            stepper_row(p, "motherships", Field::Hives);
            stepper_row(p, "swarm", Field::Swarm);
            stepper_row(p, "launch delay", Field::Delay);
            stepper_row(p, "carriers stand", Field::Stand);
            stepper_row(p, "asteroids", Field::Rocks);
            stepper_row(p, "seed", Field::Seed);
        });
}

/// What you bring.
fn your_column(commands: &mut Commands, sandbox: bool) {
    commands
        .spawn((DespawnOnExit(AppState::Setup), column(false)))
        .with_children(|p| {
            text(p, "YOUR SIDE", 12.0, MUTED);
            stepper_row(p, "flagship", Field::Flagship);
            says(p, Field::Facts);
            stepper_row(p, "wing", Field::Escorts);
            stepper_row(p, "fighters", Field::Fighters);
            if sandbox {
                stepper_row(p, "target dummy", Field::Target);
            }
        });
}

/// The MODE, in its OWN panel between the two columns, because it is not a
/// setting of either: it is what the whole form is read in the light of. A
/// battle scenario is won by killing the carriers and the base is held
/// against a tide that never stops, and the first cut put the row under THE
/// ENEMY, where it reads as one more thing the swarm brings.
fn mode_panel(commands: &mut Commands) {
    commands
        .spawn((
            DespawnOnExit(AppState::Setup),
            panel(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(40.0),
                left: Val::Px(470.0),
                width: Val::Px(340.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            text(p, "THE SKIRMISH", 12.0, MUTED);
            stepper_row(p, "mode", Field::Mode);
            says(p, Field::ModeSays);
        });
}

/// A line under a row saying what the pick above it MEANS.
///
/// Two rows carry one (the flagship's cells and guns, and what winning the
/// mode is), so it is one function: a readout is a marker and a size, and
/// two copies is two places for the size to drift.
fn says(p: &mut ChildSpawnerCommands, field: Field) {
    p.spawn((
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(MUTED),
        Readout(field),
        Pickable::IGNORE,
    ));
}

/// Back, Launch and the word for which form this is.
fn setup_footer(commands: &mut Commands, sandbox: bool) {
    let corner = |left: bool| Node {
        position_type: PositionType::Absolute,
        bottom: Val::Px(40.0),
        left: if left { Val::Px(40.0) } else { Val::Auto },
        right: if left { Val::Auto } else { Val::Px(40.0) },
        ..default()
    };
    commands
        .spawn((
            DespawnOnExit(AppState::Setup),
            corner(true),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            button(p, "Back  (esc)", TEXT, BackButton);
        });
    commands
        .spawn((
            DespawnOnExit(AppState::Setup),
            corner(false),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            let go = if sandbox {
                "Open the sandbox"
            } else {
                "Launch"
            };
            button(p, go, GOLD_TEXT, LaunchButton);
        });
    commands.spawn((
        DespawnOnExit(AppState::Setup),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(52.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        children![(
            Text::new(if sandbox { "SANDBOX" } else { "SKIRMISH" }),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            TextColor(MUTED),
            Pickable::IGNORE,
        )],
    ));
}

pub(crate) fn build_setup(mut commands: Commands, form: Res<SetupForm>) {
    enemy_column(&mut commands);
    if !form.sandbox {
        mode_panel(&mut commands);
    }
    your_column(&mut commands, form.sandbox);
    setup_footer(&mut commands, form.sandbox);
}

/// The arrows, the two buttons and the keys, and the readouts kept in step
/// with the form.
#[allow(clippy::too_many_arguments)]
pub(crate) fn setup_input(
    steppers: Query<(&Interaction, &Stepper), Changed<Interaction>>,
    launch: Query<&Interaction, (Changed<Interaction>, With<LaunchButton>)>,
    back: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    fleet: Res<Fleet>,
    mut facts: ResMut<HullFacts>,
    mut form: ResMut<SetupForm>,
    mut scene: ResMut<SceneSpec>,
    mut next: ResMut<NextState<AppState>>,
    mut readouts: Query<(&Readout, &mut Text)>,
) {
    for (i, s) in &steppers {
        if *i == Interaction::Pressed {
            form.step(s.0, s.1, &fleet);
        }
    }
    if pressed(&back) || keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Menu);
        return;
    }
    if pressed(&launch) || keys.just_pressed(KeyCode::Enter) {
        form.apply(&mut scene, &fleet);
        next.set(AppState::Playing);
        return;
    }
    if let Some(h) = fleet.0.get(form.flagship) {
        facts.learn(&h.key);
    }
    for (r, mut t) in &mut readouts {
        let want = form.show(r.0, &fleet, &facts);
        if t.0 != want {
            t.0 = want;
        }
    }
}
