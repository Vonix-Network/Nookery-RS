//! Pure Rust port of the complete `libc64/qrand` utility family.
//!
//! The source oracle is [`src/static/libc64/qrand.c`](../../src/static/libc64/qrand.c),
//! declared by [`include/libc64/qrand.h`](../../include/libc64/qrand.h).  The
//! implementation has one ordered `u32` state word and derives both float
//! forms from the exact IEEE-754 bit construction used by the source:
//!
//! ```text
//! state = state * 0x0019_666d + 0x3c6e_f35f  (mod 2^32)
//! bits  = (state >> 9) | 0x3f80_0000
//! fqrand  = f32(bits) - 1.0
//! fqrand2 = f32(bits) - 1.5
//! ```
//!
//! `QrandState` is a `#[repr(C)]` model of the two private `u32` storage words
//! (`__qrand_idum` followed by `__qrand_itemp`).  The C header exposes no
//! aggregate ABI, so this layout is useful for deterministic Rust ownership
//! and inspection but is not advertised as a C-linkage replacement.  The
//! header's four function symbols are represented by the process-global
//! functions below.  As in C, their sequence is global and ordered; callers
//! that need independent streams should use `QrandState`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::sync::atomic::{AtomicU32, Ordering};

const MULTIPLIER: u32 = 0x0019_660d;
const INCREMENT: u32 = 0x3c6e_f35f;
const FLOAT_ONE_BITS: u32 = 0x3f80_0000;

static GLOBAL_IDUM: AtomicU32 = AtomicU32::new(1);
static GLOBAL_ITEMP: AtomicU32 = AtomicU32::new(0);

/// The private state represented by the original qrand implementation.
///
/// The `itemp` field is updated by the float-producing methods exactly as in
/// C, although it does not feed the next state transition.  Its presence keeps
/// the source's storage order and size explicit without exposing raw pointers.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QrandState {
    /// The 32-bit linear-congruential state (`__qrand_idum`).
    pub idum: u32,
    /// The last float bit pattern (`__qrand_itemp`).
    pub itemp: u32,
}

impl Default for QrandState {
    fn default() -> Self {
        Self::new(1)
    }
}

impl QrandState {
    /// Creates a stream with the exact state installed by `sqrand(seed)`.
    pub const fn new(seed: u32) -> Self {
        Self {
            idum: seed,
            itemp: 0,
        }
    }

    /// Installs a seed without changing the last float bit pattern, matching
    /// `sqrand`'s update of `__qrand_idum` only.
    pub fn sqrand(&mut self, seed: u32) {
        self.idum = seed;
    }

    /// Advances the stream and returns the new 32-bit state, matching `qrand`.
    pub fn qrand(&mut self) -> u32 {
        self.idum = self.idum.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.idum
    }

    /// Advances the stream and returns the exact `fqrand` bit-derived float.
    pub fn fqrand(&mut self) -> f32 {
        self.itemp = (self.qrand() >> 9) | FLOAT_ONE_BITS;
        f32::from_bits(self.itemp) - 1.0
    }

    /// Advances the stream and returns the exact `fqrand2` bit-derived float.
    pub fn fqrand2(&mut self) -> f32 {
        self.itemp = (self.qrand() >> 9) | FLOAT_ONE_BITS;
        f32::from_bits(self.itemp) - 1.5
    }
}

fn next_global() -> u32 {
    let mut state = GLOBAL_IDUM.load(Ordering::Relaxed);
    loop {
        let next = state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        match GLOBAL_IDUM.compare_exchange_weak(state, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => state = observed,
        }
    }
}

/// Returns the next value from the process-global qrand stream.
pub fn qrand() -> u32 {
    next_global()
}

/// Installs `seed` into the process-global qrand stream.
pub fn sqrand(seed: u32) {
    GLOBAL_IDUM.store(seed, Ordering::Relaxed);
}

/// Returns the next process-global value in the source's `[0.0, 1.0)` form.
pub fn fqrand() -> f32 {
    let itemp = (next_global() >> 9) | FLOAT_ONE_BITS;
    GLOBAL_ITEMP.store(itemp, Ordering::Relaxed);
    f32::from_bits(itemp) - 1.0
}

/// Returns the next process-global value in the source's `[-0.5, 0.5)` form.
pub fn fqrand2() -> f32 {
    let itemp = (next_global() >> 9) | FLOAT_ONE_BITS;
    GLOBAL_ITEMP.store(itemp, Ordering::Relaxed);
    f32::from_bits(itemp) - 1.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn state_layout_matches_two_u32_words() {
        assert_eq!(size_of::<QrandState>(), 8);
        assert_eq!(align_of::<QrandState>(), 4);
        assert_eq!(std::mem::offset_of!(QrandState, idum), 0);
        assert_eq!(std::mem::offset_of!(QrandState, itemp), 4);
    }

    #[test]
    fn boundary_seeds_are_defined_by_u32_wraparound() {
        for seed in [0, 1, u32::MAX, 0x8000_0000, 0xffff_fffe] {
            let mut state = QrandState::new(seed);
            let expected = seed.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
            assert_eq!(state.qrand(), expected, "seed {seed:#010x}");
        }
    }

    #[test]
    fn float_forms_update_itemp_in_call_order() {
        let mut state = QrandState::new(0);
        let first = state.fqrand();
        let first_bits = state.itemp;
        let second = state.fqrand2();
        assert_eq!(
            first.to_bits(),
            (f32::from_bits(first_bits) - 1.0).to_bits()
        );
        assert_eq!(
            second.to_bits(),
            (f32::from_bits(state.itemp) - 1.5).to_bits()
        );
        assert_ne!(first.to_bits(), second.to_bits());
    }

    #[test]
    fn reseeding_does_not_clear_the_last_float_word() {
        let mut state = QrandState::new(1);
        state.fqrand();
        let itemp = state.itemp;
        state.sqrand(0);
        assert_eq!(state.itemp, itemp);
        assert_eq!(state.qrand(), INCREMENT);
    }

    #[test]
    fn mutation_guard_pins_constants_and_float_offsets() {
        let mut state = QrandState::new(1);
        assert_eq!(state.qrand(), 0x3c88_596c);
        assert_eq!(state.qrand(), 0x5e88_85db);

        let mut float_state = QrandState::new(1);
        assert_eq!(float_state.fqrand().to_bits(), 0x3e72_2160);
        assert_eq!(float_state.fqrand2().to_bits(), 0xbe05_ddf0);
    }

    #[test]
    fn global_api_can_be_reseeded_like_sqrand() {
        sqrand(0x1234_5678);
        let first = qrand();
        sqrand(0x1234_5678);
        assert_eq!(qrand(), first);
    }
}
