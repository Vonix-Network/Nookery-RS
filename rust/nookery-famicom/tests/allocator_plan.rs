//! PC arena plan: 4 MiB request, 16-byte alignment trim, `0xFF` fill, cleanup
//! ordering, and the `Famicom_MallocInfo` boundary.

use core::num::NonZeroUsize;
use core::ptr::NonNull;
use nookery_famicom::{
    ALLOCATION_FILL_BYTE, ARENA_ALIGNMENT, ArenaPlan, ArenaPlanError, CleanupStep,
    FamicomAllocator, MessageFlags, PC_ARENA_REQUEST_BYTES, PC_CLEANUP_SEQUENCE,
    accepts_extra_block, pc_arena_plan,
};

#[test]
fn pc_request_is_four_mebibytes_at_sixteen_byte_alignment() {
    assert_eq!(PC_ARENA_REQUEST_BYTES, 0x40_0000);
    assert_eq!(PC_ARENA_REQUEST_BYTES, 4 * 1024 * 1024);
    assert_eq!(ARENA_ALIGNMENT, 16);
    assert_eq!(ALLOCATION_FILL_BYTE, 0xFF);
}

#[test]
fn unaligned_base_trims_only_the_leading_slack() {
    // 0x1003 → next 16-byte boundary is 0x1010, a 13-byte trim.
    let plan = ArenaPlan::new(0x1003, 0x40_0000, 16).unwrap();
    assert_eq!(plan.trim(), 13);
    assert_eq!(plan.arena_start(), 0x1010);
    assert_eq!(plan.arena_bytes(), 0x40_0000 - 13);
    assert_eq!(plan.host_base(), 0x1003);
}

#[test]
fn aligned_base_trims_nothing() {
    let plan = ArenaPlan::new(0x1000, 0x40_0000, 16).unwrap();
    assert_eq!(plan.trim(), 0);
    assert_eq!(plan.arena_start(), 0x1000);
    assert_eq!(plan.arena_bytes(), 0x40_0000);
}

#[test]
fn trim_matches_the_align_next_macro_for_every_offset() {
    for offset in 0..64usize {
        let base = 0x8000_0000 + offset;
        let plan = pc_arena_plan(base);
        let expected = (16 - (base % 16)) % 16;
        assert_eq!(plan.trim(), expected, "offset {offset}");
        assert_eq!(plan.arena_start() % 16, 0, "offset {offset}");
        assert_eq!(plan.arena_start() + plan.arena_bytes(), base + 0x40_0000);
    }
}

#[test]
fn pc_plan_always_accepts_the_full_four_mebibyte_request() {
    // Worst case trim is 15 bytes, so the arena always keeps 4 MiB - 15.
    for base in [0usize, 1, 7, 15, 0xDEAD_BEEF, usize::MAX - 0x40_0000] {
        let plan = pc_arena_plan(base);
        assert!(
            plan.arena_bytes() >= PC_ARENA_REQUEST_BYTES - 15,
            "base {base:#x}"
        );
        assert!(plan.fits(PC_ARENA_REQUEST_BYTES - 15), "base {base:#x}");
    }
}

#[test]
fn fits_reports_capacity_without_claiming_fragmentation() {
    let plan = pc_arena_plan(0);
    assert!(plan.fits(0));
    assert!(plan.fits(plan.arena_bytes()));
    assert!(!plan.fits(plan.arena_bytes() + 1));
}

#[test]
fn plan_rejects_zero_request() {
    assert_eq!(ArenaPlan::new(0, 0, 16), Err(ArenaPlanError::EmptyRequest));
}

#[test]
fn plan_rejects_non_power_of_two_alignment() {
    for alignment in [0usize, 3, 5, 12, 24] {
        assert_eq!(
            ArenaPlan::new(0, 1024, alignment),
            Err(ArenaPlanError::AlignmentNotPowerOfTwo { alignment }),
            "alignment {alignment}"
        );
    }
}

#[test]
fn plan_rejects_a_trim_that_consumes_the_request() {
    // 13-byte trim against a 13-byte request leaves nothing.
    assert_eq!(
        ArenaPlan::new(0x1003, 13, 16),
        Err(ArenaPlanError::TrimConsumesArena {
            trim: 13,
            requested: 13
        })
    );
}

#[test]
fn plan_rejects_a_base_that_cannot_be_aligned_without_overflow() {
    // Trim of 15 at the very top of the address space has no room above it.
    let base = usize::MAX;
    assert_eq!(
        ArenaPlan::new(base, 1024, 16),
        Err(ArenaPlanError::UnrepresentableBase {
            base,
            alignment: 16
        })
    );
}

#[test]
fn trim_for_never_overflows_near_the_top_of_the_address_space() {
    assert_eq!(ArenaPlan::trim_for(usize::MAX, 16), 1);
    assert_eq!(ArenaPlan::trim_for(usize::MAX - 7, 16), 8);
    assert_eq!(ArenaPlan::trim_for(0, 16), 0);
    // Equivalent to `ALIGN_NEXT(u, 16) - u` wherever the macro is defined.
    for base in (usize::MAX - 64)..=usize::MAX - 16 {
        let macro_form = (base + 15) & !15;
        assert_eq!(
            ArenaPlan::trim_for(base, 16),
            macro_form - base,
            "base {base:#x}"
        );
    }
}

#[test]
fn plan_errors_describe_themselves() {
    let texts = [
        ArenaPlanError::EmptyRequest.to_string(),
        ArenaPlanError::AlignmentNotPowerOfTwo { alignment: 3 }.to_string(),
        ArenaPlanError::TrimConsumesArena {
            trim: 13,
            requested: 13,
        }
        .to_string(),
        ArenaPlanError::UnrepresentableBase {
            base: usize::MAX,
            alignment: 16,
        }
        .to_string(),
    ];
    for text in texts {
        assert!(!text.is_empty());
    }
    assert!(
        ArenaPlanError::AlignmentNotPowerOfTwo { alignment: 3 }
            .to_string()
            .contains('3')
    );
}

#[test]
fn extra_block_gate_matches_the_upstream_null_and_zero_check() {
    assert!(!accepts_extra_block(0, 1024));
    assert!(!accepts_extra_block(0x1000, 0));
    assert!(!accepts_extra_block(0, 0));
    assert!(accepts_extra_block(0x1000, 1024));
}

#[test]
fn cleanup_order_matches_famicom_emu_cleanup() {
    assert_eq!(
        PC_CLEANUP_SEQUENCE,
        [
            CleanupStep::FadeIn,
            CleanupStep::EmulatorCleanup,
            CleanupStep::ArenaCleanup,
            CleanupStep::ReleaseHostBlock,
            CleanupStep::SubGameEnd,
            CleanupStep::ClearActiveFlag,
        ]
    );
}

#[test]
fn message_flags_match_the_upstream_bit_values() {
    assert_eq!(MessageFlags::LOAD_FAILED.bits(), 1);
    assert_eq!(MessageFlags::CLEANUP_FAILED.bits(), 2);
}

#[test]
fn cleanup_order_is_fade_in_before_arena_teardown_before_release() {
    let position = |step: CleanupStep| {
        PC_CLEANUP_SEQUENCE
            .iter()
            .position(|&s| s == step)
            .expect("step present")
    };
    assert!(position(CleanupStep::FadeIn) < position(CleanupStep::EmulatorCleanup));
    assert!(position(CleanupStep::EmulatorCleanup) < position(CleanupStep::ArenaCleanup));
    assert!(position(CleanupStep::ArenaCleanup) < position(CleanupStep::ReleaseHostBlock));
    assert!(position(CleanupStep::ReleaseHostBlock) < position(CleanupStep::SubGameEnd));
    assert!(position(CleanupStep::SubGameEnd) < position(CleanupStep::ClearActiveFlag));
}

/// Minimal host arena proving the `FamicomAllocator` contract can be
/// implemented externally: every allocation lands inside the plan and is
/// filled with `0xFF`.
struct BumpArena {
    storage: Vec<u8>,
    plan: ArenaPlan,
    cursor: usize,
}

impl BumpArena {
    fn new(plan: ArenaPlan) -> Self {
        Self {
            storage: vec![0u8; plan.arena_bytes()],
            plan,
            cursor: 0,
        }
    }
}

impl FamicomAllocator for BumpArena {
    fn malloc_align(&mut self, size: NonZeroUsize, align: NonZeroUsize) -> Option<NonNull<u8>> {
        assert!(align.is_power_of_two());
        let base = self.storage.as_ptr() as usize;
        let aligned = (base + self.cursor).next_multiple_of(align.get());
        let offset = aligned - base;
        if offset + size.get() > self.plan.arena_bytes() {
            return None;
        }
        self.cursor = offset + size.get();
        let ptr = NonNull::new(self.storage[offset..].as_mut_ptr())?;
        // The upstream `my_zelda_malloc_align` fill.
        self.storage[offset..offset + size.get()].fill(ALLOCATION_FILL_BYTE);
        Some(ptr)
    }

    fn free(&mut self, _ptr: NonNull<u8>) {
        // Bump allocation: no reclaim, matching the source's lack of a
        // per-allocation reuse guarantee.
    }

    fn mem_block_size(&self, _ptr: NonNull<u8>) -> usize {
        self.plan.arena_bytes()
    }

    fn total_free_size(&self) -> usize {
        self.plan.arena_bytes() - self.cursor
    }
}

#[test]
fn allocator_contract_fills_every_allocation_with_ff() {
    let plan = pc_arena_plan(0);
    let mut arena = BumpArena::new(plan);

    let size = NonZeroUsize::new(4096).unwrap();
    let align = NonZeroUsize::new(16).unwrap();
    let ptr = arena.malloc_align(size, align).expect("fits");

    assert_eq!(ptr.as_ptr() as usize % 16, 0, "16-byte aligned");
    // SAFETY: a bump arena hands out disjoint live ranges for the test's
    // lifetime and `size` bytes were reserved above.
    let slice = unsafe { core::slice::from_raw_parts(ptr.as_ptr(), size.get()) };
    assert!(slice.iter().all(|&b| b == 0xFF));

    assert_eq!(arena.total_free_size(), plan.arena_bytes() - 4096);
    assert_eq!(arena.mem_block_size(ptr), plan.arena_bytes());
    arena.free(ptr);
}

#[test]
fn allocator_contract_reports_failure_past_the_arena() {
    let plan = pc_arena_plan(0);
    let mut arena = BumpArena::new(plan);
    let too_big = NonZeroUsize::new(plan.arena_bytes() + 1).unwrap();
    assert!(
        arena
            .malloc_align(too_big, NonZeroUsize::new(16).unwrap())
            .is_none()
    );
}
