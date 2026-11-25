//! Portable logic from `src/game/m_diary.c`.

pub const DIARY_ENTRY_SIZE: usize = 992;
pub const CHAR_SPACE: u8 = 32;

/// Rust layout of the C `mDi_entry_c` payload.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiaryEntry {
    pub text: [u8; DIARY_ENTRY_SIZE],
}

impl Default for DiaryEntry {
    fn default() -> Self {
        Self {
            text: [CHAR_SPACE; DIARY_ENTRY_SIZE],
        }
    }
}

/// Port of `mDi_strlen`.
///
/// The C caller supplies a valid buffer containing at least `max_len` bytes;
/// the fixed slice form keeps that precondition explicit at the Rust boundary.
pub fn diary_strlen(bytes: &[u8], max_len: usize, end_char: u8) -> usize {
    assert!(
        max_len <= bytes.len(),
        "mDi_strlen input is shorter than max_len"
    );
    if max_len == 0 {
        return 0;
    }

    for i in (0..max_len).rev() {
        if bytes[i] != end_char {
            return i + 1;
        }
    }
    0
}

/// Port of `mDi_init_diary`; the preceding C `bzero` is overwritten by the
/// source's `mem_clear(..., CHAR_SPACE)` and is therefore represented by the
/// single final fill.
pub fn init_diary(entry: &mut DiaryEntry) {
    entry.text.fill(CHAR_SPACE);
}
