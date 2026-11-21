//! Bounded texture-pack indexing without decoding or retaining texture bytes.

use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Maximum number of filesystem entries inspected by one index operation.
pub const MAX_INDEXED_ENTRIES: usize = 65_536;

/// Texture formats named by the PC settings contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    /// Portable Network Graphics replacement.
    Png,
    /// DirectDraw Surface replacement.
    Dds,
}

/// One metadata-only replacement registration candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureRegistration {
    /// Normalized, slash-separated relative path used as the deterministic key.
    pub key: String,
    /// Original path below the indexed root.
    pub path: PathBuf,
    /// File format selected from the extension.
    pub format: TextureFormat,
    /// Size reported by the filesystem, never read into memory.
    pub byte_len: u64,
}

/// Counts for deterministic handling of non-registration cases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureIndexStats {
    /// Valid PNG/DDS files retained as registration metadata.
    pub registered: usize,
    /// Later files whose normalized relative key already won.
    pub duplicates: usize,
    /// Files that disappeared before metadata or header validation.
    pub missing: usize,
    /// PNG/DDS candidates with invalid fixed headers or unsafe path text.
    pub invalid: usize,
    /// Symlinks encountered and deliberately not followed.
    pub symlinks: usize,
    /// Other regular files ignored by the texture loader.
    pub unsupported: usize,
}

/// An indexed texture-pack directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureIndex {
    root: PathBuf,
    registrations: Vec<TextureRegistration>,
    stats: TextureIndexStats,
}

impl TextureIndex {
    /// Root path used for this index.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Valid registration metadata in deterministic key order.
    #[must_use]
    pub fn registrations(&self) -> &[TextureRegistration] {
        &self.registrations
    }

    /// Counts for registered, duplicate, missing, invalid, symlink, and
    /// unsupported entries.
    #[must_use]
    pub const fn stats(&self) -> TextureIndexStats {
        self.stats
    }

    /// Number of candidates that can be handed to a decoder/registrar.
    #[must_use]
    pub const fn registration_count(&self) -> usize {
        self.stats.registered
    }
}

/// Root/index failure before per-file metadata can be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureIndexError {
    /// The root itself is a symlink and was rejected.
    RootSymlink,
    /// The root is missing or not a directory.
    RootNotDirectory,
    /// Directory enumeration or metadata I/O failed.
    Io,
    /// The bounded entry budget was exceeded.
    EntryLimitExceeded,
}

/// Index valid PNG/DDS metadata under `root` without following symlinks.
///
/// Entries are traversed in sorted path order, so when normalized keys collide
/// the lexicographically first path wins. File bytes are limited to a 24-byte
/// PNG header or a 128-byte DDS header; texture payloads are never loaded.
pub fn index_texture_pack(root: &Path) -> Result<TextureIndex, TextureIndexError> {
    if has_symlink_component(root) {
        return Err(TextureIndexError::RootSymlink);
    }
    let root_metadata =
        fs::symlink_metadata(root).map_err(|_| TextureIndexError::RootNotDirectory)?;
    if root_metadata.file_type().is_symlink() {
        return Err(TextureIndexError::RootSymlink);
    }
    if !root_metadata.is_dir() {
        return Err(TextureIndexError::RootNotDirectory);
    }

    let mut state = IndexState {
        root: root.to_path_buf(),
        registrations: Vec::new(),
        keys: HashSet::new(),
        stats: TextureIndexStats::default(),
        examined: 0,
    };
    visit_directory(root, Path::new(""), &mut state)?;
    state
        .registrations
        .sort_by(|left, right| left.key.cmp(&right.key));
    Ok(TextureIndex {
        root: state.root,
        registrations: state.registrations,
        stats: state.stats,
    })
}

fn has_symlink_component(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

struct IndexState {
    root: PathBuf,
    registrations: Vec<TextureRegistration>,
    keys: HashSet<String>,
    stats: TextureIndexStats,
    examined: usize,
}

fn visit_directory(
    absolute: &Path,
    relative: &Path,
    state: &mut IndexState,
) -> Result<(), TextureIndexError> {
    let mut children = Vec::new();
    let entries = fs::read_dir(absolute).map_err(|_| TextureIndexError::Io)?;
    for entry in entries {
        if children.len() >= MAX_INDEXED_ENTRIES {
            return Err(TextureIndexError::EntryLimitExceeded);
        }
        let entry = entry.map_err(|_| TextureIndexError::Io)?;
        children.push(entry.path());
    }
    children.sort_by_key(|left| path_sort_key(left));

    for child in children {
        state.examined = state.examined.saturating_add(1);
        if state.examined > MAX_INDEXED_ENTRIES {
            return Err(TextureIndexError::EntryLimitExceeded);
        }
        let child_relative = relative.join(child.file_name().ok_or(TextureIndexError::Io)?);
        let metadata = match fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                state.stats.missing += 1;
                continue;
            }
            Err(_) => return Err(TextureIndexError::Io),
        };
        if metadata.file_type().is_symlink() {
            state.stats.symlinks += 1;
            continue;
        }
        if metadata.is_dir() {
            visit_directory(&child, &child_relative, state)?;
            continue;
        }
        if !metadata.is_file() {
            state.stats.invalid += 1;
            continue;
        }

        let Some(format) = format_for(&child) else {
            state.stats.unsupported += 1;
            continue;
        };
        let Some(key) = normalized_key(&child_relative) else {
            state.stats.invalid += 1;
            continue;
        };
        if state.keys.contains(&key) {
            state.stats.duplicates += 1;
            continue;
        }
        if !valid_texture_header(&child, format, &metadata) {
            state.stats.invalid += 1;
            continue;
        }
        state.keys.insert(key.clone());
        state.registrations.push(TextureRegistration {
            key,
            path: child,
            format,
            byte_len: metadata.len(),
        });
        state.stats.registered += 1;
    }
    Ok(())
}

fn path_sort_key(path: &Path) -> (String, String) {
    let exact = path.to_string_lossy().into_owned();
    (exact.to_ascii_lowercase(), exact)
}

fn normalized_key(path: &Path) -> Option<String> {
    let value = path.to_str()?.replace('\\', "/");
    if value.is_empty() || value.starts_with('/') || value.split('/').any(|part| part == "..") {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn format_for(path: &Path) -> Option<TextureFormat> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("png") => Some(TextureFormat::Png),
        Some(extension) if extension.eq_ignore_ascii_case("dds") => Some(TextureFormat::Dds),
        _ => None,
    }
}

fn valid_texture_header(path: &Path, format: TextureFormat, metadata: &Metadata) -> bool {
    let required: usize = match format {
        TextureFormat::Png => 24,
        TextureFormat::Dds => 128,
    };
    if metadata.len() < required as u64 {
        return false;
    }
    let Ok(current_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if current_metadata.file_type().is_symlink() || !current_metadata.is_file() {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 128];
    if file.read_exact(&mut header[..required]).is_err() {
        return false;
    }
    match format {
        TextureFormat::Png => {
            header[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
                && u32::from_be_bytes(header[8..12].try_into().unwrap()) == 13
                && &header[12..16] == b"IHDR"
                && u32::from_be_bytes(header[16..20].try_into().unwrap()) != 0
                && u32::from_be_bytes(header[20..24].try_into().unwrap()) != 0
        }
        TextureFormat::Dds => {
            &header[..4] == b"DDS "
                && u32::from_le_bytes(header[4..8].try_into().unwrap()) == 124
                && u32::from_le_bytes(header[12..16].try_into().unwrap()) != 0
                && u32::from_le_bytes(header[16..20].try_into().unwrap()) != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nookery-texture-index-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create temporary test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn png() -> [u8; 24] {
        let mut value = [0u8; 24];
        value[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        value[8..12].copy_from_slice(&13u32.to_be_bytes());
        value[12..16].copy_from_slice(b"IHDR");
        value[16..20].copy_from_slice(&1u32.to_be_bytes());
        value[20..24].copy_from_slice(&1u32.to_be_bytes());
        value
    }

    #[test]
    fn indexes_valid_files_and_counts_invalid_and_unsupported_entries() {
        let root = TempDir::new("basic");
        fs::write(root.path().join("a.png"), png()).expect("write PNG fixture");
        fs::write(root.path().join("bad.png"), b"bad").expect("write invalid fixture");
        fs::write(root.path().join("readme.txt"), b"ignored").expect("write ignored fixture");
        let index = index_texture_pack(root.path()).expect("index texture pack");
        assert_eq!(index.registration_count(), 1);
        assert_eq!(index.stats().invalid, 1);
        assert_eq!(index.stats().unsupported, 1);
        assert_eq!(index.registrations()[0].key, "a.png");
    }

    #[test]
    fn duplicate_normalized_paths_keep_sorted_first_entry() {
        let root = TempDir::new("duplicate");
        fs::write(root.path().join("TEX.PNG"), png()).expect("write first fixture");
        fs::write(root.path().join("tex.png"), png()).expect("write duplicate fixture");
        let index = index_texture_pack(root.path()).expect("index texture pack");
        assert_eq!(index.registration_count(), 1);
        assert_eq!(index.stats().duplicates, 1);
        assert_eq!(
            index.registrations()[0].path.file_name().unwrap(),
            "TEX.PNG"
        );
    }

    #[test]
    fn missing_root_is_reported_without_scanning() {
        let root = TempDir::new("missing");
        let missing = root.path().join("texture_pack");
        assert_eq!(
            index_texture_pack(&missing),
            Err(TextureIndexError::RootNotDirectory)
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_entries_are_counted_and_not_followed() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new("symlink");
        let target = TempDir::new("symlink-target");
        fs::write(target.path().join("hidden.png"), png()).expect("write target fixture");
        symlink(
            target.path().join("hidden.png"),
            root.path().join("hidden.png"),
        )
        .expect("create file symlink");
        let index = index_texture_pack(root.path()).expect("index texture pack");
        assert_eq!(index.registration_count(), 0);
        assert_eq!(index.stats().symlinks, 1);
    }
}
