//! Rust port of `include/TwoHeadArena.h` / `src/TwoHeadArena.c`.
//!
//! # Ported functions
//!
//! [`TwoHeadArena::ct`] (`THA_ct`), [`TwoHeadArena::dt`] (`THA_dt`, the
//! `bzero`-everything destructor), [`TwoHeadArena::alloc16`] (`THA_alloc16`),
//! [`TwoHeadArena::alloc_align`] (`THA_allocAlign`),
//! [`TwoHeadArena::get_free_bytes_align`] (`THA_getFreeBytesAlign`),
//! [`TwoHeadArena::get_free_bytes16`] (`THA_getFreeBytes16`),
//! [`TwoHeadArena::get_free_bytes`] (`THA_getFreeBytes`),
//! [`TwoHeadArena::is_crash`] (`THA_isCrash`), [`TwoHeadArena::init`]
//! (`THA_init`).
//!
//! Not ported: the six `/* @fabricated */` functions that are commented out
//! upstream (`THA_alloc`, `THA_getHeadPtr`, `THA_setHeadPtr`, `THA_getTailPtr`,
//! `THA_nextPtrN`, `THA_nextPtr1`).  They are not active behavior at this pin.
//!
//! # Arithmetic contract
//!
//! The two allocators are pure address arithmetic; no memory is touched, so the
//! port keeps them free of `unsafe` and operates on [`Addr`] values:
//!
//! ```c
//! this->tail_p = (char*)((((uintptr_t)this->tail_p & mask) - siz) & mask);
//! ```
//!
//! The `- siz` subtraction is wrapping in C.  [`TwoHeadArena::alloc16`] and
//! [`TwoHeadArena::alloc_align`] reproduce that exactly; the `try_` variants
//! reject the wrap instead of storing an address outside the arena.
//!
//! The head pointer only moves in the fabricated `THA_nextPtrN`, which is not
//! ported, so head-side allocations do not exist in this crate.  `head_p` is
//! still tracked because every free-byte query and [`TwoHeadArena::is_crash`]
//! depend on it.

use crate::storage::Addr;

/// Mask `~(16 - 1)`, the alignment `THA_alloc16` hardcodes.
pub const MASK_16: i32 = !(16 - 1);

/// Mask `-1`, the "no alignment" mask `THA_getFreeBytes` passes.
pub const MASK_NONE: i32 = -1;

/// Why an arena operation was refused.
///
/// The C functions cannot report these: they wrap, store an out-of-arena tail,
/// and let [`TwoHeadArena::is_crash`] notice later (or dereference the result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// `buf_p + size` does not fit the 32-bit address model.
    AddressOverflow,
    /// The allocation would start below address 0 (the `- siz` wrap).
    Underflow,
    /// The allocation would start below `head_p`, i.e. collide with the other
    /// head of the arena.  Upstream reports this later through `THA_isCrash`.
    Collision,
    /// The range is not inside the attached buffer.
    OutOfRange,
    /// The mask is not of the form `!(n - 1)` for some power-of-two `n`.
    InvalidMask,
}

impl std::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::AddressOverflow => "arena end leaves the 32-bit address space",
            Self::Underflow => "allocation start wrapped below address 0",
            Self::Collision => "allocation collides with the arena head",
            Self::OutOfRange => "allocation leaves the arena buffer",
            Self::InvalidMask => "alignment mask is not !(n - 1)",
        };
        f.write_str(text)
    }
}

impl std::error::Error for ArenaError {}

/// Two-headed arena: the tail grows down from `buf_p + size`, the head grows up
/// from `buf_p`.  Layout matches `TwoHeadArena_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TwoHeadArena {
    size: Addr,
    buf_p: Addr,
    head_p: Addr,
    tail_p: Addr,
}

/// The `(usize)(s64)mask` conversion both `THA_allocAlign` and
/// `THA_getFreeBytesAlign` perform, done in the 32-bit address space.
#[inline]
const fn native_mask(mask: i32) -> u32 {
    mask as u32
}

/// `(u32)~mask` from `THA_getFreeBytesAlign`.
#[inline]
const fn inverse_mask(mask: i32) -> u32 {
    !(mask as u32)
}

/// Whether `mask` is `!(n - 1)` for a power-of-two `n`.
///
/// `-1` (align 1), `-16` (align 16) and `~0x1F` (align 32, the `m_scene.c` call
/// site) all satisfy this; arbitrary integers do not, and would make an
/// "alignment" that moves the tail unpredictably.
pub fn is_alignment_mask(mask: i32) -> bool {
    let bits = (mask as u32).trailing_zeros();
    match 1u32.checked_shl(bits) {
        Some(align) => mask as u32 == !(align - 1),
        None => false,
    }
}

impl TwoHeadArena {
    /// An arena in the state `THA_dt` leaves behind: every field zero, usable
    /// only after [`Self::ct`] or [`Self::init`].
    pub const fn new() -> Self {
        Self {
            size: 0,
            buf_p: 0,
            head_p: 0,
            tail_p: 0,
        }
    }

    /// `THA_ct`: attach `size` bytes at `buf_p` and reset both heads.
    ///
    /// Matches the source, which stores the pointer and length unvalidated and
    /// then calls `THA_init`.  `buf_p + size` wraps silently, exactly as it does
    /// on a 32-bit target; use [`Self::try_ct`] to reject that.
    pub fn ct(&mut self, buf_p: Addr, size: Addr) {
        self.buf_p = buf_p;
        self.size = size;
        self.init();
    }

    /// `THA_ct` with the wrapped end address rejected.
    pub fn try_ct(&mut self, buf_p: Addr, size: Addr) -> Result<(), ArenaError> {
        if buf_p.checked_add(size).is_none() {
            return Err(ArenaError::AddressOverflow);
        }
        self.ct(buf_p, size);
        Ok(())
    }

    /// `THA_init`: `head_p = buf_p`, `tail_p = buf_p + size`.
    ///
    /// The addition wraps like the source's pointer arithmetic.
    pub fn init(&mut self) {
        self.head_p = self.buf_p;
        self.tail_p = self.buf_p.wrapping_add(self.size);
    }

    /// `THA_dt`: zero the whole structure, including `buf_p` and `size`.
    ///
    /// Upstream `bzero`s `sizeof(TwoHeadArena)`; the port zeroes every field so
    /// a detached arena reports zero free bytes and cannot allocate.
    pub fn dt(&mut self) {
        *self = Self::new();
    }

    /// `THA_alloc16`: align the tail down to 16 bytes, subtract `siz`, align
    /// again, and return the new tail.
    ///
    /// Wrapping subtraction, as in the source.
    pub fn alloc16(&mut self, siz: Addr) -> Addr {
        self.alloc_align(siz, MASK_16)
    }

    /// `THA_alloc16` that refuses to wrap out of the arena.
    pub fn try_alloc16(&mut self, siz: Addr) -> Result<Addr, ArenaError> {
        self.try_alloc_align(siz, MASK_16)
    }

    /// `THA_allocAlign`: the tail-side allocation with a caller-supplied mask.
    ///
    /// `mask` is an `int` as in the header, converted with
    /// `(uintptr_t)(s64)mask`, i.e. sign-extended and then taken as an address
    /// mask.  `-1` allocates without alignment, `~0x1F` aligns to 32 bytes.
    pub fn alloc_align(&mut self, siz: Addr, mask: i32) -> Addr {
        let native = native_mask(mask);
        // Written to match the source exactly: ((tail & mask) - siz) & mask.
        self.tail_p = (self.tail_p & native).wrapping_sub(siz) & native;
        self.tail_p
    }

    /// `THA_allocAlign` with the wrap, the arena collision and any out-of-range
    /// result rejected instead of stored.
    ///
    /// On success the new tail is inside `[buf_p, head_p]`, so the block
    /// `[tail, tail + siz)` cannot overlap an earlier tail allocation and lies
    /// entirely inside the attached buffer.
    pub fn try_alloc_align(&mut self, siz: Addr, mask: i32) -> Result<Addr, ArenaError> {
        if !is_alignment_mask(mask) {
            return Err(ArenaError::InvalidMask);
        }
        let native = native_mask(mask);
        let aligned = self.tail_p & native;
        let new_tail = aligned.checked_sub(siz).ok_or(ArenaError::Underflow)? & native;
        // `head_p` only ever equals `buf_p` at this pin (the head advances in
        // the fabricated `THA_nextPtrN`, which is not active), so this single
        // check is both the collision test and the "left the buffer" test.
        if new_tail < self.head_p {
            return Err(ArenaError::Collision);
        }
        self.tail_p = new_tail;
        Ok(new_tail)
    }

    /// `THA_getFreeBytesAlign`: free bytes below the tail after aligning the
    /// head up by `~(mask)`.
    ///
    /// Note the source's own asymmetry: the head is aligned with `~mask`
    /// (which for `-16` is `15`) while the tail is aligned with `mask`.  The
    /// result is truncated to `int`, so a wrapped tail reports negative.
    pub fn get_free_bytes_align(&self, mask: i32) -> i32 {
        let native = native_mask(mask);
        let aligned_head = self.head_p.wrapping_add(inverse_mask(mask)) & native;
        self.tail_p.wrapping_sub(aligned_head) as i32
    }

    /// `THA_getFreeBytes16`: free bytes assuming 16-byte tail alignment.
    pub fn get_free_bytes16(&self) -> i32 {
        self.get_free_bytes_align(MASK_16)
    }

    /// `THA_getFreeBytes`: unaligned free bytes.
    pub fn get_free_bytes(&self) -> i32 {
        self.get_free_bytes_align(MASK_NONE)
    }

    /// `THA_isCrash`: the arena is over-allocated when the free-byte query goes
    /// negative.
    pub fn is_crash(&self) -> bool {
        self.get_free_bytes() < 0
    }

    /// Attached buffer address (`buf_p`).
    pub const fn buf_p(&self) -> Addr {
        self.buf_p
    }

    /// Attached buffer length (`size`).
    pub const fn size(&self) -> Addr {
        self.size
    }

    /// Head pointer, the low end of the arena.
    pub const fn head_p(&self) -> Addr {
        self.head_p
    }

    /// Tail pointer, the next tail allocation's high end.
    pub const fn tail_p(&self) -> Addr {
        self.tail_p
    }

    /// End of the attached buffer, `buf_p + size`.
    pub fn end_p(&self) -> Addr {
        self.buf_p.wrapping_add(self.size)
    }

    /// Whether `[addr, addr + len)` lies inside the attached buffer.
    pub fn contains(&self, addr: Addr, len: Addr) -> bool {
        let end = match addr.checked_add(len) {
            Some(end) => end,
            None => return false,
        };
        addr >= self.buf_p && end <= self.end_p()
    }
}

/// An arena bound to a real buffer, so tail allocations become byte slices.
///
/// This is the crate's only "storage adapter" on the arena side: [`Addr`]
/// values stay synthetic numbers, and a slice is produced by checked offset
/// arithmetic against the attached `Vec<u8>`.  No raw pointer is ever formed.
#[derive(Debug)]
pub struct Arena {
    buf: Vec<u8>,
    base: Addr,
    tha: TwoHeadArena,
}

impl Arena {
    /// Attach an arena of `capacity` zeroed bytes whose first byte is address
    /// `base`.
    ///
    /// `base` is a synthetic number, not a host address: the buffer lives in a
    /// `Vec<u8>` and every address is resolved by `addr - base`.  Only
    /// `base + capacity` is validated, because that sum must stay inside the
    /// 32-bit model.
    pub fn at(base: Addr, capacity: Addr) -> Result<Self, ArenaError> {
        if base.checked_add(capacity).is_none() {
            return Err(ArenaError::AddressOverflow);
        }
        let mut tha = TwoHeadArena::new();
        tha.ct(base, capacity);
        Ok(Self {
            buf: vec![0; capacity as usize],
            base,
            tha,
        })
    }

    /// The attached arena state.
    pub const fn tha(&self) -> &TwoHeadArena {
        &self.tha
    }

    /// Buffer length.
    pub fn capacity(&self) -> Addr {
        self.tha.size()
    }

    /// Address of byte 0 of the buffer.
    pub const fn base(&self) -> Addr {
        self.base
    }

    /// Reset both heads without touching buffer contents (`THA_init`).
    pub fn reset(&mut self) {
        self.tha.init();
    }

    /// `THA_alloc16` plus a bounds-checked slice for the allocated block.
    pub fn alloc16(&mut self, siz: Addr) -> Result<&mut [u8], ArenaError> {
        self.alloc_align(siz, MASK_16)
    }

    /// `THA_allocAlign` plus a bounds-checked slice for the allocated block.
    pub fn alloc_align(&mut self, siz: Addr, mask: i32) -> Result<&mut [u8], ArenaError> {
        let addr = self.tha.try_alloc_align(siz, mask)?;
        self.slice_mut(addr, siz)
    }

    /// Bytes at `[addr, addr + len)`, if they lie inside the buffer.
    ///
    /// Intended for regions produced by the raw [`TwoHeadArena`] functions,
    /// whose results are addresses rather than slices.
    pub fn bytes(&self, addr: Addr, len: Addr) -> Result<&[u8], ArenaError> {
        let (start, end) = self.range(addr, len)?;
        Ok(&self.buf[start..end])
    }

    /// Mutable form of [`Self::bytes`].
    pub fn bytes_mut(&mut self, addr: Addr, len: Addr) -> Result<&mut [u8], ArenaError> {
        let (start, end) = self.range(addr, len)?;
        Ok(&mut self.buf[start..end])
    }

    /// `THA_getFreeBytes` on the attached arena.
    pub fn free_bytes(&self) -> i32 {
        self.tha.get_free_bytes()
    }

    /// `THA_getFreeBytes16` on the attached arena.
    pub fn free_bytes16(&self) -> i32 {
        self.tha.get_free_bytes16()
    }

    /// `THA_isCrash` on the attached arena.
    pub fn is_crash(&self) -> bool {
        self.tha.is_crash()
    }

    fn range(&self, addr: Addr, len: Addr) -> Result<(usize, usize), ArenaError> {
        if !self.tha.contains(addr, len) {
            return Err(ArenaError::OutOfRange);
        }
        let start = addr.wrapping_sub(self.base) as usize;
        let end = start + len as usize;
        if end > self.buf.len() {
            return Err(ArenaError::OutOfRange);
        }
        Ok((start, end))
    }

    fn slice_mut(&mut self, addr: Addr, siz: Addr) -> Result<&mut [u8], ArenaError> {
        let (start, end) = self.range(addr, siz)?;
        Ok(&mut self.buf[start..end])
    }
}
