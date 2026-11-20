//! Owned handle state and migration-plan storage.
//!
//! The handle is the only mutable state behind the ABI.  It is created with an
//! explicit allocation (not `Box::new`) so an allocation failure is reported as
//! [`STATUS_OUT_OF_MEMORY`] instead of aborting the process — `panic = "abort"`
//! means an abort here would be an undeclared process exit, not a status.

use std::alloc::{Layout, alloc, dealloc};
use std::path::{Path, PathBuf};

use nookery_core::{DataLayout, MigrationPlan, Status};

use crate::abi::{NookeryDataBuffer, RawStatus, STATUS_OK, STATUS_OUT_OF_MEMORY, validate_root};

/// The state a C caller reaches through `NookeryDataHandle *`.
#[derive(Debug)]
pub struct DataHandle {
    layout: DataLayout,
    base_root: Option<PathBuf>,
    initialized: bool,
    last_status: RawStatus,
    last_message: String,
}

impl DataHandle {
    /// Build an uninitialized handle for `root` and an optional base directory.
    ///
    /// Both inputs are validated here: they must be absolute and free of `..`.
    pub fn new(root: &str, base_root: Option<&str>) -> Result<Self, RawStatus> {
        let root = Path::new(root);
        validate_root(root)?;
        let base_root = match base_root {
            Some(text) => {
                let path = Path::new(text);
                validate_root(path)?;
                Some(path.to_path_buf())
            }
            None => None,
        };
        Ok(Self {
            layout: DataLayout::new(root),
            base_root,
            initialized: false,
            last_status: STATUS_OK,
            last_message: String::new(),
        })
    }

    pub fn layout(&self) -> &DataLayout {
        &self.layout
    }

    pub fn base_root(&self) -> Option<&Path> {
        self.base_root.as_deref()
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn mark_initialized(&mut self, layout: DataLayout) {
        self.layout = layout;
        self.initialized = true;
    }

    pub fn last_status(&self) -> RawStatus {
        self.last_status
    }

    /// Record the outcome of the most recent operation, mirroring upstream's
    /// single-`Status` model (each call replaces the previous diagnostic).
    pub fn record(&mut self, status: RawStatus) {
        self.last_status = status;
        self.last_message.clear();
    }

    /// Record a core status, rendering its upstream-shaped message eagerly.
    pub fn record_status(&mut self, status: &Status) {
        self.last_status = crate::abi::status_of(status.code);
        self.last_message.clear();
        self.last_message.push_str(&status.to_string());
    }

    pub fn write_last_message(&self, out: &mut NookeryDataBuffer) -> RawStatus {
        out.write(self.last_message.as_bytes())
    }

    /// Allocate a handle on the C heap.  Returns null on allocation failure.
    pub fn into_raw(handle: DataHandle) -> *mut Self {
        let layout = Layout::new::<Self>();
        // SAFETY: `layout` has the size/alignment of `Self`; a non-null result
        // is a fresh, unaliased, correctly aligned allocation.
        let raw = unsafe { alloc(layout) }.cast::<Self>();
        if raw.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: `raw` is valid for writes of `Self` and uninitialized, so
        // moving the value in is not a double-drop.
        unsafe { raw.write(handle) };
        raw
    }

    /// # Safety
    /// `raw` must come from [`DataHandle::into_raw`], be non-null, and be handed
    /// over exactly once.
    pub unsafe fn from_raw(raw: *mut Self) -> Self {
        // SAFETY: caller guarantees the pointer came from `into_raw` and is
        // still valid; `read` moves the value out without dropping it twice.
        unsafe { raw.read() }
    }

    /// # Safety
    /// As [`DataHandle::from_raw`]; use [`destroy`] for the normal path.
    pub unsafe fn free_raw(raw: *mut Self) {
        if raw.is_null() {
            return;
        }
        // SAFETY: caller guarantees a live `into_raw` allocation.  Drop runs
        // the contained `String`/`PathBuf` destructors before the backing bytes
        // are released.
        unsafe {
            std::ptr::drop_in_place(raw);
            dealloc(raw.cast::<u8>(), Layout::new::<Self>());
        }
    }
}

/// Free a handle, accepting null.
///
/// # Safety
/// `raw` must be null or an `into_raw` pointer not yet freed.
pub unsafe fn destroy(raw: *mut DataHandle) {
    // SAFETY: forwarded contract.
    unsafe { DataHandle::free_raw(raw) };
}

/// The opaque migration plan handed to C.
#[derive(Debug, Default)]
pub struct DataPlan {
    directories: Vec<PathBuf>,
    files: Vec<PathBuf>,
}

impl DataPlan {
    /// Copy a core plan into owned storage, reserving exactly.
    ///
    /// `try_reserve` is used so exhaustion surfaces as [`STATUS_OUT_OF_MEMORY`]
    /// rather than an abort.
    pub fn from_core(plan: MigrationPlan) -> Result<Self, RawStatus> {
        let mut directories = Vec::new();
        directories
            .try_reserve_exact(plan.directories.len())
            .map_err(|_| STATUS_OUT_OF_MEMORY)?;
        directories.extend(plan.directories);

        let mut files = Vec::new();
        files
            .try_reserve_exact(plan.files.len())
            .map_err(|_| STATUS_OUT_OF_MEMORY)?;
        files.extend(plan.files);

        Ok(Self { directories, files })
    }

    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn directory(&self, index: usize) -> Option<&Path> {
        self.directories.get(index).map(PathBuf::as_path)
    }

    pub fn file(&self, index: usize) -> Option<&Path> {
        self.files.get(index).map(PathBuf::as_path)
    }

    pub fn into_raw(plan: Self) -> *mut Self {
        let layout = Layout::new::<Self>();
        // SAFETY: identical reasoning to `DataHandle::into_raw`.
        let raw = unsafe { alloc(layout) }.cast::<Self>();
        if raw.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: valid, uninitialized storage of the right type.
        unsafe { raw.write(plan) };
        raw
    }

    /// # Safety
    /// `raw` must be null or an `into_raw` pointer not yet freed.
    pub unsafe fn free_raw(raw: *mut Self) {
        if raw.is_null() {
            return;
        }
        // SAFETY: forwarded contract.
        unsafe {
            std::ptr::drop_in_place(raw);
            dealloc(raw.cast::<u8>(), Layout::new::<Self>());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_validates_both_roots() {
        assert!(DataHandle::new("/data", None).is_ok());
        assert!(DataHandle::new("/data", Some("/opt/nookery")).is_ok());
        assert_eq!(
            DataHandle::new("data", None).unwrap_err(),
            crate::abi::STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            DataHandle::new("/data", Some("../base")).unwrap_err(),
            crate::abi::STATUS_INVALID_ARGUMENT
        );
    }

    #[test]
    fn handle_starts_uninitialized_with_a_success_status() {
        let handle = DataHandle::new("/data", None).expect("handle");
        assert!(!handle.is_initialized());
        assert_eq!(handle.last_status(), STATUS_OK);
        assert_eq!(handle.layout().root(), Path::new("/data"));
        assert_eq!(handle.base_root(), None);
    }
}
