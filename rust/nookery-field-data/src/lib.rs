//! Source-grounded Rust tables for the portable field-data translation units.
//!
//! The crate contains the 182 field C sources that have no generated
//! `assets/*.inc` payload dependency.  Pointer-bearing C records are modeled
//! with stable source-symbol names, while scalar fields retain their source
//! width and ordering.  This keeps the data usable without a C compiler or an
//! FFI wrapper and makes the disc-derived payload boundary explicit.

/// A source symbol used by a data pointer or an enum-like C constant.
pub type Symbol = &'static str;

/// One texture-scroll key from `EVW_ANIME_SCROLL`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scroll {
    pub x: i8,
    pub y: i8,
    pub width: u8,
    pub height: u8,
}

/// One source `EVW_ANIME_DATA` entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Animation {
    pub segment: i8,
    pub kind: i16,
    pub scrolls: &'static [Scroll],
}

/// One source `mFM_move_actor_data_c` row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveActorData {
    pub name_id: Symbol,
    pub block_x: i32,
    pub block_z: i32,
    pub ut_x: u8,
    pub ut_z: u8,
    pub npc_info_idx: i32,
    pub arg: i32,
    pub desired_block_kind: Symbol,
}

/// Raw binary form consumed by the `TARGET_PC` endian bootstrap in
/// `field_data.c`. The six museum actables are generated disc payloads, so
/// their rows are represented here as an explicit byte-swappable type rather
/// than being fabricated in this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawMoveActorData {
    pub name_id: u16,
    pub block_x: u32,
    pub block_z: u32,
    pub ut_x: u8,
    pub ut_z: u8,
    pub npc_info_idx: i8,
    pub arg: i16,
    pub desired_block_kind: u32,
}

/// Port `mFM_SwapActableOnce`: swap each raw row in place until `RSV_NO`.
pub fn swap_actable_once(table: &mut [RawMoveActorData]) {
    for row in table {
        if row.name_id == u16::MAX {
            break;
        }
        row.name_id = row.name_id.swap_bytes();
        row.block_x = row.block_x.swap_bytes();
        row.block_z = row.block_z.swap_bytes();
        row.arg = row.arg.swap_bytes();
        row.desired_block_kind = row.desired_block_kind.swap_bytes();
    }
}

/// Port `mFM_InitActableEndian` for the six disc-derived museum tables.
pub fn init_actable_endian(tables: &mut [&mut [RawMoveActorData]; 6]) {
    for table in tables {
        swap_actable_once(table);
    }
}

/// One packed field block-combination entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Combination {
    pub combination_type: Symbol,
    pub height: u8,
}

/// One source `mFM_field_data_c` record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldData {
    pub field_name: Symbol,
    pub block_x_max: u8,
    pub block_z_max: u8,
    pub combinations: &'static [Combination],
    pub move_actor_table: Option<Symbol>,
    pub reserved: [u32; 4],
}

/// One collision unit from a source `mCoBG_Collision_u` initializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Collision {
    /// Bitfield members in C declaration order: slate, center, top-left,
    /// bottom-left, bottom-right, and top-right.
    pub values: [u8; 6],
    pub attribute: Symbol,
}

/// One background sound source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundSource {
    pub kind: Symbol,
    pub ut_x: u8,
    pub ut_z: u8,
}

/// One source `mFM_bg_data_c` record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BgData {
    pub bg_id: Symbol,
    pub opaque_gfx: Option<Symbol>,
    pub translucent_gfx: Option<Symbol>,
    pub animation: Option<Symbol>,
    pub animation_count: i32,
    pub rom_start_addr: u32,
    pub rom_end_addr: u32,
    pub collision: &'static [Collision],
    pub sound_source: &'static [SoundSource],
}

/// Manifest entry for one converted source translation unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFile {
    pub path: &'static str,
    pub kind: &'static str,
    pub symbol: &'static str,
    pub sha256: &'static str,
}

#[rustfmt::skip]
mod generated;
mod source_manifest;

pub use generated::*;
pub use source_manifest::SOURCE_MANIFEST;

/// Number of field translation units converted into this crate.
pub const CONVERTED_SOURCE_COUNT: usize = 182;

/// Number of field translation units whose payload is generated from a disc.
pub const GENERATED_FROM_DISC_COUNT: usize = 274;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_manifest_and_tables_have_expected_scope() {
        assert_eq!(SOURCE_MANIFEST.len(), CONVERTED_SOURCE_COUNT);
        assert_eq!(data_fdd.len(), 52);
        assert_eq!(data_bgd.len(), 295);
    }

    #[test]
    fn generated_disc_payloads_are_not_embedded() {
        assert_eq!(GENERATED_FROM_DISC_COUNT, 274);
        assert!(
            SOURCE_MANIFEST
                .iter()
                .all(|entry| !entry.path.contains("assets/"))
        );
    }

    #[test]
    fn aggregate_tables_preserve_fixed_source_shapes() {
        assert!(data_fdd.iter().all(|entry| entry.combinations.len() == 70));
        assert!(data_bgd.iter().all(|entry| entry.collision.len() == 256));
        assert!(data_bgd.iter().all(|entry| entry.sound_source.len() == 6));
    }

    #[test]
    fn raw_actable_swap_stops_at_the_reserved_sentinel() {
        let mut rows = [
            RawMoveActorData {
                name_id: 0x1234,
                block_x: 0x0102_0304,
                block_z: 0xA0B0_C0D0,
                ut_x: 0,
                ut_z: 0,
                npc_info_idx: -1,
                arg: 0x1122,
                desired_block_kind: 0x5566_7788,
            },
            RawMoveActorData {
                name_id: u16::MAX,
                block_x: 0,
                block_z: 0,
                ut_x: 0,
                ut_z: 0,
                npc_info_idx: 0,
                arg: 0,
                desired_block_kind: 0,
            },
        ];
        swap_actable_once(&mut rows);
        assert_eq!(rows[0].name_id, 0x3412);
        assert_eq!(rows[0].block_x, 0x0403_0201);
        assert_eq!(rows[0].block_z, 0xD0C0_B0A0);
        assert_eq!(rows[0].arg, 0x2211);
        assert_eq!(rows[0].desired_block_kind, 0x8877_6655);
        assert_eq!(rows[1].name_id, u16::MAX);
    }
}
