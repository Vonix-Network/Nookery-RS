//! Behavioural tests over the public API.

use crate::{
    CSTICK_DEADZONE, DiscVerificationState, InterfaceTheme, MASTER_VOLUME, MAX_FRAME_RATE,
    PRELOAD_TEXTURES, Renderer, STICK_DEADZONE, SettingValue, Settings, SettingsError,
    TravelMoveOutChance, UI_SCALE, WINDOW_HEIGHT, WINDOW_MODE, WINDOW_WIDTH, parse_legacy_settings,
    sanitize_ui_scale,
};

fn int(value: i64) -> SettingValue {
    SettingValue::Int(value)
}

fn boolean(value: bool) -> SettingValue {
    SettingValue::Bool(value)
}

fn text(value: &str) -> SettingValue {
    SettingValue::Text(value.to_owned())
}

#[test]
fn defaults_match_registry_declarations() {
    let s = Settings::default();
    assert_eq!(s.backend.disc_path, "game.rvz");
    assert_eq!(s.backend.disc_verification, DiscVerificationState::Unknown);
    assert!(!s.backend.skip_prelaunch);
    assert!(s.backend.check_for_updates);

    assert!(s.video.renderer.is_auto());
    assert_eq!(s.video.renderer.as_str(), "auto");
    assert_eq!(s.video.window_width, 1280);
    assert_eq!(s.video.window_height, 960);
    assert_eq!(s.video.window_mode, 0);
    assert_eq!(s.video.ui_scale, 0);
    assert_eq!(s.video.theme, InterfaceTheme::Dynamic);
    assert!(s.video.vsync);
    assert_eq!(s.video.max_frame_rate, 60);
    assert!(!s.video.hud_at_edges);

    assert_eq!(s.enhancements.preload_textures, 0);
    assert!(!s.enhancements.disable_resetti);
    assert!(!s.enhancements.disable_shop_visitor_requirement);
    assert!(s.enhancements.borderless_acres);
    assert!(s.enhancements.pillarbox_nes);
    assert!(s.enhancements.fast_text);
    assert_eq!(
        s.enhancements.travel_move_out_chance,
        TravelMoveOutChance::Default
    );

    assert_eq!(s.audio.master_volume, 100);

    assert_eq!(s.input.stick_deadzone, 12);
    assert_eq!(s.input.cstick_deadzone, 12);
    assert!(s.input.auto_assign_controller);
    assert!(!s.input.single_port_migrated);
    assert!(s.input.allow_background_input);
    assert!(!s.input.touch_controls);
    assert!(s.input.touch_haptics);
    assert!(!s.input.keyboard_typing);

    // A default model is valid by construction.
    assert_eq!(s.validate(), Ok(()));
}

#[test]
fn every_declared_key_resolves_and_round_trips() {
    let mut s = Settings::default();
    for key in Settings::KEYS {
        let value = s
            .get(key)
            .unwrap_or_else(|| panic!("key {key} missing from get"));
        s.set(key, value.clone())
            .unwrap_or_else(|e| panic!("key {key} rejected its own stored value: {e}"));
        assert_eq!(
            s.get(key).as_ref(),
            Some(&value),
            "key {key} did not round-trip"
        );
    }
    assert_eq!(s, Settings::default());
    assert_eq!(s.get("video.windowWidths"), None);
}

#[test]
fn integer_ranges_accept_bounds_and_reject_outside() {
    let cases: &[(&str, &std::ops::RangeInclusive<i64>)] = &[
        ("video.windowWidth", &WINDOW_WIDTH),
        ("video.windowHeight", &WINDOW_HEIGHT),
        ("video.windowMode", &WINDOW_MODE),
        ("video.maxFrameRate", &MAX_FRAME_RATE),
        ("enhancements.preloadTextures", &PRELOAD_TEXTURES),
        ("audio.masterVolume", &MASTER_VOLUME),
        ("input.stickDeadzone", &STICK_DEADZONE),
        ("input.cstickDeadzone", &CSTICK_DEADZONE),
    ];
    for (key, range) in cases {
        let (min, max) = (*range.start(), *range.end());
        let mut s = Settings::default();
        s.set(key, int(min))
            .unwrap_or_else(|e| panic!("{key} rejected min {min}: {e}"));
        assert_eq!(s.get(key).unwrap().as_int(), Some(min));
        s.set(key, int(max))
            .unwrap_or_else(|e| panic!("{key} rejected max {max}: {e}"));
        assert_eq!(s.get(key).unwrap().as_int(), Some(max));

        for bad in [min - 1, max + 1] {
            let mut s = Settings::default();
            let error = s
                .set(key, int(bad))
                .expect_err(&format!("{key} accepted {bad}"));
            assert_eq!(
                error,
                SettingsError::OutOfRange {
                    key: (*key).to_owned(),
                    value: bad,
                    min,
                    max
                },
                "{key} reported the wrong rejection"
            );
            assert_eq!(
                s,
                Settings::default(),
                "{key} mutated the model on rejection"
            );
        }
    }
}

#[test]
fn declared_range_constants_match_upstream() {
    assert_eq!(WINDOW_WIDTH, 320..=16384);
    assert_eq!(WINDOW_HEIGHT, 240..=16384);
    assert_eq!(WINDOW_MODE, 0..=2);
    assert_eq!(MAX_FRAME_RATE, 0..=1000);
    assert_eq!(PRELOAD_TEXTURES, 0..=2);
    assert_eq!(MASTER_VOLUME, 0..=100);
    assert_eq!(STICK_DEADZONE, 0..=40);
    assert_eq!(CSTICK_DEADZONE, 0..=40);
    assert_eq!(UI_SCALE, 50..=200);
}

#[test]
fn ui_scale_zero_stays_automatic_and_other_values_clamp() {
    assert_eq!(sanitize_ui_scale(0), 0);
    assert_eq!(sanitize_ui_scale(1), 50);
    assert_eq!(sanitize_ui_scale(49), 50);
    assert_eq!(sanitize_ui_scale(50), 50);
    assert_eq!(sanitize_ui_scale(125), 125);
    assert_eq!(sanitize_ui_scale(200), 200);
    assert_eq!(sanitize_ui_scale(201), 200);
    assert_eq!(sanitize_ui_scale(9_000), 200);
    assert_eq!(sanitize_ui_scale(-5), 50);
}

#[test]
fn validate_rejects_ui_scale_below_and_above_window() {
    let mut s = Settings::default();
    s.video.ui_scale = 0;
    assert_eq!(s.validate(), Ok(()));

    s.video.ui_scale = 49;
    assert_eq!(
        s.validate(),
        Err(vec![SettingsError::OutOfRange {
            key: "video.uiScale".to_owned(),
            value: 49,
            min: 50,
            max: 200
        }])
    );

    s.video.ui_scale = 201;
    assert_eq!(
        s.validate(),
        Err(vec![SettingsError::OutOfRange {
            key: "video.uiScale".to_owned(),
            value: 201,
            min: 50,
            max: 200
        }])
    );
}

#[test]
fn validate_reports_every_out_of_range_field() {
    let mut s = Settings::default();
    s.video.window_width = 319;
    s.video.window_height = 16_385;
    s.audio.master_volume = 101;
    s.video.ui_scale = 49;
    let errors = s.validate().expect_err("invalid model passed validation");
    let keys: Vec<_> = errors
        .iter()
        .map(|e| match e {
            SettingsError::OutOfRange { key, .. } => key.as_str(),
            other => panic!("unexpected error variant {other:?}"),
        })
        .collect();
    assert_eq!(
        keys,
        [
            "video.windowWidth",
            "video.windowHeight",
            "audio.masterVolume",
            "video.uiScale"
        ]
    );
}

#[test]
fn sanitize_repairs_ranged_fields_and_keeps_enums() {
    let mut s = Settings::default();
    s.video.window_width = 1;
    s.video.window_height = 99_999;
    s.video.window_mode = 7;
    s.video.max_frame_rate = -1;
    s.video.ui_scale = 10;
    s.enhancements.preload_textures = 3;
    s.audio.master_volume = 500;
    s.input.stick_deadzone = -3;
    s.input.cstick_deadzone = 41;
    s.video.theme = InterfaceTheme::Night;

    s.sanitize();

    assert_eq!(s.video.window_width, 320);
    assert_eq!(s.video.window_height, 16_384);
    assert_eq!(s.video.window_mode, 2);
    assert_eq!(s.video.max_frame_rate, 0);
    assert_eq!(s.video.ui_scale, 50);
    assert_eq!(s.enhancements.preload_textures, 2);
    assert_eq!(s.audio.master_volume, 100);
    assert_eq!(s.input.stick_deadzone, 0);
    assert_eq!(s.input.cstick_deadzone, 40);
    assert_eq!(s.video.theme, InterfaceTheme::Night);
    assert_eq!(s.validate(), Ok(()));

    s.video.ui_scale = 0;
    s.sanitize();
    assert_eq!(s.video.ui_scale, 0, "automatic must stay automatic");
}

#[test]
fn unknown_key_and_bad_value_have_distinct_errors() {
    let mut s = Settings::default();
    assert_eq!(
        s.set("video.nope", int(1)),
        Err(SettingsError::UnknownKey("video.nope".to_owned()))
    );
    assert!(matches!(
        s.set("video.vsync", int(1)),
        Err(SettingsError::InvalidValue { key, .. }) if key == "video.vsync"
    ));
    assert!(matches!(
        s.set("video.windowWidth", text("1280")),
        Err(SettingsError::InvalidValue { key, .. }) if key == "video.windowWidth"
    ));
    assert_eq!(s, Settings::default());
}

#[test]
fn enums_encode_and_decode_every_variant_both_ways() {
    for (variant, string, code) in [
        (DiscVerificationState::Unknown, "unknown", 0),
        (DiscVerificationState::Success, "success", 1),
        (DiscVerificationState::HashMismatch, "hash_mismatch", 2),
    ] {
        assert_eq!(variant.as_config_str(), string);
        assert_eq!(
            DiscVerificationState::from_config_str(string),
            Some(variant)
        );
        assert_eq!(variant.to_code(), code);
        assert_eq!(DiscVerificationState::from_code(code), Some(variant));
    }
    assert_eq!(DiscVerificationState::ALL.len(), 3);
    assert_eq!(DiscVerificationState::from_config_str("HashMismatch"), None);
    assert_eq!(DiscVerificationState::from_code(3), None);
    assert_eq!(
        DiscVerificationState::default(),
        DiscVerificationState::Unknown
    );

    for (variant, string, code) in [
        (InterfaceTheme::Dynamic, "dynamic", 0),
        (InterfaceTheme::Day, "day", 1),
        (InterfaceTheme::Night, "night", 2),
    ] {
        assert_eq!(variant.as_config_str(), string);
        assert_eq!(InterfaceTheme::from_config_str(string), Some(variant));
        assert_eq!(variant.to_code(), code);
        assert_eq!(InterfaceTheme::from_code(code), Some(variant));
    }
    assert_eq!(InterfaceTheme::ALL.len(), 3);
    assert_eq!(InterfaceTheme::default(), InterfaceTheme::Dynamic);

    for (variant, string, code) in [
        (TravelMoveOutChance::Ten, "10", 0),
        (TravelMoveOutChance::Fifty, "50", 1),
        (TravelMoveOutChance::Default, "default", 2),
    ] {
        assert_eq!(variant.as_config_str(), string);
        assert_eq!(TravelMoveOutChance::from_config_str(string), Some(variant));
        assert_eq!(variant.to_code(), code);
        assert_eq!(TravelMoveOutChance::from_code(code), Some(variant));
    }
    assert_eq!(TravelMoveOutChance::ALL.len(), 3);
    assert_eq!(TravelMoveOutChance::default(), TravelMoveOutChance::Default);
    // "10"/"50" are strings, not numbers, in the settings file.
    assert_eq!(TravelMoveOutChance::from_config_str("1"), None);
}

#[test]
fn enum_keys_accept_canonical_strings_and_reject_numbers() {
    let mut s = Settings::default();
    s.set("video.theme", text("night")).unwrap();
    assert_eq!(s.video.theme, InterfaceTheme::Night);
    s.set("backend.discVerification", text("hash_mismatch"))
        .unwrap();
    assert_eq!(
        s.backend.disc_verification,
        DiscVerificationState::HashMismatch
    );
    s.set("gameplay.travelMoveOutChance", text("10")).unwrap();
    assert_eq!(
        s.enhancements.travel_move_out_chance,
        TravelMoveOutChance::Ten
    );

    assert!(matches!(
        s.set("video.theme", int(1)),
        Err(SettingsError::InvalidValue { .. })
    ));
    assert!(matches!(
        s.set("gameplay.travelMoveOutChance", text("25")),
        Err(SettingsError::InvalidValue { .. })
    ));
    assert_eq!(
        s.video.theme,
        InterfaceTheme::Night,
        "rejection must not mutate"
    );
}

#[test]
fn renderer_is_a_stable_string_repaired_by_availability_list() {
    let mut s = Settings::default();
    assert!(Renderer::new("vulkan").is_ok());
    assert!(Renderer::new("").is_err());
    assert!(Renderer::new("Vulkan").is_err());
    assert!(Renderer::new("= bad").is_err());

    s.set("video.renderer", text("opengl")).unwrap();
    assert_eq!(s.video.renderer.as_str(), "opengl");
    s.sanitize_renderer(&["auto", "vulkan"]);
    assert!(s.video.renderer.is_auto());
    s.set("video.renderer", text("opengl")).unwrap();
    s.sanitize_renderer(&["auto", "vulkan", "opengl"]);
    assert_eq!(
        s.video.renderer.as_str(),
        "opengl",
        "supported value is preserved"
    );
    s.sanitize_renderer(&[]);
    assert!(
        s.video.renderer.is_auto(),
        "unsupported value falls back to auto"
    );

    assert!(matches!(
        s.set("video.renderer", int(2)),
        Err(SettingsError::InvalidValue { .. })
    ));
}

#[test]
fn legacy_parser_imports_recognized_lines_and_counts_them() {
    let text = "\
# legacy 3ds settings
; comment
[something]

window_width=1024
window_height=768
fullscreen=2
vsync=0
max_fps=30
preload_textures=1
disable_resetti=1
disable_shop_visitor_req=1
borderless_acres=0
nes_aspect=0
master_volume=42
stick_deadzone=7
cstick_deadzone=9
";
    let result = parse_legacy_settings(text);
    assert_eq!(result.imported, 13);
    assert!(result.is_effective());

    let s = &result.settings;
    assert_eq!(s.video.window_width, 1024);
    assert_eq!(s.video.window_height, 768);
    assert_eq!(s.video.window_mode, 2);
    assert!(!s.video.vsync);
    assert_eq!(s.video.max_frame_rate, 30);
    assert_eq!(s.enhancements.preload_textures, 1);
    assert!(s.enhancements.disable_resetti);
    assert!(s.enhancements.disable_shop_visitor_requirement);
    assert!(!s.enhancements.borderless_acres);
    assert!(!s.enhancements.pillarbox_nes);
    assert_eq!(s.audio.master_volume, 42);
    assert_eq!(s.input.stick_deadzone, 7);
    assert_eq!(s.input.cstick_deadzone, 9);
    assert_eq!(s.validate(), Ok(()));
}

#[test]
fn legacy_parser_ignores_malformed_and_out_of_rule_input() {
    let text = "\
window_width=639
window_height=479
fullscreen=3
vsync=2
max_fps=1001
preload_textures=3
disable_resetti=5
master_volume=101
stick_deadzone=41
unknown_key=1
window_width=not_a_number
window_width=
window_width=1024.5
	    window_height   =   600
no_equals_sign
=12
window_width=+900
window_width=99999999999999999999
";
    let result = parse_legacy_settings(text);
    // Only `window_height = 600` (whitespace-tolerant) is recognized:
    // `+900` fails from_chars, the overflow fails parsing, `.`/empty/word fail.
    assert_eq!(result.imported, 1);
    assert_eq!(result.settings.video.window_height, 600);
    assert_eq!(
        result.settings.video.window_width, 1280,
        "rejected values must not apply"
    );
    assert_eq!(result.settings.audio.master_volume, 100);
    assert_eq!(
        result.settings.enhancements.travel_move_out_chance,
        TravelMoveOutChance::Default
    );
}

#[test]
fn legacy_window_width_has_only_a_lower_bound_then_sanitize_repairs() {
    // Upstream imports `value >= 640` without an upper bound; the registry range
    // is enforced separately, which is why `sanitize` exists.
    let result = parse_legacy_settings("window_width=999999\nwindow_height=999999\n");
    assert_eq!(result.imported, 2);
    assert_eq!(result.settings.video.window_width, 999_999);
    assert!(result.settings.validate().is_err());

    let mut repaired = result.settings;
    repaired.sanitize();
    assert_eq!(repaired.video.window_width, 16_384);
    assert_eq!(repaired.video.window_height, 16_384);
    assert_eq!(repaired.validate(), Ok(()));
}

#[test]
fn legacy_parser_reports_zero_for_empty_or_unrecognized_file() {
    for text in [
        "",
        "\n\n",
        "# only comments\n",
        "[section]\n",
        "junk\n",
        "vsync=nope\n",
    ] {
        let result = parse_legacy_settings(text);
        assert_eq!(result.imported, 0, "input {text:?} should import nothing");
        assert!(!result.is_effective());
        assert_eq!(
            result.settings,
            Settings::default(),
            "input {text:?} must not change settings"
        );
    }
}

#[test]
fn legacy_zero_and_one_booleans_map_to_false_and_true() {
    let on =
        parse_legacy_settings("vsync=1\ndisable_resetti=1\nborderless_acres=1\nnes_aspect=1\n")
            .settings;
    assert!(on.video.vsync);
    assert!(on.enhancements.disable_resetti);
    assert!(on.enhancements.borderless_acres);
    assert!(on.enhancements.pillarbox_nes);

    let off =
        parse_legacy_settings("vsync=0\ndisable_resetti=0\nborderless_acres=0\nnes_aspect=0\n")
            .settings;
    assert!(!off.video.vsync);
    assert!(!off.enhancements.disable_resetti);
    assert!(!off.enhancements.borderless_acres);
    assert!(!off.enhancements.pillarbox_nes);
}

#[test]
fn duplicate_legacy_keys_keep_last_recognized_value_and_count_each() {
    let result = parse_legacy_settings("master_volume=10\nmaster_volume=20\nmaster_volume=200\n");
    // The third line is out of range, so it is not counted and does not apply.
    assert_eq!(result.imported, 2);
    assert_eq!(result.settings.audio.master_volume, 20);
}

#[test]
fn setting_value_accessors_are_variant_exact() {
    assert_eq!(int(5).as_int(), Some(5));
    assert_eq!(int(5).as_bool(), None);
    assert_eq!(int(5).as_text(), None);
    assert_eq!(boolean(true).as_bool(), Some(true));
    assert_eq!(boolean(true).as_int(), None);
    assert_eq!(text("x").as_text(), Some("x"));
    assert_eq!(text("x").as_bool(), None);
}
