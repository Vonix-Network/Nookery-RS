//! Independent, standard-library-only RARC manifest extractor.
//!
//! This helper intentionally does not call the Rust crate or Python oracle. It
//! reads raw big-endian RARC files and prints one manifest record per reachable
//! file, preserving directory-entry traversal order.

use std::env;
use std::fs;

#[derive(Clone, Copy)]
enum Kind {
    File,
    Folder(usize),
    Shortcut,
}

struct Node {
    first: usize,
    count: usize,
}

struct Entry {
    name: String,
    offset: usize,
    size: usize,
    kind: Kind,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn name(strings: &[u8], offset: usize) -> Result<String, String> {
    if offset >= strings.len() {
        return Err(format!("name offset {offset} outside string table"));
    }
    let end = strings[offset..]
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("unterminated name at {offset}"))?;
    if !strings[offset..offset + end].is_ascii() {
        return Err(format!("non-ASCII name at {offset}"));
    }
    Ok(String::from_utf8(strings[offset..offset + end].to_vec()).unwrap())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn manifest(bytes: &[u8]) -> Result<Vec<(String, String, usize, u64)>, String> {
    if bytes.len() < 0x20 {
        return Err("truncated header".to_owned());
    }
    if &bytes[..4] != b"RARC" {
        return Err("fixture is not a raw RARC (Yaz0 must be unwrapped explicitly)".to_owned());
    }
    let file_length = u32_at(bytes, 4) as usize;
    let header_length = u32_at(bytes, 8) as usize;
    let info_length = u32_at(bytes, 12) as usize;
    let data_length = u32_at(bytes, 16) as usize;
    let info_end = header_length
        .checked_add(info_length)
        .ok_or_else(|| "metadata overflow".to_owned())?;
    let data_end = info_end
        .checked_add(data_length)
        .ok_or_else(|| "data overflow".to_owned())?;
    if file_length > bytes.len() || data_end > file_length || info_length < 0x20 {
        return Err("archive range is outside fixture".to_owned());
    }
    let info = &bytes[header_length..info_end];
    let node_count = u32_at(info, 0) as usize;
    let node_offset = u32_at(info, 4) as usize;
    let entry_count = u32_at(info, 8) as usize;
    let entry_offset = u32_at(info, 12) as usize;
    let string_length = u32_at(info, 16) as usize;
    let string_offset = u32_at(info, 20) as usize;
    let node_end = node_offset
        .checked_add(node_count.checked_mul(0x10).ok_or("node overflow")?)
        .ok_or("node overflow")?;
    let entry_end = entry_offset
        .checked_add(entry_count.checked_mul(0x14).ok_or("entry overflow")?)
        .ok_or("entry overflow")?;
    let string_end = string_offset.checked_add(string_length).ok_or("string overflow")?;
    if node_end > info.len() || entry_end > info.len() || string_end > info.len() || node_count == 0 {
        return Err("metadata table outside info block".to_owned());
    }
    let strings = &info[string_offset..string_end];
    let mut nodes = Vec::with_capacity(node_count);
    for index in 0..node_count {
        let at = node_offset + index * 0x10;
        let first = u32_at(info, at + 12) as usize;
        let count = u16_at(info, at + 10) as usize;
        if first.checked_add(count).ok_or("entry range overflow")? > entry_count {
            return Err("node entry range outside table".to_owned());
        }
        let _ = name(strings, u32_at(info, at + 4) as usize)?;
        nodes.push(Node { first, count });
    }
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let at = entry_offset + index * 0x14;
        let packed = u32_at(info, at + 4);
        let flags = (packed >> 24) as u8;
        let entry_name = name(strings, (packed & 0x00ff_ffff) as usize)?;
        let raw_offset = u32_at(info, at + 8);
        let offset = raw_offset as usize;
        let size = u32_at(info, at + 12) as usize;
        let kind = if flags & 2 != 0 && raw_offset != u32::MAX {
            if offset >= node_count {
                return Err("directory node index outside table".to_owned());
            }
            if entry_name == "." || entry_name == ".." {
                Kind::Shortcut
            } else {
                Kind::Folder(offset)
            }
        } else if flags & 1 != 0 {
            if offset > data_length || size > data_length - offset {
                return Err("file range outside data block".to_owned());
            }
            Kind::File
        } else if flags & 2 != 0 && entry_name == ".." {
            Kind::Shortcut
        } else {
            return Err("invalid entry kind".to_owned());
        };
        entries.push(Entry {
            name: entry_name,
            offset,
            size,
            kind,
        });
    }

    let root_name = name(strings, u32_at(info, node_offset + 4) as usize)?;
    let data = &bytes[info_end..data_end];
    let mut result = Vec::new();
    let mut active = vec![false; node_count];
    active[0] = true;
    let mut stack = vec![(0usize, root_name, 0usize)];
    while let Some((node, path, next)) = stack.last_mut() {
        if *next == nodes[*node].count {
            active[*node] = false;
            stack.pop();
            continue;
        }
        let index = nodes[*node].first + *next;
        *next += 1;
        let entry = &entries[index];
        match entry.kind {
            Kind::File => {
                let mut file_path = path.clone();
                file_path.push('/');
                file_path.push_str(&entry.name);
                let start = entry.offset;
                result.push((file_path, entry.name.clone(), entry.size, fnv1a64(&data[start..start + entry.size])));
            }
            Kind::Folder(child) => {
                if active[child] {
                    return Err("directory cycle".to_owned());
                }
                let mut child_path = path.clone();
                child_path.push('/');
                child_path.push_str(&entry.name);
                active[child] = true;
                stack.push((child, child_path, 0));
            }
            Kind::Shortcut => {}
        }
    }
    Ok(result)
}

fn main() -> Result<(), String> {
    let path = env::args().nth(1).ok_or_else(|| "usage: rarc_oracle ARCHIVE".to_owned())?;
    let bytes = fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
    for (file_path, file_name, size, digest) in manifest(&bytes)? {
        println!("FILE\t{file_path}\t{file_name}\t{size}\t{digest:016x}");
    }
    Ok(())
}
