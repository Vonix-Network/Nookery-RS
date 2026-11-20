//! ROM resource catalog, ported from `famicom_gba_getImage` in
//! `src/famicom_emu.c`.
//!
//! Upstream table, verbatim (19 slots, `NULL` = slot present but unshipped):
//!
//! ```c
//! static char* names[] = {
//!     "cluclu", "usa_balloon",  "donkey", "usa_jr_math", "pinball",  "tennis", "usa_golf",
//!     NULL,     "usa_baseball", NULL,     "usa_donkey3", "donkeyjr", "soccer", "exbike",
//!     NULL,     "usa_icecl",    "mario",  "smario",      NULL,
//! };
//! ```
//!
//! Lookup rules, all reproduced here:
//!
//! * `rom_id > 19` returns `NULL` before the table is touched.
//! * `rom_id == 0` aliases slot 1 (`names[0]`).
//! * Any other id indexes `names[rom_id - 1]`.
//! * An empty slot returns `NULL`, so the caller sees "no resource" rather
//!   than a path that would fail to resolve at runtime.
//!
//! The only thing this module produces is the resource path string. No `.szs`
//! asset is read, checked in, or embedded.

use core::fmt;

/// Number of slots in the upstream `names[]` table.
pub const ROM_SLOT_COUNT: usize = 19;

/// Highest accepted `rom_id`; `19` itself is a reserved-but-empty slot.
pub const MAX_ROM_ID: u32 = ROM_SLOT_COUNT as u32;

/// Upstream `names[]`, in slot order. `None` marks a `NULL` entry.
pub const ROM_NAMES: [Option<&str>; ROM_SLOT_COUNT] = [
    Some("cluclu"),
    Some("usa_balloon"),
    Some("donkey"),
    Some("usa_jr_math"),
    Some("pinball"),
    Some("tennis"),
    Some("usa_golf"),
    None,
    Some("usa_baseball"),
    None,
    Some("usa_donkey3"),
    Some("donkeyjr"),
    Some("soccer"),
    Some("exbike"),
    None,
    Some("usa_icecl"),
    Some("mario"),
    Some("smario"),
    None,
];

/// Bytes reserved for a formatted resource path, including the sentinel byte.
///
/// The longest value any table entry can produce is 35 bytes: a 16-byte
/// prefix (`/FAMICOM/GBA/jb_`), an 11-byte name (`usa_balloon`), and the
/// 8-byte suffix (`.bin.szs`). The bound is padded rather than tight so a
/// future table entry cannot silently truncate a path.
pub const RESOURCE_PATH_CAPACITY: usize = 48;

const PATH_PREFIX: &str = "/FAMICOM/GBA/jb_";
const PATH_SUFFIX: &str = ".bin.szs";

/// Rejection reason for an out-of-range `rom_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RomIdError {
    /// `rom_id > 19`, which upstream rejects before indexing the table.
    OutOfRange {
        /// The rejected id.
        id: u32,
    },
}

impl fmt::Display for RomIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { id } => {
                write!(f, "famicom rom id {id} is greater than {MAX_ROM_ID}")
            }
        }
    }
}

impl core::error::Error for RomIdError {}

/// A `rom_id` that survived the `rom_id > 19` gate.
///
/// Constructing one is the whole validation step; `0` is legal and aliases
/// slot 1, exactly like `names[(rom_id == 0) ? 0 : rom_id - 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RomId(u32);

impl RomId {
    /// Id that aliases the first table slot.
    pub const ZERO: Self = Self(0);

    /// Validates `id` against the upstream `rom_id > 19` rejection.
    ///
    /// # Errors
    ///
    /// Returns [`RomIdError::OutOfRange`] when `id > 19`.
    pub const fn new(id: u32) -> Result<Self, RomIdError> {
        if id > MAX_ROM_ID {
            Err(RomIdError::OutOfRange { id })
        } else {
            Ok(Self(id))
        }
    }

    /// The raw id passed in by the caller.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Index into [`ROM_NAMES`] using the upstream `(id == 0) ? 0 : id - 1`
    /// rule.
    #[must_use]
    pub const fn slot_index(self) -> usize {
        if self.0 == 0 {
            0
        } else {
            (self.0 - 1) as usize
        }
    }

    /// Bundled ROM name for this id, or `None` when the upstream slot is
    /// `NULL`.
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        ROM_NAMES[self.slot_index()]
    }

    /// Resource path this id maps to, or `None` when the slot is empty.
    ///
    /// Equivalent to the upstream `sprintf(buf, "/FAMICOM/GBA/jb_%s.bin.szs",
    /// rom)` followed by `JC__JKRGetResource`, minus the resource lookup.
    #[must_use]
    pub fn resource_path(self) -> Option<ResourcePath> {
        let name = self.name()?;
        ResourcePath::for_name(name)
    }
}

/// Fixed-capacity, allocation-free resource path string.
///
/// Holds a NUL-terminated byte buffer so it can be handed to C-shaped host
/// code without a second conversion.
#[derive(Clone, Copy)]
pub struct ResourcePath {
    bytes: [u8; RESOURCE_PATH_CAPACITY],
    len: u8,
}

impl ResourcePath {
    /// Builds `"/FAMICOM/GBA/jb_<name>.bin.szs"`.
    ///
    /// Returns `None` if the result would not fit [`RESOURCE_PATH_CAPACITY`],
    /// which cannot happen for any name in [`ROM_NAMES`].
    #[must_use]
    pub fn for_name(name: &str) -> Option<Self> {
        let total = PATH_PREFIX.len() + name.len() + PATH_SUFFIX.len();
        // Keep one byte for the sentinel.
        if total + 1 > RESOURCE_PATH_CAPACITY {
            return None;
        }

        let mut bytes = [0u8; RESOURCE_PATH_CAPACITY];
        let mut at = 0;
        for part in [PATH_PREFIX, name, PATH_SUFFIX] {
            bytes[at..at + part.len()].copy_from_slice(part.as_bytes());
            at += part.len();
        }
        // `at <= RESOURCE_PATH_CAPACITY - 1`, so `at` fits u8 for any table
        // name; the cast is guarded by the capacity check above.
        Some(Self {
            bytes,
            len: u8::try_from(at).ok()?,
        })
    }

    /// Path as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Invariant: `for_name` copies only bytes of `&str` parts, so
        // `bytes[..len]` is always valid UTF-8. `len <= u8::MAX` is also
        // checked at construction, so the index is in bounds.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("resource path was built from str parts only")
    }

    /// NUL-terminated view suitable for a C string argument.
    #[must_use]
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len) + 1]
    }
}

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResourcePath").field(&self.as_str()).finish()
    }
}

impl PartialEq for ResourcePath {
    fn eq(&self, other: &Self) -> bool {
        self.bytes[..usize::from(self.len)] == other.bytes[..usize::from(other.len)]
    }
}

impl Eq for ResourcePath {}

impl PartialEq<str> for ResourcePath {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
