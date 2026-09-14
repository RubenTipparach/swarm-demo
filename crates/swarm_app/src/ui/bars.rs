//! Health bars and the band box: UI nodes, because a bar is pixels tall.

use crate::*;

/// The band box, and the bars over your own ships.
///
/// Both are UI NODES positioned from a world to screen projection rather than
/// meshes in the scene, and that is the right call for exactly these two
/// things: a health bar is a fixed number of pixels tall whatever the range,
/// and a selection box is in screen space by definition. Anything that has to
/// hold its size in the WORLD stays a mesh, which is why the nav disc and the
/// beams are not here.
#[derive(Component)]
pub(crate) struct MarqueeBox;

/// One bar over one selected ship.
#[derive(Component)]
pub(crate) struct HealthBar;

/// The pool of bars, kept between frames rather than respawned.
#[derive(Resource, Default)]
pub(crate) struct BarPool(pub(crate) Vec<Entity>);

pub(crate) fn draw_marquee(
    mode: Res<OrderMode>,
    marquee: Res<Marquee>,
    scale: Res<UiScale>,
    windows: Query<&Window>,
    mut q: Query<&mut Node, With<MarqueeBox>>,
) {
    let Ok(mut n) = q.single_mut() else { return };
    if *mode != OrderMode::Box {
        n.display = Display::None;
        return;
    }
    let (mut lo, mut hi) = (marquee.from.min(marquee.to), marquee.from.max(marquee.to));
    // A few pixels is a click, not a box, and drawing one for it is a
    // flicker on every single selection.
    if (hi - lo).length() < CLICK_PX {
        n.display = Display::None;
        return;
    }
    // Clamped to the window for DRAWING only; the selection uses the true
    // corners, so a drag that left the window still selects what it covered.
    if let Ok(w) = windows.single() {
        let size = Vec2::new(w.width(), w.height());
        lo = lo.clamp(Vec2::ZERO, size);
        hi = hi.clamp(Vec2::ZERO, size);
    }
    // DIVIDED BY `UiScale`, exactly as `draw_bars` below does and for exactly
    // the same reason, which is the whole of the drift the owner reported.
    //
    // A `Val::Px` is an AUTHORED pixel and a cursor position is a WINDOW one.
    // The deck is authored at 1600 by 900 and scaled by the window's height,
    // so a box placed straight from the cursor is drawn at `cursor * scale`:
    // it is pushed away from the top left corner by a fifth of its own
    // distance from it at 1080, and by more on a taller screen. That reads as
    // a band box that follows the pointer at the wrong speed and sits well
    // clear of it by the time the drag is any size, which is what it did.
    //
    // The lesson is the one this file already records one function down, and
    // it is the third time it has been learned here: the fix landed in
    // `place_sensor_marks`, then in `draw_bars`, and the band box in the SAME
    // FILE as `draw_bars` was left because nobody was looking at it that day.
    // A rule that arrives after the code it applies to has to be carried to
    // every place it applies to, in the change that establishes it.
    let k = scale.0.max(0.01);
    n.display = Display::Flex;
    n.left = Val::Px(lo.x / k);
    n.top = Val::Px(lo.y / k);
    n.width = Val::Px((hi.x - lo.x) / k);
    n.height = Val::Px((hi.y - lo.y) / k);
}

/// A bar over every SELECTED ship of yours, and nothing else.
///
/// That is how an RTS says a unit is selected: the bar and the ring appear
/// when you pick it and go when you pick something else. The first cut drew a
/// bar over every ship you owned and brightened the selected ones, and a bar
/// on everything says nothing about what is selected. The swarm's carriers get
/// none either way: a health bar over an enemy turns a siege into a progress
/// bar, and what tells you a carrier is hurt is that it is bleeding and
/// burning, which it already does.
pub(crate) fn draw_bars(
    mut commands: Commands,
    hud: Res<Hud>,
    scale: Res<UiScale>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ships: Query<
        (&Transform, Option<&Hull>, Option<&Fighter>),
        (
            Or<(With<Hull>, With<Fighter>)>,
            Without<Hive>,
            With<Selected>,
        ),
    >,
    mut pool: ResMut<BarPool>,
    mut bars: Query<(&mut Node, &Children), With<HealthBar>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), Without<HealthBar>>,
) {
    let Ok((cam, cam_xf)) = cams.single() else {
        return;
    };
    // ---- DIVIDED BY `UiScale`, which is the whole of the drift ----
    //
    // The deck is authored at 1600 by 900 and scaled by the window's height,
    // so a `Val::Px` is an AUTHORED pixel and what `world_to_viewport` hands
    // back is a WINDOW one. Placed straight from the projection a bar lands at
    // that fraction of the way across the screen, which is why it sat beside
    // its ship rather than over it and drifted further the further out the
    // ship was: at 1080 the scale is 1.2, so the error is a fifth of the bar's
    // own distance from the corner rather than a fixed offset anybody would
    // have spotted as a constant.
    //
    // `place_sensor_marks` already does this and says so; this file predates
    // the deck and never learned. Same lesson as the two clamps: a rule that
    // arrives after the code it applies to is a rule the old code will miss.
    let k = scale.0.max(0.01);
    let mut want: Vec<(Vec2, f32)> = Vec::new();
    for (xf, hull, fighter) in &ships {
        let (share, radius): (f32, f32) = match (hull, fighter) {
            (Some(h), _) => {
                if h.dead_hull {
                    continue;
                }
                let core = h.reactor.len().max(1);
                let gone = h.reactor.iter().filter(|&&c| h.damage.is_dead(c)).count();
                // What the bar MEANS is how close the reactor is to going, not
                // how much plating is left. Plating comes off and the ship
                // keeps flying; the reactor is the only thing that kills it,
                // so it is the only honest thing to put on a bar.
                (
                    1.0 - (gone as f32 / core as f32) / REACTOR_LOSS,
                    h.model.radius(),
                )
            }
            (_, Some(f)) => (f.hp, FIGHTER_RADIUS),
            _ => continue,
        };
        let Ok(p) = cam.world_to_viewport(cam_xf, xf.translation + Vec3::Y * radius * 0.9) else {
            continue;
        };
        want.push((p, share.clamp(0.0, 1.0)));
    }

    while pool.0.len() < want.len() {
        let fill = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.95, 0.4)),
                Pickable::IGNORE,
            ))
            .id();
        let bar = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(BAR_W),
                    height: Val::Px(BAR_H),
                    display: Display::None,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                HealthBar,
                Pickable::IGNORE,
                children![],
            ))
            .id();
        commands.entity(bar).add_child(fill);
        pool.0.push(bar);
    }

    for (n, &bar) in pool.0.iter().enumerate() {
        let Ok((mut node, kids)) = bars.get_mut(bar) else {
            continue;
        };
        match want.get(n) {
            Some(&(p, share)) if hud.show_bars => {
                node.display = Display::Flex;
                node.left = Val::Px(p.x / k - BAR_W * 0.5);
                node.top = Val::Px(p.y / k);
                if let Some(&fill) = kids.iter().next().as_ref() {
                    if let Ok((mut fnode, mut fbg)) = fills.get_mut(fill) {
                        fnode.width = Val::Percent(share * 100.0);
                        // Green over a half, gold over a quarter, red below:
                        // three states a player can name, which is what the
                        // prototype settled on over a ramp nobody can read.
                        fbg.0 = if share > 0.5 {
                            Color::srgb(0.21, 0.91, 0.35)
                        } else if share > 0.25 {
                            Color::srgb(0.91, 0.82, 0.29)
                        } else {
                            Color::srgb(1.0, 0.35, 0.29)
                        };
                    }
                }
            }
            _ => node.display = Display::None,
        }
    }
}

/// How big a health bar is, in pixels.
pub(crate) const BAR_W: f32 = 56.0;

pub(crate) const BAR_H: f32 = 5.0;

/// What a fighter is worth, in world units, wherever something has to be
/// drawn round one: the ring, the bar's height.
pub(crate) const FIGHTER_RADIUS: f32 = 0.6;

/// The bars do not belong to a scene's entities, so a teardown leaves them:
/// this takes them down on the way out of `Playing`, or the last fight's
/// bars would hang over the menu.
pub(crate) fn clear_bars(
    mut commands: Commands,
    mut pool: ResMut<BarPool>,
    bars: Query<Entity, With<HealthBar>>,
) {
    for e in &bars {
        commands.entity(e).despawn();
    }
    pool.0.clear();
}
