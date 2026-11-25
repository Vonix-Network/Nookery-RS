//! The 32-bit MSL linear-congruential generator from `rand.c`.

use std::sync::Mutex;

static NEXT: Mutex<u32> = Mutex::new(1);

/// A stateful copy of the MSL generator, useful when callers need isolation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Random {
    next: u32,
}

impl Random {
    /// Creates a generator with the C default state.
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// Sets the generator state exactly as `srand` does.
    pub const fn seeded(seed: u32) -> Self {
        Self { next: seed }
    }

    /// Sets the generator state.
    pub const fn seed(&mut self, seed: u32) {
        self.next = seed;
    }

    /// Produces one MSL `rand` result.
    pub fn next_value(&mut self) -> i32 {
        self.next = self.next.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.next >> 16) & 0x7fff) as i32
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}

/// Sets the process-level generator state, matching the C global `next`.
pub fn srand(seed: u32) {
    *NEXT.lock().expect("random state lock poisoned") = seed;
}

/// Produces one value from the process-level MSL generator.
pub fn rand() -> i32 {
    let mut next = NEXT.lock().expect("random state lock poisoned");
    *next = next.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*next >> 16) & 0x7fff) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_the_source_sequence_and_32_bit_wrap() {
        let mut rng = Random::seeded(1);
        assert_eq!(rng.next_value(), 16_838);
        assert_eq!(rng.next_value(), 5_758);
        let mut wrapped = Random::seeded(u32::MAX);
        assert_eq!(wrapped.next_value(), 15_929);
    }
}
