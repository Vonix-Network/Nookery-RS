//! Settings-file migrations, mirroring `migrate_settings` /
//! `migrate_enum` in `pc/src/acgc_settings.cpp`.
//!
//! JSON is the settings format upstream's registry writes, hence `serde_json`.
//! Nothing here touches a file: callers parse, migrate the value tree, and write
//! it back themselves.

use serde_json::Value;

/// Maps a codec-owned numeric enum value to its canonical string.
///
/// Adapter boundary: upstream calls `borealis::config::Codec<T>::encode(decode(n))`,
/// and that mapping lives in the Borealis/Aurora runtime. This crate cannot query
/// it, so migration takes a codec. Returning `None` reproduces upstream's
/// behaviour of leaving an undecodable value untouched for a later correction.
pub trait EnumCodec {
    /// Numeric value -> canonical settings string.
    ///
    /// `key` is part of the adapter contract because each upstream enum has its
    /// own codec table even when the JSON migration walks one object.
    fn decode_to_config_str(&self, key: &str, code: i64) -> Option<&'static str>;
}

/// Runs every migration on a settings value tree.
pub fn migrate_settings(root: &mut Value, codec: &dyn EnumCodec) {
    migrate_enum(root, "backend.discVerification", codec);
    migrate_enum(root, "video.theme", codec);
    migrate_enum(root, "video.renderer", codec);
    migrate_travel_move_out(root);
}

/// Rewrites a numeric enum field to its canonical string form. Non-numeric and
/// absent fields are left alone; undecodable numbers are preserved.
pub fn migrate_enum(root: &mut Value, key: &str, codec: &dyn EnumCodec) {
    let Some(mapped) = root.as_object_mut() else {
        return;
    };
    let Some(value) = mapped.get(key) else {
        return;
    };
    let Some(code) = numeric_code(value) else {
        return;
    };
    if let Some(text) = codec.decode_to_config_str(key, code) {
        mapped.insert(key.to_owned(), Value::String(text.to_owned()));
    }
}

/// Matches Borealis' integer codec: signed/unsigned JSON integers are accepted
/// when they fit the adapter's signed input, and integral finite floats are
/// accepted only through the exact binary-integer range used by the C++ codec.
fn numeric_code(value: &Value) -> Option<i64> {
    if let Some(code) = value.as_i64() {
        return Some(code);
    }
    if let Some(code) = value.as_u64() {
        return i64::try_from(code).ok();
    }
    let code = value.as_f64()?;
    const EXACT_LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53
    if code.is_finite() && code.fract() == 0.0 && code.abs() <= EXACT_LIMIT {
        Some(code as i64)
    } else {
        None
    }
}

/// `gameplay.reduceTravelMoveOut=true` becomes `gameplay.travelMoveOutChance="10"`
/// only when the target key is absent; the old key is always removed.
///
/// Upstream has no `false` case: a `false` (or non-boolean) value is dropped
/// without writing anything, which leaves the new key's default in effect.
pub fn migrate_travel_move_out(root: &mut Value) {
    let Some(mapped) = root.as_object_mut() else {
        return;
    };
    let Some(old) = mapped.remove("gameplay.reduceTravelMoveOut") else {
        return;
    };
    if old.as_bool() == Some(true) && !mapped.contains_key("gameplay.travelMoveOutChance") {
        mapped.insert(
            "gameplay.travelMoveOutChance".to_owned(),
            Value::String("10".to_owned()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestCodec;

    impl EnumCodec for TestCodec {
        fn decode_to_config_str(&self, _key: &str, code: i64) -> Option<&'static str> {
            match code {
                0 => Some("unknown"),
                1 => Some("success"),
                _ => None,
            }
        }
    }

    #[test]
    fn travel_move_out_true_becomes_ten_when_target_absent() {
        let mut root = json!({ "gameplay.reduceTravelMoveOut": true });
        migrate_travel_move_out(&mut root);
        assert_eq!(root["gameplay.travelMoveOutChance"], json!("10"));
        assert!(root.get("gameplay.reduceTravelMoveOut").is_none());
    }

    #[test]
    fn travel_move_out_does_not_overwrite_existing_target() {
        let mut root = json!({
            "gameplay.reduceTravelMoveOut": true,
            "gameplay.travelMoveOutChance": "50",
        });
        migrate_travel_move_out(&mut root);
        assert_eq!(root["gameplay.travelMoveOutChance"], json!("50"));
        assert!(root.get("gameplay.reduceTravelMoveOut").is_none());
    }

    #[test]
    fn travel_move_out_false_is_dropped_without_writing() {
        let mut root = json!({ "gameplay.reduceTravelMoveOut": false });
        migrate_travel_move_out(&mut root);
        assert!(root.get("gameplay.reduceTravelMoveOut").is_none());
        assert!(root.get("gameplay.travelMoveOutChance").is_none());
    }

    #[test]
    fn enum_migration_accepts_integral_float_and_rejects_non_integral_or_wide_codes() {
        let mut root = json!({
            "video.theme": 1.0,
            "backend.discVerification": 2.5,
            "video.renderer": 18_446_744_073_709_551_615u64,
        });
        migrate_settings(&mut root, &TestCodec);
        assert_eq!(root["video.theme"], json!("success"));
        assert_eq!(root["backend.discVerification"], json!(2.5));
        assert_eq!(root["video.renderer"], json!(18_446_744_073_709_551_615u64));
    }
}
