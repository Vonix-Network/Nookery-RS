//! Source-grounded PC memory-card manager behavior.
//!
//! This module ports the pure decisions in `pc/src/pc_m_card.c`.  The game
//! globals, UI, thread callbacks, and Dolphin CARD channel out-parameters are
//! represented by explicit state or omitted at the boundary; no C object
//! layout is guessed here.  Paths are injected so callers and tests can use
//! isolated trees.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::dir::HEADER_SIZE;
use crate::error::{GciError, SaveResult};
use crate::io::{read_town_record_recovering, write_town_record};
use crate::layout::{
    AUX_START, FILE_DATA_SIZE, KEEP_DIARY_SIZE, KEEP_MAIL_SIZE, KEEP_ORIGINAL_SIZE,
    LAND_ID_BITMASK, LAND_INFO_ID_OFFSET, RECORD_SIZE, SAVE_SIZE,
};
use crate::{is_gci_file, is_town_file, read_card_dir};

/// The C manager's fixed path bound (`PC_GCI_PATH_MAX`).
pub const PC_GCI_PATH_MAX: usize = 1024;
/// The C manager's default GCI filename (`PC_GCI_FILENAME`).
pub const PC_GCI_FILENAME: &str = "DobutsunomoriP_MURA.gci";
/// The prefixed legacy filename checked before scanning Card A.
pub const PC_GCI_PREFIXED_FILENAME: &str = "8P-GAFE-DobutsunomoriP_MURA.gci";
/// Maximum number of Card-B entries (`PC_CARD_TOWN_MAX`).
pub const PC_CARD_TOWN_MAX: usize = 64;
/// Card-A slot number from `mCD_SLOT_A`.
pub const CARD_SLOT_A: u8 = 0;
/// Card-B slot number from `mCD_SLOT_B`.
pub const CARD_SLOT_B: u8 = 1;
/// Birthday-active private-state flag from `mPr_FLAG_BIRTHDAY_ACTIVE`.
pub const BIRTHDAY_ACTIVE_FLAG: u32 = 1 << 10;
/// First Wisp inventory item (`ITM_SPIRIT0`).
pub const WISP_FIRST: u16 = 0x2D28;
/// Last Wisp inventory item (`ITM_SPIRIT4`).
pub const WISP_LAST: u16 = 0x2D2C;
/// Empty inventory item (`EMPTY_NO`).
pub const EMPTY_ITEM: u16 = 0;
/// Number of pocket slots (`mPr_POCKETS_SLOT_COUNT`).
pub const POCKET_COUNT: usize = 15;
/// Number of diary pages (`PLAYER_NUM`).
pub const DIARY_PAGE_COUNT: usize = 4;
/// Number of diary entries per page (`lbRTC_MONTHS_MAX`).
pub const DIARY_ENTRY_COUNT: usize = 12;
/// Number of keep-mail/original pages.
pub const KEEP_PAGE_COUNT: usize = 8;
/// Number of keep-mail entries per page.
pub const KEEP_MAIL_COUNT: usize = 20;
/// Mail's fixed in-memory size (`sizeof(Mail_c)`).
pub const MAIL_SIZE: usize = 0x12A;
/// Diary entry text size (`mDI_ENTRY_SIZE`).
pub const DIARY_ENTRY_SIZE: usize = 992;
/// Original-design size (`sizeof(mNW_original_design_c)`).
pub const ORIGINAL_DESIGN_SIZE: usize = 0x220;

/// A typed error from manager path, scan, import, or staging operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardManagerError {
    /// A derived path would not fit in the C 1024-byte buffer.
    PathTooLong,
    /// Filesystem failure, retaining the portable error category.
    Io(ErrorKind),
    /// A GCI record failed validation.
    Gci(GciError),
    /// A requested Card-B index is outside the collected list.
    InvalidTownIndex,
    /// A write was attempted before the C manager marked the save ready.
    NotReady,
    /// A required ARAM block was not staged.
    MissingAramBlock,
}

impl CardManagerError {
    /// Map the error to the C `pc_save_result` category.
    #[must_use]
    pub fn as_save_result(&self) -> SaveResult {
        match self {
            Self::PathTooLong | Self::Io(_) | Self::MissingAramBlock | Self::NotReady => {
                SaveResult::IoError
            }
            Self::Gci(error) => error.as_save_result(),
            Self::InvalidTownIndex => SaveResult::Invalid,
        }
    }
}

impl From<GciError> for CardManagerError {
    fn from(error: GciError) -> Self {
        Self::Gci(error)
    }
}

impl std::fmt::Display for CardManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathTooLong => formatter.write_str("save path exceeds PC_GCI_PATH_MAX"),
            Self::Io(kind) => write!(formatter, "save filesystem error: {kind}"),
            Self::Gci(error) => error.fmt(formatter),
            Self::InvalidTownIndex => formatter.write_str("Card-B town index is out of range"),
            Self::NotReady => formatter.write_str("save manager is not ready to write"),
            Self::MissingAramBlock => formatter.write_str("an ARAM save block is not staged"),
        }
    }
}

impl std::error::Error for CardManagerError {}

/// Resolved equivalents of `pc_save_paths_init`'s five paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePaths {
    user_root: PathBuf,
    working_root: PathBuf,
    base_root: PathBuf,
    /// `user_root/save`.
    pub save_dir: PathBuf,
    /// `user_root/save/card_a`.
    pub card_a_dir: PathBuf,
    /// `user_root/save/card_b`.
    pub card_b_dir: PathBuf,
    /// `user_root/save/card_a/DobutsunomoriP_MURA.gci`.
    pub gci_path: PathBuf,
    /// `user_root/save/DobutsunomoriP_MURA.gci`.
    pub legacy_gci_path: PathBuf,
}

impl SavePaths {
    /// Resolve manager paths below injected user, working-directory, and base roots.
    ///
    /// `working_root` models the process CWD and `base_root` models
    /// `SDL_GetBasePath`; both are used only for legacy imports, exactly as in
    /// `pc_save_migrate_legacy` and `pc_card_scan_towns`.
    pub fn new(
        user_root: impl Into<PathBuf>,
        working_root: impl Into<PathBuf>,
        base_root: impl Into<PathBuf>,
    ) -> Result<Self, CardManagerError> {
        let user_root = user_root.into();
        let working_root = working_root.into();
        let base_root = base_root.into();
        let save_dir = user_root.join("save");
        let card_a_dir = save_dir.join("card_a");
        let card_b_dir = save_dir.join("card_b");
        let gci_path = card_a_dir.join(PC_GCI_FILENAME);
        let legacy_gci_path = save_dir.join(PC_GCI_FILENAME);
        let paths = Self {
            user_root,
            working_root,
            base_root,
            save_dir,
            card_a_dir,
            card_b_dir,
            gci_path,
            legacy_gci_path,
        };
        paths.validate()
    }

    /// Port of `pc_save_paths_init` using injected roots.
    pub fn pc_save_paths_init(
        user_root: impl Into<PathBuf>,
        working_root: impl Into<PathBuf>,
        base_root: impl Into<PathBuf>,
    ) -> Result<Self, CardManagerError> {
        Self::new(user_root, working_root, base_root)
    }

    /// Validate every path against the C `snprintf` success boundary.
    pub fn validate(&self) -> Result<Self, CardManagerError> {
        for path in [
            &self.save_dir,
            &self.card_a_dir,
            &self.card_b_dir,
            &self.gci_path,
            &self.legacy_gci_path,
        ] {
            if path.to_string_lossy().len() >= PC_GCI_PATH_MAX {
                return Err(CardManagerError::PathTooLong);
            }
        }
        Ok(self.clone())
    }

    /// The relative path used for one slot's town directory.
    #[must_use]
    pub fn relative_card_dir(channel: u8) -> &'static str {
        if channel == CARD_SLOT_B {
            "save/card_b"
        } else {
            "save/card_a"
        }
    }

    fn active_card_dir(&self, channel: u8) -> &Path {
        if channel == CARD_SLOT_B {
            &self.card_b_dir
        } else {
            &self.card_a_dir
        }
    }

    fn working_card_dir(&self, channel: u8) -> PathBuf {
        self.working_root.join(Self::relative_card_dir(channel))
    }

    fn base_card_dir(&self, channel: u8) -> PathBuf {
        self.base_root.join(Self::relative_card_dir(channel))
    }
}

/// Port of `pc_ensure_save_dirs` for an injected path set.
pub fn pc_ensure_save_dirs(paths: &SavePaths) -> Result<(), CardManagerError> {
    for directory in [&paths.save_dir, &paths.card_a_dir, &paths.card_b_dir] {
        fs::create_dir_all(directory).map_err(|error| CardManagerError::Io(error.kind()))?;
    }
    Ok(())
}

/// A town's identity as read from the selected `Save` copy's `land_info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownIdentity {
    /// Eight raw game name bytes.
    pub name: [u8; 8],
    /// The game land identifier.
    pub id: u16,
}

impl TownIdentity {
    /// Port of `mLd_CheckId`.
    #[must_use]
    pub const fn has_valid_id(self) -> bool {
        self.id & LAND_ID_BITMASK == LAND_ID_BITMASK
    }

    const fn has_nonblank_name(self) -> bool {
        let mut index = 0;
        while index < self.name.len() {
            if self.name[index] != b' ' {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// A Card-B list entry, corresponding to `pc_card_town_entry` plus its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TownEntry {
    /// Imported/active GCI path.
    pub path: PathBuf,
    /// Filename portion used by the selector.
    pub file: String,
    /// Raw eight-byte town name; zeroed for an unreadable entry.
    pub name: [u8; 8],
    /// Whether the complete record and land id were usable.
    pub usable: bool,
}

/// Read the selected save copy's land identity without trusting file bytes.
pub fn town_identity(record: &crate::record::TownRecord) -> TownIdentity {
    let save = record.save_block();
    let mut name = [0u8; 8];
    name.copy_from_slice(&save[LAND_INFO_ID_OFFSET - 0x0A..LAND_INFO_ID_OFFSET - 0x02]);
    let id = u16::from_be_bytes([save[LAND_INFO_ID_OFFSET], save[LAND_INFO_ID_OFFSET + 1]]);
    TownIdentity { name, id }
}

fn path_is_same_town(candidate: TownIdentity, current: Option<TownIdentity>) -> bool {
    candidate.has_nonblank_name()
        && current.is_some_and(TownIdentity::has_nonblank_name)
        && current == Some(candidate)
}

fn sorted_gci_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let is_gci = path.extension().is_some_and(|extension| extension == "gci");
            if !is_gci || !entry.file_type().ok()?.is_file() {
                return None;
            }
            let header = read_card_dir(&path).ok()?;
            if !header.game_name().starts_with(b"GAF") {
                return None;
            }
            entry.file_name().into_string().ok()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn copy_if_absent(source: &Path, destination: &Path) -> bool {
    if source == destination || destination.exists() || !is_gci_file(source) {
        return false;
    }
    let import_path = PathBuf::from(format!("{}.import", destination.display()));
    if import_path.to_string_lossy().len() >= PC_GCI_PATH_MAX {
        return false;
    }
    let _ = fs::remove_file(&import_path);
    if fs::copy(source, &import_path).is_err() {
        let _ = fs::remove_file(&import_path);
        return false;
    }
    let result = !destination.exists() && fs::rename(&import_path, destination).is_ok();
    if !result {
        let _ = fs::remove_file(import_path);
    }
    result
}

fn copy_companions(source: &Path, destination: &Path) {
    for suffix in [
        ".bak1",
        ".bak2",
        ".bak3",
        ".temp",
        ".write",
        ".write.temp",
        ".tmp",
    ] {
        let source_path = PathBuf::from(format!("{}{}", source.display(), suffix));
        let destination_path = PathBuf::from(format!("{}{}", destination.display(), suffix));
        let _ = copy_if_absent(&source_path, &destination_path);
    }
}

/// Port of `pc_gci_import`: staged copy, source retention, and companion copy.
pub fn pc_gci_import(source: &Path, destination: &Path) -> bool {
    if !copy_if_absent(source, destination) {
        return false;
    }
    copy_companions(source, destination);
    true
}

/// Scan one card directory in the exact active/working/base order.
///
/// With `import_legacy == true`, only the first directory producing a town is
/// used; legacy files are copied into the active directory before being
/// returned.  This mirrors the `found == 0` loop in `pc_card_scan_towns`.
pub fn pc_card_scan_towns(paths: &SavePaths, channel: u8, import_legacy: bool) -> Vec<PathBuf> {
    let active_dir = paths.active_card_dir(channel).to_path_buf();
    let mut directories = vec![active_dir.clone()];
    if import_legacy {
        directories.push(paths.working_card_dir(channel));
        directories.push(paths.base_card_dir(channel));
    }
    for (index, directory) in directories.iter().enumerate() {
        let mut found = Vec::new();
        for filename in sorted_gci_names(directory) {
            let active_path = active_dir.join(&filename);
            if index == 0 {
                if is_town_file(&active_path) {
                    found.push(active_path);
                }
            } else {
                let source = directory.join(&filename);
                if is_town_file(&source) && pc_gci_import(&source, &active_path) {
                    found.push(active_path);
                }
            }
        }
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// Return the first town in `pc_card_scan_towns` order.
pub fn pc_card_take_first_town(paths: &SavePaths, channel: u8) -> Option<PathBuf> {
    pc_card_scan_towns(paths, channel, true).into_iter().next()
}

/// Port of `pc_card_find_town`.
pub fn pc_card_find_town(paths: &SavePaths, channel: u8) -> Option<PathBuf> {
    pc_card_take_first_town(paths, channel)
}

/// Port of `pc_card_count_town` over the active Card-B directory.
pub fn pc_card_count_town(paths: &SavePaths, channel: u8) -> usize {
    pc_card_scan_towns(paths, channel, false).len()
}

/// Result of the manager's save-path candidate scan.
pub type SaveScan = Result<crate::record::TownRecord, CardManagerError>;

/// Port of `pc_save_scan_gci_dir`, including its three candidate stages.
pub fn pc_save_scan_gci_dir(paths: &SavePaths) -> SaveScan {
    let primary = read_town_record_recovering(&paths.gci_path);
    if !matches!(primary, Err(GciError::Missing)) {
        return primary.map_err(Into::into);
    }
    let prefixed = paths.card_a_dir.join(PC_GCI_PREFIXED_FILENAME);
    let candidate = read_town_record_recovering(prefixed);
    if !matches!(candidate, Err(GciError::Missing)) {
        return candidate.map_err(Into::into);
    }
    pc_card_find_town(paths, CARD_SLOT_A)
        .ok_or(CardManagerError::Gci(GciError::Missing))
        .and_then(|path| read_town_record_recovering(path).map_err(Into::into))
}

/// Copy a legacy source into the active Card-A filename, if one is available.
///
/// The C implementation copies only a GAF-sized source when the destination is
/// absent, keeps the source, and copies valid companion files opportunistically.
/// It does not delete or overwrite any candidate.
pub fn pc_save_migrate_legacy(paths: &SavePaths) -> bool {
    let sources = [
        paths.legacy_gci_path.clone(),
        paths.working_root.join("save/card_a").join(PC_GCI_FILENAME),
        paths
            .working_root
            .join("save/card_a")
            .join(PC_GCI_PREFIXED_FILENAME),
        paths.working_root.join("save").join(PC_GCI_FILENAME),
        paths.base_root.join("save/card_a").join(PC_GCI_FILENAME),
        paths
            .base_root
            .join("save/card_a")
            .join(PC_GCI_PREFIXED_FILENAME),
        paths.base_root.join("save").join(PC_GCI_FILENAME),
    ];
    let _ = pc_ensure_save_dirs(paths);
    sources
        .iter()
        .any(|source| pc_gci_import(source, &paths.gci_path))
}

/// Port of `pc_save_find_and_load`, returning its precise failure category.
pub fn pc_save_find_and_load(paths: &SavePaths) -> SaveScan {
    pc_ensure_save_dirs(paths)?;
    let primary = read_town_record_recovering(&paths.gci_path);
    if !matches!(primary, Err(GciError::Missing)) {
        return primary.map_err(Into::into);
    }
    let _ = pc_save_migrate_legacy(paths);
    pc_save_scan_gci_dir(paths)
}

/// Staged ARAM blocks in `mCD_ARAM_DATA_*` index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AramBlocks {
    blocks: [Vec<u8>; 3],
}

impl Default for AramBlocks {
    fn default() -> Self {
        Self::new()
    }
}

impl AramBlocks {
    /// Allocate and initialize all three fixed-size blocks.
    #[must_use]
    pub fn new() -> Self {
        let mut blocks = [
            vec![0; KEEP_ORIGINAL_SIZE],
            vec![0; KEEP_MAIL_SIZE],
            vec![0; KEEP_DIARY_SIZE],
        ];
        for (index, block) in blocks.iter_mut().enumerate() {
            initialize_aram_block(index, block);
        }
        Self { blocks }
    }

    /// Port of `mCD_save_data_aram_malloc`; blocks are already owned by this value.
    #[allow(non_snake_case)]
    pub fn mCD_save_data_aram_malloc(&mut self) {}

    /// Reinitialize every allocated block, porting `mCD_set_aram_save_data`.
    #[allow(non_snake_case)]
    pub fn mCD_set_aram_save_data(&mut self) {
        for (index, block) in self.blocks.iter_mut().enumerate() {
            initialize_aram_block(index, block);
        }
    }

    /// Return the exact allocated size for an ARAM index, with C's invalid-index fallback.
    #[must_use]
    pub fn block_size(index: usize) -> usize {
        match index {
            0 => KEEP_ORIGINAL_SIZE,
            1 => KEEP_MAIL_SIZE,
            2 => KEEP_DIARY_SIZE,
            _ => KEEP_ORIGINAL_SIZE,
        }
    }

    /// Read at most a block's allocation size into `destination`.
    #[allow(non_snake_case)]
    pub fn mCD_save_data_aram_to_main(
        &self,
        destination: &mut [u8],
        size: usize,
        index: usize,
    ) -> bool {
        let block = &self.blocks[normalize_aram_index(index)];
        let count = size.min(block.len()).min(destination.len());
        destination[..count].copy_from_slice(&block[..count]);
        true
    }

    /// Copy at most a block's allocation size from `source` into ARAM.
    #[allow(non_snake_case)]
    pub fn mCD_save_data_main_to_aram(&mut self, source: &[u8], size: usize, index: usize) -> bool {
        let block = &mut self.blocks[normalize_aram_index(index)];
        let count = size.min(block.len()).min(source.len());
        block[..count].copy_from_slice(&source[..count]);
        true
    }

    /// Borrow one staged ARAM block in `mCD_ARAM_DATA_*` order.
    pub fn block(&self, index: usize) -> Option<&[u8]> {
        self.blocks.get(index).map(Vec::as_slice)
    }

    /// Borrow one mutable staged ARAM block in `mCD_ARAM_DATA_*` order.
    pub fn block_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        self.blocks.get_mut(index).map(Vec::as_mut_slice)
    }

    /// True when all C writer inputs are available.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.blocks.iter().all(|block| !block.is_empty())
    }
}

fn normalize_aram_index(index: usize) -> usize {
    if index < 3 { index } else { 0 }
}

fn initialize_aram_block(index: usize, block: &mut [u8]) {
    block.fill(0);
    match index {
        0 => pc_init_original_entries(block),
        1 => pc_init_mail_entries(block),
        2 => pc_init_diary_entries(block),
        _ => {}
    }
}

/// Port of `pc_init_aram_block` for one caller-owned fixed-size block.
pub fn pc_init_aram_block(index: usize, block: &mut [u8]) {
    initialize_aram_block(index, block);
}

/// Port `pc_init_diary_entries` using the exact page/entry byte layout.
pub fn pc_init_diary_entries(block: &mut [u8]) {
    let entries_start = 2;
    let entries = DIARY_PAGE_COUNT * DIARY_ENTRY_COUNT;
    for entry in 0..entries {
        let start = entries_start + entry * DIARY_ENTRY_SIZE;
        let end = start.saturating_add(DIARY_ENTRY_SIZE).min(block.len());
        if start < end {
            block[start..end].fill(b' ');
        }
    }
}

/// Port `pc_init_mail_entries` using `mMl_clear_mail`'s blank values.
pub fn pc_init_mail_entries(block: &mut [u8]) {
    let folder_start = 4;
    let folder_len = 12;
    for page in 0..KEEP_PAGE_COUNT {
        let start = folder_start + page * folder_len;
        if let Some(folder) = block.get_mut(start..start + folder_len) {
            folder.fill(b' ');
        }
    }
    let mail_start = folder_start + KEEP_PAGE_COUNT * folder_len;
    for entry in 0..KEEP_PAGE_COUNT * KEEP_MAIL_COUNT {
        let start = mail_start + entry * MAIL_SIZE;
        let Some(mail) = block.get_mut(start..start + MAIL_SIZE) else {
            break;
        };
        mail.fill(0);
        mail[0..8].fill(b' ');
        mail[8..16].fill(b' ');
        mail[0x10..0x14].copy_from_slice(&u16::MAX.to_ne_bytes().repeat(2));
        mail[0x16..0x1E].fill(b' ');
        mail[0x1E..0x26].fill(b' ');
        mail[0x26..0x2A].copy_from_slice(&u16::MAX.to_ne_bytes().repeat(2));
        mail[0x14] = 0xFF;
        mail[0x2A] = 0xFF;
        mail[0x2E] = 0xFF;
        mail[0x32..0x32 + 0x18 + 0xC0 + 0x20].fill(b' ');
    }
}

/// Port `pc_init_original_entries` using blank designs and space folder names.
pub fn pc_init_original_entries(block: &mut [u8]) {
    let folder_start = 4;
    let folder_len = 12;
    for page in 0..KEEP_PAGE_COUNT {
        let start = folder_start + page * folder_len;
        if let Some(folder) = block.get_mut(start..start + folder_len) {
            folder.fill(b' ');
        }
    }
    let design_start = folder_start + KEEP_PAGE_COUNT * folder_len;
    for design in 0..KEEP_PAGE_COUNT * 12 {
        let start = design_start + design * ORIGINAL_DESIGN_SIZE;
        let Some(original) = block.get_mut(start..start + ORIGINAL_DESIGN_SIZE) else {
            break;
        };
        original.fill(0);
        original[..16].copy_from_slice(b"blank           ");
        original[0x20..0x20 + 512].fill(0xFF);
    }
}

/// Port of `pc_fix_zeroed_original_entries` for an in-memory native block.
///
/// The C caller invokes this only after byte-swapping a successfully read GCI
/// image.  `land_id` is therefore stored in native host order here.  The flat
/// checksum is the same two-byte native-word sum used by `mFRm_GetFlatCheckSum`.
pub fn pc_fix_zeroed_original_entries(original: &mut [u8], land_id: u16) -> bool {
    if original.is_empty() || original.iter().any(|byte| *byte != 0) {
        return false;
    }
    pc_init_original_entries(original);
    if original.len() >= 4 {
        original[2..4].copy_from_slice(&land_id.to_ne_bytes());
    }
    if original.len() >= 2 {
        original[0..2].fill(0);
        let sum = original
            .chunks_exact(2)
            .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]) as u32)
            .fold(0u32, |sum, value| sum.wrapping_add(value));
        original[0..2].copy_from_slice(&(0u16.wrapping_sub(sum as u16)).to_ne_bytes());
    }
    true
}

/// Small private-state mirror sufficient for reset-code/pre-write parity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateState {
    /// `Private_c.state_flags`.
    pub state_flags: u32,
    /// `Private_c.reset_code`.
    pub reset_code: u32,
    /// `Private_c.reset_count`.
    pub reset_count: u32,
    /// `Private_c.inventory.pockets`.
    pub pockets: [u16; POCKET_COUNT],
}

impl Default for PrivateState {
    fn default() -> Self {
        Self {
            state_flags: 0,
            reset_code: 0,
            reset_count: 0,
            pockets: [EMPTY_ITEM; POCKET_COUNT],
        }
    }
}

/// Port of `pc_check_reset_code`; birthday-active also clears the code.
pub fn pc_check_reset_code(private: &mut PrivateState) -> bool {
    if private.state_flags & BIRTHDAY_ACTIVE_FLAG != 0 {
        private.reset_code = 0;
    }
    private.reset_code == 0
}

/// Port of `pc_set_reset_code`, with the already-sampled random value injected.
pub fn pc_set_reset_code(private: &mut PrivateState, random_below_usht_max: u32) {
    private.reset_code = random_below_usht_max.saturating_add(1);
}

/// Port of `pc_get_land_copy_protect`, with the already-sampled RNG value injected.
#[must_use]
pub fn pc_get_land_copy_protect(random_below_fff0: u32) -> u16 {
    (random_below_fff0.min(0xFFEF) as u16).saturating_add(1)
}

/// Host-side state changed by `pc_save_pre_write_side_effects`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreWriteState {
    /// Optional current private data (`Now_Private`).
    pub private: Option<PrivateState>,
    /// Save/common copy-protect value.
    pub copy_protect: u16,
    /// Host-provided equivalent of `lbRTC_HardTime()`.
    pub travel_hard_time: i64,
    /// Whether full-save money-stone cleanup was requested.
    pub money_stone_cleared: bool,
}

impl Default for PreWriteState {
    fn default() -> Self {
        Self {
            private: Some(PrivateState::default()),
            copy_protect: 0,
            travel_hard_time: 0,
            money_stone_cleared: false,
        }
    }
}

/// Port the pure pre-write mutations; game play-time and ground cleanup are boundary inputs.
pub fn pc_save_pre_write_side_effects(
    state: &mut PreWriteState,
    save_mode: i32,
    reset_random: u32,
    copy_protect_random: u32,
    travel_hard_time: i64,
) {
    if let Some(private) = state.private.as_mut() {
        if save_mode == 0 {
            private.reset_code = 0;
            for item in &mut private.pockets {
                if (WISP_FIRST..=WISP_LAST).contains(item) {
                    *item = EMPTY_ITEM;
                }
            }
        } else if pc_check_reset_code(private) {
            pc_set_reset_code(private, reset_random);
        }
    }
    state.money_stone_cleared = save_mode == 0;
    state.copy_protect = pc_get_land_copy_protect(copy_protect_random);
    state.travel_hard_time = travel_hard_time;
}

/// Manager state needed by `pc_save_check_and_load`, Card B selection, and writing.
#[derive(Debug, Clone)]
pub struct CardManager {
    /// Injected path set.
    pub paths: SavePaths,
    /// Last C-equivalent load result.
    pub load_status: SaveResult,
    /// Whether the last load produced a valid record.
    pub save_loaded: bool,
    /// C's `pc_save_ready` gate.
    pub save_ready: bool,
    /// Last loaded home record, if any.
    pub loaded_record: Option<crate::record::TownRecord>,
    /// Current town identity used by `mLd_CheckThisLand` filtering.
    pub current_town: Option<TownIdentity>,
    /// Card-B entries collected by `pc_card_b_refresh_towns`.
    pub card_b_towns: Vec<TownEntry>,
    /// Active Card-B selection path.
    pub card_b_path: Option<PathBuf>,
    /// Whether the Card-B path was explicitly selected.
    pub card_b_town_selected: bool,
    /// Staged ARAM save blocks.
    pub aram: AramBlocks,
}

impl CardManager {
    /// Create an idle manager with initialized ARAM blocks.
    #[must_use]
    pub fn new(paths: SavePaths) -> Self {
        Self {
            paths,
            load_status: SaveResult::Missing,
            save_loaded: false,
            save_ready: false,
            loaded_record: None,
            current_town: None,
            card_b_towns: Vec::new(),
            card_b_path: None,
            card_b_town_selected: false,
            aram: AramBlocks::new(),
        }
    }

    /// Port of `pc_save_check_and_load`, including its previous-status IO rule.
    pub fn pc_save_check_and_load(&mut self) -> SaveResult {
        self.save_ready = false;
        let result = match pc_save_find_and_load(&self.paths) {
            Ok(record) => {
                self.loaded_record = Some(record);
                SaveResult::Loaded
            }
            Err(error) => error.as_save_result(),
        };
        let result = if result == SaveResult::Missing && self.load_status != SaveResult::Missing {
            SaveResult::IoError
        } else {
            result
        };
        self.load_status = result;
        self.save_loaded = result == SaveResult::Loaded;
        if !self.save_loaded {
            self.loaded_record = None;
        }
        result
    }

    /// Port of `pc_card_b_find_town`; imports the first Card-B town.
    pub fn pc_card_b_find_town(&mut self) -> bool {
        self.card_b_path = pc_card_find_town(&self.paths, CARD_SLOT_B);
        self.card_b_path.is_some()
    }

    /// Port of `pc_card_b_refresh_towns`; returns the usable count.
    pub fn pc_card_b_refresh_towns(&mut self, is_foreigner: bool) -> usize {
        self.card_b_towns.clear();
        self.card_b_town_selected = false;
        self.card_b_path = None;
        if is_foreigner {
            return 0;
        }
        for path in pc_card_scan_towns(&self.paths, CARD_SLOT_B, true) {
            if self.card_b_towns.len() >= PC_CARD_TOWN_MAX {
                break;
            }
            if let Some(entry) = self.pc_card_b_collect_town(path) {
                self.card_b_towns.push(entry);
            }
        }
        self.card_b_towns.iter().filter(|town| town.usable).count()
    }

    /// Port of `pc_card_b_collect_town` for one active/imported path.
    pub fn pc_card_b_collect_town(&self, path: PathBuf) -> Option<TownEntry> {
        let file = path.file_name()?.to_string_lossy().into_owned();
        let (name, usable) = match read_town_record_recovering(&path) {
            Ok(record) => {
                let identity = town_identity(&record);
                if !identity.has_valid_id() || path_is_same_town(identity, self.current_town) {
                    if path_is_same_town(identity, self.current_town) {
                        return None;
                    }
                    ([0; 8], false)
                } else {
                    (identity.name, true)
                }
            }
            Err(_) => ([0; 8], false),
        };
        Some(TownEntry {
            path,
            file,
            name,
            usable,
        })
    }

    /// Number of collected Card-B entries, including damaged entries.
    #[must_use]
    pub fn pc_card_b_town_count(&self) -> usize {
        self.card_b_towns.len()
    }

    /// Borrow one collected Card-B entry.
    #[must_use]
    pub fn pc_card_b_town(&self, index: usize) -> Option<&TownEntry> {
        self.card_b_towns.get(index)
    }

    /// Count active Card-B town files without importing legacy directories.
    #[must_use]
    pub fn pc_card_b_scan_town_files(&self) -> usize {
        pc_card_count_town(&self.paths, CARD_SLOT_B)
    }

    /// Derive the selected Card-B path without its final extension.
    #[must_use]
    pub fn pc_card_b_town_base(&self) -> Option<PathBuf> {
        let path = self.card_b_path.as_ref()?;
        let text = path.to_string_lossy();
        let separator = text.rfind('/');
        let extension = text.rfind('.');
        let length = if extension.is_none() || extension < separator {
            text.len()
        } else {
            extension?
        };
        if length == 0 || separator.is_some_and(|position| length <= position + 1) {
            return None;
        }
        Some(PathBuf::from(&text[..length]))
    }

    /// Port of `pc_card_b_is_legacy_town`: compare the selected path to the first active town.
    #[must_use]
    pub fn pc_card_b_is_legacy_town(&self) -> bool {
        let Some(selected) = self.card_b_path.as_ref() else {
            return false;
        };
        pc_card_scan_towns(&self.paths, CARD_SLOT_B, false)
            .first()
            .is_some_and(|first| first == selected)
    }

    /// Select a usable Card-B entry, refusing foreign-player selection.
    pub fn pc_card_b_select_town(&mut self, index: usize, is_foreigner: bool) -> bool {
        let Some(town) = self.card_b_towns.get(index) else {
            return false;
        };
        if is_foreigner || !town.usable {
            return false;
        }
        self.card_b_path = Some(town.path.clone());
        self.card_b_town_selected = true;
        true
    }

    /// Port of `pc_save_write_gci_to` over a prebuilt validated synthetic record.
    ///
    /// The C writer constructs and byte-swaps `Save_t`/ARAM records from game
    /// globals.  This crate receives the already-built bytes at that boundary;
    /// staged writing, readiness gates, and recovery rotation are identical.
    pub fn pc_save_write_gci_to(
        &mut self,
        path: &Path,
        record: &[u8],
    ) -> Result<(), CardManagerError> {
        if !self.save_ready || matches!(self.load_status, SaveResult::Invalid | SaveResult::IoError)
        {
            return Err(CardManagerError::NotReady);
        }
        if !self.aram.is_complete() {
            return Err(CardManagerError::MissingAramBlock);
        }
        pc_ensure_save_dirs(&self.paths)?;
        write_town_record(path, record).map_err(CardManagerError::from)?;
        if path == self.paths.gci_path {
            self.save_loaded = true;
            self.load_status = SaveResult::Loaded;
        }
        Ok(())
    }
}

/// Port of `pc_card_b_town_base` as a pure path helper.
#[must_use]
pub fn pc_card_b_town_base(path: &Path) -> Option<PathBuf> {
    let mut manager = CardManager::new(SavePaths::new(".", ".", ".").ok()?);
    manager.card_b_path = Some(path.to_path_buf());
    manager.pc_card_b_town_base()
}

/// Return the selected save's raw auxiliary block offsets in ARAM index order.
///
/// This is the same fixed geometry probed by `pc_gci_aux_offsets`; it is
/// exposed for callers that stage bytes without decoding their game structs.
#[must_use]
pub fn aram_offsets(legacy_order: bool) -> [usize; 3] {
    if legacy_order {
        [
            AUX_START,
            AUX_START + KEEP_ORIGINAL_SIZE,
            AUX_START + KEEP_ORIGINAL_SIZE + KEEP_MAIL_SIZE,
        ]
    } else {
        [
            AUX_START + KEEP_MAIL_SIZE,
            AUX_START,
            AUX_START + KEEP_MAIL_SIZE + KEEP_ORIGINAL_SIZE,
        ]
    }
}

/// Report the manager constants used by the source's payload layout.
#[must_use]
pub const fn manager_layout() -> (usize, usize, usize, usize) {
    (RECORD_SIZE, FILE_DATA_SIZE, SAVE_SIZE, HEADER_SIZE)
}

/// Port of the C write path using a manager's readiness and staged ARAM state.
pub fn pc_save_write_gci_to(
    manager: &mut CardManager,
    path: &Path,
    record: &[u8],
) -> Result<(), CardManagerError> {
    manager.pc_save_write_gci_to(path, record)
}

/// Port of `pc_save_check_and_load` as a free-function seam.
pub fn pc_save_check_and_load(manager: &mut CardManager) -> SaveResult {
    manager.pc_save_check_and_load()
}

/// Port of `pc_card_b_find_town` as a free-function seam.
pub fn pc_card_b_find_town(manager: &mut CardManager) -> bool {
    manager.pc_card_b_find_town()
}

/// Port of `pc_card_b_refresh_towns` as a free-function seam.
pub fn pc_card_b_refresh_towns(manager: &mut CardManager, is_foreigner: bool) -> usize {
    manager.pc_card_b_refresh_towns(is_foreigner)
}

/// Port of `pc_card_b_collect_town` as a free-function seam.
pub fn pc_card_b_collect_town(manager: &CardManager, path: PathBuf) -> Option<TownEntry> {
    manager.pc_card_b_collect_town(path)
}

/// Port of `pc_card_b_town_count` as a free-function seam.
#[must_use]
pub fn pc_card_b_town_count(manager: &CardManager) -> usize {
    manager.pc_card_b_town_count()
}

/// Port of `pc_card_b_town` as a free-function seam.
#[must_use]
pub fn pc_card_b_town(manager: &CardManager, index: usize) -> Option<&TownEntry> {
    manager.pc_card_b_town(index)
}

/// Port of `pc_card_b_scan_town_files` as a free-function seam.
#[must_use]
pub fn pc_card_b_scan_town_files(manager: &CardManager) -> usize {
    manager.pc_card_b_scan_town_files()
}

/// Port of `pc_card_b_is_legacy_town` as a free-function seam.
#[must_use]
pub fn pc_card_b_is_legacy_town(manager: &CardManager) -> bool {
    manager.pc_card_b_is_legacy_town()
}

/// Port of `pc_card_b_select_town` as a free-function seam.
pub fn pc_card_b_select_town(manager: &mut CardManager, index: usize, is_foreigner: bool) -> bool {
    manager.pc_card_b_select_town(index, is_foreigner)
}

/// Port of `mCD_save_data_aram_malloc` for explicit ARAM state.
#[allow(non_snake_case)]
pub fn mCD_save_data_aram_malloc(aram: &mut AramBlocks) {
    aram.mCD_save_data_aram_malloc();
}

/// Port of `mCD_set_aram_save_data` for explicit ARAM state.
#[allow(non_snake_case)]
pub fn mCD_set_aram_save_data(aram: &mut AramBlocks) {
    aram.mCD_set_aram_save_data();
}

/// Port of `mCD_save_data_aram_to_main` for explicit ARAM state.
#[allow(non_snake_case)]
pub fn mCD_save_data_aram_to_main(
    aram: &AramBlocks,
    destination: &mut [u8],
    size: usize,
    index: usize,
) -> bool {
    aram.mCD_save_data_aram_to_main(destination, size, index)
}

/// Port of `mCD_save_data_main_to_aram` for explicit ARAM state.
#[allow(non_snake_case)]
pub fn mCD_save_data_main_to_aram(
    aram: &mut AramBlocks,
    source: &[u8],
    size: usize,
    index: usize,
) -> bool {
    aram.mCD_save_data_main_to_aram(source, size, index)
}
