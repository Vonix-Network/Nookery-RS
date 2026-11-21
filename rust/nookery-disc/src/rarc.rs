//! Read-only big-endian RARC/ResourceArchive parsing and access.
//!
//! The parser follows `tools/pyjkernel/jkrarchive.py` and the PC validation
//! path in `JKRArchive::convertBigEndianRarcMetadata`. RARC metadata offsets
//! are relative to the info block; file-data offsets are relative to the file
//! data block. No filesystem access is performed by this module.
//!
//! A Yaz0 wrapper is deliberately an explicit operation:
//! [`RarcArchive::parse_yaz0`] reuses [`crate::yaz0`] and then parses the
//! resulting RARC bytes. [`RarcArchive::parse`] accepts only an already
//! unwrapped RARC buffer.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use crate::image::{DiscImage, ImageError};
use crate::yaz0::{self, Yaz0Error};

/// RARC header size in bytes.
pub const HEADER_SIZE: usize = 0x20;
/// RARC info block size used by the PC port's `SArcDataInfo`.
pub const INFO_SIZE: usize = 0x20;
/// RARC node size in bytes.
pub const NODE_SIZE: usize = 0x10;
/// RARC directory-entry size in bytes.
pub const ENTRY_SIZE: usize = 0x14;
/// Big-endian RARC signature.
pub const MAGIC: [u8; 4] = *b"RARC";

/// Directory-entry attribute bits.
pub mod attributes {
    /// Entry is a file.
    pub const FILE: u8 = 0x01;
    /// Entry is a directory.
    pub const FOLDER: u8 = 0x02;
    /// Entry payload is compressed according to JKR metadata.
    pub const COMPRESSED: u8 = 0x04;
    /// File is preloaded into main RAM.
    pub const PRELOAD_MRAM: u8 = 0x10;
    /// File is preloaded into ARAM.
    pub const PRELOAD_ARAM: u8 = 0x20;
    /// File is read from DVD on demand.
    pub const PRELOAD_DVD: u8 = 0x40;
    /// JKR's SZS/Yay0 selector bit.
    pub const USE_SZS: u8 = 0x80;
}

/// Why an RARC buffer was rejected or an access operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RarcError {
    /// The input ended before a required structure or declared archive range.
    Truncated {
        /// Minimum bytes required.
        needed: usize,
        /// Bytes supplied by the caller.
        actual: usize,
    },
    /// The first four bytes were not `RARC`.
    WrongMagic {
        /// Four bytes found at the input start.
        found: [u8; 4],
    },
    /// A header field violates the RARC header relationship.
    InvalidHeader(&'static str),
    /// A table or metadata range lies outside the info block.
    OutOfRange {
        /// Table being checked.
        table: &'static str,
        /// Relative table offset.
        offset: u64,
        /// Table size.
        size: u64,
        /// Metadata limit.
        limit: u64,
    },
    /// A name offset does not identify a NUL-terminated ASCII string.
    InvalidNameOffset {
        /// String-bearing structure.
        table: &'static str,
        /// Structure index.
        index: usize,
        /// Relative name offset.
        offset: u32,
    },
    /// A node's entry span is outside the directory-entry table.
    InvalidEntryRange {
        /// Node index.
        node: usize,
        /// First entry index.
        first: u32,
        /// Entry count.
        count: u16,
        /// Total entry count.
        total: u32,
    },
    /// A file payload offset and length do not fit within file data.
    DataRangeOverflow {
        /// Directory-entry index.
        entry: usize,
        /// Relative payload offset.
        offset: u32,
        /// Payload length.
        size: u32,
        /// Declared file-data length.
        data_size: u32,
    },
    /// A directory entry refers to a node that does not exist.
    InvalidNodeIndex {
        /// Directory-entry index.
        entry: usize,
        /// Referenced node index.
        node: u32,
        /// Total node count.
        total: u32,
    },
    /// An entry has neither a usable file nor a usable folder/shortcut form.
    InvalidEntryKind {
        /// Directory-entry index.
        entry: usize,
        /// High-byte entry attributes.
        flags: u8,
        /// Entry name.
        name: String,
    },
    /// A file has no preload bit, which the Python archive oracle rejects.
    MissingPreloadType {
        /// Directory-entry index.
        entry: usize,
        /// File name.
        name: String,
    },
    /// A string contains a non-ASCII byte.
    InvalidNameEncoding {
        /// String-bearing structure.
        table: &'static str,
        /// Structure index.
        index: usize,
    },
    /// A reachable directory graph contains a cycle.
    DirectoryCycle {
        /// Cyclic node index.
        node: usize,
    },
    /// A requested path does not exist.
    NotFound {
        /// Requested path.
        path: String,
    },
    /// A requested path names a file where a directory is required.
    NotDirectory {
        /// Requested path.
        path: String,
    },
    /// The explicit Yaz0 wrapper failed.
    Yaz0(Yaz0Error),
    /// The logical disc backend failed while handing off archive bytes.
    Image(ImageError),
    /// An archive range overflows or lies beyond the logical disc.
    ImageRange {
        /// Logical archive start offset.
        offset: u64,
        /// Declared archive length.
        length: u64,
        /// Logical image length.
        image_size: u64,
    },
    /// The archive declares more bytes than the caller permits the handoff to
    /// allocate.
    ImageLimit {
        /// Declared archive length.
        declared: u32,
        /// Caller-provided allocation limit.
        limit: usize,
    },
}

impl fmt::Display for RarcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { needed, actual } => {
                write!(f, "truncated RARC: need {needed} bytes, have {actual}")
            }
            Self::WrongMagic { found } => write!(f, "wrong RARC magic: {found:02x?}"),
            Self::InvalidHeader(reason) => write!(f, "invalid RARC header: {reason}"),
            Self::OutOfRange {
                table,
                offset,
                size,
                limit,
            } => write!(f, "RARC {table} range {offset}+{size} exceeds {limit}"),
            Self::InvalidNameOffset {
                table,
                index,
                offset,
            } => write!(f, "invalid {table} name offset {offset} at {index}"),
            Self::InvalidEntryRange {
                node,
                first,
                count,
                total,
            } => write!(
                f,
                "invalid RARC node {node} entry range {first}+{count} of {total}"
            ),
            Self::DataRangeOverflow {
                entry,
                offset,
                size,
                data_size,
            } => write!(
                f,
                "RARC file {entry} range {offset}+{size} exceeds {data_size}"
            ),
            Self::InvalidNodeIndex { entry, node, total } => {
                write!(f, "RARC entry {entry} node index {node} exceeds {total}")
            }
            Self::InvalidEntryKind { entry, flags, name } => {
                write!(
                    f,
                    "invalid RARC entry {entry}: flags {flags:02x}, name {name:?}"
                )
            }
            Self::MissingPreloadType { entry, name } => {
                write!(f, "RARC file {entry} ({name:?}) has no preload type")
            }
            Self::InvalidNameEncoding { table, index } => {
                write!(f, "non-ASCII name in {table} at {index}")
            }
            Self::DirectoryCycle { node } => write!(f, "RARC directory cycle at node {node}"),
            Self::NotFound { path } => write!(f, "RARC path does not exist: {path}"),
            Self::NotDirectory { path } => write!(f, "RARC path is not a directory: {path}"),
            Self::Yaz0(error) => write!(f, "Yaz0 wrapper failed: {error:?}"),
            Self::Image(error) => write!(f, "RARC image handoff failed: {error:?}"),
            Self::ImageRange {
                offset,
                length,
                image_size,
            } => write!(
                f,
                "RARC image range {offset}+{length} exceeds logical size {image_size}"
            ),
            Self::ImageLimit { declared, limit } => {
                write!(
                    f,
                    "RARC archive length {declared} exceeds handoff limit {limit}"
                )
            }
        }
    }
}

impl std::error::Error for RarcError {}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    first_entry: usize,
    entry_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Folder(usize),
    Shortcut,
}

#[derive(Debug, Clone)]
struct Entry {
    id: u16,
    hash: u16,
    flags: u8,
    data_offset: u32,
    data_size: u32,
    name: String,
    kind: EntryKind,
}

/// A parsed, owned, read-only RARC archive.
#[derive(Debug, Clone)]
pub struct RarcArchive {
    bytes: Vec<u8>,
    data_range: Range<usize>,
    root_name: String,
    nodes: Vec<Node>,
    entries: Vec<Entry>,
    directories: BTreeMap<String, usize>,
    files: BTreeMap<String, usize>,
    all_files: Vec<(String, usize)>,
}

/// A borrowed file view into a [`RarcArchive`].
#[derive(Debug, Clone, Copy)]
pub struct RarcFile<'a> {
    archive: &'a RarcArchive,
    entry: usize,
    path: &'a str,
}

impl RarcArchive {
    /// Parse an already-unwrapped big-endian RARC buffer.
    ///
    /// This method never consults the filesystem and does not auto-detect or
    /// decompress Yaz0. Use [`Self::parse_yaz0`] when the wrapper is known.
    pub fn parse(input: &[u8]) -> Result<Self, RarcError> {
        if input.len() < HEADER_SIZE {
            return Err(RarcError::Truncated {
                needed: HEADER_SIZE,
                actual: input.len(),
            });
        }
        let found = [input[0], input[1], input[2], input[3]];
        if found != MAGIC {
            return Err(RarcError::WrongMagic { found });
        }

        let file_length = read_u32(input, 0x04) as usize;
        let header_length = read_u32(input, 0x08) as usize;
        let info_length = read_u32(input, 0x0c) as usize;
        let file_data_length = read_u32(input, 0x10);

        if header_length < HEADER_SIZE {
            return Err(RarcError::InvalidHeader("header length is below 0x20"));
        }
        if file_length < header_length {
            return Err(RarcError::InvalidHeader(
                "file length is below header length",
            ));
        }
        if file_length > input.len() {
            return Err(RarcError::Truncated {
                needed: file_length,
                actual: input.len(),
            });
        }
        let metadata_end = header_length
            .checked_add(info_length)
            .ok_or(RarcError::InvalidHeader("metadata offset overflows"))?;
        if metadata_end > file_length {
            return Err(RarcError::OutOfRange {
                table: "metadata",
                offset: header_length as u64,
                size: info_length as u64,
                limit: file_length as u64,
            });
        }
        let file_data_end = metadata_end
            .checked_add(file_data_length as usize)
            .ok_or(RarcError::InvalidHeader("file data offset overflows"))?;
        if file_data_end > file_length {
            return Err(RarcError::OutOfRange {
                table: "file data",
                offset: metadata_end as u64,
                size: file_data_length as u64,
                limit: file_length as u64,
            });
        }
        if info_length < INFO_SIZE {
            return Err(RarcError::Truncated {
                needed: header_length + INFO_SIZE,
                actual: metadata_end,
            });
        }

        let metadata = &input[header_length..metadata_end];
        let node_count = read_u32(metadata, 0x00);
        let node_offset = read_u32(metadata, 0x04);
        let entry_count = read_u32(metadata, 0x08);
        let entry_offset = read_u32(metadata, 0x0c);
        let string_length = read_u32(metadata, 0x10);
        let string_offset = read_u32(metadata, 0x14);

        let nodes_range =
            checked_table_range("nodes", node_offset, node_count, NODE_SIZE, metadata.len())?;
        let entries_range = checked_table_range(
            "entries",
            entry_offset,
            entry_count,
            ENTRY_SIZE,
            metadata.len(),
        )?;
        let strings_range =
            checked_table_range("strings", string_offset, string_length, 1, metadata.len())?;
        if node_count == 0 || string_length == 0 {
            return Err(RarcError::InvalidHeader("archive has no nodes or strings"));
        }

        let strings = &metadata[strings_range];
        let mut nodes = Vec::with_capacity(node_count as usize);
        for index in 0..node_count as usize {
            let off = nodes_range.start + index * NODE_SIZE;
            let name_offset = read_u32(metadata, off + 0x04);
            let name = read_name(strings, name_offset, "node", index)?;
            let first = read_u32(metadata, off + 0x0c);
            let count = read_u16(metadata, off + 0x0a) as usize;
            if first > entry_count || count > entry_count as usize - first as usize {
                return Err(RarcError::InvalidEntryRange {
                    node: index,
                    first,
                    count: count as u16,
                    total: entry_count,
                });
            }
            nodes.push(Node {
                name,
                first_entry: first as usize,
                entry_count: count,
            });
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        for index in 0..entry_count as usize {
            let off = entries_range.start + index * ENTRY_SIZE;
            let id = read_u16(metadata, off);
            let hash = read_u16(metadata, off + 0x02);
            let packed_flags = read_u32(metadata, off + 0x04);
            let flags = (packed_flags >> 24) as u8;
            let name_offset = packed_flags & 0x00ff_ffff;
            let data_offset = read_u32(metadata, off + 0x08);
            let data_size = read_u32(metadata, off + 0x0c);
            let name = read_name(strings, name_offset, "entry", index)?;

            let kind = if flags & attributes::FOLDER != 0 && data_offset != u32::MAX {
                if data_offset >= node_count {
                    return Err(RarcError::InvalidNodeIndex {
                        entry: index,
                        node: data_offset,
                        total: node_count,
                    });
                }
                if name == "." || name == ".." {
                    EntryKind::Shortcut
                } else {
                    EntryKind::Folder(data_offset as usize)
                }
            } else if flags & attributes::FILE != 0 {
                if flags
                    & (attributes::PRELOAD_MRAM
                        | attributes::PRELOAD_ARAM
                        | attributes::PRELOAD_DVD)
                    == 0
                {
                    return Err(RarcError::MissingPreloadType { entry: index, name });
                }
                if data_offset > file_data_length || data_size > file_data_length - data_offset {
                    return Err(RarcError::DataRangeOverflow {
                        entry: index,
                        offset: data_offset,
                        size: data_size,
                        data_size: file_data_length,
                    });
                }
                EntryKind::File
            } else if flags & attributes::FOLDER != 0 && data_offset == u32::MAX && name == ".." {
                EntryKind::Shortcut
            } else {
                return Err(RarcError::InvalidEntryKind {
                    entry: index,
                    flags,
                    name,
                });
            };

            entries.push(Entry {
                id,
                hash,
                flags,
                data_offset,
                data_size,
                name,
                kind,
            });
        }

        let root_name = nodes[0].name.clone();
        let mut archive = Self {
            bytes: input[..file_length].to_vec(),
            data_range: metadata_end..file_data_end,
            root_name,
            nodes,
            entries,
            directories: BTreeMap::new(),
            files: BTreeMap::new(),
            all_files: Vec::new(),
        };
        archive.initialize_lookup()?;
        Ok(archive)
    }

    /// Reads one bounded, already-unwrapped RARC from a logical disc image.
    ///
    /// The archive's declared file length is checked against both
    /// `max_size` and the image's logical size before allocation. This is the
    /// content handoff equivalent of Aurora's DVD file reads and works for
    /// raw ISO as well as sparse CISO images without requiring bundled data.
    pub fn parse_from_image(
        image: &mut dyn DiscImage,
        offset: u64,
        max_size: usize,
    ) -> Result<Self, RarcError> {
        let image_size = image.logical_size();
        let header_end = offset
            .checked_add(HEADER_SIZE as u64)
            .ok_or(RarcError::ImageRange {
                offset,
                length: HEADER_SIZE as u64,
                image_size,
            })?;
        if header_end > image_size {
            return Err(RarcError::Truncated {
                needed: HEADER_SIZE,
                actual: image_size.saturating_sub(offset) as usize,
            });
        }

        let mut header = [0u8; HEADER_SIZE];
        read_image_exact(image, offset, &mut header)?;
        if header[..4] != MAGIC {
            return Self::parse(&header);
        }
        let declared = read_u32(&header, 0x04);
        let declared_usize = usize::try_from(declared).map_err(|_| RarcError::ImageLimit {
            declared,
            limit: max_size,
        })?;
        if declared_usize > max_size {
            return Err(RarcError::ImageLimit {
                declared,
                limit: max_size,
            });
        }
        if declared_usize < HEADER_SIZE {
            return Self::parse(&header);
        }
        let archive_end = offset
            .checked_add(u64::from(declared))
            .ok_or(RarcError::ImageRange {
                offset,
                length: u64::from(declared),
                image_size,
            })?;
        if archive_end > image_size {
            return Err(RarcError::ImageRange {
                offset,
                length: u64::from(declared),
                image_size,
            });
        }
        let mut bytes = vec![0u8; declared_usize];
        bytes[..HEADER_SIZE].copy_from_slice(&header);
        read_image_exact(
            image,
            offset + HEADER_SIZE as u64,
            &mut bytes[HEADER_SIZE..],
        )?;
        Self::parse(&bytes)
    }

    /// Decode an explicitly supplied Yaz0-wrapped RARC buffer and parse it.
    pub fn parse_yaz0(input: &[u8], limit: usize) -> Result<Self, RarcError> {
        let decoded = yaz0::decode(input, limit).map_err(RarcError::Yaz0)?;
        Self::parse(&decoded)
    }

    /// Alias for [`Self::parse`].
    pub fn from_bytes(input: &[u8]) -> Result<Self, RarcError> {
        Self::parse(input)
    }

    /// Alias for [`Self::parse_yaz0`].
    pub fn from_yaz0(input: &[u8], limit: usize) -> Result<Self, RarcError> {
        Self::parse_yaz0(input, limit)
    }

    /// The archive root node's original case-sensitive name.
    #[must_use]
    pub fn root_name(&self) -> &str {
        &self.root_name
    }

    /// Return whether a root-qualified file or directory path exists.
    ///
    /// The Python oracle accepts `root/path` without a leading slash. For
    /// convenience, `/root/path`, `/path`, and `/` are also accepted using the
    /// corresponding JKR absolute-root forms.
    #[must_use]
    pub fn directory_exists(&self, path: &str) -> bool {
        let key = self.lookup_key(path);
        self.directories.contains_key(&key) || self.files.contains_key(&key)
    }

    /// List direct file children in archive directory order.
    pub fn list_files(&self, path: &str) -> Result<Vec<RarcFile<'_>>, RarcError> {
        let node = self.directory_node(path)?;
        let end = self.nodes[node].first_entry + self.nodes[node].entry_count;
        Ok((self.nodes[node].first_entry..end)
            .filter(|index| self.entries[*index].kind == EntryKind::File)
            .map(|entry| RarcFile {
                archive: self,
                entry,
                path: self
                    .file_path_for_entry(entry)
                    .expect("reachable file has a path"),
            })
            .collect())
    }

    /// List direct folder names in archive directory order.
    pub fn list_folders(&self, path: &str) -> Result<Vec<&str>, RarcError> {
        let node = self.directory_node(path)?;
        let end = self.nodes[node].first_entry + self.nodes[node].entry_count;
        Ok((self.nodes[node].first_entry..end)
            .filter_map(|index| match self.entries[index].kind {
                EntryKind::Folder(_) => Some(self.entries[index].name.as_str()),
                EntryKind::File | EntryKind::Shortcut => None,
            })
            .collect())
    }

    /// Find the file selected by the case-insensitive archive lookup map.
    /// Duplicate names intentionally select the last entry inserted by the
    /// oracle-compatible depth-first traversal.
    pub fn get_file(&self, path: &str) -> Result<RarcFile<'_>, RarcError> {
        let key = self.lookup_key(path);
        let entry = *self.files.get(&key).ok_or_else(|| RarcError::NotFound {
            path: path.to_owned(),
        })?;
        let file_path = self
            .file_path_for_entry(entry)
            .expect("lookup file has a path");
        Ok(RarcFile {
            archive: self,
            entry,
            path: file_path,
        })
    }

    /// Return all reachable files in traversal order, including duplicate
    /// entries that a lookup map would collapse.
    pub fn files(&self) -> Vec<RarcFile<'_>> {
        self.all_files
            .iter()
            .map(|(path, entry)| RarcFile {
                archive: self,
                entry: *entry,
                path,
            })
            .collect()
    }

    /// Return the number of raw directory entries, including shortcuts.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Return the number of parsed nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn initialize_lookup(&mut self) -> Result<(), RarcError> {
        let root_path = self.root_name.clone();
        self.directories.insert(root_path.to_ascii_lowercase(), 0);
        let mut active = vec![false; self.nodes.len()];
        active[0] = true;
        let mut stack = vec![(0usize, root_path, 0usize)];

        while let Some((node, path, next)) = stack.last_mut() {
            if *next == self.nodes[*node].entry_count {
                active[*node] = false;
                stack.pop();
                continue;
            }
            let entry_index = self.nodes[*node].first_entry + *next;
            *next += 1;
            let (kind, name) = {
                let entry = &self.entries[entry_index];
                (entry.kind, entry.name.clone())
            };
            match kind {
                EntryKind::File => {
                    let file_path = join_path(path, &name);
                    self.files
                        .insert(file_path.to_ascii_lowercase(), entry_index);
                    self.all_files.push((file_path, entry_index));
                }
                EntryKind::Folder(child) => {
                    let child_path = join_path(path, &name);
                    self.directories
                        .insert(child_path.to_ascii_lowercase(), child);
                    if active[child] {
                        return Err(RarcError::DirectoryCycle { node: child });
                    }
                    active[child] = true;
                    stack.push((child, child_path, 0));
                }
                EntryKind::Shortcut => {}
            }
        }
        Ok(())
    }

    fn lookup_key(&self, path: &str) -> String {
        if path == "/" {
            return self.root_name.to_ascii_lowercase();
        }
        let path = path.strip_prefix('/').unwrap_or(path);
        let root = self.root_name.to_ascii_lowercase();
        let lower = path.to_ascii_lowercase();
        if lower == root || lower.starts_with(&(root.clone() + "/")) {
            lower
        } else {
            format!("{root}/{lower}")
        }
    }

    fn directory_node(&self, path: &str) -> Result<usize, RarcError> {
        let key = self.lookup_key(path);
        if let Some(node) = self.directories.get(&key) {
            return Ok(*node);
        }
        if self.files.contains_key(&key) {
            return Err(RarcError::NotDirectory {
                path: path.to_owned(),
            });
        }
        Err(RarcError::NotFound {
            path: path.to_owned(),
        })
    }

    fn file_path_for_entry(&self, entry: usize) -> Option<&str> {
        self.all_files
            .iter()
            .find_map(|(path, index)| (*index == entry).then_some(path.as_str()))
    }
}

impl<'a> RarcFile<'a> {
    /// The original file name, without parent directories.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.archive.entries[self.entry].name
    }

    /// The root-qualified, original-case path used by archive traversal.
    #[must_use]
    pub fn path(&self) -> &str {
        self.path
    }

    /// The original directory-entry file ID.
    #[must_use]
    pub fn id(&self) -> u16 {
        self.archive.entries[self.entry].id
    }

    /// The original directory-entry name hash.
    #[must_use]
    pub fn name_hash(&self) -> u16 {
        self.archive.entries[self.entry].hash
    }

    /// The high-byte JKR attribute flags.
    #[must_use]
    pub fn attributes(&self) -> u8 {
        self.archive.entries[self.entry].flags
    }

    /// The raw stored file bytes, without per-file decompression.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        let entry = &self.archive.entries[self.entry];
        let start = self.archive.data_range.start + entry.data_offset as usize;
        &self.archive.bytes[start..start + entry.data_size as usize]
    }

    /// The raw stored file size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.archive.entries[self.entry].data_size as usize
    }

    /// Decode this file as an explicitly supplied Yaz0 payload.
    pub fn decode_yaz0(&self, limit: usize) -> Result<Vec<u8>, RarcError> {
        yaz0::decode(self.data(), limit).map_err(RarcError::Yaz0)
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_image_exact(
    image: &mut dyn DiscImage,
    offset: u64,
    output: &mut [u8],
) -> Result<(), RarcError> {
    let mut filled = 0;
    while filled < output.len() {
        let at = offset
            .checked_add(filled as u64)
            .ok_or(RarcError::ImageRange {
                offset,
                length: output.len() as u64,
                image_size: image.logical_size(),
            })?;
        let read = image
            .read_at(at, &mut output[filled..])
            .map_err(RarcError::Image)?;
        if read == 0 {
            return Err(RarcError::Truncated {
                needed: output.len(),
                actual: filled,
            });
        }
        filled += read;
    }
    Ok(())
}

fn checked_table_range(
    table: &'static str,
    offset: u32,
    count: u32,
    item_size: usize,
    limit: usize,
) -> Result<Range<usize>, RarcError> {
    let offset_usize = usize::try_from(offset).map_err(|_| RarcError::OutOfRange {
        table,
        offset: u64::from(offset),
        size: 0,
        limit: limit as u64,
    })?;
    let size = usize::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(item_size))
        .ok_or(RarcError::OutOfRange {
            table,
            offset: u64::from(offset),
            size: u64::from(count).saturating_mul(item_size as u64),
            limit: limit as u64,
        })?;
    let end = offset_usize
        .checked_add(size)
        .ok_or(RarcError::OutOfRange {
            table,
            offset: u64::from(offset),
            size: size as u64,
            limit: limit as u64,
        })?;
    if end > limit {
        return Err(RarcError::OutOfRange {
            table,
            offset: u64::from(offset),
            size: size as u64,
            limit: limit as u64,
        });
    }
    Ok(offset_usize..end)
}

fn read_name(
    strings: &[u8],
    offset: u32,
    table: &'static str,
    index: usize,
) -> Result<String, RarcError> {
    let offset = usize::try_from(offset).map_err(|_| RarcError::InvalidNameOffset {
        table,
        index,
        offset,
    })?;
    if offset >= strings.len() {
        return Err(RarcError::InvalidNameOffset {
            table,
            index,
            offset: offset as u32,
        });
    }
    let tail = &strings[offset..];
    let end = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(RarcError::InvalidNameOffset {
            table,
            index,
            offset: offset as u32,
        })?;
    let name = &tail[..end];
    if !name.is_ascii() {
        return Err(RarcError::InvalidNameEncoding { table, index });
    }
    Ok(String::from_utf8(name.to_vec()).expect("ASCII is valid UTF-8"))
}

fn join_path(parent: &str, name: &str) -> String {
    let mut path = String::with_capacity(parent.len() + 1 + name.len());
    path.push_str(parent);
    path.push('/');
    path.push_str(name);
    path
}
