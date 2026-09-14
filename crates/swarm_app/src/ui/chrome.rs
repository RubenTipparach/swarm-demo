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
///
/// The TITLE carries a marker too, for the same reason: a panel whose body
/// changes under it has a heading that has to change with it, and a heading
/// that says BUILD MENU over the research list is the tabs lighting and
/// nothing else all over again.
pub(crate) fn head(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    title: &str,
    close: Option<impl Bundle>,
    named: impl Bundle,
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
        readout(h, &title.to_uppercase(), 14.0, tok.gold, named);
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

/// How far a deck panel's content sits inside its own frame, and how tall the
/// caption line above it is.
///
/// ONE pair for all three panels, which is the whole of "align the buttons
/// with the content": three panels each choosing their own inset is three
/// left edges a player can see are different, and a caption on one and not
/// the next is a row of buttons that starts lower in one panel than in the
/// one beside it.
/// Measured against the 92 the row is tall, not chosen: 5 either side and an
/// 11 pixel caption over a 3 pixel gap leave 68 for the body, which is two
/// rows of command cells at 32 and a gap, or one row of five at the full 68.
pub(crate) const PAD: f32 = 5.0;
pub(crate) const CAP_H: f32 = 11.0;

/// One panel of the bottom deck: a frame, a caption line, and the body under
/// it, which every caller fills.
///
/// The caption is not decoration. Each one carries the one number that says
/// what the panel is looking at right now, so the row reads left to right as
/// what is selected, what it is, and what it carries.
pub(crate) fn deck_panel(
    p: &mut ChildSpawnerCommands,
    skin: &Skin,
    node: Node,
    caption: (&str, DeckStat),
    body: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let tok = skin.tok();
    // `frame` and not `framed`: that builder LISTS its own padding before
    // `..node`, and Rust's struct update syntax means the listed field wins,
    // so a caller asking for five got ten. This panel's whole grid is measured
    // off its own inset, and at ten the twelve command cells wrapped to five
    // across instead of six and the last two fell off the bottom. Same trap
    // the deck already hit once on `flex_direction`.
    p.spawn((
        Node {
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(PAD)),
            row_gap: Val::Px(3.0),
            overflow: Overflow::clip(),
            ..node
        },
        frame(skin, Frame::Panel),
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        c.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(CAP_H),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|h| {
            label(h, caption.0, 10.0, tok.dim);
            readout(h, "", 11.0, tok.ink, caption.1);
        });
        body(c);
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

/// The largest box of `bake`'s shape that fits inside `w` by `h`.
pub(crate) fn fit_node(bake: (u32, u32), w: f32, h: f32) -> Node {
    let a = aspect(bake);
    let (fw, fh) = if w / h.max(0.001) > a {
        (h * a, h)
    } else {
        (w, w / a.max(0.001))
    };
    Node {
        width: Val::Px(fw),
        height: Val::Px(fh),
        ..default()
    }
}
