//! One palette and one set of builders for every screen, so the menu, the
//! setup, the result and the sandbox panel are the HUD's own style rather
//! than four opinions about what a button looks like.

use crate::*;

pub(crate) const PANEL: Color = Color::srgba(0.04, 0.10, 0.16, 0.88);
pub(crate) const PANEL_LIT: Color = Color::srgba(0.12, 0.26, 0.38, 0.92);
pub(crate) const LINE: Color = Color::srgba(0.45, 0.85, 1.0, 0.55);
pub(crate) const TEXT: Color = Color::srgb(0.80, 0.94, 1.0);
pub(crate) const MUTED: Color = Color::srgba(0.62, 0.74, 0.86, 0.72);
pub(crate) const GOLD_TEXT: Color = Color::srgb(1.0, 0.86, 0.45);
pub(crate) const RED_TEXT: Color = Color::srgb(1.0, 0.40, 0.34);
pub(crate) const GREEN: Color = Color::srgb(0.56, 0.94, 0.63);
pub(crate) const CYAN_TEXT: Color = Color::srgba(0.55, 0.92, 0.95, 0.95);
pub(crate) const DIM: Color = Color::srgba(0.35, 0.44, 0.53, 0.55);

/// A panel: the translucent dark box with a cyan hairline, laid out as a
/// column. Position it with the node you pass.
pub(crate) fn panel(node: Node) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(14.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..node
        },
        BorderColor::all(LINE),
        BackgroundColor(PANEL),
    )
}

/// One line of text, not pickable, so a click goes to whatever is under it.
pub(crate) fn text(p: &mut ChildSpawnerCommands, s: &str, size: f32, colour: Color) -> Entity {
    p.spawn((
        Text::new(s),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(colour),
        Pickable::IGNORE,
    ))
    .id()
}

/// A button: bordered, with its label as a child, carrying `marker`.
pub(crate) fn button(
    p: &mut ChildSpawnerCommands,
    label: &str,
    colour: Color,
    marker: impl Bundle,
) -> Entity {
    let mut b = p.spawn((
        Button,
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(9.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BorderColor::all(if colour == TEXT { LINE } else { colour }),
        BackgroundColor(PANEL),
        marker,
    ));
    b.with_children(|t| {
        text(t, label, 15.0, colour);
    });
    b.id()
}

/// A row with a label on the left and something on the right.
pub(crate) fn row(
    p: &mut ChildSpawnerCommands,
    label: &str,
    right: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            min_height: Val::Px(28.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|r| {
        text(r, label, 13.0, MUTED);
        r.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(right);
    });
}

/// Whether any button of a kind was pressed this frame.
pub(crate) fn pressed<M: Component>(
    q: &Query<&Interaction, (Changed<Interaction>, With<M>)>,
) -> bool {
    q.iter().any(|i| *i == Interaction::Pressed)
}

/// Buttons light up under the pointer, on every screen the HUD is not on
/// (the HUD does its own, and two systems on one button would fight).
#[allow(clippy::type_complexity)]
pub(crate) fn hover_buttons(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
) {
    for (i, mut bg) in &mut buttons {
        bg.0 = match i {
            Interaction::Pressed | Interaction::Hovered => PANEL_LIT,
            Interaction::None => PANEL,
        };
    }
}

/// The classes on the shelf, in the order the setup offers them: navy by
/// navy up its ladder, then the civil trades. Read off the hull files rather
/// than typed, so a class added tomorrow is in the list tomorrow.
#[derive(Resource)]
pub(crate) struct Fleet(pub(crate) Vec<HullEntry>);

pub(crate) struct HullEntry {
    pub(crate) key: String,
    pub(crate) label: String,
}

impl Fleet {
    pub(crate) fn load() -> Fleet {
        const NAVIES: [&str; 5] = ["terran", "karisen", "rogue", "benefactor", "civil"];
        const RUNGS: [&str; 4] = ["corvette", "frigate", "destroyer", "cruiser"];
        let mut keys: Vec<String> = std::fs::read_dir(HULLS)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().into_string().ok()?;
                        name.strip_suffix(".ftvx").map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rank = |k: &str| {
            let (navy, rung) = k.split_once('_').unwrap_or((k, ""));
            let n = NAVIES
                .iter()
                .position(|x| *x == navy)
                .unwrap_or(NAVIES.len());
            let r = RUNGS.iter().position(|x| *x == rung).unwrap_or(RUNGS.len());
            (n, r, k.to_string())
        };
        keys.sort_by_key(|k| rank(k));
        let entries = keys
            .into_iter()
            .map(|key| {
                let mut label = key.replace('_', " ");
                if let Some(c) = label.get_mut(0..1) {
                    c.make_ascii_uppercase();
                }
                HullEntry { key, label }
            })
            .collect();
        Fleet(entries)
    }

    pub(crate) fn index_of(&self, key: &str) -> usize {
        self.0.iter().position(|h| h.key == key).unwrap_or(0)
    }
}
