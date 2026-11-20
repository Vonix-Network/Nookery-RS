//! Differential-style tests for the ported primitives.
//!
//! Every assertion is derived from the C source, not from this port's output:
//! the arithmetic cases are computed by hand from
//! `src/TwoHeadArena.c`/`src/gamealloc.c` and written as literals so a drift in
//! either direction fails.

use nookery_arena::gamealloc::{AllocError, GameAlloc, Handle, NODE_HEADER_SIZE};
use nookery_arena::storage::{BlockStore, SlabStore, StoreError};
use nookery_arena::two_head_arena::{
    Arena, ArenaError, MASK_16, MASK_NONE, TwoHeadArena, is_alignment_mask,
};

// ---------------------------------------------------------------- helpers

fn slab(capacity: u32) -> SlabStore {
    SlabStore::at(0x1000, capacity).expect("slab")
}

fn alloc_with_capacity(capacity: u32) -> GameAlloc<SlabStore> {
    GameAlloc::new(slab(capacity))
}

fn node_header_words(store: &SlabStore, handle: Handle) -> (u32, u32, u32, u32) {
    let node = handle.addr() - NODE_HEADER_SIZE;
    let bytes = store.bytes(node, NODE_HEADER_SIZE).expect("node header");
    let word = |i: usize| u32::from_ne_bytes(bytes[i..i + 4].try_into().unwrap());
    (word(0), word(4), word(8), word(12))
}

// ------------------------------------------------------- TwoHeadArena set-up

#[test]
fn ct_places_heads_at_buffer_ends() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);

    assert_eq!(tha.buf_p(), 0x2000);
    assert_eq!(tha.size(), 0x100);
    assert_eq!(tha.head_p(), 0x2000);
    assert_eq!(tha.tail_p(), 0x2100);
    assert_eq!(tha.end_p(), 0x2100);
}

#[test]
fn init_resets_heads_without_changing_buffer() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    // Move the tail by hand through a raw allocation, then reset.
    assert_eq!(tha.alloc16(0x20), 0x20E0);
    tha.init();

    assert_eq!(tha.buf_p(), 0x2000);
    assert_eq!(tha.size(), 0x100);
    assert_eq!(tha.head_p(), 0x2000);
    assert_eq!(tha.tail_p(), 0x2100);
    assert_eq!(tha.get_free_bytes(), 0x100);
}

#[test]
fn dt_zeroes_every_field() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    tha.alloc16(0x40);
    tha.dt();

    // Upstream `bzero(this, sizeof(TwoHeadArena))`: every field is zero, so the
    // arena reports no free bytes and cannot hand out memory.
    assert_eq!(tha.buf_p(), 0);
    assert_eq!(tha.size(), 0);
    assert_eq!(tha.head_p(), 0);
    assert_eq!(tha.tail_p(), 0);
    assert_eq!(tha.get_free_bytes(), 0);
    assert_eq!(tha.get_free_bytes16(), 0);
    assert!(!tha.is_crash());
    // Zeroing the buffer address does not make the arena safe to allocate from:
    // `0 - 0x10` wraps, exactly as the C arithmetic would.
    assert_eq!(tha.alloc16(0x10), 0xFFFF_FFF0);
    assert!(tha.is_crash());
}

#[test]
fn dt_matches_repurposed_new() {
    let mut fresh = TwoHeadArena::new();
    fresh.ct(0x10, 0);
    fresh.dt();

    assert_eq!(fresh, TwoHeadArena::new());
}

// ------------------------------------------------------ alignment + masks

#[test]
fn alignment_mask_predicate_matches_upstream_constants() {
    // The three masks the game actually passes: THA_alloc16/getFreeBytes16,
    // THA_getFreeBytes, and m_scene.c's `~0x1F`.
    assert!(is_alignment_mask(MASK_16));
    assert!(is_alignment_mask(MASK_NONE));
    assert!(is_alignment_mask(!0x1F));
    assert!(is_alignment_mask(!0x7));
    // Non-masks must be rejected: an alignment that is not a power of two
    // would move the tail by an arbitrary amount.
    assert!(!is_alignment_mask(0));
    assert!(!is_alignment_mask(0x0F));
    assert!(!is_alignment_mask(!0x1E));
}

#[test]
fn alloc16_aligns_down_then_subtracts_then_aligns() {
    // THA_alloc16: ((tail & ~15) - siz) & ~15.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);

    // tail = 0x2100 -> 0x2100 - 0x20 = 0x20E0.
    assert_eq!(tha.alloc16(0x20), 0x20E0);
    // tail = 0x20E0 -> 0x20E0 - 0x10 = 0x20D0.
    assert_eq!(tha.alloc16(0x10), 0x20D0);
    // tail = 0x20D0 - 0x11 = 0x20BF -> & ~15 = 0x20B0.
    assert_eq!(tha.alloc16(0x11), 0x20B0);
}

#[test]
fn alloc16_rounds_tail_down_before_subtracting() {
    // The mask is applied to the tail *before* the subtraction, so an
    // unaligned tail loses the low bits once instead of being preserved.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    tha.ct(0x2000, 0x108); // tail = 0x2108

    // (0x2108 & ~15) - 0x10 = 0x2100 - 0x10 = 0x20F0.
    assert_eq!(tha.alloc16(0x10), 0x20F0);
}

#[test]
fn alloc_align_honors_32_byte_mask_from_m_scene() {
    // m_scene.c: THA_allocAlign(&play->game.tha, mSc_ARENA_SIZE, ~0x1F).
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x1000);

    // (0x3000 & ~0x1F) - 0x40 = 0x2FC0.
    assert_eq!(tha.alloc_align(0x40, !0x1F), 0x2FC0);
    // 0x2FC0 - 0x41 = 0x2F7F -> & ~0x1F = 0x2F60.
    assert_eq!(tha.alloc_align(0x41, !0x1F), 0x2F60);
}

#[test]
fn alloc_align_with_mask_none_is_plain_subtraction() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);

    assert_eq!(tha.alloc_align(0x33, MASK_NONE), 0x20CD);
    assert_eq!(tha.alloc_align(1, MASK_NONE), 0x20CC);
}

#[test]
fn zero_size_allocations_do_not_move_the_tail() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    tha.alloc16(0x20);

    let before = tha.tail_p();
    assert_eq!(tha.alloc16(0), before);
    assert_eq!(tha.alloc_align(0, MASK_NONE), before);
    assert_eq!(tha.get_free_bytes(), 0xE0);
}

#[test]
fn negative_mask_sign_extends_like_the_source_cast() {
    // `(uintptr_t)(s64)mask`: a mask with the low bit set times sign-extends to
    // a full-width mask, which is why `-1` means "no alignment".
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    assert_eq!(tha.alloc_align(0x10, -2), 0x20F0);
    assert_eq!(tha.tail_p() & 1, 0);
}

// ------------------------------------------------------------- free bytes

#[test]
fn free_bytes_queries_match_the_source_formulas() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);

    // THA_getFreeBytes: ((head + (u32)~(-1)) & -1) -> 0x2000; 0x2100 - 0x2000.
    assert_eq!(tha.get_free_bytes(), 0x100);
    // THA_getFreeBytes16: ((head + (u32)~(15)) & ~15) -> 0x2000.
    assert_eq!(tha.get_free_bytes16(), 0x100);
    // Explicit align query: ((0x2000 + 0x1F) & ~0x1F) = 0x201F & ~0x1F = 0x2000.
    assert_eq!(tha.get_free_bytes_align(!0x1F), 0x100);

    tha.alloc16(0x40); // tail = 0x20C0
    assert_eq!(tha.get_free_bytes(), 0xC0);
    assert_eq!(tha.get_free_bytes16(), 0xC0);
}

#[test]
fn free_bytes16_aligns_the_head_up_for_non_16_aligned_buffers() {
    // A buffer whose base is 8-mod-16: getFreeBytes uses the raw head while
    // getFreeBytes16 adds 15 and rounds down, losing the low 8 bits.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2008, 0x100);

    assert_eq!(tha.get_free_bytes(), 0x100);
    // (0x2008 + 15) & ~15 = 0x2017 & ~15 = 0x2010; 0x2108 - 0x2010 = 0xF8.
    assert_eq!(tha.get_free_bytes16(), 0xF8);
}

#[test]
fn free_bytes_align_mask_is_not_symmetric_between_heads() {
    // Upstream aligns the head with `~mask` and the tail with `mask`; for
    // `!0x1F` the head term is `(u32)~mask == 0x1F`.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2001, 0x100);

    // head: ((0x2001 + 0x1F) & ~0x1F) = 0x2020 & ~0x1F = 0x2020.
    // tail: 0x2101 -> 0x2101 - 0x2020 = 0xE1.
    assert_eq!(tha.get_free_bytes_align(!0x1F), 0xE1);
}

// ------------------------------------------------------- crash detection

#[test]
fn over_allocation_reports_crash_and_wraps_the_tail() {
    // Exact upstream behavior: the tail wraps past the head, no panic, and
    // THA_isCrash notices afterwards.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);

    assert!(!tha.is_crash());
    let head = tha.alloc16(0x100); // tail = 0x2000, free = 0
    assert_eq!(head, 0x2000);
    assert_eq!(tha.get_free_bytes(), 0);
    assert!(!tha.is_crash());

    let collided = tha.alloc16(0x10); // tail = 0x1FF0, below the head
    assert_eq!(collided, 0x1FF0);
    assert_eq!(tha.get_free_bytes(), -0x10);
    assert!(tha.is_crash());
}

#[test]
fn collision_is_rejected_by_the_checked_path() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    tha.try_alloc16(0x100).expect("exact fit");

    assert_eq!(tha.try_alloc16(0x10), Err(ArenaError::Collision));
    // The rejected call must not have moved the tail.
    assert_eq!(tha.tail_p(), 0x2000);
    assert!(!tha.is_crash());
}

#[test]
fn try_alloc_rejects_underflow_instead_of_wrapping() {
    // 32-bit tail at 0x10 with a 0x20 request wraps to 0xFFFFFFF0 upstream.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x10, 0x10);

    assert_eq!(tha.alloc16(0x30), 0xFFFF_FFF0);
    assert_eq!(tha.get_free_bytes(), -0x20);

    tha.init();
    assert_eq!(tha.try_alloc16(0x30), Err(ArenaError::Underflow));
    assert_eq!(tha.tail_p(), 0x20);
}

#[test]
fn try_ct_rejects_an_end_address_past_the_address_space() {
    let mut tha = TwoHeadArena::new();
    assert_eq!(
        tha.try_ct(0xFFFF_FF00, 0x200),
        Err(ArenaError::AddressOverflow)
    );
    assert_eq!(tha.buf_p(), 0);

    // The raw port wraps exactly like a 32-bit target: the end address lands at
    // 0x100, below the head, and the free-byte query is positive again.  This is
    // the upstream behavior the checked path exists to prevent.
    tha.ct(0xFFFF_FF00, 0x200);
    assert_eq!(tha.end_p(), 0x100);
    assert_eq!(tha.tail_p(), 0x100);
}

#[test]
fn try_alloc_rejects_non_power_of_two_masks() {
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x100);
    assert_eq!(
        tha.try_alloc_align(0x10, 0x0F),
        Err(ArenaError::InvalidMask)
    );
    assert_eq!(tha.tail_p(), 0x2100);
}

// ------------------------------------------------------------ Arena adapter

#[test]
fn arena_hands_out_writable_disjoint_slices() {
    let mut arena = Arena::at(0x4000, 0x100).expect("arena");

    let first = arena.alloc16(0x10).expect("first");
    assert_eq!(first.len(), 0x10);
    first.fill(0xAB);
    let second = arena.alloc16(0x10).expect("second");
    second.fill(0xCD);

    assert_eq!(arena.free_bytes(), 0xE0);
    assert_eq!(arena.bytes(0x40F0, 0x10).unwrap(), &[0xAB; 0x10]);
    assert_eq!(arena.bytes(0x40E0, 0x10).unwrap(), &[0xCD; 0x10]);
    assert_eq!(arena.base(), 0x4000);
    assert_eq!(arena.capacity(), 0x100);
}

#[test]
fn arena_rejects_allocations_past_the_head_and_keeps_state() {
    let mut arena = Arena::at(0x4000, 0x20).expect("arena");
    arena.alloc16(0x20).expect("exact fit");

    assert_eq!(
        arena.alloc16(0x10).map(|slice| slice.len()),
        Err(ArenaError::Collision)
    );
    assert_eq!(arena.tha().tail_p(), 0x4000);
    assert!(!arena.is_crash());
}

#[test]
fn arena_rejects_out_of_range_byte_requests() {
    let arena = Arena::at(0x4000, 0x10).expect("arena");
    assert_eq!(arena.bytes(0x3FF0, 0x10), Err(ArenaError::OutOfRange));
    assert_eq!(arena.bytes(0x4008, 0x10), Err(ArenaError::OutOfRange));
    assert_eq!(arena.bytes(0x4000, 0x10).map(<[u8]>::len), Ok(0x10));
}

#[test]
fn arena_rejects_capacity_past_the_address_space() {
    assert_eq!(
        Arena::at(0xFFFF_FFF0, 0x100).map(|a| a.capacity()),
        Err(ArenaError::AddressOverflow)
    );
}

#[test]
fn arena_reset_keeps_contents_but_restores_heads() {
    let mut arena = Arena::at(0x4000, 0x40).expect("arena");
    arena.alloc16(0x20).expect("first").fill(7);
    assert_eq!(arena.free_bytes(), 0x20);

    arena.reset();
    assert_eq!(arena.free_bytes(), 0x40);
    assert_eq!(arena.tha().tail_p(), 0x4040);
    // THA_init moves heads only; prior contents survive.
    assert_eq!(arena.bytes(0x4020, 0x20).expect("kept")[0], 7);
}

// -------------------------------------------------------------- GameAlloc

#[test]
fn init_produces_a_self_linked_ring() {
    let alloc = alloc_with_capacity(0x1000);

    // gamealloc_init: tail = &head; head.next = tail; head.prev = tail.
    assert_eq!(alloc.tail_addr(), 0);
    assert_eq!(alloc.head_next_addr(), 0);
    assert_eq!(alloc.head_prev_addr(), 0);
    assert_eq!(alloc.len(), 0);
    assert!(alloc.is_empty());
}

#[test]
fn malloc_inserts_at_the_tail_and_links_the_sentinel() {
    let mut alloc = alloc_with_capacity(0x1000);

    let first = alloc.malloc(0x20).unwrap().unwrap();
    assert_eq!(first.addr(), 0x1000 + NODE_HEADER_SIZE);
    // tail -> first, head.next -> first, head.prev -> first.
    assert_eq!(alloc.tail_addr(), 0x1000);
    assert_eq!(alloc.head_next_addr(), 0x1000);
    assert_eq!(alloc.head_prev_addr(), 0x1000);

    let second = alloc.malloc(0x10).unwrap().unwrap();
    let second_node = second.addr() - NODE_HEADER_SIZE;
    assert_eq!(alloc.tail_addr(), second_node);
    assert_eq!(alloc.head_next_addr(), 0x1000);
    assert_eq!(alloc.head_prev_addr(), second_node);

    // Node headers: (next, prev, alloc_size, pad).
    assert_eq!(
        node_header_words(alloc.store(), first),
        (second_node, 0, 0x20, 0)
    );
    assert_eq!(
        node_header_words(alloc.store(), second).0,
        0,
        "tail node's next is the sentinel, encoded as 0"
    );
    assert_eq!(
        node_header_words(alloc.store(), second).1,
        0x1000,
        "tail node's prev is the first node"
    );
    assert_eq!(node_header_words(alloc.store(), second).2, 0x10);
    assert_eq!(alloc.handles().unwrap(), vec![first, second]);
}

#[test]
fn malloc_returns_none_when_the_store_is_exhausted() {
    let mut alloc = alloc_with_capacity(0x20);

    let handle = alloc.malloc(0x10).unwrap().unwrap();
    assert_eq!(alloc.len(), 1);

    // A second node needs 0x10 + 16 bytes; only 0x10 is left.
    assert_eq!(alloc.malloc(0x10).unwrap(), None);
    // The failed allocation must not disturb the ring.
    assert_eq!(alloc.len(), 1);
    assert_eq!(alloc.handles().unwrap(), vec![handle]);
    assert_eq!(alloc.tail_addr(), 0x1000);
}

#[test]
fn zero_size_allocation_still_allocates_a_node() {
    let mut alloc = alloc_with_capacity(0x100);

    let handle = alloc.malloc(0).unwrap().unwrap();
    assert_eq!(alloc.len(), 1);
    assert_eq!(alloc.alloc_size(handle).unwrap(), 0);
    assert_eq!(alloc.data(handle).unwrap(), &[] as &[u8]);
    assert_eq!(handle.addr() - NODE_HEADER_SIZE, 0x1000);
}

#[test]
fn malloc_rejects_a_size_that_overflows_the_header_add() {
    let mut alloc = alloc_with_capacity(0x100);

    // Upstream would compute (u32)(size + 16) and wrap to a small block.
    assert_eq!(
        alloc.malloc(u32::MAX - 8).unwrap_err(),
        AllocError::SizeOverflow
    );
    assert_eq!(
        alloc.malloc(u32::MAX).unwrap_err(),
        AllocError::SizeOverflow
    );
    assert!(alloc.is_empty());
    assert_eq!(alloc.store().free_bytes(), 0x100);
}

#[test]
fn free_of_none_is_a_no_op() {
    let mut alloc = alloc_with_capacity(0x100);
    let handle = alloc.malloc(0x10).unwrap().unwrap();

    alloc.free(None).unwrap();
    assert_eq!(alloc.len(), 1);
    assert_eq!(alloc.handles().unwrap(), vec![handle]);
    // Upstream returns before touching anything; the store is unchanged.
    assert_eq!(alloc.store().live_blocks().count(), 1);
}

#[test]
fn free_unlinks_the_middle_node_and_recomputes_tail() {
    let mut alloc = alloc_with_capacity(0x1000);
    let first = alloc.malloc(0x10).unwrap().unwrap();
    let middle = alloc.malloc(0x20).unwrap().unwrap();
    let last = alloc.malloc(0x30).unwrap().unwrap();

    alloc.free(Some(middle)).unwrap();

    assert_eq!(alloc.len(), 2);
    assert_eq!(alloc.handles().unwrap(), vec![first, last]);
    // gamealloc->tail = head.prev, which is the last node.
    assert_eq!(alloc.tail_addr(), last.addr() - NODE_HEADER_SIZE);
    // The first node now skips the middle one.
    let first_next = node_header_words(alloc.store(), first).0;
    assert_eq!(first_next, last.addr() - NODE_HEADER_SIZE);
    let last_prev = node_header_words(alloc.store(), last).1;
    assert_eq!(last_prev, first.addr() - NODE_HEADER_SIZE);
}

#[test]
fn free_of_the_tail_node_seats_the_sentinel() {
    let mut alloc = alloc_with_capacity(0x1000);
    let first = alloc.malloc(0x10).unwrap().unwrap();
    let last = alloc.malloc(0x20).unwrap().unwrap();

    alloc.free(Some(last)).unwrap();

    assert_eq!(alloc.tail_addr(), first.addr() - NODE_HEADER_SIZE);
    assert_eq!(alloc.head_prev_addr(), first.addr() - NODE_HEADER_SIZE);
    // first->next is the sentinel again.
    assert_eq!(node_header_words(alloc.store(), first).0, 0);
}

#[test]
fn freeing_the_only_node_returns_to_an_empty_ring() {
    let mut alloc = alloc_with_capacity(0x100);
    let handle = alloc.malloc(0x10).unwrap().unwrap();

    alloc.free(Some(handle)).unwrap();

    assert!(alloc.is_empty());
    assert_eq!(alloc.tail_addr(), 0);
    assert_eq!(alloc.head_next_addr(), 0);
    assert_eq!(alloc.head_prev_addr(), 0);
    assert_eq!(alloc.store().free_bytes(), 0x100);
}

#[test]
fn double_free_and_foreign_handles_are_rejected() {
    let mut alloc = alloc_with_capacity(0x100);
    let handle = alloc.malloc(0x10).unwrap().unwrap();
    alloc.free(Some(handle)).unwrap();

    assert_eq!(alloc.free(Some(handle)), Err(AllocError::NotOwned));
    assert_eq!(alloc.alloc_size(handle), Err(AllocError::NotOwned));
    assert_eq!(alloc.data(handle), Err(AllocError::NotOwned));

    // A handle from a different ring, and one that never existed.
    let other = Handle::from_addr(0x1008);
    assert_eq!(alloc.free(Some(other)), Err(AllocError::NotOwned));
    assert_eq!(
        alloc.free(Some(Handle::from_addr(1))),
        Err(AllocError::NotOwned)
    );
    assert!(alloc.is_empty());
}

#[test]
fn stale_handles_after_init_are_rejected_not_spliced() {
    let mut alloc = alloc_with_capacity(0x100);
    let handle = alloc.malloc(0x10).unwrap().unwrap();

    // gamealloc_init without cleanup: upstream leaks the node and a later free
    // of the stale pointer would splice it into the fresh ring.
    alloc.init();
    assert!(alloc.is_empty());
    assert_eq!(alloc.free(Some(handle)), Err(AllocError::NotOwned));
    assert_eq!(alloc.head_next_addr(), 0);
}

#[test]
fn init_on_a_dirty_ring_resets_every_header_link() {
    // `gamealloc_init` writes all three header fields, so calling it over a
    // populated ring must not leave `head.prev` or `tail` pointing at a node.
    let mut alloc = alloc_with_capacity(0x100);
    alloc.malloc(0x10).unwrap().unwrap();
    alloc.malloc(0x20).unwrap().unwrap();
    assert_ne!(alloc.head_prev_addr(), 0);
    assert_ne!(alloc.tail_addr(), 0);

    alloc.init();

    assert_eq!(alloc.tail_addr(), 0);
    assert_eq!(alloc.head_next_addr(), 0);
    assert_eq!(alloc.head_prev_addr(), 0);
    assert!(alloc.is_empty());
}

#[test]
fn cleanup_releases_every_node_and_resets_the_ring() {
    let mut alloc = alloc_with_capacity(0x1000);
    let handles: Vec<Handle> = (0..4)
        .map(|i| alloc.malloc(0x10 + i).unwrap().unwrap())
        .collect();
    assert_eq!(alloc.len(), 4);
    let free_before = alloc.store().free_bytes();

    alloc.cleanup().unwrap();

    assert!(alloc.is_empty());
    assert_eq!(alloc.tail_addr(), 0);
    assert_eq!(alloc.head_next_addr(), 0);
    assert_eq!(alloc.head_prev_addr(), 0);
    assert_eq!(alloc.store().free_bytes(), 0x1000);
    assert_eq!(alloc.store().live_blocks().count(), 0);
    assert!(free_before < 0x1000);
    for handle in handles {
        assert!(!alloc.owns(handle));
    }
}

#[test]
fn cleanup_on_an_empty_ring_is_idempotent() {
    let mut alloc = alloc_with_capacity(0x100);
    alloc.cleanup().unwrap();
    alloc.cleanup().unwrap();
    assert!(alloc.is_empty());
    assert_eq!(alloc.store().free_bytes(), 0x100);
}

#[test]
fn payload_bytes_are_independent_per_allocation() {
    let mut alloc = alloc_with_capacity(0x100);
    let first = alloc.malloc(4).unwrap().unwrap();
    let second = alloc.malloc(4).unwrap().unwrap();
    alloc.data_mut(first).unwrap().copy_from_slice(b"aaaa");
    alloc.data_mut(second).unwrap().copy_from_slice(b"bbbb");

    assert_eq!(alloc.data(first).unwrap(), b"aaaa");
    assert_eq!(alloc.data(second).unwrap(), b"bbbb");
    assert_eq!(alloc.alloc_size(first).unwrap(), 4);
    assert_eq!(alloc.alloc_size(second).unwrap(), 4);
}

#[test]
fn node_header_is_sixteen_bytes_at_the_documented_offsets() {
    // sizeof(GameAllocList) == 0x10 with next/prev/alloc_size/pad at 0/4/8/12.
    let mut alloc = alloc_with_capacity(0x100);
    let payload = 0x40u32;
    let handle = alloc.malloc(payload).unwrap().unwrap();
    let node = handle.addr() - NODE_HEADER_SIZE;

    // The block the store handed out is exactly header + payload.
    assert_eq!(alloc.store().block_len(node), Some(payload + 0x10));
    let (next, prev, size, _pad) = node_header_words(alloc.store(), handle);
    assert_eq!((next, prev, size), (0, 0, payload));
    // Payload starts 16 bytes after the node.
    assert_eq!(handle.addr(), node + 0x10);
}

#[test]
fn many_allocations_preserve_ring_order_and_unlink_order() {
    let mut alloc = alloc_with_capacity(0x4000);
    let handles: Vec<Handle> = (0..16)
        .map(|i| alloc.malloc(1 + i).unwrap().unwrap())
        .collect();
    assert_eq!(alloc.handles().unwrap(), handles);
    assert_eq!(alloc.tail_addr(), handles[15].addr() - NODE_HEADER_SIZE);

    // Free every other node, then check the ring still closes both ways.
    for handle in handles.iter().step_by(2) {
        alloc.free(Some(*handle)).unwrap();
    }
    let expected: Vec<Handle> = handles.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(alloc.handles().unwrap(), expected);
    assert_eq!(
        alloc.tail_addr(),
        expected.last().unwrap().addr() - NODE_HEADER_SIZE
    );

    alloc.cleanup().unwrap();
    assert_eq!(alloc.store().free_bytes(), 0x4000);
}

// ------------------------------------------------------- store-level guards

#[test]
fn slab_store_rejects_the_reserved_and_overflowing_bases() {
    assert_eq!(
        SlabStore::at(0, 0x10).unwrap_err(),
        StoreError::ReservedBase
    );
    assert_eq!(
        SlabStore::at(0xFFFF_FFF0, 0x20).unwrap_err(),
        StoreError::AddressOverflow
    );
}

#[test]
fn slab_store_rejects_release_of_unknown_blocks() {
    let mut store = slab(0x100);
    let addr = store.allocate(0x20).unwrap();
    store.release(addr, 0x20).unwrap();
    assert_eq!(store.release(addr, 0x20).unwrap_err(), StoreError::NotOwned);
    assert_eq!(
        store.release(addr + 0x40, 0x20).unwrap_err(),
        StoreError::NotOwned
    );
    // Wrong length for a live block is not the same block.
    let again = store.allocate(0x20).unwrap();
    assert_eq!(
        store.release(again, 0x10).unwrap_err(),
        StoreError::NotOwned
    );
}

#[test]
fn slab_store_coalesces_neighbors_so_large_requests_still_fit() {
    let mut store = slab(0x100);
    let a = store.allocate(0x40).unwrap();
    let b = store.allocate(0x40).unwrap();
    let c = store.allocate(0x40).unwrap();
    assert_eq!(store.allocate(0x80), None);

    store.release(b, 0x40).unwrap();
    store.release(a, 0x40).unwrap();
    // a and b are now one 0x80-byte region; c stays live.
    let merged = store.allocate(0x80).expect("coalesced region");
    assert_eq!(merged, a);
    store.release(c, 0x40).unwrap();
    store.release(merged, 0x80).unwrap();
    assert_eq!(store.free_bytes(), 0x100);
}

#[test]
fn slab_store_byte_access_is_bounds_checked() {
    let mut store = slab(0x40);
    let addr = store.allocate(0x10).unwrap();
    assert_eq!(store.bytes(addr, 0x10).map(<[u8]>::len), Ok(0x10));
    assert_eq!(store.bytes(addr, 0x20).unwrap_err(), StoreError::OutOfRange);
    assert_eq!(
        store.bytes(addr - 1, 0x10).unwrap_err(),
        StoreError::OutOfRange
    );
    assert_eq!(store.bytes_mut(addr, 0x10).map(|b| b.len()), Ok(0x10));
}

#[test]
fn try_alloc_align_masks_the_tail_before_subtracting() {
    // The source masks the tail *first*: ((tail & mask) - siz) & mask.  For a
    // tail that is unaligned and a size smaller than the misalignment, that
    // differs from the double-mask-only form ((tail - siz) & mask):
    //   tail = 0x2108, siz = 1, mask = ~15
    //   source: (0x2100 - 1) & ~15 = 0x20FF & ~15 = 0x20F0
    //   other : (0x2108 - 1) & ~15 = 0x2107 & ~15 = 0x2100
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x108); // tail = 0x2108
    assert_eq!(tha.tail_p(), 0x2108);

    assert_eq!(tha.try_alloc_align(1, MASK_16).unwrap(), 0x20F0);
    assert_eq!(tha.tail_p(), 0x20F0);

    // Same shape through the raw path, so both agree.
    let mut raw = TwoHeadArena::new();
    raw.ct(0x2000, 0x108);
    assert_eq!(raw.alloc16(1), 0x20F0);
}

#[test]
fn try_alloc_align_rejects_the_wrap_before_masking() {
    // The pre-mask must not let an underflowing request slip through: the
    // checked path rejects it instead of masking a wrapped address.
    let mut tha = TwoHeadArena::new();
    tha.ct(0x2000, 0x108);
    tha.alloc16(0x100); // tail = 0x2000
    assert_eq!(tha.try_alloc16(0x8), Err(ArenaError::Collision));
    assert_eq!(tha.tail_p(), 0x2000);
}

#[test]
fn slab_store_never_loses_capacity_to_sub_alignment_slack() {
    // A request just under the region length must not drop the remainder: that
    // would lose bytes permanently and make a later release give back less
    // than the block took.
    let mut store = slab(0x100);
    let addr = store.allocate(0xF8).unwrap();
    assert_eq!(store.free_bytes(), 0x8, "0x08-byte remainder stays free");
    store.release(addr, 0xF8).unwrap();
    assert_eq!(store.free_bytes(), 0x100, "release restores the whole slab");
    assert_eq!(store.allocate(0x100), Some(0x1000));

    // Odd sizes leave odd remainders; capacity must still be conserved.
    let mut store = slab(0x100);
    let a = store.allocate(0x11).unwrap();
    let b = store.allocate(0x1).unwrap();
    assert_eq!(store.free_bytes(), 0x100 - 0x12);
    store.release(a, 0x11).unwrap();
    store.release(b, 0x1).unwrap();
    assert_eq!(store.free_bytes(), 0x100);
}

#[test]
fn slab_store_capacity_equals_live_plus_free_after_every_call() {
    let mut store = slab(0x200);
    let mut held = Vec::new();
    for (i, len) in [1u32, 3, 0x10, 0x11, 0x7F, 0x40].into_iter().enumerate() {
        let addr = store.allocate(len).expect("fits");
        held.push((addr, len));
        let live: u32 = held.iter().map(|&(_, l)| l).sum();
        assert_eq!(
            store.free_bytes() + live,
            0x200,
            "leak after allocating {len} (step {i})"
        );
    }
    while let Some((addr, len)) = held.pop() {
        store.release(addr, len).unwrap();
        let live: u32 = held.iter().map(|&(_, l)| l).sum();
        assert_eq!(store.free_bytes() + live, 0x200);
    }
    assert_eq!(store.free_bytes(), 0x200);
}

#[test]
fn cleanup_restores_a_full_slab_after_mixed_sizes() {
    // Regression for the slack loss: the second request leaves a 15-byte
    // remainder, which the old split rule discarded, so `cleanup` could not
    // return the store to its full capacity.
    //   0x200 slab: alloc 0x1E0 -> region 0x20; alloc 0x11 -> 0x0F left over.
    let mut alloc = alloc_with_capacity(0x200);
    for size in [0x1D0u32, 0x1] {
        alloc.malloc(size).unwrap().expect("fits");
    }
    alloc.cleanup().unwrap();
    assert_eq!(alloc.store().free_bytes(), 0x200);
    assert_eq!(alloc.store().live_blocks().count(), 0);
}

#[test]
fn malloc_leaves_the_pad_word_untouched() {
    // Upstream `gamealloc_malloc` writes `alloc_size` and `prev` only; `pad` at
    // offset 12 keeps whatever `malloc` returned.  Poke a recognisable value
    // into the region, free it, and re-allocate the same block: the pad word
    // must survive, which fails if the port starts zeroing it.
    let mut alloc = alloc_with_capacity(0x100);
    let first = alloc.malloc(0x10).unwrap().unwrap();
    let node = first.addr() - NODE_HEADER_SIZE;
    alloc
        .store_mut()
        .bytes_mut(node + 12, 4)
        .expect("pad bytes")
        .copy_from_slice(&0xDEAD_BEEFu32.to_ne_bytes());

    alloc.free(Some(first)).unwrap();
    let again = alloc.malloc(0x10).unwrap().unwrap();
    assert_eq!(again.addr(), first.addr(), "slab reuses the same block");

    let header = alloc.store().bytes(node, NODE_HEADER_SIZE).expect("node");
    let pad = u32::from_ne_bytes(header[12..16].try_into().unwrap());
    assert_eq!(pad, 0xDEAD_BEEF, "pad must not be initialized by malloc");
}

#[test]
fn slab_store_rejects_zero_length_requests() {
    // `gamealloc_malloc` never asks for 0 bytes (the header add makes the
    // minimum 16), and both stores refuse a 0-byte block rather than handing
    // out an address that cannot be released with a matching length.
    let mut store = slab(0x10);
    assert_eq!(store.allocate(0), None);
    assert_eq!(store.free_bytes(), 0x10);
}

#[test]
fn slab_store_never_returns_the_sentinel_address_for_a_block() {
    let mut store = slab(0x100);
    for _ in 0..4 {
        assert_ne!(store.allocate(0x10).unwrap(), 0);
    }
}
