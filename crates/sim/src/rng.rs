//! A small, fast, seedable random number generator.
//!
//! The original used `Math.random()`. A seeded generator makes the simulation
//! *reproducible*: the same seed always gives the same herd, which makes bugs
//! repeatable and lets tests check exact behaviour. It also avoids needing the
//! system clock or OS randomness, which aren't directly available in WebAssembly.
//!
//! The algorithm is SplitMix64: a few lines, good statistical quality for games,
//! and not suitable for cryptography (nothing here needs that).

use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Rng {
    state: u64, // Private: callers can only change it through the methods below.
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits. `&mut self` because generating a number advances
    /// the internal state.
    pub fn next_u64(&mut self) -> u64 {
        // `wrapping_add`/`wrapping_mul` overflow silently by design. Plain `+` and
        // `*` would panic on overflow in debug builds, because Rust treats integer
        // overflow as a bug unless you say you want wrap-around.
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0.0..1.0`, like `Math.random()`.
    pub fn f32(&mut self) -> f32 {
        // Keep the top 24 bits: exactly as many as an f32's mantissa can hold, so
        // every result is representable and the value can never round up to 1.0.
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `range` (start inclusive, end exclusive), e.g. `rng.range(2.0..7.0)`.
    pub fn range(&mut self, range: Range<f32>) -> f32 {
        range.start + self.f32() * (range.end - range.start)
    }

    /// A random angle in `0..2π` (radians).
    pub fn angle(&mut self) -> f32 {
        self.range(0.0..std::f32::consts::TAU)
    }

    /// A random element of a non-empty slice. Returns a reference into the slice,
    /// so nothing is copied.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        // The `'a` *lifetime* says: the returned reference lives as long as `items`
        // does. Without it the compiler couldn't tell whether the result borrows
        // from `items` or from `self`.
        let index = (self.f32() * items.len() as f32) as usize;
        &items[index.min(items.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn floats_stay_in_range() {
        let mut rng = Rng::new(42);
        for _ in 0..10_000 {
            let v = rng.f32();
            assert!((0.0..1.0).contains(&v), "{v} out of 0..1");
            let r = rng.range(-3.0..5.0);
            assert!((-3.0..5.0).contains(&r), "{r} out of -3..5");
        }
    }
}
