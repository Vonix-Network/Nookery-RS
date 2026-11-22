use std::fs;
use std::path::Path;

use nookery_item_data::{SOURCE_MANIFEST, SourceManifest, values_hash};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

struct Expected {
    path: &'static str,
    symbol: &'static str,
    declared_len: Option<usize>,
    entry_count: usize,
    source_bytes: usize,
    source_sha256: &'static str,
    values_fnv1a64: u64,
}

// These fixed expectations make a hand-mutated generated value fail even if
// someone also changes the generated manifest metadata.
const EXPECTED: [Expected; 5] = [
    Expected {
        path: "src/data/item/md_price.c",
        symbol: "md_price_table",
        declared_len: None,
        entry_count: 56,
        source_bytes: 487,
        source_sha256: "535dc52df84d319b4674068cb8767f22458484ff30b7fddb40037bc89283557e",
        values_fnv1a64: 0xe6d3cc6b17f0f8a7,
    },
    Expected {
        path: "src/data/item/plant_price.c",
        symbol: "plant_price_table",
        declared_len: None,
        entry_count: 12,
        source_bytes: 147,
        source_sha256: "02287bd68df10e1a049aade79dd6f0fe78d2419b6735a8e3b70fe6c13127f76e",
        values_fnv1a64: 0xda1bc4d2dacab883,
    },
    Expected {
        path: "src/data/item/insect_price.c",
        symbol: "insect_price_table",
        declared_len: Some(46),
        entry_count: 46,
        source_bytes: 471,
        source_sha256: "75c9b9f5b008c55057cafd4b791a0104f6a28e18d50941f23701bdf3b937b39d",
        values_fnv1a64: 0xc98572794cc88a09,
    },
    Expected {
        path: "src/data/item/fish_price.c",
        symbol: "fish_price_table",
        declared_len: None,
        entry_count: 41,
        source_bytes: 451,
        source_sha256: "29ce1498abef5154b9d82332dd44480750a9e86fecedb65caf9df0cc5511f4d4",
        values_fnv1a64: 0xdf129e8cfdd35fa2,
    },
    Expected {
        path: "src/data/item/food_price.c",
        symbol: "food_price_table",
        declared_len: None,
        entry_count: 9,
        source_bytes: 123,
        source_sha256: "c120939c1de06fb321b3f44800284a679b17eb04d10209e9c1734e52e8e3dda6",
        values_fnv1a64: 0x7e3bdc36efbed688,
    },
];

fn manifest_for(symbol: &str) -> &'static SourceManifest {
    SOURCE_MANIFEST
        .iter()
        .find(|entry| entry.symbol == symbol)
        .expect("generated manifest entry")
}

#[test]
fn source_bytes_and_generated_values_match_fixed_oracle_manifest() {
    for expected in EXPECTED {
        let manifest = manifest_for(expected.symbol);
        assert_eq!(manifest.source_path, expected.path);
        assert_eq!(manifest.declared_len, expected.declared_len);
        assert_eq!(manifest.entry_count, expected.entry_count);
        assert_eq!(manifest.source_bytes, expected.source_bytes);
        assert_eq!(manifest.source_sha256, expected.source_sha256);
        assert_eq!(manifest.values_fnv1a64, expected.values_fnv1a64);

        let source = repo_root().join(expected.path);
        let bytes = fs::read(&source).expect("read C oracle source");
        assert_eq!(bytes.len(), expected.source_bytes, "byte count: {source:?}");
        assert_eq!(
            sha256_hex(&bytes),
            expected.source_sha256,
            "source hash: {source:?}"
        );
    }
}

#[test]
fn all_generated_tables_have_exact_width_and_hashes() {
    for expected in EXPECTED {
        let values = match expected.symbol {
            "md_price_table" => nookery_item_data::md_price_table,
            "plant_price_table" => nookery_item_data::plant_price_table,
            "insect_price_table" => nookery_item_data::insect_price_table,
            "fish_price_table" => nookery_item_data::fish_price_table,
            "food_price_table" => nookery_item_data::food_price_table,
            symbol => panic!("unexpected symbol {symbol}"),
        };
        assert_eq!(values.len(), expected.entry_count);
        assert_eq!(values_hash(values), expected.values_fnv1a64);
        assert_eq!(values.last(), Some(&u16::MAX));
        assert_eq!(values.iter().filter(|value| **value == u16::MAX).count(), 1);
    }
}

fn repo_root() -> &'static Path {
    Path::new(ROOT)
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
}

// Small dependency-free SHA-256 implementation for parity checks over the
// source bytes. The generator uses hashlib.sha256 for the same digest.
fn sha256_hex(input: &[u8]) -> String {
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64) * 8;
    let mut data = input.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    for chunk in data.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (word, bytes) in schedule[..16].iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("four bytes"));
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let mut working = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let choice = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(majority);
            working = [
                temp1.wrapping_add(temp2),
                working[0],
                working[1],
                working[2],
                working[3].wrapping_add(temp1),
                working[4],
                working[5],
                working[6],
            ];
        }
        for (word, value) in state.iter_mut().zip(working) {
            *word = word.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}
