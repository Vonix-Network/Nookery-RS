//! Portable logic from `src/game/m_melody.c`.
//!
//! The C file stores sixteen four-bit notes in one save `u64`.  Save and
//! audio ownership stay outside this crate; these functions take the packed
//! value and note buffer as parameters.

pub const MELODY_LEN: usize = 16;
pub const ACTOR_PART_NPC: u8 = 4;

pub const DEFAULT_MELODY: [u8; MELODY_LEN] = [
    0x7, 0xC, 0xF, 0x7, 0x6, 0xB, 0xF, 0x9, 0xA, 0xE, 0xD, 0xE, 0x3, 0xF, 0xE, 0xE,
];

/// Port of `mMld_TransformMelodyData_u64_2_u8`.
pub fn melody_from_u64(src: u64) -> [u8; MELODY_LEN] {
    let mut dst = [0; MELODY_LEN];
    for (i, note) in dst.iter_mut().enumerate() {
        *note = ((src >> (60 - i * 4)) & 0xF) as u8;
    }
    dst
}

/// Port of `mMld_TransformMelodyData_u8_2_u64`.
pub fn melody_to_u64(src: &[u8; MELODY_LEN]) -> u64 {
    let mut dst = 0;
    for (i, note) in src.iter().enumerate() {
        dst |= u64::from(*note & 0xF) << (60 - i * 4);
    }
    dst
}

/// Port of the save write performed by `mMld_SetSaveMelody`.
pub fn set_save_melody(save: &mut u64, melody: &[u8; MELODY_LEN]) {
    *save = melody_to_u64(melody);
}

/// Port of `mMld_GetMelody` for a caller-provided save value.
pub fn get_melody(save: u64) -> [u8; MELODY_LEN] {
    melody_from_u64(save)
}

/// Port of `mMld_SetDefaultMelody` for a caller-provided save value.
pub fn set_default_melody(save: &mut u64) {
    set_save_melody(save, &DEFAULT_MELODY);
}

/// Port of `mMld_MakeMelody`; the C audio singleton is represented by the
/// required callback and therefore cannot silently disappear at this seam.
pub fn make_melody<F>(save: u64, inst: u16, mut play: F)
where
    F: FnMut(u16, &[u8; MELODY_LEN]),
{
    let melody = get_melody(save);
    play(inst, &melody);
}

/// The actor fields consumed by the two C actor helpers: a non-null actor is
/// represented by `Some`, and its `part` and `talk_info.melody_inst` are kept
/// as separate values just as the source reads them.
pub type ActorMelodyInfo = (u8, u16);

/// Port of `mMld_ActorMakeThisMelody`.
pub fn actor_make_this_melody<F>(
    melody: &[u8; MELODY_LEN],
    actor: Option<ActorMelodyInfo>,
    mut play: F,
) where
    F: FnMut(u16, &[u8; MELODY_LEN]),
{
    if let Some((part, inst)) = actor {
        if part == ACTOR_PART_NPC && inst != 0 {
            play(inst, melody);
        }
    }
}

/// Port of `mMld_ActorMakeMelody`.
pub fn actor_make_melody<F>(save: u64, actor: Option<ActorMelodyInfo>, mut play: F)
where
    F: FnMut(u16, &[u8; MELODY_LEN]),
{
    if let Some((part, inst)) = actor {
        if part == ACTOR_PART_NPC && inst != 0 {
            make_melody(save, inst, &mut play);
        }
    }
}
