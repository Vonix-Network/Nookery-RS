//! Typed byte-swap tests over the public API, using hand-built byte images.
//!
//! Every vector below is derived from the C sources, not from this crate's
//! output: layouts from the headers cited in `typed_swap`, swap sets from
//! `pc/src/pc_save_bswap.c:181-201`.

use nookery_save::typed_swap::{
    AnimalPersonalId, Direction, LandInfo, Mail, MailContent, MailHeader, MailName, PersonalId,
    RtcTime, RtcYmd, TypedSwap,
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

/// `Mail_nm_c` has a PersonalID at `0x00`, type at `0x14`, and implicit pad at
/// `0x15`; the explicit Rust pad makes that C byte lossless.
#[test]
fn mail_name_size_alignment_and_padding_match_c_layout() {
    assert_eq!(size_of::<MailName>(), 0x16);
    assert_eq!(align_of::<MailName>(), 2);
    assert_eq!(<MailName as TypedSwap>::SIZE, 0x16);

    let name = MailName {
        personal_id: PersonalId::default(),
        type_: 0xA5,
        pad: 0x5A,
    };
    let bytes = name.to_bytes();
    assert_eq!(bytes[0x14], 0xA5);
    assert_eq!(bytes[0x15], 0x5A);
}

/// `Mail_hdr_c` is two adjacent `Mail_nm_c` values, recipient then sender.
#[test]
fn mail_header_size_and_nested_offsets_match_c_layout() {
    assert_eq!(size_of::<MailHeader>(), 0x2C);
    assert_eq!(align_of::<MailHeader>(), 2);
    assert_eq!(<MailHeader as TypedSwap>::SIZE, 0x2C);

    let header = MailHeader {
        recipient: MailName {
            personal_id: PersonalId {
                player_id: 0x1234,
                ..PersonalId::default()
            },
            type_: 1,
            pad: 2,
        },
        sender: MailName {
            personal_id: PersonalId {
                player_id: 0x5678,
                ..PersonalId::default()
            },
            type_: 3,
            pad: 4,
        },
    };
    let bytes = header.to_bytes();
    assert_eq!(&bytes[0x10..0x12], &0x1234u16.to_ne_bytes());
    assert_eq!(bytes[0x14], 1);
    assert_eq!(bytes[0x15], 2);
    assert_eq!(&bytes[0x26..0x28], &0x5678u16.to_ne_bytes());
    assert_eq!(bytes[0x2A], 3);
    assert_eq!(bytes[0x2B], 4);
}

/// `Mail_ct_c` consists entirely of bytes, with lengths `0x18`, `0xC0`, and
/// `0x20` for its three arrays.
#[test]
fn mail_content_is_byte_preserving_and_size_matches_c_layout() {
    assert_eq!(size_of::<MailContent>(), 0xFC);
    assert_eq!(align_of::<MailContent>(), 1);
    assert_eq!(<MailContent as TypedSwap>::SIZE, 0xFC);

    let image = std::array::from_fn(|index| (index as u8).wrapping_mul(37));
    let mut content = MailContent::from_bytes(&image);
    content.swap_from_be();
    assert_eq!(content.to_bytes(), image);
    content.swap_to_be();
    assert_eq!(content.to_bytes(), image);
}

/// `Mail_c` places `present` immediately after its `0x2C`-byte header.
#[test]
fn mail_size_and_present_offset_match_c_layout() {
    assert_eq!(size_of::<Mail>(), 0x12A);
    assert_eq!(align_of::<Mail>(), 2);
    assert_eq!(<Mail as TypedSwap>::SIZE, 0x12A);

    let mail = Mail {
        present: 0x1234,
        ..Mail::default()
    };
    let bytes = mail.to_bytes();
    assert_eq!(&bytes[0x02C..0x02E], &0x1234u16.to_ne_bytes());
    assert_eq!(&bytes[0x02E..0x12A], &[0u8; 0xFC]);
}

/// Direct expected image: only the two PersonalID IDs reverse; type and the
/// otherwise implicit pad remain exactly where the C layout puts them.
#[test]
fn mail_name_swaps_personal_ids_but_preserves_type_and_padding() {
    let mut disk = [0u8; 0x16];
    disk[0x00..0x08].copy_from_slice(b"RECIP   ");
    disk[0x08..0x10].copy_from_slice(b"LAND    ");
    disk[0x10..0x12].copy_from_slice(&[0x12, 0x34]);
    disk[0x12..0x14].copy_from_slice(&[0x56, 0x78]);
    disk[0x14] = 0xA1;
    disk[0x15] = 0xB2;

    let mut name = MailName::from_bytes(&disk);
    name.swap_from_be();

    let expected = [
        b'R', b'E', b'C', b'I', b'P', b' ', b' ', b' ', b'L', b'A', b'N', b'D', b' ', b' ', b' ',
        b' ', 0x34, 0x12, 0x78, 0x56, 0xA1, 0xB2,
    ];
    assert_eq!(name.to_bytes(), expected);
    assert_eq!(name.type_, 0xA1);
    assert_eq!(name.pad, 0xB2);
}

/// Direct expected image: recipient and sender IDs reverse, while all other
/// bytes in both names stay unchanged.
#[test]
fn mail_header_swaps_both_names_only() {
    let mut disk = [0u8; 0x2C];
    for (index, byte) in disk.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let mut expected = disk;
    expected[0x10..0x12].reverse();
    expected[0x12..0x14].reverse();
    expected[0x26..0x28].reverse();
    expected[0x28..0x2A].reverse();

    let mut header = MailHeader::from_bytes(&disk);
    header.swap_from_be();
    assert_eq!(header.to_bytes(), expected);
}

/// `swap_Mail` recurses through the header and swaps `present`; every content
/// byte remains the original byte image.
#[test]
fn mail_swaps_present_and_header_but_not_content() {
    let mut disk = [0u8; Mail::SIZE];
    for (index, byte) in disk.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(11).wrapping_add(3);
    }
    let mut expected = disk;
    expected[0x10..0x12].reverse();
    expected[0x12..0x14].reverse();
    expected[0x26..0x28].reverse();
    expected[0x28..0x2A].reverse();
    expected[0x2C..0x2E].reverse();

    let mut mail = Mail::from_bytes(&disk);
    mail.swap_from_be();
    assert_eq!(mail.to_bytes(), expected);
    assert_eq!(&mail.to_bytes()[0x02E..], &disk[0x02E..]);
}

/// Both public directions are byte-identical involutions for the complete
/// Mail mirror, including nested names and byte-only content.
#[test]
fn mail_round_trip_is_lossless_in_both_directions() {
    let image = std::array::from_fn(|index| (index as u8).wrapping_mul(19).wrapping_add(7));

    let mut from_be = Mail::from_bytes(&image);
    from_be.swap_from_be();
    from_be.swap_to_be();
    assert_eq!(from_be.to_bytes(), image);

    let mut to_be = Mail::from_bytes(&image);
    to_be.swap_to_be();
    to_be.swap_from_be();
    assert_eq!(to_be.to_bytes(), image);
}

/// Degenerate byte images still exercise every nested swap and are
/// involutive in both directions.
#[test]
fn mail_zero_and_all_ones_images_are_involutive() {
    for image in [[0u8; Mail::SIZE], [0xFFu8; Mail::SIZE]] {
        let mut from_be = Mail::from_bytes(&image);
        from_be.swap_from_be();
        from_be.swap_from_be();
        assert_eq!(from_be.to_bytes(), image);

        let mut to_be = Mail::from_bytes(&image);
        to_be.swap_to_be();
        to_be.swap_to_be();
        assert_eq!(to_be.to_bytes(), image);
    }
}
