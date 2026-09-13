//! Where the nth ship of a wing stands, and in what shape.
//!
//! A station is an offset in the LEADER's own frame, so a formation turns with
//! the ship it is flying beside instead of sliding round it. Which offset is
//! the shape's business and the shape is the player's, which is why this is a
//! rule and not a table in the spawner: three shapes with one implementation
//! each, asked for by index, so a wing of two and a wing of six are the same
//! function.

/// The shape a wing keeps.
///
/// Three, because a player can point at three and nobody can point at a
/// continuum. Each one answers the same question differently: a wedge is for
/// covering the leader's flanks, a line is for presenting every broadside down
/// one bearing, and a sphere is for being hard to catch in one blast.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Shape {
    /// A shallow V behind the leader, alternating to port and starboard. The
    /// formation the game has always flown.
    #[default]
    Wedge,
    /// Abreast on the leader's beam, alternating out to each side.
    Line,
    /// Spread over a shell around the leader, on the golden angle spiral the
    /// carriers and the chewers already stand on.
    Sphere,
}

impl Shape {
    /// Its name on the button.
    pub fn label(self) -> &'static str {
        match self {
            Shape::Wedge => "wedge",
            Shape::Line => "line",
            Shape::Sphere => "sphere",
        }
    }

    /// The three, in the order the button cycles them.
    pub const ALL: [Shape; 3] = [Shape::Wedge, Shape::Line, Shape::Sphere];

    /// The next one round, because the cell is one button rather than three.
    pub fn next(self) -> Shape {
        match self {
            Shape::Wedge => Shape::Line,
            Shape::Line => Shape::Sphere,
            Shape::Sphere => Shape::Wedge,
        }
    }
}

/// Where the `n`th escort stands, in the leader's frame, in hull radii.
///
/// Indexed from nought and laid out in RANKS of two, which is what makes the
/// wedge alternate: the rank is how far back and the side is which way out.
/// Multiplied by the leader's radius at the call site, so a wing on a corvette
/// and a wing on a cruiser are the same shape at two sizes.
///
/// The two FLAT shapes keep every ship behind the leader, because one ahead of
/// it masks its guns and is shot first. The sphere is the exception and that is
/// what it is for: it surrounds, which is what makes it the shape a blast does
/// not take all of.
pub fn station(shape: Shape, n: usize) -> [f32; 3] {
    let rank = (n / 2 + 1) as f32;
    // Starboard for an even index and port for an odd one, so a wing built up
    // one ship at a time stays balanced rather than growing down one side.
    // That way round because it is the way `call_one` has always laid a wedge
    // out, and a formation that changed sides the day it was factored out
    // would be a change nobody asked for hiding inside a refactor.
    let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
    match shape {
        // The shallow V the game has always flown: out and back together.
        Shape::Wedge => [side * rank * 2.6, 0.0, -rank * 2.2],
        // Abreast, so every gun bears down one line and nothing is masked by
        // the ship in front of it. Stepped back a little per rank, or the
        // outermost pair would be a length clear of the leader's own beam.
        Shape::Line => [side * rank * 3.4, 0.0, -rank * 0.6],
        // The golden angle spiral, which is the same one the carriers stand
        // on: whatever the count, no two end up on the same bearing.
        Shape::Sphere => {
            let t = (n as f32 + 0.5) / 8.0;
            let y = 1.0 - 2.0 * (t - t.floor());
            let r = (1.0 - y * y).max(0.0).sqrt();
            let a = n as f32 * 2.399_963_2;
            let reach = 3.2;
            [r * a.cos() * reach, y * reach, r * a.sin() * reach]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wedge is what the game has always flown, so it has to come out of
    /// this function exactly as the spawner used to write it inline.
    #[test]
    fn the_wedge_is_the_formation_the_wing_already_flew() {
        // Transcribed from `call_one` as it stood before this module existed,
        // sides and all: the point of the test is that factoring the wedge out
        // moved no ship, so it has to hold the OLD expression and not a fresh
        // reading of the new one.
        for n in 0..6usize {
            let rank = (n / 2) as f32 + 1.0;
            let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
            let want = [side * rank * 2.6, 0.0, -rank * 2.2];
            assert_eq!(station(Shape::Wedge, n), want, "station {n}");
        }
    }

    /// A SCREEN may not fly in front of the thing it is screening: a ship
    /// ahead of the leader masks its guns and is shot first.
    ///
    /// The sphere is deliberately exempt and that is the whole of what it is
    /// for. Surrounding the leader is the point of it, and a sphere clipped to
    /// the half behind would be a wedge with extra arithmetic. The first cut
    /// of this test asserted it over all three and the sphere failed, which is
    /// the check finding out that the invariant was the flat shapes' and not
    /// the formation's.
    #[test]
    fn a_flat_formation_never_flies_ahead_of_its_leader() {
        for shape in [Shape::Wedge, Shape::Line] {
            for n in 0..12 {
                let s = station(shape, n);
                assert!(
                    s[2] <= 0.001,
                    "{} station {n} is {} ahead of the leader",
                    shape.label(),
                    s[2]
                );
            }
        }
        // And the sphere really does go all the way round, or picking it buys
        // nothing over a wedge.
        let ahead = (0..12).any(|n| station(Shape::Sphere, n)[2] > 0.5);
        let behind = (0..12).any(|n| station(Shape::Sphere, n)[2] < -0.5);
        assert!(ahead && behind, "the sphere does not surround anything");
    }

    /// A wing built one ship at a time stays balanced rather than growing down
    /// one side, which is what the alternating index is for.
    #[test]
    fn the_wedge_and_the_line_alternate_port_and_starboard() {
        for shape in [Shape::Wedge, Shape::Line] {
            for n in (0..12).step_by(2) {
                let a = station(shape, n);
                let b = station(shape, n + 1);
                assert!(a[0] > 0.0 && b[0] < 0.0, "{} pair {n}", shape.label());
                assert!(
                    (a[0] + b[0]).abs() < 1e-5,
                    "{} pair {n} is not a mirror",
                    shape.label()
                );
            }
        }
    }

    /// No two ships on the same spot, in any shape, at any count a wing can
    /// reach. A formation that stacked two hulls would be one hull nobody can
    /// see and one collision nobody ordered.
    #[test]
    fn no_two_stations_share_a_place() {
        for shape in Shape::ALL {
            let all: Vec<[f32; 3]> = (0..12).map(|n| station(shape, n)).collect();
            for i in 0..all.len() {
                for j in i + 1..all.len() {
                    let d = (0..3)
                        .map(|a| (all[i][a] - all[j][a]).powi(2))
                        .sum::<f32>()
                        .sqrt();
                    assert!(d > 0.5, "{} {i} and {j} are {d} apart", shape.label());
                }
            }
        }
    }

    /// The sphere spreads over three dimensions, which is the whole reason to
    /// pick it: the other two are flat and a blast takes a flat formation.
    #[test]
    fn only_the_sphere_leaves_the_plane() {
        for n in 0..8 {
            assert_eq!(station(Shape::Wedge, n)[1], 0.0);
            assert_eq!(station(Shape::Line, n)[1], 0.0);
        }
        let lift: f32 = (0..8)
            .map(|n| station(Shape::Sphere, n)[1].abs())
            .fold(0.0, f32::max);
        assert!(lift > 1.0, "the sphere is flat, worst lift {lift}");
    }

    #[test]
    fn the_button_cycles_every_shape_and_comes_home() {
        let mut s = Shape::default();
        let mut seen = Vec::new();
        for _ in 0..Shape::ALL.len() {
            seen.push(s);
            s = s.next();
        }
        assert_eq!(s, Shape::default(), "the cycle does not come home");
        for want in Shape::ALL {
            assert!(seen.contains(&want), "{} is unreachable", want.label());
        }
        for shape in Shape::ALL {
            assert!(!shape.label().is_empty());
        }
    }

    /// Guard against a NaN at the point it could appear: the sphere takes a
    /// square root of one minus a square, which is negative the moment the
    /// arithmetic drifts.
    #[test]
    fn no_station_is_ever_a_nan() {
        for shape in Shape::ALL {
            for n in 0..64 {
                for a in station(shape, n) {
                    assert!(a.is_finite(), "{} station {n} is {a}", shape.label());
                }
            }
        }
    }
}
