//! A move order as a mode: the disc, the lift, the commit.

use crate::*;

/// How far one order may send a ship: the OPERATIONAL AREA, in world units.
///
/// It was twenty four radii of the biggest hull in the selection, which is two
/// mistakes in one number. It was too short, reaching a little past the
/// carriers' own standoff band and stopping there, so a disc could not name
/// most of the field a player can see; and being a multiple of the HULL it
/// meant something different for every ship, so a fighter could be sent a
/// fraction of the distance a cruiser could for no reason anybody could state.
///
/// `SENSORS_R` is the operational area the sensors view frames and is already
/// this project's answer to "how big is the battle", so an order reaches
/// exactly as far as the map a player plans on. Two numbers that have to agree
/// are one number. A point past it is clamped rather than refused, which is
/// the rule it always had.
pub(crate) fn move_range() -> f32 {
    SENSORS_R
}

/// Seconds the confirmation ring lives.
pub(crate) const PING_LIFE: f32 = 0.55;

/// Seconds a line of acknowledgement stays on the HUD.
pub(crate) const ACK_LIFE: f32 = 0.9;

/// A move order being ISSUED: not the order itself, which lives on the hull,
/// but the thing the player is still pointing at. Live only in `Move`.
///
/// Homeworld's own shape. The cursor picks a point on the horizontal plane
/// through the selection, and that alone can only ever name somewhere at the
/// ships' own height; holding shift lifts the target off that plane and draws
/// the right angle triangle back down to it, which is what makes a flat screen
/// able to say a place in three dimensions at all.
#[derive(Resource, Default)]
pub(crate) struct NavOrder {
    /// The centre of the selection when the order was opened: the disc's
    /// centre, the plane's height, and the point every ship's offset is kept
    /// from so a formation arrives as one.
    pub(crate) anchor: Vec3,
    /// On the plane through the anchor, inside the disc.
    pub(crate) on_plane: Vec3,
    /// How far off that plane, positive up.
    pub(crate) lift: f32,
    /// Whether shift has been held at any point, so the vertical is drawn
    /// even when the lift is momentarily nought.
    pub(crate) lifting: bool,
    /// The biggest selected hull's radius, which every mark is sized by.
    pub(crate) radius: f32,
}

impl NavOrder {
    pub(crate) fn target(&self) -> Vec3 {
        self.on_plane + Vec3::Y * self.lift
    }

    /// The disc's radius, in world units.
    pub(crate) fn range(&self) -> f32 {
        move_range()
    }

    /// Is the target far enough off the plane to draw the triangle?
    pub(crate) fn lifted(&self) -> bool {
        self.lifting && self.lift.abs() > self.radius * 0.015
    }

    /// Aim on the plane, clamped to the disc.
    pub(crate) fn aim_on_plane(&mut self, hit: Vec3) {
        let mut off = hit - self.anchor;
        off.y = 0.0;
        let d = off.length();
        if d > self.range() && d > 1e-6 {
            off *= self.range() / d;
        }
        self.on_plane = self.anchor + off;
    }

    /// Aim the lift FROM THE CURSOR'S POSITION, not from how far it moved.
    ///
    /// The plane point holds still and the raised target lives on the
    /// vertical through it, at whatever height puts it under the pointer: the
    /// closest point on that line to the cursor's ray. So the target is where
    /// the mouse is, every frame, and lifting reads as dragging the point up
    /// the pole rather than as winding a dial, which is what a per pixel
    /// delta felt like and what the prototype replaced.
    ///
    /// Closest points between the pole `P + t*up` and the ray `O + s*D`, with
    /// `w = P - O`. NOT `O - P`: that negates `d` and `e` and therefore `t`,
    /// which put the target below the plane when the mouse went up, and was
    /// the second cut's bug. For unit `up` and `D`, `t = (b*e - d)/(1 - b*b)`
    /// with `b = up.D`, `d = up.w`, `e = D.w`.
    pub(crate) fn aim_lift(&mut self, ray: Ray3d) {
        self.lifting = true;
        let dir: Vec3 = *ray.direction;
        let w = self.on_plane - ray.origin;
        let (b, d, e) = (dir.y, w.y, dir.dot(w));
        let denom = 1.0 - b * b;
        if denom > 1e-6 {
            self.lift = (b * e - d) / denom;
        }
    }
}

/// The rings that open and fade where an order was confirmed: the
/// acknowledgement, in the world, that the click did something. Each is where,
/// how old, and the hull radius it is scaled by.
#[derive(Resource, Default)]
pub(crate) struct Pings(pub(crate) Vec<(Vec3, f32, f32)>);

/// What the HUD says about the last order, and for how much longer.
#[derive(Resource, Default)]
pub(crate) struct Ack {
    pub(crate) text: String,
    pub(crate) left: f32,
}

/// A move order asked for by something that is not the right button.
///
/// The deck's Move button sets this and `nav_input` opens the disc from it, in
/// the SAME arm the right press opens from, because what a move order is has
/// one implementation and a second opener would be a second disc the day
/// either learned anything. It is read before the "the pointer over a button
/// belongs to the button" guard, since the press that set it was on a button
/// and that guard would eat every one of them.
#[derive(Resource, Default)]
pub(crate) struct NavAsk {
    /// Open the disc.
    pub(crate) open: bool,
    /// What the commit does: an ORDER by default, or a rally point.
    ///
    /// The disc is the only way this game names a place in three dimensions,
    /// and it took a prototype and four defects to get right. A second way to
    /// pick a rally point would be that whole elevation flow written twice, so
    /// the Rally cell opens the SAME disc and this is what the commit reads.
    /// Cleared on commit and on cancel, or the next plain move order would
    /// quietly set a rally instead.
    pub(crate) rally: bool,
}

/// The Salvage cell asking for every support ship to be put to work.
///
/// A flag rather than the cell doing it, because what a job IS belongs to
/// `retreat::work` and a button that assigned one itself would be a second
/// picker: the scripted `--job` already found that out, when its own scan sent
/// a salvager to a turret that had no cells in it.
#[derive(Resource, Default)]
pub(crate) struct WorkAsk(pub(crate) bool);

/// Homeworld's own move flow, which is a MODE rather than a drag.
///
/// Right button opens the disc on the selection, the cursor aims it on the
/// plane through the ships, shift lifts it off that plane, and the LEFT button
/// commits, which is the button Homeworld confirms with and the one the first
/// cut got wrong. Escape cancels the order and leaves the mode; escape again,
/// with nothing open, is what opens the pause menu. The right button inside an
/// open order does nothing at all.
///
/// It runs while PAUSED, deliberately. Giving orders with the world stopped is
/// the whole reason a pause key is worth having in an RTS.
///
/// It runs AFTER `select_input` and `toggle_pause`, and that order is the
/// design: a left press in `Move` is refused by `select_input` because of the
/// mode, then taken here as the confirm, and the mode goes back to `Idle` only
/// once nothing else this frame can read the same press as the start of a
/// box. Escape in `Move` has already been left alone by the menu.
#[allow(clippy::too_many_arguments)]
pub(crate) fn nav_input(
    real: Res<Time<Real>>,
    scene: Res<SceneSpec>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    over_ui: Query<&Interaction>,
    mut mode: ResMut<OrderMode>,
    mut ask: ResMut<NavAsk>,
    mut order: ResMut<NavOrder>,
    mut pings: ResMut<Pings>,
    mut ack: ResMut<Ack>,
    mut rally: ResMut<Rally>,
    mut hulls: Query<(&mut Hull, &Transform, Option<&Selected>), Without<Hive>>,
    mut aimed: Local<bool>,
) {
    // The acknowledgements age on the REAL clock: a ping plays out whether or
    // not the world is running, because it is about the click and not about
    // the simulation. Under a fixed step it is the step, like everything else,
    // or a headless picture with an order in it would depend on the machine.
    let dt = scene.step(&real);
    for p in &mut pings.0 {
        p.1 += dt;
    }
    pings.0.retain(|p| p.1 < PING_LIFE);
    ack.left = (ack.left - dt).max(0.0);

    // Whose order this is: the live hulls that are selected. Fighters can be
    // selected too, for their bars, and fly their own patrol regardless.
    let mut chosen: Vec<Vec3> = Vec::new();
    let mut radius = 0.0f32;
    for (hull, xf, sel) in &hulls {
        if sel.is_some() && !hull.dead_hull {
            chosen.push(xf.translation);
            radius = radius.max(hull.model.radius());
        }
    }
    let centre = if chosen.is_empty() {
        Vec3::ZERO
    } else {
        chosen.iter().copied().sum::<Vec3>() / chosen.len() as f32
    };

    // `--aim x,y,z` opens the disc on the first frame, aimed at that point, so
    // a headless run can photograph an order being given: the disc, the
    // triangle and the label are the picture this whole flow is judged by.
    if !*aimed {
        *aimed = true;
        if let (Some(aim), false) = (scene.aim, chosen.is_empty()) {
            *mode = OrderMode::Move;
            *order = NavOrder {
                anchor: centre,
                on_plane: centre,
                lift: 0.0,
                lifting: false,
                radius: radius.max(1e-3),
            };
            order.aim_on_plane(Vec3::new(aim.x, centre.y, aim.z));
            order.lift = aim.y - centre.y;
            order.lifting = order.lift.abs() > 1e-3;
        }
    }

    // ---- an OPEN order FOLLOWS its ships ----
    //
    // The anchor is the disc's centre, the plane's height and the point every
    // ship's offset is kept from, and it was written once when the order
    // opened and never again. So a disc opened on a ship that was already
    // under way stayed where the ship HAD been and the ship flew out of its
    // own order: the rim no longer reached the cursor, the plane was at the
    // old height, and a player aiming at something beside the ship was aiming
    // relative to a point it had left.
    //
    // `centre` is recomputed from the live selection every frame just above,
    // so this is one assignment. The aim is re-derived from the cursor below
    // whenever it moves, and the commit offset is taken against the anchor at
    // the moment of the press, so a formation still arrives as a formation.
    if *mode == OrderMode::Move && !chosen.is_empty() {
        order.anchor = centre;
        order.radius = radius.max(1e-3);
    }

    // Escape cancels, and only cancels. `toggle_pause` has already run this
    // frame and left the menu alone because the mode was `Move`.
    if keys.just_pressed(KeyCode::Escape) && *mode == OrderMode::Move {
        *mode = OrderMode::Idle;
        ack.text = "move order cancelled".into();
        ack.left = ACK_LIFE;
        return;
    }

    // Asked for by a button rather than by the right press. Opened here,
    // above the pointer guard, and aimed on the next frame the cursor moves.
    if core::mem::take(&mut ask.open) && *mode == OrderMode::Idle {
        if chosen.is_empty() {
            ack.text = "nothing to order: select a ship first".into();
            ack.left = ACK_LIFE * 2.0;
        } else {
            *mode = OrderMode::Move;
            *order = NavOrder {
                anchor: centre,
                on_plane: centre,
                lift: 0.0,
                lifting: false,
                radius: radius.max(1e-3),
            };
        }
    }

    let Ok(window) = windows.single() else { return };
    let Ok((cam, cam_xf)) = cams.single() else {
        return;
    };
    // The pointer over a button belongs to the button.
    if over_ui.iter().any(|i| *i != Interaction::None) {
        return;
    }
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    match *mode {
        // The left button is down on a band box, and nothing here may read
        // the mouse until it comes back up. On the range the click is a shot,
        // and in Guard it names the ship to guard: one system per press.
        OrderMode::Box | OrderMode::Range | OrderMode::Guard => return,
        OrderMode::Idle => {
            if !(buttons.just_pressed(MouseButton::Right) && !alt) {
                return;
            }
            if chosen.is_empty() {
                ack.text = "nothing to order: select a ship first".into();
                ack.left = ACK_LIFE * 2.0;
                return;
            }
            *mode = OrderMode::Move;
            *order = NavOrder {
                anchor: centre,
                on_plane: centre,
                lift: 0.0,
                lifting: false,
                radius,
            };
            // And aim it straight away, below, so the disc opens under the
            // cursor rather than a frame later.
        }
        OrderMode::Move => {
            if buttons.just_pressed(MouseButton::Left) && !alt {
                let to = order.target();
                // The SAME disc, committing somewhere else. A rally point is a
                // place in three dimensions and this is the only flow in the
                // game that can name one: it took a prototype and four defects
                // to get right, so the Rally cell opens this rather than a
                // second aimer of its own.
                if core::mem::take(&mut ask.rally) {
                    rally.0 = Some(to);
                    pings.0.push((to, 0.0, order.radius));
                    ack.text = "rally point set".into();
                    ack.left = ACK_LIFE;
                    *mode = OrderMode::Idle;
                    info!("rally point at ({:.1}, {:.1}, {:.1})", to.x, to.y, to.z);
                    return;
                }
                // The commit. Every selected ship gets the same point offset
                // by where it already stands relative to the group, so a
                // formation arrives as a formation instead of all piling
                // onto one coordinate.
                // SELECTED and alive, and that list is the whole of it.
                // Nothing else in this game may write `Hull.order` off a
                // press: a ship that was not picked does not move, which is
                // the rule an RTS is played on.
                let mut n = 0;
                for (mut hull, xf, sel) in &mut hulls {
                    if sel.is_some() && !hull.dead_hull {
                        hull.order = Some(to + (xf.translation - order.anchor));
                        n += 1;
                    }
                }
                pings.0.push((to, 0.0, order.radius));
                ack.text = format!("{n} {} under way", if n == 1 { "ship" } else { "ships" });
                ack.left = ACK_LIFE;
                *mode = OrderMode::Idle;
                info!(
                    "move order: {n} ships to ({:.1}, {:.1}, {:.1})",
                    to.x, to.y, to.z
                );
                return;
            }
            // The right button inside an open order does nothing. It was
            // the confirm once, and a player who reaches for it by habit
            // should find it inert rather than find it committing.
        }
    }

    // Aim, with what the cursor says right now. Off the window there is
    // nothing to say and the last aim stands.
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = cam.viewport_to_world(cam_xf, cursor) else {
        return;
    };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if shift {
        // The plane point holds still and the cursor names a height on the
        // vertical through it: the two cannot both own the mouse.
        order.aim_lift(ray);
    } else if let Some(d) = ray.intersect_plane(
        Vec3::new(0.0, order.anchor.y, 0.0),
        InfinitePlane3d::new(Vec3::Y),
    ) {
        order.aim_on_plane(ray.get_point(d));
    }
}
