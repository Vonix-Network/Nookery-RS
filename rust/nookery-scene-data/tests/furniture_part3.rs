use std::fs;
use std::path::Path;

use nookery_scene_data::{FURNITURE_PROFILES, SOURCE_MANIFEST};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under rust/")
        .to_path_buf()
}

fn profile_fields(source: &str, symbol: &str) -> Vec<String> {
    let marker = format!("aFTR_PROFILE {symbol}");
    let start = source.find(&marker).expect("profile marker");
    let open = source[start..].find('{').expect("profile opening brace") + start;
    let mut depth = 0i32;
    let mut close = open;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            close = open + offset;
            break;
        }
    }
    // Furniture sources carry `// clang-format off` markers and per-field
    // annotations inside the initializer. Strip them before splitting, or a
    // field's text starts with a comment and compares unequal to the value the
    // generator read from the same source.
    let raw_body = &source[open + 1..close];
    let mut body = String::with_capacity(raw_body.len());
    let mut rest = raw_body;
    while let Some(pos) = rest.find("//") {
        let (head, tail) = rest.split_at(pos);
        body.push_str(head);
        match tail.find('\n') {
            Some(nl) => rest = &tail[nl..],
            None => {
                rest = "";
                break;
            }
        }
    }
    body.push_str(rest);

    // Two sources wrap the height field in `#if VERSION >= VER_GAFU01_00`.
    // The tree builds with VERSION 0, so the `#else` arm is the live value;
    // collapse the conditional the same way the generator does.
    let mut resolved = String::with_capacity(body.len());
    let mut stack: Vec<bool> = Vec::new();
    for line in body.lines() {
        let s = line.trim();
        if s.starts_with("#if") {
            stack.push(false);
            continue;
        }
        if s.starts_with("#else") {
            if let Some(last) = stack.last_mut() {
                *last = true;
            }
            continue;
        }
        if s.starts_with("#endif") {
            stack.pop();
            continue;
        }
        if stack.iter().all(|kept| *kept) {
            resolved.push_str(line);
            resolved.push('\n');
        }
    }
    let body = resolved.as_str();
    let mut result = Vec::new();
    let mut current = String::new();
    let mut braces = 0i32;
    let mut parens = 0i32;
    for byte in body.bytes() {
        match byte {
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'(' => parens += 1,
            b')' => parens -= 1,
            b',' if braces == 0 && parens == 0 => {
                result.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(byte as char);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn pointer(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "NULL" || value == "0" {
        None
    } else {
        Some(value.trim_start_matches('&').to_string())
    }
}

fn number(value: &str) -> u16 {
    match value.trim() {
        "aFTR_SHAPE_TYPEB_90" | "mCoBG_FTR_TYPEA" => 0,
        "aFTR_SHAPE_TYPEB_180" | "mCoBG_FTR_TYPEB_0" => 1,
        "aFTR_SHAPE_TYPEB_270" | "mCoBG_FTR_TYPEB_180" => 2,
        "aFTR_SHAPE_TYPEB_0" | "mCoBG_FTR_TYPEB_270" => 3,
        "aFTR_SHAPE_TYPEA" | "mCoBG_FTR_TYPEB_90" => 4,
        "aFTR_SHAPE_TYPEC" | "mCoBG_FTR_TYPEC" => 5,
        "aFTR_CONTACT_ACTION_CHAIR_UNIDIRECTIONAL" => 1,
        "aFTR_CONTACT_ACTION_CHAIR_MULTIDIRECTIONAL" => 2,
        "aFTR_CONTACT_ACTION_CHAIR_SOFA" => 4,
        "aFTR_CONTACT_ACTION_BED_SINGLE" => 8,
        "aFTR_CONTACT_ACTION_BED_DOUBLE" => 16,
        "aFTR_INTERACTION_STORAGE_DRAWERS" => 1,
        "aFTR_INTERACTION_STORAGE_WARDROBE" => 2,
        "aFTR_INTERACTION_STORAGE_CLOSET" => 4,
        "aFTR_INTERACTION_MUSIC_DISK" => 8,
        "aFTR_INTERACTION_NO_COLLISION" => 16,
        "aFTR_INTERACTION_HANIWA" => 32,
        "aFTR_INTERACTION_FISH" => 64,
        "aFTR_INTERACTION_INSECT" => 128,
        "aFTR_INTERACTION_MANNEKIN" => 256,
        "aFTR_INTERACTION_UMBRELLA" => 512,
        "aFTR_INTERACTION_FOSSIL" => 1024,
        "aFTR_INTERACTION_FAMICOM" => 2048,
        "aFTR_INTERACTION_START_DISABLED" => 4096,
        "aFTR_INTERACTION_FAMICOM_ITEM" => 8192,
        "aFTR_INTERACTION_RADIO_AEROBICS" => 16384,
        "aFTR_INTERACTION_TOGGLE" => 32768,
        value => value.parse().expect("numeric profile field"),
    }
}

#[test]
fn every_generated_profile_matches_its_c_initializer() {
    // Coverage is every portable furniture source; the count is asserted
    // against the source tree below rather than hardcoded, so it cannot
    // silently drift when a source is added.
    assert!(FURNITURE_PROFILES.len() > 600, "coverage regressed");
    assert_eq!(SOURCE_MANIFEST.len(), FURNITURE_PROFILES.len());
    for profile in FURNITURE_PROFILES {
        let source_path = root().join(profile.source_path);
        let source = fs::read_to_string(&source_path).expect("read C oracle");
        let fields = profile_fields(&source, profile.symbol);
        // Two arities exist in the tree: 17 fields, and 18 where the profile
        // also names an aFTR_func callback in field 16.
        assert!(
            fields.len() == 17 || fields.len() == 18,
            "{} field count {}",
            profile.symbol,
            fields.len()
        );
        assert_eq!(
            profile
                .models
                .map(|value| value.map(str::to_owned))
                .as_slice(),
            &fields[..4]
                .iter()
                .map(|value| pointer(value))
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(profile.texture.map(str::to_owned), pointer(&fields[4]));
        assert_eq!(profile.palette.map(str::to_owned), pointer(&fields[5]));
        assert_eq!(profile.rig.map(str::to_owned), pointer(&fields[6]));
        assert_eq!(profile.tex_anim.map(str::to_owned), pointer(&fields[7]));
        assert_eq!(
            profile.height,
            fields[8].trim_end_matches('f').parse::<f32>().unwrap()
        );
        assert_eq!(
            profile.scale,
            fields[9].trim_end_matches('f').parse::<f32>().unwrap()
        );
        assert_eq!(profile.shape, number(&fields[10]) as u8);
        assert_eq!(profile.move_bg_type, number(&fields[11]) as u8);
        assert_eq!(profile.check_rotation, number(&fields[12]) as u8);
        assert_eq!(profile.kankyo_map, number(&fields[13]) as u8);
        assert_eq!(profile.contact_action, number(&fields[14]) as u8);
        assert_eq!(profile.interaction_type, number(&fields[15]));

        let manifest = SOURCE_MANIFEST
            .iter()
            .find(|entry| entry.symbol == profile.symbol)
            .expect("manifest profile");
        assert_eq!(manifest.source_path, profile.source_path);
        assert_eq!(manifest.source_bytes, source.len());
        assert_eq!(manifest.source_sha256.len(), 64);
    }
}
