//! Typed byte-swap tests over the public API, using hand-built byte images.
//!
//! Every vector below is derived from the C sources, not from this crate's
//! output: layouts from the headers cited in `typed_swap`, swap sets from
//! `pc/src/pc_save_bswap.c:181-201`.

use nookery_save::typed_swap::{
    AnimalPersonalId, Direction, LandInfo, PersonalId, RtcTime, RtcYmd, TypedSwap,
};
use std::mem::{align_of, size_of};

/// `sizeof`/`_Alignof` of each mirrored struct, straight from the headers.
#[test]
fn sizes_and_alignments_match_c_headers() {
    assert_eq!(size_of::<RtcTime>(), 0x08);
    assert_eq!(size_of::<RtcYmd>(), 0x04);
    assert_eq!(size_of::<PersonalId>(), 0x14);
    assert_eq!(size_of::<AnimalPersonalId>(), 0x0E);
    assert_eq!(size_of::<LandInfo>(), 0x0C);

    assert_eq!(align_of::<RtcTime>(), 2);
    assert_eq!(align_of::<RtcYmd>(), 2);
    assert_eq!(align_of::<PersonalId>(), 2);
    assert_eq!(align_of::<AnimalPersonalId>(), 2);
    assert_eq!(align_of::<LandInfo>(), 2);

    // `SIZE` constants agree with the real layout.
    assert_eq!(<RtcTime as TypedSwap>::SIZE, size_of::<RtcTime>());
    assert_eq!(<RtcYmd as TypedSwap>::SIZE, size_of::<RtcYmd>());
    assert_eq!(<PersonalId as TypedSwap>::SIZE, size_of::<PersonalId>());
    assert_eq!(
        <AnimalPersonalId as TypedSwap>::SIZE,
        size_of::<AnimalPersonalId>()
    );
    assert_eq!(<LandInfo as TypedSwap>::SIZE, size_of::<LandInfo>());
}

/// Raw disk bytes for `lbRTC_time_c`: `sec=1 min=2 hour=3 day=4 weekday=5
/// month=6 year=2020`, with the year big-endian as on the card.
#[test]
fn rtc_time_swaps_only_the_be_year() {
    let disk = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xE4];
    let mut t = RtcTime::from_bytes(&disk);
    t.swap_from_be();

    assert_eq!(
        t,
        RtcTime {
            sec: 1,
            min: 2,
            hour: 3,
            day: 4,
            weekday: 5,
            month: 6,
            year: 2020,
        }
    );

    // The five `u8` fields are untouched byte-for-byte.
    assert_eq!(t.to_bytes()[0..6], disk[0..6]);

    // Back to the image: the year returns to big-endian, the rest unchanged.
    t.swap_to_be();
    assert_eq!(t.to_bytes(), disk);
}

/// `lbRTC_ymd_c` is `{ u16 year; u8 month; u8 day; }`; only the year swaps.
#[test]
fn rtc_ymd_swaps_only_the_be_year() {
    let disk = [0x0C, 0x1F, 0x0C, 0x19]; // 3103-12-25
    let mut ymd = RtcYmd::from_bytes(&disk);
    ymd.swap_from_be();

    assert_eq!(
        ymd,
        RtcYmd {
            year: 3103,
            month: 12,
            day: 25
        }
    );
    assert_eq!(ymd.to_bytes()[2..4], disk[2..4]);

    ymd.swap_to_be();
    assert_eq!(ymd.to_bytes(), disk);
}

/// `PersonalID_c`: names at `0x00`/`0x08` are raw bytes; `player_id` at `0x10`
/// and `land_id` at `0x12` are the swapped pair.
#[test]
fn personal_id_swaps_both_ids_and_preserves_names() {
    let mut disk = [0u8; 0x14];
    disk[0x00..0x08].copy_from_slice(b"BOB     ");
    disk[0x08..0x10].copy_from_slice(b"NOOKERY ");
    disk[0x10..0x12].copy_from_slice(&[0x12, 0x34]);
    disk[0x12..0x14].copy_from_slice(&[0x30, 0x01]);

    let mut pid = PersonalId::from_bytes(&disk);
    pid.swap_from_be();

    assert_eq!(pid.player_id, 0x1234);
    assert_eq!(pid.land_id, 0x3001);
    assert_eq!(&pid.player_name, b"BOB     ");
    assert_eq!(&pid.land_name, b"NOOKERY ");
    assert_eq!(pid.to_bytes()[0x00..0x10], disk[0x00..0x10]);

    pid.swap_to_be();
    assert_eq!(pid.to_bytes(), disk);
}

/// `AnmPersonalID_c`: `npc_id` at `0x00` and `land_id` at `0x02` swap; the town
/// name at `0x04`, `name_id` at `0x0C`, and `looks` at `0x0D` do not.
#[test]
fn animal_personal_id_swaps_leading_ids_only() {
    let mut disk = [0u8; 0x0E];
    disk[0x00..0x02].copy_from_slice(&[0x25, 0x0F]);
    disk[0x02..0x04].copy_from_slice(&[0x30, 0x01]);
    disk[0x04..0x0C].copy_from_slice(b"NOOKERY ");
    disk[0x0C] = 0x0F;
    disk[0x0D] = 5;

    let mut anm = AnimalPersonalId::from_bytes(&disk);
    anm.swap_from_be();

    assert_eq!(anm.npc_id, 0x250F);
    assert_eq!(anm.land_id, 0x3001);
    assert_eq!(anm.name_id, 0x0F);
    assert_eq!(anm.looks, 5);
    assert_eq!(&anm.land_name, b"NOOKERY ");
    assert_eq!(anm.to_bytes()[0x04..0x0E], disk[0x04..0x0E]);

    anm.swap_to_be();
    assert_eq!(anm.to_bytes(), disk);
}

/// `mLd_land_info_c`: only `id` at `0x0A` swaps. `exists` at `0x08` and the
/// alignment byte at `0x09` survive both directions untouched.
#[test]
fn land_info_swaps_id_and_preserves_exists_and_padding() {
    let mut disk = [0u8; 0x0C];
    disk[0x00..0x08].copy_from_slice(b"NOOKERY ");
    disk[0x08] = 0xFF; // exists = -1
    disk[0x09] = 0xA5; // alignment padding: arbitrary, must not be touched
    disk[0x0A..0x0C].copy_from_slice(&[0x30, 0x01]);

    let mut li = LandInfo::from_bytes(&disk);
    li.swap_from_be();

    assert_eq!(li.id, 0x3001);
    assert_eq!(li.exists, -1);
    assert_eq!(li.pad, 0xA5, "padding byte must survive the swap");
    assert_eq!(&li.name, b"NOOKERY ");

    let after_swap = li.to_bytes();
    assert_eq!(after_swap[0x08], 0xFF);
    assert_eq!(after_swap[0x09], 0xA5);

    li.swap_to_be();
    assert_eq!(li.to_bytes(), disk);
}

/// Zero and all-`0xFF` images: the all-ones case proves no field is skipped on
/// a byte boundary and that every `u16` really is byte-reversed.
#[test]
fn zero_and_all_ones_images_round_trip_through_both_directions() {
    let mut zero = RtcTime::from_bytes(&[0u8; 8]);
    assert_eq!(zero.year, 0);
    zero.swap_from_be();
    assert_eq!(zero.to_bytes()[6..8], [0, 0]);

    let mut ones = PersonalId::from_bytes(&[0xFF; 0x14]);
    ones.swap_from_be();
    assert_eq!(ones.player_id, 0xFFFF);
    assert_eq!(ones.land_id, 0xFFFF);
    assert_eq!(ones.to_bytes(), [0xFF; 0x14]);

    let mut li = LandInfo::from_bytes(&[0xFF; 0x0C]);
    li.swap_from_be();
    assert_eq!(li.id, 0xFFFF);
    assert_eq!(li.exists, -1);
    assert_eq!(li.pad, 0xFF);
    assert_eq!(li.to_bytes(), [0xFF; 0x0C]);
}

/// Boundary years from `lbRTC_YEAR_MIN`/`lbRTC_YEAR_MAX` and the byte-swap
/// boundary values `0x0100`/`0x00FF`.
#[test]
fn boundary_u16_values_swap_correctly() {
    for (raw, expect) in [(0x00FFu16, 0xFF00u16), (0x0100, 0x0001), (0x8000, 0x0080)] {
        let mut ymd = RtcYmd {
            year: raw,
            month: 0,
            day: 0,
        };
        ymd.swap_from_be();
        assert_eq!(ymd.year, expect);
    }

    // 1901 (`lbRTC_YEAR_MIN`) and 2999 (last playable year under TARGET_PC).
    let mut t = RtcTime {
        year: 1901,
        ..RtcTime::default()
    };
    t.swap_to_be();
    let be = t.to_bytes();
    assert_eq!(be[6..8], 1901u16.to_be_bytes());
    t.swap_from_be();
    assert_eq!(t.year, 1901);

    let mut t = RtcTime {
        year: 2999,
        ..RtcTime::default()
    };
    t.swap_to_be();
    assert_eq!(t.to_bytes()[6..8], 2999u16.to_be_bytes());
}

/// `swap` dispatches on the direction enum, and each direction is an involution.
#[test]
fn direction_enum_dispatch_is_involutive() {
    let disk = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xE4];
    let original = RtcTime::from_bytes(&disk);

    let mut t = original;
    t.swap(Direction::FromBe);
    let swapped = t;
    assert_ne!(swapped, original);

    t.swap(Direction::FromBe);
    assert_eq!(t, original);

    t.swap(Direction::ToBe);
    assert_eq!(t, swapped);

    t.swap(Direction::FromBe.flipped());
    assert_eq!(t, original);

    assert_eq!(Direction::FromBe.flipped(), Direction::ToBe);
    assert_eq!(Direction::ToBe.flipped(), Direction::FromBe);
}

/// `swap_from_be` then `swap_to_be` is the identity for every type, including
/// a non-trivial image with all byte values present.
#[test]
fn from_be_then_to_be_is_the_identity() {
    let mut rtc = RtcTime::from_bytes(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
    rtc.swap_from_be();
    rtc.swap_to_be();
    assert_eq!(
        rtc.to_bytes(),
        [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
    );

    let mut ymd = RtcYmd::from_bytes(&[0x0C, 0x1F, 0x0C, 0x19]);
    ymd.swap_from_be();
    ymd.swap_to_be();
    assert_eq!(ymd.to_bytes(), [0x0C, 0x1F, 0x0C, 0x19]);

    let mut pid = PersonalId::from_bytes(&[0xA7; 0x14]);
    pid.swap_from_be();
    pid.swap_to_be();
    assert_eq!(pid.to_bytes(), [0xA7; 0x14]);

    let mut anm = AnimalPersonalId::from_bytes(&[0x3C; 0x0E]);
    anm.swap_from_be();
    anm.swap_to_be();
    assert_eq!(anm.to_bytes(), [0x3C; 0x0E]);

    let mut li = LandInfo::from_bytes(&[0x5A; 0x0C]);
    li.swap_from_be();
    li.swap_to_be();
    assert_eq!(li.to_bytes(), [0x5A; 0x0C]);
}

/// Native-image round-trip through `to_bytes`/`from_bytes` is lossless,
/// including `LandInfo`'s padding byte.
#[test]
fn byte_image_round_trip_is_lossless() {
    for seed in 0u8..8 {
        let image: Vec<u8> = (0u8..0x14)
            .map(|b| b.wrapping_mul(seed).wrapping_add(seed))
            .collect();

        let pid_bytes: [u8; 0x14] = image[..0x14].try_into().unwrap();
        assert_eq!(PersonalId::from_bytes(&pid_bytes).to_bytes(), pid_bytes);

        let anm_bytes: [u8; 0x0E] = image[..0x0E].try_into().unwrap();
        assert_eq!(
            AnimalPersonalId::from_bytes(&anm_bytes).to_bytes(),
            anm_bytes
        );

        let li_bytes: [u8; 0x0C] = image[..0x0C].try_into().unwrap();
        assert_eq!(LandInfo::from_bytes(&li_bytes).to_bytes(), li_bytes);
    }
}
