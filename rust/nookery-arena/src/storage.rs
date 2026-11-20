//! Address type and the allocation seam.
//!
//! Upstream `gamealloc_malloc` obtains its nodes from `malloc`/`free`
//! (`include/libc64/malloc.h`, implemented over `__osMalloc` in
//! `src/static/libc64/malloc.c`).  That allocator is a game-runtime object, so
//! it is not ported; [`BlockStore`] is the seam and [`SlabStore`] is the
//! adapter used by tests and by any caller that wants real, writable bytes.
//!
//! # Address width
//!
//! [`Addr`] is `u32` because `include/gamealloc.h` and `include/game.h` assert
//! the 32-bit target layout (`sizeof(struct gameAlloc_s) == 0x14`,
//! `sizeof(struct gameAllocList_s) == 0x10`, `tha` at `0x78`), which is only
//! true when `size_t` and pointers are four bytes.  The port preserves that
//! width instead of silently widening it.  Upstream's `TARGET_PC` build is
//! compiled with 64-bit `uintptr_t`; on such a build the portfolio is identical
//! for every buffer below 4 GiB, which is what [`SlabStore`] enforces.
//!
//! [`SlabStore`] keeps a synthetic address origin ([`SlabStore::base`]) that is
//! *not* a host pointer.  Byte access goes through offset arithmetic on a
//! `Vec<u8>`, so no address ever becomes a raw pointer and the crate stays free
//! of `unsafe`.

use std::collections::BTreeMap;

/// A 32-bit guest address, matching the target's `uintptr_t`.
pub type Addr = u32;

/// Why a store could not serve a request, or why an address was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// `base + capacity` does not fit the 32-bit address model.
    AddressOverflow,
    /// The slab would start at address 0, which is the reserved sentinel.
    ReservedBase,
    /// The requested length is zero.
    ZeroLength,
    /// No free region is large enough.
    Exhausted,
    /// The address/length pair is not a live block of this store.
    NotOwned,
    /// The range is outside the slab, or overflows the address space.
    OutOfRange,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::AddressOverflow => "slab leaves the 32-bit address space",
            Self::ReservedBase => "slab starts at the reserved sentinel address",
            Self::ZeroLength => "zero-length allocation",
            Self::Exhausted => "no free region is large enough",
            Self::NotOwned => "address is not a live block of this store",
            Self::OutOfRange => "range is outside the slab",
        };
        f.write_str(text)
    }
}

impl std::error::Error for StoreError {}

/// The allocation seam that stands in for upstream `malloc`/`free`.
///
/// `len` is the total block length including whatever header the caller
/// reserves; the store makes no alignment or layout promise.  Failures are
/// reported rather than panicking so `GameAlloc` can map them onto its own
/// error type.
pub trait BlockStore {
    /// Reserve `len` bytes and report their address, or `None` when out of room.
    fn allocate(&mut self, len: u32) -> Option<Addr>;

    /// Return a block obtained from [`Self::allocate`].
    fn release(&mut self, addr: Addr, len: u32) -> Result<(), StoreError>;

    /// Whether `[addr, addr + len)` is a block this store handed out.
    fn is_live(&self, addr: Addr, len: u32) -> bool;

    /// Length of the live block that starts exactly at `addr`, if any.
    fn block_len(&self, addr: Addr) -> Option<u32>;

    /// Borrow a live block's bytes.
    fn bytes(&self, addr: Addr, len: u32) -> Result<&[u8], StoreError>;

    /// Mutably borrow a live block's bytes.
    fn bytes_mut(&mut self, addr: Addr, len: u32) -> Result<&mut [u8], StoreError>;
}

/// First-fit slab allocator over a `Vec<u8>`, addressed in a synthetic 32-bit
/// space.
///
/// This is *not* the game allocator: `__osMalloc` keeps `OSMemBlock` headers,
/// block magic values and an arena lock, and it can hand back a different
/// address for the same request.  `SlabStore` reproduces only the contract
/// `gamealloc` relies on — allocate a block, free it, reuse the space — so that
/// list bookkeeping can be exercised without the runtime.
#[derive(Debug)]
pub struct SlabStore {
    buf: Vec<u8>,
    base: Addr,
    /// Free regions as `(offset, len)`, kept sorted and coalesced.
    free: Vec<(u32, u32)>,
    /// Handed-out blocks as `offset -> len`.
    live: BTreeMap<u32, u32>,
}

impl SlabStore {
    /// Create a zeroed slab of `capacity` bytes whose first byte is address
    /// `base`.
    ///
    /// Rejects `base == 0` (the `GameAlloc` sentinel address) and any
    /// `base + capacity` that does not fit in 32 bits.
    pub fn at(base: Addr, capacity: u32) -> Result<Self, StoreError> {
        if base == 0 {
            return Err(StoreError::ReservedBase);
        }
        if base.checked_add(capacity).is_none() {
            return Err(StoreError::AddressOverflow);
        }
        Ok(Self {
            buf: vec![0; capacity as usize],
            base,
            free: if capacity == 0 {
                Vec::new()
            } else {
                vec![(0, capacity)]
            },
            live: BTreeMap::new(),
        })
    }

    /// The synthetic address of byte 0.
    pub fn base(&self) -> Addr {
        self.base
    }

    /// Total slab length in bytes.
    pub fn capacity(&self) -> u32 {
        self.buf.len() as u32
    }

    /// Bytes still available for allocation.
    pub fn free_bytes(&self) -> u32 {
        self.free.iter().map(|&(_, len)| len).sum()
    }

    /// Live blocks as `(address, len)`, ordered by address.
    pub fn live_blocks(&self) -> impl Iterator<Item = (Addr, u32)> + '_ {
        self.live
            .iter()
            .map(move |(&off, &len)| (self.base.wrapping_add(off), len))
    }

    /// Offset of `addr` inside a live block that contains `[addr, addr + len)`.
    fn live_offset(&self, addr: Addr, len: u32) -> Option<u32> {
        let off = addr.checked_sub(self.base)?;
        let end = off.checked_add(len)?;
        let (&start, &block_len) = self.live.range(..=off).next_back()?;
        let block_end = start.checked_add(block_len)?;
        (end <= block_end).then_some(off)
    }

    fn coalesce(&mut self) {
        self.free.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.free.len());
        for &(off, len) in &self.free {
            match merged.last_mut() {
                Some((last_off, last_len)) if *last_off + *last_len == off => {
                    *last_len += len;
                }
                _ => merged.push((off, len)),
            }
        }
        self.free = merged;
    }
}

impl BlockStore for SlabStore {
    fn allocate(&mut self, len: u32) -> Option<Addr> {
        if len == 0 {
            return None;
        }
        let index = self
            .free
            .iter()
            .position(|&(_, region_len)| region_len >= len)?;
        let (off, region_len) = self.free[index];
        // Split exactly.  Upstream `malloc` returns the whole region it took,
        // so any leftover below the allocation granularity must stay in the
        // free list: dropping it would lose capacity permanently, and a later
        // `release` would give back less than the block took.  With this rule
        // `live bytes + free bytes == capacity` holds after every call.
        if region_len == len {
            self.free.swap_remove(index);
            self.coalesce();
        } else {
            self.free[index] = (off + len, region_len - len);
        }
        self.live.insert(off, len);
        Some(self.base + off)
    }

    fn release(&mut self, addr: Addr, len: u32) -> Result<(), StoreError> {
        let off = addr.checked_sub(self.base).ok_or(StoreError::OutOfRange)?;
        match self.live.get(&off) {
            Some(&live_len) if live_len == len => {}
            Some(_) => return Err(StoreError::NotOwned),
            None => return Err(StoreError::NotOwned),
        }
        self.live.remove(&off);
        self.free.push((off, len));
        self.coalesce();
        Ok(())
    }

    fn is_live(&self, addr: Addr, len: u32) -> bool {
        let off = addr.wrapping_sub(self.base);
        self.live.get(&off) == Some(&len)
    }

    fn block_len(&self, addr: Addr) -> Option<u32> {
        let off = addr.checked_sub(self.base)?;
        self.live.get(&off).copied()
    }

    fn bytes(&self, addr: Addr, len: u32) -> Result<&[u8], StoreError> {
        let off = self.live_offset(addr, len).ok_or(StoreError::OutOfRange)?;
        Ok(&self.buf[off as usize..(off + len) as usize])
    }

    fn bytes_mut(&mut self, addr: Addr, len: u32) -> Result<&mut [u8], StoreError> {
        let off = self.live_offset(addr, len).ok_or(StoreError::OutOfRange)?;
        Ok(&mut self.buf[off as usize..(off + len) as usize])
    }
}

/// Bump allocator that never reuses a freed block, over a `Vec<u8>`.
///
/// This is the closest analogue of the C differential harness's heap
/// (`parity/driver.c` bumps a cursor and ignores `free`), so offsets stay
/// comparable between the port and the unmodified upstream sources.
///
/// It is also the honest counter-example to [`SlabStore`]: `GameAlloc` is
/// correct against either, which shows the port depends only on the
/// `malloc`/`free` contract and not on a particular reuse policy.
#[derive(Debug)]
pub struct BumpStore {
    buf: Vec<u8>,
    base: Addr,
    cursor: u32,
    live: BTreeMap<u32, u32>,
}

impl BumpStore {
    /// Create a zeroed `capacity`-byte bump region starting at `base`.
    pub fn at(base: Addr, capacity: u32) -> Result<Self, StoreError> {
        if base == 0 {
            return Err(StoreError::ReservedBase);
        }
        if base.checked_add(capacity).is_none() {
            return Err(StoreError::AddressOverflow);
        }
        Ok(Self {
            buf: vec![0; capacity as usize],
            base,
            cursor: 0,
            live: BTreeMap::new(),
        })
    }

    /// The synthetic address of byte 0.
    pub const fn base(&self) -> Addr {
        self.base
    }

    /// Bytes consumed by the cursor so far.
    pub const fn cursor(&self) -> u32 {
        self.cursor
    }

    /// Blocks still held, as `(offset from base, len)`, ordered by offset.
    pub fn live_offsets(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.live.iter().map(|(&off, &len)| (off, len))
    }

    fn live_offset(&self, addr: Addr, len: u32) -> Option<u32> {
        let off = addr.checked_sub(self.base)?;
        let end = off.checked_add(len)?;
        let (&start, &block_len) = self.live.range(..=off).next_back()?;
        (end <= start.checked_add(block_len)?).then_some(off)
    }
}

impl BlockStore for BumpStore {
    fn allocate(&mut self, len: u32) -> Option<Addr> {
        if len == 0 {
            return None;
        }
        let end = self.cursor.checked_add(len)?;
        if end > self.buf.len() as u32 {
            return None;
        }
        let off = self.cursor;
        self.cursor = end;
        self.live.insert(off, len);
        Some(self.base + off)
    }

    fn release(&mut self, addr: Addr, len: u32) -> Result<(), StoreError> {
        let off = addr.checked_sub(self.base).ok_or(StoreError::OutOfRange)?;
        match self.live.get(&off) {
            Some(&live_len) if live_len == len => {
                self.live.remove(&off);
                // The cursor is not rewound: freed space is never reused.
                Ok(())
            }
            _ => Err(StoreError::NotOwned),
        }
    }

    fn is_live(&self, addr: Addr, len: u32) -> bool {
        self.live.get(&addr.wrapping_sub(self.base)) == Some(&len)
    }

    fn block_len(&self, addr: Addr) -> Option<u32> {
        let off = addr.checked_sub(self.base)?;
        self.live.get(&off).copied()
    }

    fn bytes(&self, addr: Addr, len: u32) -> Result<&[u8], StoreError> {
        let off = self.live_offset(addr, len).ok_or(StoreError::OutOfRange)?;
        Ok(&self.buf[off as usize..(off + len) as usize])
    }

    fn bytes_mut(&mut self, addr: Addr, len: u32) -> Result<&mut [u8], StoreError> {
        let off = self.live_offset(addr, len).ok_or(StoreError::OutOfRange)?;
        Ok(&mut self.buf[off as usize..(off + len) as usize])
    }
}
