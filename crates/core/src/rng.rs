//! Small deterministic random numbers shared by the simulation.

/// Small deterministic xorshift generator, so `core` needs no crates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rng(u32);

impl Rng {
    /// Generator for item `index` of a `seed`ed set.
    pub(crate) fn new(seed: u32, index: u32) -> Self {
        let mut v = seed ^ index.wrapping_mul(0x9e37_79b9);
        v ^= v >> 16;
        v = v.wrapping_mul(0x7feb_352d);
        v ^= v >> 15;
        v = v.wrapping_mul(0x846c_a68b);
        v ^= v >> 16;
        // Xorshift must never hold zero.
        Self(v | 1)
    }

    pub(crate) fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub(crate) fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform index in `0..n` (`n > 0`).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n - 1)
    }
}
