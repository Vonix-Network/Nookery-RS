//! Byte-string translations of `MSL_C.PPCEABI.bare.H/string.c`.

/// Invalid destination geometry for a C string operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringError {
    /// The destination cannot hold the requested result including its NUL.
    DestinationTooSmall {
        /// Number of bytes required.
        needed: usize,
        /// Number of bytes available.
        available: usize,
    },
    /// The source or destination does not contain a NUL within the supplied bytes.
    MissingTerminator,
}

impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DestinationTooSmall { needed, available } => {
                write!(
                    f,
                    "string needs {needed} bytes, destination has {available}"
                )
            }
            Self::MissingTerminator => f.write_str("C string has no NUL terminator"),
        }
    }
}

impl std::error::Error for StringError {}

fn nul_position(bytes: &[u8]) -> Result<usize, StringError> {
    bytes
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(StringError::MissingTerminator)
}

/// Returns the byte count before the first NUL.
pub fn strlen(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len())
}

/// Copies a NUL-terminated source, including its NUL.
pub fn strcpy(destination: &mut [u8], source: &[u8]) -> Result<usize, StringError> {
    let len = nul_position(source)? + 1;
    if destination.len() < len {
        return Err(StringError::DestinationTooSmall {
            needed: len,
            available: destination.len(),
        });
    }
    destination[..len].copy_from_slice(&source[..len]);
    Ok(len - 1)
}

/// Copies at most `count` source bytes and pads with NUL bytes.
pub fn strncpy(destination: &mut [u8], source: &[u8], count: usize) -> Result<(), StringError> {
    if destination.len() < count {
        return Err(StringError::DestinationTooSmall {
            needed: count,
            available: destination.len(),
        });
    }
    let source_len = strlen(source).min(count);
    destination[..source_len].copy_from_slice(&source[..source_len]);
    destination[source_len..count].fill(0);
    Ok(())
}

/// Appends a NUL-terminated source to a NUL-terminated destination.
pub fn strcat(destination: &mut [u8], source: &[u8]) -> Result<usize, StringError> {
    let destination_len = nul_position(destination)?;
    let source_len = nul_position(source)?;
    let needed = destination_len + source_len + 1;
    if destination.len() < needed {
        return Err(StringError::DestinationTooSmall {
            needed,
            available: destination.len(),
        });
    }
    destination.copy_within(
        destination_len..=destination_len,
        destination_len + source_len,
    );
    destination[destination_len..needed].copy_from_slice(&source[..=source_len]);
    Ok(destination_len + source_len)
}

/// Compares NUL-terminated byte strings, returning the original byte difference.
pub fn strcmp(left: &[u8], right: &[u8]) -> Result<i32, StringError> {
    let left_len = nul_position(left)?;
    let right_len = nul_position(right)?;
    Ok(compare(
        &left[..=left_len],
        &right[..=right_len],
        usize::MAX,
    ))
}

/// Compares at most `count` bytes, stopping at the first NUL.
pub fn strncmp(left: &[u8], right: &[u8], count: usize) -> Result<i32, StringError> {
    let left_len = nul_position(left)?;
    let right_len = nul_position(right)?;
    Ok(compare(&left[..=left_len], &right[..=right_len], count))
}

fn compare(left: &[u8], right: &[u8], count: usize) -> i32 {
    for index in 0..count {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        if a != b {
            return a as i32 - b as i32;
        }
        if a == 0 {
            return 0;
        }
    }
    0
}

/// Finds a byte in a NUL-terminated string; the NUL itself is searchable.
pub fn strchr(string: &[u8], needle: i32) -> Result<Option<usize>, StringError> {
    let end = nul_position(string)?;
    let needle = needle as u8;
    Ok(string[..=end].iter().position(|&byte| byte == needle))
}

/// Finds a NUL-terminated byte substring and returns its offset.
pub fn strstr(string: &[u8], pattern: &[u8]) -> Result<Option<usize>, StringError> {
    let string_len = nul_position(string)?;
    let pattern_len = nul_position(pattern)?;
    let string = &string[..string_len];
    let pattern = &pattern[..pattern_len];
    if pattern.is_empty() {
        return Ok(Some(0));
    }
    Ok(string
        .windows(pattern.len())
        .position(|window| window == pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_c_string_copy_and_append_order() {
        let mut destination = [0; 8];
        assert_eq!(strcpy(&mut destination, b"ab\0"), Ok(2));
        assert_eq!(strcat(&mut destination, b"cd\0"), Ok(4));
        assert_eq!(&destination, b"abcd\0\0\0\0");
        assert_eq!(strlen(&destination), 4);
    }

    #[test]
    fn returns_unsigned_byte_differences_and_nul_searches() {
        assert_eq!(strcmp(b"a\xff\0", b"a\0"), Ok(255));
        assert_eq!(strncmp(b"abc\0", b"abd\0", 2), Ok(0));
        assert_eq!(strchr(b"abc\0", 0), Ok(Some(3)));
        assert_eq!(strstr(b"abcabc\0", b"cab\0"), Ok(Some(2)));
    }
}
