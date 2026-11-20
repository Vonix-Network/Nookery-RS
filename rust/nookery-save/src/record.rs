//! A town GCI record: directory header plus the fixed `mCD_LAND_SAVE_SIZE`
//! payload, validated the way `pc_gci_read_image` validates it.

use crate::checksum;
use crate::dir::{CardDir, HEADER_SIZE};
use crate::error::GciError;
use crate::layout::{
    AUX_START, FILE_DATA_SIZE, KEEP_DIARY_SIZE, KEEP_MAIL_LANDID_OFFSET, KEEP_MAIL_SIZE,
    KEEP_ORIGINAL_SIZE, LAND_ID_BITMASK, LAND_ID_MASK, LAND_INFO_ID_OFFSET, OTHERS_SIZE,
    RECORD_SIZE, SAVE_BACK_OFFSET, SAVE_CHECK_CODE_OFFSET, SAVE_CHECK_LAND_ID_OFFSET,
    SAVE_CHECK_VERSION_OFFSET, SAVE_ID, SAVE_MAIN_OFFSET, SAVE_SIZE, SAVE_VERSION_CURRENT,
    SAVE_VERSION_LEGACY, TOWN_BLOCK_COUNT, TOWN_FILE_NAME,
};

/// Which auxiliary block a caller wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxBlock {
    /// `mCD_KEEP_ORIGINAL_SIZE` original-design block.
    Original,
    /// `mCD_KEEP_MAIL_SIZE` letter block.
    Mail,
    /// `mCD_KEEP_DIARY_SIZE` diary block.
    Diary,
}

/// Byte offsets of the three auxiliary blocks inside the payload, in the order
/// that actually passed checksum validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxLayout {
    /// Payload offset of the letter block.
    pub mail: usize,
    /// Payload offset of the original-design block.
    pub original: usize,
    /// Payload offset of the diary block.
    pub diary: usize,
    /// The mail block's `landid` did not match the save, so the legacy
    /// (original-first) ordering was used.
    pub legacy_order: bool,
}

impl AuxLayout {
    /// Payload offset of `block`.
    pub fn offset(&self, block: AuxBlock) -> usize {
        match block {
            AuxBlock::Original => self.original,
            AuxBlock::Mail => self.mail,
            AuxBlock::Diary => self.diary,
        }
    }

    /// Exact on-disk size of `block`.
    pub fn size(block: AuxBlock) -> usize {
        match block {
            AuxBlock::Original => KEEP_ORIGINAL_SIZE,
            AuxBlock::Mail => KEEP_MAIL_SIZE,
            AuxBlock::Diary => KEEP_DIARY_SIZE,
        }
    }
}

/// A validated town record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TownRecord {
    dir: CardDir,
    payload: Vec<u8>,
    save_copy: usize,
    land_id: u16,
    aux: AuxLayout,
}

impl TownRecord {
    /// Parse exactly one record: `RECORD_SIZE` bytes, header first.
    ///
    /// Short input is `TruncatedHeader`/`TruncatedPayload`; extra input is
    /// `Oversized`.  The writer in `pc_gci_write` emits exactly `RECORD_SIZE`
    /// bytes, so exactness is the record contract here.
    pub fn parse(bytes: &[u8]) -> Result<Self, GciError> {
        if bytes.len() > RECORD_SIZE {
            return Err(GciError::Oversized {
                found: bytes.len(),
                expected: RECORD_SIZE,
            });
        }
        if bytes.len() < HEADER_SIZE {
            return Err(GciError::TruncatedHeader {
                available: bytes.len(),
            });
        }
        Self::from_parts(bytes)
    }

    /// Parse a record from a buffer that may carry trailing bytes.
    ///
    /// Faithful to `pc_gci_read_image`, which `fread`s exactly
    /// `GCI_FILE_DATA_SIZE` payload bytes and never inspects anything after
    /// them.  Callers that need the strict contract use [`TownRecord::parse`].
    pub fn parse_prefix(bytes: &[u8]) -> Result<Self, GciError> {
        if bytes.len() < HEADER_SIZE {
            return Err(GciError::TruncatedHeader {
                available: bytes.len(),
            });
        }
        Self::from_parts(bytes)
    }

    fn from_parts(bytes: &[u8]) -> Result<Self, GciError> {
        let dir = CardDir::parse(bytes)?;
        if !dir.matches_file_name(TOWN_FILE_NAME) {
            return Err(GciError::FileNameMismatch);
        }
        if dir.block_count() != TOWN_BLOCK_COUNT {
            return Err(GciError::BadBlockCount {
                found: dir.block_count(),
                expected: TOWN_BLOCK_COUNT,
            });
        }
        let payload = bytes.get(HEADER_SIZE..HEADER_SIZE + FILE_DATA_SIZE).ok_or(
            GciError::TruncatedPayload {
                found: bytes.len().saturating_sub(HEADER_SIZE),
                expected: FILE_DATA_SIZE,
            },
        )?;
        let (save_copy, land_id) = select_save_copy(payload)?;
        let aux = aux_layout(payload, land_id)?;
        Ok(Self {
            dir,
            payload: payload.to_vec(),
            save_copy,
            land_id,
            aux,
        })
    }

    /// The decoded directory header.
    pub fn dir(&self) -> &CardDir {
        &self.dir
    }

    /// The full payload (`mCD_LAND_SAVE_SIZE` bytes, header excluded).
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Index of the `Save` copy that validated: 0 = main, 1 = embedded backup.
    pub fn save_copy(&self) -> usize {
        self.save_copy
    }

    /// The validated `Save` block (still big-endian, un-decoded).
    pub fn save_block(&self) -> &[u8] {
        let start = if self.save_copy == 0 {
            SAVE_MAIN_OFFSET
        } else {
            SAVE_BACK_OFFSET
        };
        &self.payload[start..start + SAVE_SIZE]
    }

    /// Land id shared by the validated `Save` copy and its land info.
    pub fn land_id(&self) -> u16 {
        self.land_id
    }

    /// The auxiliary block placement that passed checksum validation.
    pub fn aux_layout(&self) -> AuxLayout {
        self.aux
    }

    /// A checksum-validated auxiliary block (still big-endian).
    pub fn aux_block(&self, block: AuxBlock) -> &[u8] {
        let start = self.aux.offset(block);
        &self.payload[start..start + AuxLayout::size(block)]
    }
}

/// Port of `pc_gci_select_copy`: main copy first, then the embedded backup.
fn select_save_copy(payload: &[u8]) -> Result<(usize, u16), GciError> {
    for copy in 0..2usize {
        let start = SAVE_MAIN_OFFSET + copy * SAVE_SIZE;
        let block = &payload[start..start + SAVE_SIZE];
        let version =
            i32::from_be_bytes(block[SAVE_CHECK_VERSION_OFFSET..][..4].try_into().unwrap());
        let code = u32::from_be_bytes(block[SAVE_CHECK_CODE_OFFSET..][..4].try_into().unwrap());
        let check_id =
            u16::from_be_bytes(block[SAVE_CHECK_LAND_ID_OFFSET..][..2].try_into().unwrap());
        let land_id = u16::from_be_bytes(block[LAND_INFO_ID_OFFSET..][..2].try_into().unwrap());
        if (version == SAVE_VERSION_LEGACY || version == SAVE_VERSION_CURRENT)
            && code == SAVE_ID
            && check_id & LAND_ID_MASK == LAND_ID_BITMASK
            && check_id == land_id
            && checksum::verify_block(block)
        {
            return Ok((copy, land_id));
        }
    }
    Err(GciError::NoValidSaveCopy)
}

/// Port of `pc_gci_aux_offsets`: probe the auxiliary order from the mail
/// block's land id, then validate checksums for both candidate orderings.
fn aux_layout(payload: &[u8], land_id: u16) -> Result<AuxLayout, GciError> {
    let first_land_id = u16::from_be_bytes(
        payload[KEEP_MAIL_LANDID_OFFSET..KEEP_MAIL_LANDID_OFFSET + 2]
            .try_into()
            .unwrap(),
    );
    let prefer_legacy = first_land_id != land_id;

    for attempt in 0..2 {
        // Upstream: `legacy = prefer_legacy ^ attempt`.
        let legacy = prefer_legacy == (attempt == 0);
        let (first, second) = if legacy {
            (AuxBlock::Original, AuxBlock::Mail)
        } else {
            (AuxBlock::Mail, AuxBlock::Original)
        };
        let mut offset = AUX_START;
        let first_off = offset;
        offset += AuxLayout::size(first);
        let second_off = offset;
        offset += AuxLayout::size(second);
        let diary_off = offset;

        let layout = match (first, second) {
            (AuxBlock::Mail, AuxBlock::Original) => AuxLayout {
                mail: first_off,
                original: second_off,
                diary: diary_off,
                legacy_order: false,
            },
            _ => AuxLayout {
                mail: second_off,
                original: first_off,
                diary: diary_off,
                legacy_order: true,
            },
        };

        let blocks_ok = [AuxBlock::Original, AuxBlock::Mail, AuxBlock::Diary]
            .into_iter()
            .all(|block| {
                let start = layout.offset(block);
                let end = start + AuxLayout::size(block);
                end <= OTHERS_SIZE && checksum::verify_block(&payload[start..end])
            });
        if blocks_ok {
            return Ok(layout);
        }
    }
    Err(GciError::BadAuxChecksums)
}
