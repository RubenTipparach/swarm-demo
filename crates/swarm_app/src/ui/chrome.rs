//! The pieces every panel on the command deck is built from.
//!
//! One builder per thing the mockup draws, so a frame, a heading, a tab and a
//! well are written once and the layout is a call to each. Two panels that
//! drew their own headings would be two opinions about what a heading is, and
//! the day one of them gained a close button the other would not have.

use crate::*;

/// A control the deck itself lights.
///
/// One marker, one owner: the older HUD recolours every `Button` that carries
/// a `BackgroundColor`, and a deck row is one of those. Two writers for one
/// state is how a tab ends up saying something other than what is open, which
/// is a defect this project has already shipped once.
#[derive(Component)]
pub(crate) struct Chromed;

/// The nine sliced frame itself: an `ImageNode` over the node's whole box.
///
/// It is the node's own background rather than a child, so a frame costs no
/// layout and a panel's padding is measured from its own edge.
pub(crate) fn frame(skin: &Skin, kind: Frame) -> ImageNode {
    ImageNode {
        image: skin.tex(kind),
        image_mode: NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(SLICE_IN),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: 1.0,
        }),
        ..default()
    }
}

/// A framed box: the node the caller asked for, wearing the frame.
///
/// It sets the PADDING and nothing else, because a frame's inset is the same
/// on every panel and everything else is the layout's business. The first cut
/// also set the direction, and Rust's struct update syntax means a field the
/// builder lists WINS over the one the caller passed: the resource strip asked
/// for a row and got a column, silently, because `..node` only fills in what
/// is not already written. A builder may default what a caller omits and must
/// never overwrite what a caller set.
pub(crate) fn framed(skin: &Skin, kind: Frame, node: Node) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
            ..node
        },
        frame(skin, kind),
    )
}

/// A line of text that does not take the pointer, so a click reaches whatever
/// is under it. Every label on the deck goes through this.
pub(crate) fn label(p: &mut ChildSpawnerCommands, s: &str, size: f32, ink: Ink) -> Entity {
    p.spawn((
        Text::new(s),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(ink.col()),
        Pickable::IGNORE,
    ))
    .id()
}

/// The same, carrying a marker so a readout can find it again.
pub(crate) fn readout(
    p: &mut ChildSpawnerCommands,
    s: &str,
    size: f32,
    ink: Ink,
    marker: impl Bundle,
) -> Entity {
    p.spawn((
        Text::new(s),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(ink.col()),
        Pickable::IGNORE,
        marker,
    ))
    .id()
}

/// A panel's heading: the title on the left and the close cross on the right.
///
/// The cross carries whatever marker the caller hands it, because what a
/// close button DOES is the panel's business and the heading's job is only to
/// put one in the same place on every panel.
pub(crate) fn head(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    title: &str,
    close: Option<impl Bundle>,
) {
    let tok = skin.tok();
    p.spawn((
        Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(tok.edge_lit.a(0.30).col()),
        Pickable::IGNORE,
    ))
    .with_children(|h| {
        label(h, &title.to_uppercase(), 14.0, tok.gold);
        if let Some(marker) = close {
            h.spawn((
                Button,
                Node {
                    width: Val::Px(18.0),
                    height: Val::Px(16.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                frame(skin, Frame::Btn),
                Chromed,
                marker,
            ))
            .with_children(|b| {
                label(b, "X", 11.0, tok.ink);
            });
        }
    });
}

/// A bar: a well with a fill inside it, the fill carrying the marker so a
/// readout can set its width.
///
/// Every bar on the deck is this one, which is what keeps an integrity bar and
/// a fuel bar the same height: a bar whose height was written twice is a bar
/// that will be two heights.
pub(crate) fn bar(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    ink: Ink,
    height: f32,
    marker: impl Bundle,
) {
    let tok = skin.tok();
    p.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(height),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(tok.edge.col()),
        BackgroundColor(tok.row.col()),
        Pickable::IGNORE,
    ))
    .with_children(|w| {
        w.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(ink.col()),
            Pickable::IGNORE,
            marker,
        ));
    });
}

/// A row: a label on the left, a value on the right, the value marked.
pub(crate) fn kv(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    key: &str,
    value: &str,
    marker: impl Bundle,
) {
    let tok = skin.tok();
    p.spawn((
        Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|r| {
        label(r, key, 12.0, tok.dim);
        readout(r, value, 13.0, tok.ink, marker);
    });
}

/// One mark, tinted by the node rather than by the bake.
///
/// `currentColor` in the markup is the button's own state colour, and an
/// `ImageNode` multiplies its texture by its own colour, so every mark is
/// baked once in white and wears whatever the caller asks for.
pub(crate) fn mark(glyphs: &Glyphs, g: Glyph, size: f32, ink: Ink) -> impl Bundle {
    (
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            ..default()
        },
        ImageNode {
            image: glyphs.of(g),
            color: ink.col(),
            ..default()
        },
        Pickable::IGNORE,
    )
}

/// A ship's own schematic, at whatever box the caller has room for.
pub(crate) fn schematic(image: Handle<Image>, w: f32, h: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            ..default()
        },
        ImageNode { image, ..default() },
        Pickable::IGNORE,
    )
}
