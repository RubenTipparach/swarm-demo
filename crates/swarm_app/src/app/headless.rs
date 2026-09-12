//! A run with no window: N frames, a PNG, a report, exit.

use crate::*;

#[derive(Resource)]
pub(crate) struct Headless {
    pub(crate) frames: u32,
    pub(crate) out: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) shot: bool,
}

#[derive(Resource)]
pub(crate) struct HeadlessTarget(pub(crate) Handle<Image>);

/// What the retreat's loop actually did, which is the one thing a picture of
/// it cannot show: a shaft in a rock and a bank that moved look the same in
/// a still frame as a miner that never arrived.
fn retreat_line(scene: &SceneSpec, bank: &Bank, rocks: &Query<&Rock>) -> Option<String> {
    if !scene.retreat {
        return None;
    }
    let (mut ore, mut ice) = (0, 0);
    for r in rocks.iter() {
        match r.flavour {
            Flavour::Ice => ice += r.seam,
            _ => ore += r.seam,
        }
    }
    Some(format!(
        "retreat: {} rocks with {ore} ore and {ice} ice left in them; banked {} materials, {} volatiles, {} data, {:.1} fuel of {JUMP_FUEL:.0}",
        rocks.iter().len(),
        bank.materials,
        bank.volatiles,
        bank.data,
        bank.fuel
    ))
}

pub(crate) fn headless_capture(
    mut h: ResMut<Headless>,
    target: Option<Res<HeadlessTarget>>,
    clock: Res<SwarmClock>,
    hulls: Query<&Hull>,
    rocks: Query<&Rock>,
    scene: Res<SceneSpec>,
    bank: Res<Bank>,
    fx: Res<LiveFx>,
    quads: Res<BeamQuads>,
    tex: Res<Textures>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut frames: Local<u32>,
    mut started: Local<Option<Instant>>,
    mut commands: Commands,
) {
    // Wall time, from an `Instant`, and NOT the sum of `Time::delta_secs()`.
    //
    // Bevy clamps the virtual delta at 250 ms so that one stalled frame
    // cannot make everything jump; the effect is that a frame slower than
    // that is REPORTED as 250 ms however long it really took. On a software
    // rasteriser that is exactly the range these runs live in, so a report
    // built out of deltas is a report that quietly stops counting at four
    // frames a second. It was also why the frame limiter looked broken when
    // it was working: capped at two a second, the sum still said four.
    let start = *started.get_or_insert_with(Instant::now);
    *frames += 1;
    if h.shot || *frames < h.frames {
        return;
    }
    let spent = start.elapsed().as_secs_f32();
    let Some(t) = target else { return };
    h.shot = true;
    let breaches: usize = hulls.iter().map(|x| x.breaches).sum();
    let dead: usize = hulls.iter().map(|x| x.damage.dead_count()).sum();
    // And what the chewing DID, which is the number that was missing: cells
    // coming off meant nothing to any ship until the drives started counting
    // theirs, so a run that chewed hundreds of cells and one that chewed none
    // reported the same thing.
    let worst = hulls.iter().map(|x| x.thrust()).fold(1.0f32, f32::min);
    println!(
        "headless: {} frames in {:.1}s ({:.1} ms/frame mean, wall clock), swarm ticks {}, chewed {} cells ({} breaches thrown), worst thrust {:.2}",
        *frames, spent, spent * 1000.0 / *frames as f32, clock.ticks, dead, breaches, worst
    );
    if let Some(line) = retreat_line(&scene, &bank, &rocks) {
        println!("{line}");
    }
    println!(
        "fx: {} beams fired and {} flak bursts, {} beams live and {} quads on the last frame, {} blasts live, {} sparks queued",
        fx.fired, fx.flak, fx.beams.len(), quads.0, fx.blasts.len(), fx.sparked
    );
    // PROVE the textures loaded rather than asserting it: a normal map that
    // failed to decode is a material with no pixels in it, and a hull drawn
    // in flat paint looks exactly like a finish that was never applied.
    let mut missing = 0;
    let mut check = |name: String, handle: &Handle<Image>| {
        let state = assets.get_load_state(handle.id());
        let pixels = images
            .get(handle)
            .and_then(|i| i.data.as_ref())
            .map_or(0, |d| d.len());
        let ok = pixels > 0 && !matches!(state, Some(LoadState::Failed(_)));
        if !ok {
            missing += 1;
            println!("texture {name}: MISSING ({state:?}, {pixels} bytes)");
        }
    };
    for (k, hd) in &tex.finishes {
        check(format!("armour_{k}_n"), hd);
    }
    for (k, m) in &tex.windows {
        check(format!("window_{k}_c"), &m.colour);
        check(format!("window_{k}_e"), &m.emissive);
        check(format!("window_{k}_n"), &m.normal);
    }
    if let Some(c) = &tex.chitin {
        check("alien_chitin_n".into(), c);
    }
    let total = tex.finishes.len() + tex.windows.len() * 3 + 1;
    println!(
        "textures: {} of {} loaded with pixels",
        total - missing,
        total
    );
    let out = h.out.clone();
    let textures_ok = missing == 0;
    commands
        .spawn(Screenshot::image(t.0.clone()))
        .observe(save_to_disk(out.clone()))
        .observe(move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            let img: &Image = &shot.event().image;
            let (w, hh) = (img.width() as usize, img.height() as usize);
            let data = img.data.as_deref().unwrap_or(&[]);
            if data.len() < w * hh * 4 {
                println!("screenshot: no pixel data ({} bytes for {}x{}, {:?})", data.len(), w, hh, img.texture_descriptor.format);
                exit.write(AppExit::error());
                return;
            }
            let lit = |x0: usize, x1: usize, y0: usize, y1: usize| -> usize {
                let mut n = 0;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let o = (y * w + x) * 4;
                        if data[o] as u32 + data[o + 1] as u32 + data[o + 2] as u32 > 60 {
                            n += 1;
                        }
                    }
                }
                n
            };
            let all = lit(0, w, 0, hh);
            let mid = lit(w / 3, 2 * w / 3, hh / 4, 3 * hh / 4);
            // And how much light there is on the screen at all, which is what
            // an A/B of the shading is read off. A swarm that shades itself
            // puts less of it there for the same number of motes, and "it
            // looks darker now" is not a measurement.
            let mut sum = 0u64;
            for y in 0..hh {
                for x in 0..w {
                    let o = (y * w + x) * 4;
                    sum += data[o] as u64 + data[o + 1] as u64 + data[o + 2] as u64;
                }
            }
            let mean = sum as f64 / (w * hh * 3) as f64;
            println!("screenshot {out}: {}x{}, {} lit pixels ({:.1}%), {} in the middle third ({:.1}%), mean {:.2} of 255", w, hh, all, 100.0 * all as f32 / (w * hh) as f32, mid, 100.0 * mid as f32 / ((w / 3) * (hh / 2)) as f32, mean);
            let ok = all > (w * hh) / 100 && mid > 0 && textures_ok;
            exit.write(if ok { AppExit::Success } else { AppExit::error() });
        });
}
