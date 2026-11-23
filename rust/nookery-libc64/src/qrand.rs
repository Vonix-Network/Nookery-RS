//! Source-grounded port of `src/static/libc64/qrand.c`.
//!
//! The C translation unit keeps two file-scope integers and advances them with
//! a single linear-congruential step.  The port keeps the same arithmetic in a
//! caller-owned state object: a `u32` idum that is multiplied by
//! `0x19660D` and incremented by `0x3C6EF35F` with wrapping semantics, exactly
//! like the C unsigned arithmetic.
//!
//! The C entry points (`qrand`, `fqrand`, `fqrand2`, `sqrand`) operate on one
//! process-wide idum.  Here the same functions are methods of [`QRand`]; a
//! single `QRand` value is the direct analogue of that global, and
//! [`QRand::new`] mirrors `sqrand`.  [`QRand::default`] reproduces the C
//! static initializer (`__qrand_idum = 1`).
//!
//! `fqrand` and `fqrand2` build a float in `[1.0, 2.0)` from the top 23 bits of
//! the advanced idum and then subtract `1.0` / `1.5`.  The bit operations are
//! reproduced verbatim rather than replaced with division, so the emitted bit
//! patterns are identical to the C ones.

/// C `QRAND_MULTIPLIER`: the multiplier of the recurrence.
pub const MULTIPLIER: u32 = 0x0019_660D;

/// C `QRAND_INCREMENT`: the addend of the recurrence.
pub const INCREMENT: u32 = 0x3C6E_F35F;

/// The C default seed (`__qrand_idum` starts at 1).
pub const DEFAULT_SEED: u32 = 1;

/// Mantissa-carrying prefix used by `fqrand` / `fqrand2`.
const FLOAT_ONE_BITS: u32 = 0x3F80_0000;

/// A `qrand.c` generator state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct QRand {
    idum: u32,
}

impl Default for QRand {
    /// The C file-scope initializer: idum starts at 1.
    fn default() -> Self {
        Self { idum: DEFAULT_SEED }
    }
}

impl QRand {
    /// C `sqrand(seed)`: install a new seed without advancing it.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self { idum: seed }
    }

    /// C `sqrand(seed)`: replace the seed of an existing generator.
    pub fn reseed(&mut self, seed: u32) {
        self.idum = seed;
    }

    /// The current idum, i.e. the value the next `qrand` step will replace.
    ///
    /// The C stashed a separate `__qrand_itemp` that was never observable; the
    /// only externally visible state is this word.
    #[must_use]
    pub fn idum(&self) -> u32 {
        self.idum
    }

    /// C `qrand()`: advance the state and return the new idum.
    pub fn qrand(&mut self) -> u32 {
        self.idum = self.idum.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.idum
    }

    /// C `fqrand()`: advance the state and return a float in `[0.0, 1.0)`.
    ///
    /// The C forms the bit pattern `(idum >> 9) | 0x3F800000`, reinterprets it
    /// as a float in `[1.0, 2.0)`, and subtracts `1.0`.
    pub fn fqrand(&mut self) -> f32 {
        let bits = (self.qrand() >> 9) | FLOAT_ONE_BITS;
        f32::from_bits(bits) - 1.0
    }

    /// C `fqrand2()`: advance the state and return a float in `[-0.5, 0.5)`.
    ///
    /// Identical to [`Self::fqrand`] except that the C subtracts `1.5`.
    pub fn fqrand2(&mut self) -> f32 {
        let bits = (self.qrand() >> 9) | FLOAT_ONE_BITS;
        f32::from_bits(bits) - 1.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurrence_matches_the_c_constant_fold() {
        let mut rng = QRand::new(1);
        // 1 * 0x19660D + 0x3C6EF35F mod 2^32.
        assert_eq!(rng.qrand(), 0x3C88_596C);
        assert_eq!(rng.idum(), 0x3C88_596C);
        assert_eq!(rng.qrand(), 0x5E88_85DB);
    }

    #[test]
    fn seed_advance_is_one_step_per_call() {
        let mut rng = QRand::new(0);
        assert_eq!(rng.idum(), 0);
        let first = rng.qrand();
        assert_eq!(first, INCREMENT);
        let mut reference = QRand::new(INCREMENT);
        assert_eq!(rng.qrand(), reference.qrand());
    }

    #[test]
    fn float_extraction_uses_the_top_23_bits() {
        let mut rng = QRand::new(0x1234_5678);
        let value = rng.qrand();
        let mut rng = QRand::new(0x1234_5678);
        let expected = f32::from_bits((value >> 9) | FLOAT_ONE_BITS) - 1.0;
        assert_eq!(rng.fqrand().to_bits(), expected.to_bits());
        assert!((0.0..1.0).contains(&expected));
    }

    #[test]
    fn default_seed_is_the_c_static_initializer() {
        assert_eq!(QRand::default().idum(), 1);
        assert_eq!(QRand::default(), QRand::new(1));
    }
}
