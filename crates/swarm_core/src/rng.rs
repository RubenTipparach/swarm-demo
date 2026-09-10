//! Deterministic randomness.
//!
//! No platform RNG and no clock: a mote drawn from a seed on one machine is the
//! same mote on another, and a chunk's drift is a hash of the cell it came off
//! rather than a roll, so two screens watching one wound throw the same debris.

/// splitmix64: small, fast, and every seed gives a full period stream.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1). Division by 2^32 is exact in f32.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 32) as f32 / 4_294_967_296.0
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in [lo, hi].
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

/// The hash `wound.ts` picks an ember tile with, kept bit for bit so a port of
/// the wound picks the same tile for the same cell.
pub fn hash_cell(cell: u32) -> u32 {
    (cell ^ 0x9E37_79B9).wrapping_mul(2_246_822_519)
}

/// Three components in [-1, 1) hashed off a cell, for a chunk's drift.
pub fn drift_of(cell: u32, salt: u32) -> [f32; 3] {
    let h = hash_cell(cell.wrapping_add(salt.wrapping_mul(0x85EB_CA6B)));
    let a = (h & 0x3FF) as f32 / 512.0 - 1.0;
    let b = ((h >> 10) & 0x3FF) as f32 / 512.0 - 1.0;
    let c = ((h >> 20) & 0x3FF) as f32 / 512.0 - 1.0;
    [a, b, c]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn unit_stays_in_range() {
        let mut r = Rng::new(3);
        for _ in 0..10_000 {
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
        }
    }

    #[test]
    fn drift_is_a_function_of_the_cell() {
        assert_eq!(drift_of(1234, 0), drift_of(1234, 0));
        assert_ne!(drift_of(1234, 0), drift_of(1235, 0));
        for c in [0u32, 1, 99, 65535] {
            for v in drift_of(c, 5) {
                assert!((-1.0..1.0).contains(&v));
            }
        }
    }
}
