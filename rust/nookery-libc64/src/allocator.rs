//! Host-safe model of `src/static/libc64/__osMalloc.c`.
//!
//! The allocator keeps the upstream block geometry and linked-list mutations,
//! while storing the headers as Rust metadata.  The backing allocation is
//! aligned to 32 bytes, so pointer alignment and offsets have the same meaning
//! as they do in the PC C build without replacing the process allocator's
//! `malloc` or `free` symbols.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ffi::c_void;
use std::ptr::NonNull;

/// The magic value written into every live allocator header.
pub const OS_MALLOC_MAGIC: i16 = 0x7373;

/// Clear allocated payloads with the source's debug byte.
#[allow(non_upper_case_globals)]
pub const OSArena_FLAG_CLEAR_MEM_ON_ALLOC: u8 = 1 << 0;
/// Clear freed payloads with the source's debug byte.
#[allow(non_upper_case_globals)]
pub const OSArena_FLAG_CLEAR_MEM_ON_FREE: u8 = 1 << 1;
/// Enable the source's free-block poison check.
#[allow(non_upper_case_globals)]
pub const OSArena_FLAG_FREE_BLOCK_TEST: u8 = 1 << 2;

/// The PC C layout of an `OSMemBlock` header.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OsMemBlock {
    /// Header magic.
    pub magic: i16,
    /// Zero for allocated, non-zero for free.
    pub free: i16,
    /// Payload size in bytes.
    pub size: i32,
    /// Next physical block (opaque in this Rust model).
    pub next: *mut OsMemBlock,
    /// Previous physical block (opaque in this Rust model).
    pub prev: *mut OsMemBlock,
    /// Source file recorded by debug builds.
    pub file: *const i8,
    /// Source line recorded by debug builds.
    pub line: i32,
    /// Source thread id.
    pub thread_id: i32,
    /// Owning arena (opaque in this Rust model).
    pub arena: *mut c_void,
    /// Allocation time.
    pub time: i64,
    /// Header padding.
    pub pad: [u8; 8],
}

/// The allocator's observable failure/status codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocatorStatus {
    /// The operation completed without an allocator error.
    Ok,
    /// The requested allocation could not fit any free block.
    ArenaExhausted,
    /// The pointer does not identify a valid block header (C line 738).
    InvalidFree,
    /// The pointer identifies an already-free block (C line 743).
    DoubleFree,
    /// The block belongs to a different arena (C line 750).
    WrongArena,
}

/// A linked-list block snapshot for differential tests and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockSnapshot {
    /// Offset of the header from the backing allocation's beginning.
    pub offset: usize,
    /// Offset of the next linked-list header, if any.
    pub next: Option<usize>,
    /// Offset of the previous linked-list header, if any.
    pub prev: Option<usize>,
    /// Payload size in bytes.
    pub size: u32,
    /// Whether the block is free.
    pub free: bool,
    /// Header magic, retained for invalid-pointer/double-free observations.
    pub magic: i16,
}

#[derive(Clone, Debug)]
struct Block {
    offset: usize,
    size: u32,
    free: bool,
    magic: i16,
    next: Option<usize>,
    prev: Option<usize>,
    active: bool,
}

struct Storage {
    ptr: NonNull<u8>,
    len: usize,
}

impl Storage {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len.max(1), 256).expect("valid allocator layout");
        // SAFETY: the layout has non-zero size and a supported power-of-two
        // alignment.  The pointer is released by `Drop` with the same layout.
        let ptr = unsafe { alloc_zeroed(layout) };
        let ptr =
            NonNull::new(ptr).unwrap_or_else(|| panic!("allocator backing allocation failed"));
        Self {
            ptr,
            len: len.max(1),
        }
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len, 256).expect("valid allocator layout");
        // SAFETY: `ptr` came from `alloc_zeroed` with exactly this layout and
        // is not used after `Storage` is dropped.
        unsafe { dealloc(self.ptr.as_ptr(), layout) };
    }
}

/// A single source-compatible arena backed by a stable, 32-byte-aligned host
/// allocation.
pub struct Arena {
    storage: Storage,
    arena_size: usize,
    blocks: Vec<Block>,
    head: Option<usize>,
    flags: u8,
    initialized: bool,
    last_status: AllocatorStatus,
}

impl Arena {
    /// Initialize an arena with `size` bytes beginning at `base_offset`.
    ///
    /// `base_offset` is useful for reproducing the C source's behavior when an
    /// intentionally misaligned base pointer is supplied.  The backing store
    /// has enough room for the complete requested span.
    pub fn new(size: usize, base_offset: usize) -> Self {
        let storage = Storage::new(base_offset.saturating_add(size));
        let mut arena = Self {
            storage,
            arena_size: size,
            blocks: Vec::new(),
            head: None,
            flags: 0,
            initialized: false,
            last_status: AllocatorStatus::Ok,
        };
        arena.add_block(base_offset, size);
        arena.initialized = true;
        arena
    }

    /// Set the source arena debug flags.
    pub fn set_flags(&mut self, flags: u8) {
        self.flags = flags;
    }

    /// Read the source arena debug flags.
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Whether `__osMallocInit` has completed.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Return the most recent source-level status observed by this arena.
    pub fn last_status(&self) -> AllocatorStatus {
        self.last_status
    }

    /// Add another source-style free block to the arena.
    pub fn add_block(&mut self, base_offset: usize, size: usize) {
        if base_offset >= self.storage.len || size == 0 {
            return;
        }
        let block_offset = align_next_usize(self.storage.ptr.as_ptr() as usize + base_offset, 32)
            - self.storage.ptr.as_ptr() as usize;
        let consumed = block_offset.saturating_sub(base_offset);
        let align_size = align_prev_u32(size.saturating_sub(consumed) as u32, 32) as usize;
        let header = std::mem::size_of::<OsMemBlock>();
        if align_size <= header || block_offset.saturating_add(align_size) > self.storage.len {
            return;
        }

        let index = self.blocks.len();
        let last = self.search_last();
        self.blocks.push(Block {
            offset: block_offset,
            size: (align_size - header) as u32,
            free: true,
            magic: OS_MALLOC_MAGIC,
            next: None,
            prev: last,
            active: true,
        });
        if let Some(last) = last {
            self.blocks[last].next = Some(index);
        } else {
            self.head = Some(index);
        }
        if self.arena_size == 0 {
            self.arena_size = size;
        }
    }

    /// Remove all linked blocks and zero the arena's observable state.
    pub fn cleanup(&mut self) {
        let mut current = self.head;
        while let Some(index) = current {
            current = self.blocks[index].next;
            self.poison_header(index, 0xAB);
            self.blocks[index].magic = 0xABAB_u16 as i16;
            self.blocks[index].active = false;
        }
        self.head = None;
        self.blocks.clear();
        self.initialized = false;
        self.last_status = AllocatorStatus::Ok;
    }

    /// Allocate from the first suitable free block with source alignment rules.
    pub fn malloc_align(&mut self, size: u32, align: u32) -> *mut u8 {
        self.last_status = AllocatorStatus::Ok;
        let size = align_next_u32(size, 32);
        let full_size = size.wrapping_add(std::mem::size_of::<OsMemBlock>() as u32);
        let align = normalize_alignment(align);
        let mask = align - 1;
        let mut current = self.head;
        while let Some(index) = current {
            let next = self.blocks[index].next;
            if self.blocks[index].active && self.blocks[index].free {
                let payload_addr = self
                    .address(self.blocks[index].offset)
                    .wrapping_add(std::mem::size_of::<OsMemBlock>());
                let remain = payload_addr & mask as usize;
                let alignment_bytes = if remain == 0 {
                    0
                } else {
                    align as usize - remain
                };
                if self.blocks[index].size >= alignment_bytes as u32
                    && self.blocks[index].size - alignment_bytes as u32 >= size
                {
                    self.allocate_at(index, alignment_bytes, size, full_size);
                    return self.payload_ptr(index);
                }
            }
            current = next;
        }
        self.last_status = AllocatorStatus::ArenaExhausted;
        std::ptr::null_mut()
    }

    /// Allocate with the source's default alignment.
    pub fn malloc(&mut self, size: u32) -> *mut u8 {
        self.malloc_align(size, 0)
    }

    /// Allocate from the last suitable free block, matching `__osMallocR`.
    pub fn malloc_reverse(&mut self, size: u32) -> *mut u8 {
        self.last_status = AllocatorStatus::Ok;
        let size = align_next_u32(size, 32);
        let full_size = size.wrapping_add(std::mem::size_of::<OsMemBlock>() as u32);
        let mut current = self.search_last();
        while let Some(index) = current {
            let prev = self.blocks[index].prev;
            if self.blocks[index].active
                && self.blocks[index].free
                && self.blocks[index].size >= size
            {
                if self.blocks[index].size > full_size {
                    let new_offset = self.blocks[index].offset + self.blocks[index].size as usize
                        - size as usize;
                    let new_index = self.insert_block_after(index, new_offset, size, false);
                    self.blocks[index].size -= full_size;
                    self.mark_used(new_index);
                    return self.payload_ptr(new_index);
                }
                self.mark_used(index);
                return self.payload_ptr(index);
            }
            current = prev;
        }
        self.last_status = AllocatorStatus::ArenaExhausted;
        std::ptr::null_mut()
    }

    /// Release a pointer and return the source-level status.
    pub fn free(&mut self, ptr: *mut u8) -> AllocatorStatus {
        self.last_status = AllocatorStatus::Ok;
        if ptr.is_null() {
            return AllocatorStatus::Ok;
        }
        let Some(index) = self.index_for_payload(ptr) else {
            self.last_status = AllocatorStatus::InvalidFree;
            return self.last_status;
        };
        if self.blocks[index].magic != OS_MALLOC_MAGIC {
            self.last_status = AllocatorStatus::InvalidFree;
            return self.last_status;
        }
        if self.blocks[index].free {
            self.last_status = AllocatorStatus::DoubleFree;
            return self.last_status;
        }
        if !self.blocks[index].active {
            self.last_status = AllocatorStatus::InvalidFree;
            return self.last_status;
        }

        self.blocks[index].free = true;
        if self.flags & OSArena_FLAG_CLEAR_MEM_ON_FREE != 0 {
            self.fill_payload(index, 0xEF);
        }

        let mut current = index;
        if let Some(next) = self.blocks[current].next {
            if self.blocks[next].free
                && self.blocks[next].offset
                    == self.blocks[current].offset
                        + std::mem::size_of::<OsMemBlock>()
                        + self.blocks[current].size as usize
            {
                let after = self.blocks[next].next;
                self.blocks[current].size = self.blocks[current]
                    .size
                    .wrapping_add(self.blocks[next].size)
                    .wrapping_add(std::mem::size_of::<OsMemBlock>() as u32);
                self.blocks[current].next = after;
                if let Some(after) = after {
                    self.blocks[after].prev = Some(current);
                }
                if self.flags & OSArena_FLAG_CLEAR_MEM_ON_FREE != 0 {
                    self.blocks[next].magic = 0xEFEF_u16 as i16;
                }
                self.blocks[next].active = false;
            }
        }
        if let Some(prev) = self.blocks[current].prev {
            if self.blocks[prev].free
                && self.blocks[current].offset
                    == self.blocks[prev].offset
                        + std::mem::size_of::<OsMemBlock>()
                        + self.blocks[prev].size as usize
            {
                let after = self.blocks[current].next;
                self.blocks[prev].next = after;
                self.blocks[prev].size = self.blocks[prev]
                    .size
                    .wrapping_add(self.blocks[current].size)
                    .wrapping_add(std::mem::size_of::<OsMemBlock>() as u32);
                if let Some(after) = after {
                    self.blocks[after].prev = Some(prev);
                }
                if self.flags & OSArena_FLAG_CLEAR_MEM_ON_FREE != 0 {
                    self.blocks[current].magic = 0xEFEF_u16 as i16;
                }
                self.blocks[current].active = false;
                current = prev;
            }
        }
        let _ = current;
        self.last_status
    }

    /// Reallocate a source pointer, preserving payload bytes on movement.
    pub fn realloc(&mut self, ptr: *mut u8, size: u32) -> *mut u8 {
        if ptr.is_null() {
            return self.malloc(size);
        }
        if size == 0 {
            self.free(ptr);
            return std::ptr::null_mut();
        }
        let Some(index) = self.index_for_payload(ptr) else {
            self.last_status = AllocatorStatus::InvalidFree;
            return std::ptr::null_mut();
        };
        if self.blocks[index].magic != OS_MALLOC_MAGIC || !self.blocks[index].active {
            self.last_status = AllocatorStatus::InvalidFree;
            return std::ptr::null_mut();
        }
        let size = align_next_u32(size, 32);
        if size == self.blocks[index].size {
            return ptr;
        }
        let old_size = self.blocks[index].size;
        if size > old_size {
            let need = size - old_size;
            if let Some(next) = self.blocks[index].next {
                if self.blocks[next].free
                    && self.blocks[next].offset
                        == self.blocks[index].offset
                            + std::mem::size_of::<OsMemBlock>()
                            + old_size as usize
                    && self.blocks[next].size >= need
                {
                    let new_next_offset = self.blocks[next].offset + need as usize;
                    let after = self.blocks[next].next;
                    self.blocks[next].offset = new_next_offset;
                    self.blocks[next].size -= need;
                    self.blocks[index].size = size;
                    if let Some(after) = after {
                        self.blocks[after].prev = Some(next);
                    }
                    return ptr;
                }
            }
            let old = self.read_payload(ptr, old_size as usize);
            let new_ptr = self.malloc(size);
            if !new_ptr.is_null() {
                self.write_payload(new_ptr, &old);
                self.free(ptr);
            }
            return new_ptr;
        }

        let full_size = size.wrapping_add(std::mem::size_of::<OsMemBlock>() as u32);
        if let Some(next) = self.blocks[index].next {
            if self.blocks[next].free {
                self.blocks[next].offset = self.blocks[index].offset + full_size as usize;
                self.blocks[next].size = self.blocks[next]
                    .size
                    .wrapping_add(old_size)
                    .wrapping_sub(size);
                self.blocks[index].size = size;
                return ptr;
            }
        } else if size.wrapping_add(std::mem::size_of::<OsMemBlock>() as u32) < old_size {
            let next_offset = self.blocks[index].offset + full_size as usize;
            let next = self.insert_block_after(index, next_offset, old_size - full_size, true);
            self.blocks[index].size = size;
            self.blocks[next].prev = Some(index);
            return ptr;
        }
        self.last_status = AllocatorStatus::ArenaExhausted;
        std::ptr::null_mut()
    }

    /// Return largest free payload, total free payload, and used payload.
    pub fn analyze(&self) -> (u32, u32, u32) {
        let mut max: u32 = 0;
        let mut free: u32 = 0;
        let mut used: u32 = 0;
        let mut current = self.head;
        while let Some(index) = current {
            let block = &self.blocks[index];
            if block.free {
                max = max.max(block.size);
                free = free.wrapping_add(block.size);
            } else {
                used = used.wrapping_add(block.size);
            }
            current = block.next;
        }
        (max, free, used)
    }

    /// Return the largest free payload.
    pub fn free_size(&self) -> u32 {
        self.analyze().0
    }

    /// Return total free payload.
    pub fn total_free_size(&self) -> u32 {
        self.analyze().1
    }

    /// Return a block's payload size, or `-1` for an invalid pointer.
    pub fn mem_block_size(&self, ptr: *mut u8) -> i32 {
        self.index_for_payload(ptr)
            .filter(|&index| self.blocks[index].magic == OS_MALLOC_MAGIC)
            .map(|index| self.blocks[index].size as i32)
            .unwrap_or(-1)
    }

    /// Return the currently linked blocks in source order.
    pub fn snapshot(&self) -> Vec<BlockSnapshot> {
        let mut result = Vec::new();
        let mut current = self.head;
        while let Some(index) = current {
            let block = &self.blocks[index];
            result.push(BlockSnapshot {
                offset: block.offset,
                next: block.next.map(|next| self.blocks[next].offset),
                prev: block.prev.map(|prev| self.blocks[prev].offset),
                size: block.size,
                free: block.free,
                magic: block.magic,
            });
            current = block.next;
        }
        result
    }

    /// Return a pointer to the beginning of the backing allocation.
    pub fn backing_ptr(&self) -> *mut u8 {
        self.storage.ptr.as_ptr()
    }

    fn address(&self, offset: usize) -> usize {
        self.storage.ptr.as_ptr() as usize + offset
    }

    fn payload_ptr(&self, index: usize) -> *mut u8 {
        (self.address(self.blocks[index].offset) + std::mem::size_of::<OsMemBlock>()) as *mut u8
    }

    fn index_for_payload(&self, ptr: *mut u8) -> Option<usize> {
        let address = ptr as usize;
        self.blocks.iter().position(|block| {
            block.offset + std::mem::size_of::<OsMemBlock>() + self.storage.ptr.as_ptr() as usize
                == address
        })
    }

    fn search_last(&self) -> Option<usize> {
        let mut current = self.head;
        let mut last = None;
        while let Some(index) = current {
            last = Some(index);
            current = self.blocks[index].next;
        }
        last
    }

    fn allocate_at(&mut self, original: usize, alignment_bytes: usize, size: u32, full_size: u32) {
        let index = original;
        if alignment_bytes != 0 {
            let old_prev = self.blocks[index].prev;
            let old_next = self.blocks[index].next;
            let aligned_offset = self.blocks[index].offset + alignment_bytes;
            self.blocks[index].offset = aligned_offset;
            self.blocks[index].size -= alignment_bytes as u32;
            if let Some(prev) = old_prev {
                self.blocks[prev].next = Some(index);
                self.blocks[prev].size =
                    self.blocks[prev].size.wrapping_add(alignment_bytes as u32);
            } else {
                self.head = Some(index);
            }
            self.blocks[index].prev = old_prev;
            self.blocks[index].next = old_next;
            if let Some(next) = old_next {
                self.blocks[next].prev = Some(index);
            }
        }
        if self.blocks[index].size > full_size {
            let next_offset = self.blocks[index].offset + full_size as usize;
            let remainder = self.blocks[index].size - full_size;
            let next = self.insert_block_after(index, next_offset, remainder, true);
            self.blocks[index].size = size;
            self.blocks[index].next = Some(next);
            self.blocks[next].prev = Some(index);
        }
        self.mark_used(index);
    }

    fn mark_used(&mut self, index: usize) {
        self.blocks[index].free = false;
        if self.flags & OSArena_FLAG_CLEAR_MEM_ON_ALLOC != 0 {
            self.fill_payload(index, 0xCD);
        }
    }

    fn insert_block_after(&mut self, index: usize, offset: usize, size: u32, free: bool) -> usize {
        let next = self.blocks[index].next;
        let new_index = self.blocks.len();
        self.blocks.push(Block {
            offset,
            size,
            free,
            magic: OS_MALLOC_MAGIC,
            next,
            prev: Some(index),
            active: true,
        });
        self.blocks[index].next = Some(new_index);
        if let Some(next) = next {
            self.blocks[next].prev = Some(new_index);
        }
        new_index
    }

    fn poison_header(&mut self, index: usize, byte: u8) {
        if self.flags & OSArena_FLAG_CLEAR_MEM_ON_FREE != 0 {
            let start = self.blocks[index].offset;
            let end = (start + std::mem::size_of::<OsMemBlock>()).min(self.storage.len);
            // SAFETY: the range is contained in the backing allocation.
            unsafe {
                std::ptr::write_bytes(self.storage.ptr.as_ptr().add(start), byte, end - start)
            };
        }
    }

    fn fill_payload(&mut self, index: usize, byte: u8) {
        let start = self.blocks[index].offset + std::mem::size_of::<OsMemBlock>();
        let end = (start + self.blocks[index].size as usize).min(self.storage.len);
        // SAFETY: source block geometry is clipped to the backing allocation.
        unsafe { std::ptr::write_bytes(self.storage.ptr.as_ptr().add(start), byte, end - start) };
    }

    fn read_payload(&self, ptr: *mut u8, size: usize) -> Vec<u8> {
        let mut output = vec![0; size];
        // SAFETY: `ptr` is a live payload pointer obtained from this arena.
        unsafe { std::ptr::copy_nonoverlapping(ptr, output.as_mut_ptr(), size) };
        output
    }

    fn write_payload(&self, ptr: *mut u8, bytes: &[u8]) {
        // SAFETY: `ptr` is a live payload pointer and `bytes` is no larger than
        // the destination block requested by the caller.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
    }
}

fn align_next_u32(value: u32, align: u32) -> u32 {
    value.wrapping_add(align - 1) & !(align - 1)
}

fn align_prev_u32(value: u32, align: u32) -> u32 {
    value & !(align - 1)
}

fn align_next_usize(value: usize, align: usize) -> usize {
    value.saturating_add(align - 1) & !(align - 1)
}

fn normalize_alignment(align: u32) -> u32 {
    if align <= 16 {
        16
    } else if align <= 32 {
        32
    } else if align <= 64 {
        64
    } else if align <= 128 {
        128
    } else if align <= 256 {
        256
    } else if align <= 1024 {
        1024
    } else {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_header_uses_the_host_oracle_width() {
        assert_eq!(std::mem::size_of::<OsMemBlock>(), 64);
    }

    #[test]
    fn zero_size_consumes_a_header_and_is_freeable() {
        let mut arena = Arena::new(512, 3);
        let ptr = arena.malloc(0);
        assert!(!ptr.is_null());
        assert_eq!(arena.mem_block_size(ptr), 0);
        assert_eq!(arena.free(ptr), AllocatorStatus::Ok);
    }
}
