//! The orbit camera and its input, and the shells that ride the eye.

use crate::*;

/// How fast the camera pans, as a share of its own distance per second.
pub(crate) const PAN_RATE: f32 = 0.9;

/// The most one mouse event may turn the camera, in pixels. A real drag is a
/// few dozen a frame; anything past this is the window system, not a hand.
pub(crate) const MAX_DRAG: f32 = 120.0;

/// Rides the eye: a thing with a direction and no position, which a camera
/// move must not slide across the sky.
///
/// It carries its OWN offset, and that is the fix for a sun nobody could find.
/// The rider used to write the eye straight over the translation, which threw
/// away whatever the thing had been spawned at: the star shell wants exactly
/// that, since a shell is centred on the eye, and the sun does not. The sun is
/// spawned a long way along `SUN` and was being put at the camera's own point
/// every frame, so a sphere of forty units emitting at forty sat on the eye
/// for the whole of this project's life, invisible only because a sphere seen
/// from the inside is back face culled. The file says "the sun is a body at
/// the key light's direction" and it was a body at the camera.
#[derive(Component)]
pub(crate) struct AtInfinity(pub(crate) Vec3);

#[derive(Component)]
pub(crate) struct Orbit {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) dist: f32,
    /// The place the camera is looking at. The player's, not the ship's.
    pub(crate) target: Vec3,
    /// Easing onto the flagship because the player asked for it. Cleared when
    /// it arrives and cleared the instant the player pans, because a focus
    /// that fought the pan keys would be a camera arguing with its own user.
    pub(crate) follow: bool,
    /// Where the camera is actually PLACED, eased toward either the player's
    /// own `dist` or the sensors manager's own.
    ///
    /// Two numbers rather than one saved and restored, because the mockup's
    /// rule is that the angle and the pivot are the player's and the distance
    /// is the map's: keeping the zoom untouched means coming back out of the
    /// sensors view lands on exactly the zoom that went in, with nothing to
    /// remember and nothing to get wrong.
    pub(crate) eye: f32,
}

/// How far the eye stands back in the sensors manager, in world units.
///
/// The operational area is what it has to frame, and that is a great deal
/// wider than a fight: the field camera lives inside 400 and this is five
/// times it, so the fleet becomes the cluster it actually IS and the empty
/// space round it is the part worth seeing, because empty space is where the
/// swarm is not yet. Measured against the horizon rather than taken from the
/// mockup: at the two thousand it specifies the ring is cut hard at two
/// corners, because the eye looks across the plane at a pitch rather than
/// straight down at it.
pub(crate) const SENSORS_EYE: f32 = 2600.0;

/// The horizon it draws, which is the operational area itself.
pub(crate) const SENSORS_R: f32 = 900.0;

/// The camera's own controls, and nothing else touches them.
///
/// Left drag turns it, the wheel pulls it in and out, WASD and the arrows pan
/// the FOCUS across the plane the camera is looking along, and space snaps it
/// to the flagship. Right belongs to the nav order: the two cannot share a
/// button, and Homeworld gives the world to the right.
pub(crate) fn orbit_input(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    hud: Res<Hud>,
    over_ui: Query<&Interaction>,
    mut q: Query<&mut Orbit>,
) {
    let Ok(mut o) = q.single_mut() else { return };
    let dt = scene.step(&time);

    // The pointer over a button belongs to the BUTTON. `orbit_input` reads the
    // raw mouse and Bevy's UI does not consume it, so pressing Call
    // reinforcements used to drag the camera at the same time: every click on
    // the HUD threw the view sideways, which is most of what "the camera keeps
    // snapping" was. Drained rather than ignored, so the motion of a drag that
    // started on a button does not arrive in one lump when it leaves.
    let on_ui = over_ui.iter().any(|i| *i != Interaction::None);
    if on_ui || hud.paused {
        motion.clear();
        wheel.clear();
        return;
    }

    // ---- the drag, and why it used to throw the camera across the map ----
    //
    // Motion is DRAINED whenever a drag is not in progress, including on the
    // frame the button goes down. `MessageReader` keeps everything that
    // arrived since this system last read it, so a reader that only consumes
    // events while the button is held is a reader with a backlog: move the
    // mouse across the desk with the button up and the whole journey is
    // waiting, and it all applies on the first frame of the next drag.
    //
    // And a single event's delta is CLAMPED. The window handing back focus,
    // the pointer leaving and re-entering, or a compositor releasing a grab
    // all deliver one event carrying thousands of pixels. At 0.005 radians a
    // pixel that is several whole turns inside one frame, which is exactly
    // "the camera jumped to a random angle", and it happens at the edge of the
    // screen, which is why it seemed to depend on which way you had turned.
    // MIDDLE, or alt and left. The left button belongs to selection now,
    // which is what an RTS gives it; alt and left is the alias for a mouse
    // with no middle button.
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let dragging =
        buttons.pressed(MouseButton::Middle) || (alt && buttons.pressed(MouseButton::Left));
    let started = buttons.just_pressed(MouseButton::Middle)
        || (alt && buttons.just_pressed(MouseButton::Left));
    if !dragging || started {
        motion.clear();
    } else {
        for m in motion.read() {
            let d = m.delta.clamp(Vec2::splat(-MAX_DRAG), Vec2::splat(MAX_DRAG));
            o.yaw -= d.x * 0.005;
            o.pitch = (o.pitch + d.y * 0.005).clamp(-1.4, 1.4);
        }
    }
    // Wrapped, so a long session cannot walk the yaw out to where a float has
    // no precision left and the camera moves in visible steps.
    o.yaw = o.yaw.rem_euclid(std::f32::consts::TAU);

    // Zoom is MULTIPLICATIVE and goes through `exp`, which cannot return a
    // negative number however big the input is. That is the whole fix for the
    // snap: it was `dist * (1.0 - y * 0.08)`, and a wheel reporting PIXELS
    // rather than lines hands over a y of a hundred or more per notch, so the
    // factor came out at minus seven, the distance went negative, and the
    // clamp slammed the camera to its near stop. One notch the other way and
    // it slammed to the far one. A trackpad does this on every scroll.
    for w in wheel.read() {
        let step = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 40.0,
        };
        o.dist = (o.dist * (-step.clamp(-4.0, 4.0) * 0.12).exp()).clamp(2.0, 400.0);
    }

    // Panning is in the CAMERA's frame, not the world's: pressing left has to
    // move the view left whatever way the camera is turned, or the keys mean
    // something different at every heading and nobody can learn them.
    let fwd = Vec3::new(o.yaw.sin(), 0.0, o.yaw.cos());
    let right = Vec3::new(o.yaw.cos(), 0.0, -o.yaw.sin());
    let mut pan = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan -= fwd;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan += fwd;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan -= right;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan += right;
    }
    // And up and down, because this is a game in three dimensions and a
    // camera that could only pan on one plane could not be put above or below
    // a fight that is happening at an angle to it.
    if keys.pressed(KeyCode::KeyE) || keys.pressed(KeyCode::PageUp) {
        pan += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::PageDown) {
        pan -= Vec3::Y;
    }
    if pan != Vec3::ZERO {
        // Scaled by how far out the camera is, so one press covers the same
        // share of the screen at every zoom: panning at a hundred units with
        // a step tuned for ten is a camera that will not move.
        let step = o.eye.max(o.dist) * PAN_RATE * dt;
        o.target += pan.normalize() * step;
        // The player has taken the wheel, so the focus lets go.
        o.follow = false;
    }
    // F, not space. Space pauses, which is what an RTS does with it and what
    // makes "stop the world and give orders" possible at all.
    if keys.just_pressed(KeyCode::KeyF) {
        o.follow = true;
    }

    // Nothing leaves this function as a NaN. Every expression above is guarded
    // at the point it could go wrong, so this should never fire; it is here
    // because a NaN in the camera is not a wrong picture, it is EVERY picture
    // wrong from now on, since the bad value is stored and fed back in next
    // frame. A guard that can only ever be redundant is the right price for
    // that.
    if !o.yaw.is_finite() || !o.pitch.is_finite() || !o.dist.is_finite() || !o.target.is_finite() {
        warn!("camera went non finite, reset");
        *o = Orbit {
            yaw: 0.6,
            pitch: 0.38,
            dist: 40.0,
            eye: 40.0,
            target: Vec3::ZERO,
            follow: true,
        };
    }
}

/// An RTS camera: the focus is a PLACE, and the angles are the player's.
///
/// It used to ease its focus onto the flagship every frame, which is a chase
/// camera wearing an orbit's controls. Two things are wrong with that on a
/// game about where you put your ships. You cannot look at anything except
/// the ship, so the swarm, the carriers and the rocks can only be seen by
/// flying to them; and the moment you give a move order the whole world
/// slides under you, which is the ship staying still and everything else
/// moving, exactly backwards from what an order is.
///
/// So the focus is a point in the world the player drives, and `Orbit.follow`
/// is how it gets to a ship: set by a key, eased in, and DROPPED the moment
/// the player pans. Focusing is a thing you ask for rather than a state you
/// are stuck in.
///
/// **And what it goes to is the SELECTION**, not the flagship. It went to the
/// flagship for as long as this camera has existed, which was invisible while
/// there was one ship and is simply wrong in an RTS: F is the key that takes
/// you to the thing you have picked, so pressing it with an escort or a miner
/// selected flew the camera to a different ship entirely. A whole selection
/// focuses on its own CENTROID, which is what makes F usable on a group: a
/// wing spread over a system has no single ship the camera should choose.
///
/// The flagship is the fallback when nothing is picked, because "take me to
/// my fleet" is what F means with an empty selection and there is no better
/// answer to it.
pub(crate) fn orbit_camera(
    time: Res<Time>,
    scene: Res<SceneSpec>,
    views: Res<Views>,
    picked: Query<&Transform, (With<Selected>, Without<Camera3d>)>,
    // No `Without<Selected>` on this one. Both are read only, so Bevy lets
    // them overlap, and the flagship is only ever read when nothing is
    // picked: the filter that would keep them disjoint is a filter that can
    // never matter, and it is one clippy counts.
    hulls: Query<&Transform, (With<Flagship>, Without<Camera3d>)>,
    mut q: Query<(&mut Orbit, &mut Transform), With<Camera3d>>,
) {
    let dt = scene.step(&time);
    let sensors = views.open == Some(ViewTab::Sensors);
    // Where F goes: the middle of what is picked, or the flagship when nothing
    // is. Summed rather than `single()`, because a selection is a group and
    // `single()` on a group is the "do nothing at all" this project has been
    // caught by once already, the day one hull became several.
    let mut sum = Vec3::ZERO;
    let mut n = 0u32;
    for xf in picked
        .iter()
        .chain(hulls.iter().take(usize::from(picked.is_empty())))
    {
        sum += xf.translation;
        n += 1;
    }
    let want = (n > 0).then(|| sum / n as f32);
    for (mut o, mut xf) in &mut q {
        if o.follow {
            if let Some(hull) = want {
                // Eased on `1 - exp(-k dt)` so the ease takes the same wall
                // time at twenty frames a second as at a hundred and twenty,
                // which is the rule redux-tribes' camera keeps.
                let k = 1.0 - (-3.4 * dt).exp();
                o.target = o.target.lerp(hull, k);
                // And it LETS GO once it has arrived, so a focus is a move to
                // a place rather than a lock: the ship then flies out of the
                // middle of the view under its own power, which is what says
                // it is going somewhere.
                // Against the camera's own DISTANCE, not against how far the
                // ship happens to be from the world origin. The old test grew
                // its own threshold as the ship flew away from nothing in
                // particular, so a focus let go at a different gap depending
                // on where in the map it was asked for.
                if o.target.distance(hull) < o.dist * 0.004 {
                    o.follow = false;
                }
            } else {
                o.follow = false;
            }
        }
        // The eye EASES between the player's zoom and the map's own, so the
        // sensors manager is the same camera pulling back off the same battle
        // rather than a second view cutting to it. On the same time constant
        // everything else here uses, so it takes the same wall time at twenty
        // frames a second as at a hundred and twenty.
        let want = if sensors { SENSORS_EYE } else { o.dist };
        let k = 1.0 - (-2.6 * dt).exp();
        o.eye += (want - o.eye) * k;
        if !o.eye.is_finite() {
            o.eye = o.dist;
        }
        let eye = o.target
            + Vec3::new(
                o.yaw.sin() * o.pitch.cos(),
                o.pitch.sin(),
                o.yaw.cos() * o.pitch.cos(),
            ) * o.eye;
        *xf = Transform::from_translation(eye).looking_at(o.target, Vec3::Y);
    }
}

/// Copy the eye onto everything at infinity, translation only: a star has a
/// direction and no position, and must not turn with the camera either.
///
/// Plus the thing's own offset, so a body at infinity keeps the direction it
/// was spawned along. See [`AtInfinity`].
///
/// It runs AFTER `orbit_camera`, explicitly, and that ordering is the other
/// half of the white flash. Registered as a bare tuple the two had no order
/// between them, so Bevy was free to run this one first and to pick
/// differently from one frame to the next; then the backdrop rode LAST
/// frame's eye. That is nothing at a pan and hundreds of units during the
/// ease out to the sensors view, which put a sphere emitting at forty right
/// across the picture for about a third of a second. This is the same defect
/// `orbit_input` and `orbit_camera` had, written down in this project's own
/// notes, in the same file, two systems apart: **a tuple inside a `chain` is
/// one link of that chain and is not itself chained.**
pub(crate) fn ride_the_eye(
    cam: Query<&Transform, (With<Camera3d>, Without<AtInfinity>)>,
    mut far: Query<(&mut Transform, &AtInfinity)>,
) {
    let Ok(c) = cam.single() else { return };
    for (mut xf, off) in &mut far {
        xf.translation = c.translation + off.0;
    }
}
