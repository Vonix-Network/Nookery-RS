//! Source-grounded item price tables from the C oracle.
//!
//! The generated tables preserve the C declaration order, duplicate values,
//! `unsigned short` width, and terminating `-1` initializer. In Rust, that
//! C conversion is represented by the exact `u16` value `65535` (`0xFFFF`).
//! The generated source and manifest are checked by the integration tests.

mod generated;

pub use generated::{
    FISH_PRICE_TABLE, FOOD_PRICE_TABLE, INSECT_PRICE_TABLE, MD_PRICE_TABLE, PLANT_PRICE_TABLE,
    SOURCE_MANIFEST, SourceManifest, fish_price_table, food_price_table, insect_price_table,
    md_price_table, plant_price_table,
};

/// The bounded set of item-price tables ported by this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriceTable {
    /// `src/data/item/md_price.c`.
    Md,
    /// `src/data/item/plant_price.c`.
    Plant,
    /// `src/data/item/insect_price.c`.
    Insect,
    /// `src/data/item/fish_price.c`.
    Fish,
    /// `src/data/item/food_price.c`.
    Food,
}

impl PriceTable {
    /// The exact C symbol represented by this table.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Md => "md_price_table",
            Self::Plant => "plant_price_table",
            Self::Insect => "insect_price_table",
            Self::Fish => "fish_price_table",
            Self::Food => "food_price_table",
        }
    }

    /// The generated table values, including the C sentinel.
    #[must_use]
    pub const fn values(self) -> &'static [u16] {
        match self {
            Self::Md => md_price_table,
            Self::Plant => plant_price_table,
            Self::Insect => insect_price_table,
            Self::Fish => fish_price_table,
            Self::Food => food_price_table,
        }
    }
}

/// FNV-1a over bytes, used by the source manifest and parity tests.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// FNV-1a over the table's little-endian `u16` byte representation.
#[must_use]
pub fn values_hash(values: &[u16]) -> u64 {
    let mut bytes_hash = 0xcbf29ce484222325;
    for value in values {
        for byte in value.to_le_bytes() {
            bytes_hash ^= u64::from(byte);
            bytes_hash = bytes_hash.wrapping_mul(0x100000001b3);
        }
    }
    bytes_hash
}

#[cfg(test)]
mod tests {
    use super::{PriceTable, SOURCE_MANIFEST};

    #[test]
    fn manifest_covers_each_public_table_once() {
        assert_eq!(SOURCE_MANIFEST.len(), 5);
        for table in [
            PriceTable::Md,
            PriceTable::Plant,
            PriceTable::Insect,
            PriceTable::Fish,
            PriceTable::Food,
        ] {
            let matches = SOURCE_MANIFEST
                .iter()
                .filter(|entry| entry.symbol == table.symbol())
                .count();
            assert_eq!(matches, 1, "missing manifest entry for {}", table.symbol());
        }
    }

    #[test]
    fn every_table_ends_in_the_unsigned_short_sentinel() {
        for table in [
            PriceTable::Md,
            PriceTable::Plant,
            PriceTable::Insect,
            PriceTable::Fish,
            PriceTable::Food,
        ] {
            assert_eq!(table.values().last(), Some(&u16::MAX));
        }
    }
}
