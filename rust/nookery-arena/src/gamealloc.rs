//! Rust port of `include/gamealloc.h` / `src/gamealloc.c`.
//!
//! Upstream is a circular doubly-linked list threaded through the embedded
//! `head` sentinel, with one `GameAllocList` header in front of every payload:
//!
//! ```c
//! /* 0x00 */ GameAllocList* next;
//! /* 0x04 */ GameAllocList* prev;
//! /* 0x08 */ size_t alloc_size;
//! /* 0x0C */ u32 pad;
//! ```
//!
//! `sizeof(struct gameAllocList_s) == 0x10` and `sizeof(struct gameAlloc_s) ==
//! 0x14` per the header's own annotations, which is the 32-bit target layout
//! that `include/game.h` depends on (`tha` at `0x78`, `gamealloc` at `0x88`).
//! The port keeps that layout: [`NODE_HEADER_SIZE`] is 16 and the fields sit at
//! offsets 0/4/8/12.
//!
//! # Design
//!
//! The list is generic over a [`BlockStore`], which stands in for the runtime's
//! `malloc`/`free`.  Node headers live inside the blocks the store hands out, so
//! the header bytes are read and written through the store, not through raw
//! pointers.  The `&gamealloc->head` sentinel is modeled as a private `Link::Head`
//! (encoded as address `0`, which no [`BlockStore`] may hand out), so the
//! sentinel never needs a real address.
//!
//! # Ported functions
//!
//! [`GameAlloc::init`] (`gamealloc_init`), [`GameAlloc::malloc`]
//! (`gamealloc_malloc`), [`GameAlloc::free`] (`gamealloc_free`),
//! [`GameAlloc::cleanup`] (`gamealloc_cleanup`).
//!
//! # Preserved quirks
//!
//! * `alloc->next` is never written directly by `malloc`; it is set by
//!   `gamealloc->tail->next = &gamealloc->head` after the tail moves.  The port
//!   performs the same stores in the same order.
//! * `GameAllocList::pad` is never initialized by `malloc` (which does not
//!   zero).  The port likewise leaves those four bytes as the store left them.
//! * `gamealloc_free` recomputes `tail` from `head.prev` *after* unlinking, and
//!   recomputes it even when the freed node was not the tail.
//! * A `NULL` free is a no-op ([`GameAlloc::free`] takes `Option`).
//! * A failed allocation returns `NULL` and leaves the list untouched
//!   (`Ok(None)` here).
//! * `cleanup` frees every node and re-seats it as an empty ring with
//!   `tail == head`.

use std::collections::BTreeSet;

use crate::storage::{Addr, BlockStore, SlabStore, StoreError};

/// `sizeof(GameAllocList)`: next, prev, alloc_size, pad.
pub const NODE_HEADER_SIZE: u32 = 16;

/// Offset of `GameAllocList::next`.
const OFF_NEXT: usize = 0;
/// Offset of `GameAllocList::prev`.
const OFF_PREV: usize = 4;
/// Offset of `GameAllocList::alloc_size`.
const OFF_ALLOC_SIZE: usize = 8;

/// A payload address returned by [`GameAlloc::malloc`], i.e. `(alloc + 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(Addr);

impl Handle {
    /// Wrap a raw payload address.
    ///
    /// Needed to represent a pointer that came from C, or a stale/foreign value
    /// in a differential test.  Wrapping does not grant ownership:
    /// [`GameAlloc::free`] still rejects any node the ring does not currently
    /// hold.
    pub const fn from_addr(addr: Addr) -> Self {
        Self(addr)
    }

    /// The payload address.  The node header starts [`NODE_HEADER_SIZE`] bytes
    /// before it.
    pub const fn addr(self) -> Addr {
        self.0
    }
}

/// One link of the ring: the sentinel or a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    /// `&gamealloc->head`, encoded as address `0`.
    Head,
    /// A node header address.
    Node(Addr),
}

/// Encoding of [`Link::Head`].  [`BlockStore`] implementations must never hand
/// out address `0`.
const LINK_HEAD: Addr = 0;

impl Link {
    const fn encode(self) -> Addr {
        match self {
            Self::Head => LINK_HEAD,
            Self::Node(addr) => addr,
        }
    }

    const fn decode(addr: Addr) -> Self {
        if addr == LINK_HEAD {
            Self::Head
        } else {
            Self::Node(addr)
        }
    }
}

/// Why a `GameAlloc` operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// `size + sizeof(GameAllocList)` does not fit the 32-bit `size_t`.
    ///
    /// Upstream wraps here and then hands out a block smaller than requested.
    SizeOverflow,
    /// The handle was not allocated by this `GameAlloc`, or the node header no
    /// longer agrees with the store's block.
    NotOwned,
    /// The ring did not close after the recorded number of nodes.
    Corrupt,
    /// The backing store rejected an operation it had previously accepted.
    Store(StoreError),
}

impl From<StoreError> for AllocError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl std::fmt::Display for AllocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SizeOverflow => f.write_str("size plus node header does not fit a 32-bit size"),
            Self::NotOwned => f.write_str("handle is not owned by this GameAlloc"),
            Self::Corrupt => f.write_str("allocation ring did not close"),
            Self::Store(error) => write!(f, "backing store error: {error}"),
        }
    }
}

impl std::error::Error for AllocError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            _ => None,
        }
    }
}

/// The `GameAlloc` ring plus its backing store.
///
/// The ring fields mirror `struct gameAlloc_s`: the embedded list header
/// (`head.next`, `head.prev`) and `tail`.  `GameAlloc<S>` is *not* layout
/// compatible with `struct gameAlloc_s` — it owns its store and an ownership
/// set — while the node headers it writes are byte-identical to
/// `GameAllocList` at the 32-bit target layout.
///
/// `owned` is port-side bookkeeping with no upstream counterpart.  It exists so
/// `free` can reject a handle this ring does not currently hold (a foreign
/// pointer, a double free, or a handle from before `init` reset the ring), which
/// upstream would follow into undefined behavior.  It does not affect the order
/// or the bytes of any store operation.
///
/// # Pointer-stability boundary
///
/// Upstream hands out real pointers and its callers (`src/game.c`,
/// `src/game/m_field_make.c`) hold them across allocations, so a live node must
/// never move.  This type preserves the *observable* part of that requirement
/// and nothing more:
///
/// * A `Handle` is an address into a buffer that is allocated once and never
///   reallocated or resized, so its offset stays fixed for as long as the block
///   is live, and two live blocks never share an address.
/// * It is **not** a `*mut u8`, is not `Send`-able to C, and must not be
///   dereferenced.  Reaching the bytes goes through [`Self::data`] /
///   [`Self::data_mut`], which re-check that the address still names a live
///   block of `self.store`.
/// * Dropping the `GameAlloc` frees the whole backing `Vec<u8>`; there is no
///   C `free` to call and no `gamealloc_cleanup` required for memory safety
///   (it is still needed to reproduce the ring state).
///
/// Anything that needs the shipped C layout or a C-visible address is out of
/// scope for this crate; see the crate-level semantic boundaries.
#[derive(Debug)]
pub struct GameAlloc<S: BlockStore = SlabStore> {
    head_next: Link,
    head_prev: Link,
    tail: Link,
    owned: BTreeSet<Addr>,
    store: S,
}

impl<S: BlockStore> GameAlloc<S> {
    /// An initialized ring over `store` (the state `gamealloc_init` produces).
    pub fn new(store: S) -> Self {
        let mut this = Self {
            head_next: Link::Head,
            head_prev: Link::Head,
            tail: Link::Head,
            owned: BTreeSet::new(),
            store,
        };
        this.init();
        this
    }

    /// `gamealloc_init`: `tail = &head; head.next = tail; head.prev = tail;`
    ///
    /// Only the header is reset.  Any node still tracked by the store is leaked,
    /// exactly as upstream leaks it if `init` is called without `cleanup`
    /// (`game_ct` pairs `gamealloc_init` with `gamealloc_cleanup` in `game_dt`).
    ///
    /// Leaked handles stop being owned, so a later [`Self::free`] of one reports
    /// [`AllocError::NotOwned`] instead of splicing a stale link into the new
    /// ring.
    pub fn init(&mut self) {
        self.tail = Link::Head;
        self.head_next = self.tail;
        self.head_prev = self.tail;
        self.owned.clear();
    }

    /// `gamealloc_malloc`: prepend-to-tail insertion of a `size + 16` byte node.
    ///
    /// Returns `Ok(None)` where upstream returns `NULL` (store exhausted); the
    /// list is untouched in that case.  The payload address is the node address
    /// plus [`NODE_HEADER_SIZE`].
    pub fn malloc(&mut self, size: u32) -> Result<Option<Handle>, AllocError> {
        let total = size
            .checked_add(NODE_HEADER_SIZE)
            .ok_or(AllocError::SizeOverflow)?;
        let node = match self.store.allocate(total) {
            Some(node) => node,
            None => return Ok(None),
        };

        // alloc->alloc_size = size; alloc->prev = gamealloc->tail;
        // (the pad field is intentionally left as the store returned it)
        let prev = self.tail;
        {
            let header = self.node_mut(node)?;
            put_u32(header, OFF_ALLOC_SIZE, size);
            put_u32(header, OFF_PREV, prev.encode());
        }
        // gamealloc->tail->next = alloc;
        self.set_next(self.tail, Link::Node(node))?;
        // gamealloc->tail = alloc; gamealloc->tail->next = &gamealloc->head;
        self.tail = Link::Node(node);
        self.set_next(self.tail, Link::Head)?;
        // gamealloc->head.prev = gamealloc->tail;
        self.head_prev = self.tail;
        self.owned.insert(node);

        Ok(Some(Handle(node.wrapping_add(NODE_HEADER_SIZE))))
    }

    /// `gamealloc_free`: unlink the node header in front of `handle`, recompute
    /// `tail` from `head.prev`, and release the block.
    ///
    /// `None` reproduces the `if (ptr != NULL)` guard.  A handle this ring does
    /// not own (never allocated, already freed, or from another `GameAlloc`) is
    /// reported as [`AllocError::NotOwned`] instead of writing through a stale
    /// address, which upstream would do.
    pub fn free(&mut self, handle: Option<Handle>) -> Result<(), AllocError> {
        let Some(handle) = handle else {
            return Ok(());
        };
        let node = handle
            .0
            .checked_sub(NODE_HEADER_SIZE)
            .ok_or(AllocError::NotOwned)?;
        if !self.owned.contains(&node) {
            return Err(AllocError::NotOwned);
        }
        let total = self.store.block_len(node).ok_or(AllocError::NotOwned)?;
        let (prev, next, alloc_size) = {
            let header = self.node(node)?;
            (
                Link::decode(get_u32(header, OFF_PREV)),
                Link::decode(get_u32(header, OFF_NEXT)),
                get_u32(header, OFF_ALLOC_SIZE),
            )
        };
        // The store's block length is the request plus the header; a mismatch
        // means the bytes at `node` are not the header we wrote.
        if alloc_size.checked_add(NODE_HEADER_SIZE) != Some(total) {
            return Err(AllocError::NotOwned);
        }

        // alloc->prev->next = alloc->next; alloc->next->prev = alloc->prev;
        self.set_next(prev, next)?;
        self.set_prev(next, prev)?;
        // gamealloc->tail = gamealloc->head.prev;
        self.tail = self.head_prev;

        self.store.release(node, total)?;
        self.owned.remove(&node);
        Ok(())
    }

    /// `gamealloc_cleanup`: release every node in ring order and re-seat the
    /// header as an empty ring.
    ///
    /// Upstream walks until it wraps back to `&head`; a corrupt ring therefore
    /// spins forever.  The port bounds the walk by the recorded node count and
    /// reports [`AllocError::Corrupt`].
    pub fn cleanup(&mut self) -> Result<(), AllocError> {
        let mut now = self.head_next;
        let mut steps = 0usize;
        while now != Link::Head {
            if steps > self.owned.len() {
                return Err(AllocError::Corrupt);
            }
            steps += 1;
            let temp = match now {
                Link::Node(addr) => addr,
                Link::Head => unreachable!("loop condition excludes the sentinel"),
            };
            // now_p = now_p->next; free(temp_p);
            let next = Link::decode(get_u32(self.node(temp)?, OFF_NEXT));
            let total = self.store.block_len(temp).ok_or(AllocError::NotOwned)?;
            self.store.release(temp, total)?;
            now = next;
        }

        // gamealloc->tail = end_p; head.next = tail; head.prev = tail;
        self.tail = Link::Head;
        self.head_next = self.tail;
        self.head_prev = self.tail;
        self.owned.clear();
        Ok(())
    }

    /// Live allocation payload addresses, head first.
    pub fn handles(&self) -> Result<Vec<Handle>, AllocError> {
        let mut out = Vec::with_capacity(self.owned.len());
        let mut now = self.head_next;
        let mut steps = 0usize;
        while now != Link::Head {
            if steps > self.owned.len() {
                return Err(AllocError::Corrupt);
            }
            steps += 1;
            let node = match now {
                Link::Node(addr) => addr,
                Link::Head => unreachable!("loop condition excludes the sentinel"),
            };
            out.push(Handle(node.wrapping_add(NODE_HEADER_SIZE)));
            now = Link::decode(get_u32(self.node(node)?, OFF_NEXT));
        }
        Ok(out)
    }

    /// Number of live allocations.
    pub fn len(&self) -> usize {
        self.owned.len()
    }

    /// Whether the ring holds no allocations.
    pub fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }

    /// Whether `handle` is currently held by this ring.
    pub fn owns(&self, handle: Handle) -> bool {
        handle
            .0
            .checked_sub(NODE_HEADER_SIZE)
            .is_some_and(|node| self.owned.contains(&node))
    }

    /// The `alloc_size` recorded in `handle`'s node header.
    pub fn alloc_size(&self, handle: Handle) -> Result<u32, AllocError> {
        let node = handle
            .0
            .checked_sub(NODE_HEADER_SIZE)
            .ok_or(AllocError::NotOwned)?;
        if !self.owned.contains(&node) {
            return Err(AllocError::NotOwned);
        }
        Ok(get_u32(self.node(node)?, OFF_ALLOC_SIZE))
    }

    /// The payload bytes of `handle`, exactly `alloc_size` long.
    pub fn data(&self, handle: Handle) -> Result<&[u8], AllocError> {
        let size = self.alloc_size(handle)?;
        Ok(self.store.bytes(handle.0, size)?)
    }

    /// Mutable payload bytes of `handle`.
    pub fn data_mut(&mut self, handle: Handle) -> Result<&mut [u8], AllocError> {
        let size = self.alloc_size(handle)?;
        Ok(self.store.bytes_mut(handle.0, size)?)
    }

    /// The backing store.
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// The backing store, mutably.
    pub const fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// The address `tail` points at, `0` when it points at the sentinel.
    ///
    /// Exposed for differential checks of the list shape.
    pub const fn tail_addr(&self) -> Addr {
        self.tail.encode()
    }

    /// The address `head.next` points at, `0` when the ring is empty.
    pub const fn head_next_addr(&self) -> Addr {
        self.head_next.encode()
    }

    /// The address `head.prev` points at, `0` when the ring is empty.
    pub const fn head_prev_addr(&self) -> Addr {
        self.head_prev.encode()
    }

    fn node(&self, addr: Addr) -> Result<&[u8; NODE_HEADER_SIZE as usize], AllocError> {
        self.store
            .bytes(addr, NODE_HEADER_SIZE)?
            .try_into()
            .map_err(|_| AllocError::NotOwned)
    }

    fn node_mut(&mut self, addr: Addr) -> Result<&mut [u8; NODE_HEADER_SIZE as usize], AllocError> {
        self.store
            .bytes_mut(addr, NODE_HEADER_SIZE)?
            .try_into()
            .map_err(|_| AllocError::NotOwned)
    }

    /// `link->next = next`, where `link` may be the sentinel.
    fn set_next(&mut self, link: Link, next: Link) -> Result<(), AllocError> {
        match link {
            Link::Head => self.head_next = next,
            Link::Node(addr) => put_u32(self.node_mut(addr)?, OFF_NEXT, next.encode()),
        }
        Ok(())
    }

    /// `link->prev = prev`, where `link` may be the sentinel.
    fn set_prev(&mut self, link: Link, prev: Link) -> Result<(), AllocError> {
        match link {
            Link::Head => self.head_prev = prev,
            Link::Node(addr) => put_u32(self.node_mut(addr)?, OFF_PREV, prev.encode()),
        }
        Ok(())
    }
}

/// Node fields are written in native byte order, as the C struct is.
fn get_u32(header: &[u8; NODE_HEADER_SIZE as usize], offset: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&header[offset..offset + 4]);
    u32::from_ne_bytes(word)
}

fn put_u32(header: &mut [u8; NODE_HEADER_SIZE as usize], offset: usize, value: u32) {
    header[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}
