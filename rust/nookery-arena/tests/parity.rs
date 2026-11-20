//! Transcript parity against the unmodified upstream sources.
//!
//! `parity/driver.c` compiles the *real* `src/TwoHeadArena.c` and
//! `src/gamealloc.c` from pin `a81c1aaf` at 32-bit width and prints one line per
//! observable step into `parity/expected.txt`.  This test replays the same steps
//! through the crate and requires byte-identical output, so the port is checked
//! against the C implementation rather than against hand-copied literals.
//!
//! Regenerate with `parity/run.sh`.  See `parity/README.md` for the exact
//! command and for why the harness is freestanding.

use std::fmt::Write as _;

use nookery_arena::gamealloc::{GameAlloc, Handle, NODE_HEADER_SIZE};
use nookery_arena::storage::{Addr, BlockStore, BumpStore, SlabStore};
use nookery_arena::two_head_arena::TwoHeadArena;

/// The harness places its heap at a fixed address and precedes it with
/// `0x10` bytes of padding so the first node is at offset `0x10`, like the C
/// harness's first `malloc`.
const HEAP_BASE: Addr = 0x0804_9000;
const HEAP_LEN: Addr = 0x4000;

/// Marker for `&gamealloc->head`, which the C harness prints as `HEAD`.
const HEAD: &str = "HEAD";
const NULL: &str = "NULL";

struct Recorder {
    out: String,
}

impl Recorder {
    fn new() -> Self {
        Self { out: String::new() }
    }

    fn line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// `pr()` from the harness: heap-relative offset, or `NULL`/`HEAD`.
    ///
    /// A handle of `0` is upstream's `NULL`.  Calls that render a *link* must
    /// use [`Self::link`], because the port encodes `&gamealloc->head` as `0`.
    fn ptr(&self, addr: Addr) -> String {
        if addr == 0 {
            return NULL.to_owned();
        }
        let heap_end = HEAP_BASE + HEAP_LEN;
        if addr < HEAP_BASE || addr >= heap_end {
            return HEAD.to_owned();
        }
        format!("{:08x}", addr - HEAP_BASE)
    }

    /// `pr()` for a list link: the sentinel renders as `HEAD`.
    fn link(&self, addr: Addr) -> String {
        if addr == 0 {
            HEAD.to_owned()
        } else {
            self.ptr(addr)
        }
    }
}

fn tha_state(rec: &mut Recorder, tag: &str, tha: &TwoHeadArena) {
    rec.line(&format!(
        "{tag} tail={:08x} free={} free16={} crash={} head={:08x}",
        tha.tail_p(),
        tha.get_free_bytes(),
        tha.get_free_bytes16(),
        i32::from(tha.is_crash()),
        tha.head_p(),
    ));
}

fn gamealloc_state(rec: &mut Recorder, tag: &str, alloc: &GameAlloc<BumpStore>) {
    let next = rec.link(alloc.head_next_addr());
    let prev = rec.link(alloc.head_prev_addr());
    let tail = rec.link(alloc.tail_addr());
    rec.line(&format!(
        "{tag} head.next={next} head.prev={prev} tail={tail}"
    ));
}

/// Build the transcript the C driver produces.
fn transcript() -> String {
    let mut rec = Recorder::new();

    // ---------------- TwoHeadArena: arithmetic only, synthetic addresses.
    let mut t = TwoHeadArena::new();
    t.ct(0x2000, 0x100);
    tha_state(&mut rec, "ct_2000_100", &t);

    rec.line(&format!("a16_20={:08x}", t.alloc16(0x20)));
    tha_state(&mut rec, "after_a16_20", &t);
    rec.line(&format!("a16_11={:08x}", t.alloc16(0x11)));
    rec.line(&format!("a16_0={:08x}", t.alloc16(0)));
    tha_state(&mut rec, "after_zero", &t);

    t.init();
    tha_state(&mut rec, "reinit", &t);
    rec.line(&format!("aa_40_mask~1f={:08x}", t.alloc_align(0x40, !0x1F)));
    rec.line(&format!("aa_41_mask~1f={:08x}", t.alloc_align(0x41, !0x1F)));
    rec.line(&format!("aa_10_mask-1={:08x}", t.alloc_align(0x10, -1)));
    rec.line(&format!("aa_10_mask-2={:08x}", t.alloc_align(0x10, -2)));
    tha_state(&mut rec, "after_aa", &t);

    t.ct(0x2008, 0x100);
    tha_state(&mut rec, "ct_2008_100", &t);
    t.ct(0x2001, 0x100);
    rec.line(&format!("gfb_~1f={}", t.get_free_bytes_align(!0x1F)));
    rec.line(&format!("gfb_-16={}", t.get_free_bytes_align(-16)));
    rec.line(&format!("gfb_-1={}", t.get_free_bytes_align(-1)));

    t.ct(0x2000, 0x100);
    rec.line(&format!("exact={:08x}", t.alloc16(0x100)));
    tha_state(&mut rec, "at_exact", &t);
    rec.line(&format!("over={:08x}", t.alloc16(0x10)));
    tha_state(&mut rec, "after_over", &t);

    t.ct(0x10, 0x10);
    rec.line(&format!("wrap={:08x}", t.alloc16(0x30)));
    tha_state(&mut rec, "after_wrap", &t);

    t.dt();
    tha_state(&mut rec, "dt", &t);

    t.ct(0xFFFF_FF00, 0x200);
    tha_state(&mut rec, "ct_wrap_end", &t);

    // Fixed-seed interleaving, identical LCG and call order to driver.c.
    t.ct(0x3000, 0x400);
    let mut seed: u32 = 0x0123_4567;
    for i in 0..24 {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let r = (seed >> 16) & 0x7FFF;
        let siz = if r & 0x7 == 0 { 0 } else { (r & 0x7F) + 1 };
        let mask: i32 = match r & 3 {
            0 => -1,
            1 => -16,
            2 => !0x1F,
            _ => -2,
        };
        let ptr = t.alloc_align(siz, mask);
        rec.line(&format!(
            "sim={i} siz={siz} mask={mask} ptr={ptr:08x} free={} free16={} crash={}",
            t.get_free_bytes(),
            t.get_free_bytes16(),
            i32::from(t.is_crash()),
        ));
    }
    tha_state(&mut rec, "sim_end", &t);

    // ---------------- GameAlloc against the C harness's bump heap.
    let mut g = GameAlloc::new(BumpStore::at(HEAP_BASE, HEAP_LEN).expect("heap"));
    gamealloc_state(&mut rec, "ga_init", &g);

    // The C harness's first `malloc` lands at heap offset 0x10 because the
    // driver's cursor starts at 0 and the arena's own allocations precede it;
    // here the bump cursor starts at 0, so pad by hand to keep offsets equal.
    g.store_mut().allocate(0x10).expect("padding");

    let p1 = g.malloc(0x20).unwrap().expect("m1");
    rec.line(&format!("ga_m1={}", rec.ptr(p1.addr())));
    gamealloc_state(&mut rec, "ga_after_m1", &g);
    let p2 = g.malloc(0x10).unwrap().expect("m2");
    rec.line(&format!("ga_m2={}", rec.ptr(p2.addr())));
    let p3 = g.malloc(0).unwrap().expect("m3");
    rec.line(&format!("ga_m3={}", rec.ptr(p3.addr())));
    gamealloc_state(&mut rec, "ga_after_m3", &g);

    for (i, handle) in [p1, p2, p3].into_iter().enumerate() {
        let node = handle.addr() - NODE_HEADER_SIZE;
        let header = g.store().bytes(node, NODE_HEADER_SIZE).expect("node");
        let word = |off: usize| u32::from_ne_bytes(header[off..off + 4].try_into().unwrap());
        let next = rec.link(word(0));
        let prev = rec.link(word(4));
        let size = word(8);
        rec.line(&format!("node={i} next={next} prev={prev} size={size}"));
    }

    g.free(Some(p2)).unwrap();
    gamealloc_state(&mut rec, "ga_after_free_m2", &g);
    let node0 = p1.addr() - NODE_HEADER_SIZE;
    let node2 = p3.addr() - NODE_HEADER_SIZE;
    let next0 = u32::from_ne_bytes(
        g.store().bytes(node0, NODE_HEADER_SIZE).expect("node0")[0..4]
            .try_into()
            .unwrap(),
    );
    let prev2 = u32::from_ne_bytes(
        g.store().bytes(node2, NODE_HEADER_SIZE).expect("node2")[4..8]
            .try_into()
            .unwrap(),
    );
    let next0 = rec.link(next0);
    let prev2 = rec.link(prev2);
    rec.line(&format!("node0_next_after_free={next0}"));
    rec.line(&format!("node2_prev_after_free={prev2}"));

    g.free(None).unwrap();
    gamealloc_state(&mut rec, "ga_after_free_null", &g);

    let p4 = g.malloc(0x40).unwrap().expect("m4");
    rec.line(&format!("ga_m4={}", rec.ptr(p4.addr())));
    gamealloc_state(&mut rec, "ga_after_m4", &g);

    g.cleanup().unwrap();
    gamealloc_state(&mut rec, "ga_cleanup", &g);

    let p5 = g.malloc(0x8).unwrap().expect("m5");
    rec.line(&format!("ga_after_cleanup_m5={}", rec.ptr(p5.addr())));
    gamealloc_state(&mut rec, "ga_after_m5", &g);

    // Exhaust the store: the C harness sets its cursor to the end of the heap.
    let remaining = HEAP_LEN - g.store().cursor();
    g.store_mut().allocate(remaining).expect("drain");
    let exhausted = match g.malloc(0x10).unwrap() {
        Some(handle) => rec.ptr(handle.addr()),
        None => rec.ptr(0),
    };
    rec.line(&format!("ga_exhausted={exhausted}"));
    gamealloc_state(&mut rec, "ga_after_exhausted", &g);

    rec.out
}

#[test]
fn transcript_matches_the_unmodified_c_sources() {
    let expected = include_str!("../parity/expected.txt");
    let actual = transcript();

    if actual != expected {
        // Report the first divergence rather than a wall of diff text.
        let mut report = String::new();
        for (i, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
            if want != got {
                let _ = writeln!(report, "line {}:\n  C   : {want}\n  port: {got}", i + 1);
                break;
            }
        }
        let want_lines = expected.lines().count();
        let got_lines = actual.lines().count();
        if report.is_empty() {
            let _ = writeln!(
                report,
                "line count differs: C {want_lines}, port {got_lines}"
            );
        }
        panic!("transcript diverged from upstream C:\n{report}");
    }
}

#[test]
fn bump_store_matches_slab_store_for_the_same_call_sequence() {
    // The list logic must depend only on the malloc/free contract, not on how
    // the store reuses space.  Both stores see the same sequence.
    let mut bump = GameAlloc::new(BumpStore::at(HEAP_BASE, HEAP_LEN).unwrap());
    let mut slab = GameAlloc::new(SlabStore::at(HEAP_BASE, HEAP_LEN).unwrap());

    for size in [0x20u32, 0, 0x10, 0x100, 0x8] {
        let a = bump.malloc(size).unwrap().expect("bump");
        let b = slab.malloc(size).unwrap().expect("slab");
        assert_eq!(a.addr(), b.addr(), "same offsets on a fresh store");
        assert_eq!(bump.alloc_size(a).unwrap(), slab.alloc_size(b).unwrap());
        assert_eq!(bump.len(), slab.len());
        assert_eq!(bump.head_next_addr(), slab.head_next_addr());
        assert_eq!(bump.head_prev_addr(), slab.head_prev_addr());
        assert_eq!(bump.tail_addr(), slab.tail_addr());
    }

    let bump_handles = bump.handles().unwrap();
    let slab_handles = slab.handles().unwrap();
    assert_eq!(bump_handles, slab_handles);

    // Free every other handle in both.
    for (i, handle) in bump_handles.iter().enumerate().step_by(2) {
        bump.free(Some(*handle)).unwrap();
        slab.free(Some(slab_handles[i])).unwrap();
    }
    assert_eq!(bump.handles().unwrap().len(), slab.handles().unwrap().len());
    assert_eq!(bump.tail_addr(), slab.tail_addr());

    let bump_consumed = bump.store().cursor();
    bump.cleanup().unwrap();
    slab.cleanup().unwrap();
    assert!(bump.is_empty() && slab.is_empty());
    assert_eq!(bump.head_next_addr(), slab.head_next_addr());
    // The stores differ where they are allowed to: slab coalesces, bump does not.
    assert_eq!(slab.store().free_bytes(), HEAP_LEN);
    // Bump never reuses, so consumed bytes stay consumed.
    assert_eq!(bump.store().cursor(), bump_consumed);
}

#[test]
fn handle_round_trips_through_its_address() {
    let mut alloc = GameAlloc::new(SlabStore::at(0x1000, 0x100).unwrap());
    let handle = alloc.malloc(0x8).unwrap().unwrap();
    assert_eq!(Handle::from_addr(handle.addr()), handle);
    assert!(alloc.owns(handle));
    assert_ne!(handle.addr(), 0);
}
