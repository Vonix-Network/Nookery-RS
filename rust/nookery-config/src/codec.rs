//! `config.json` parsing, canonical serialization, and unknown-key retention.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value};

use crate::migration::{EnumCodec, migrate_settings};
use crate::settings::{SETTING_REGISTRY, SettingType, SettingValue, Settings};

/// A non-fatal issue found while applying a recognized JSON key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// The recognized key whose value was ignored.
    pub key: String,
    /// Human-readable reason for ignoring it.
    pub reason: String,
}

/// File-level result for a load operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFileStatus {
    /// A JSON object was read and parsed.
    Loaded,
    /// The path did not exist; defaults were returned.
    FileMissing,
    /// The path existed but contained no non-whitespace bytes.
    Empty,
}

/// Typed failures which prevent a JSON document from being loaded or saved.
#[derive(Debug)]
pub enum ConfigError {
    /// The document was not a JSON object.
    RootType,
    /// JSON syntax or a JSON serialization error.
    Json(serde_json::Error),
    /// Filesystem failure.
    Io(io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootType => f.write_str("settings JSON root must be an object"),
            Self::Json(error) => write!(f, "settings JSON error: {error}"),
            Self::Io(error) => write!(f, "settings file error: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<serde_json::Error> for ConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The parsed settings plus the unknown JSON fields retained for saving.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigDocument {
    /// Known settings after defaults, migration, and clamping.
    pub settings: Settings,
    /// Unknown fields retained as parsed JSON values.
    pub unknown: BTreeMap<String, Value>,
    /// Recognized fields ignored because their JSON type or enum spelling was invalid.
    pub issues: Vec<ConfigIssue>,
}

/// Result of loading a path, including a non-error missing/empty indication.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    /// The loaded document, or defaults for a missing/empty file.
    pub document: ConfigDocument,
    /// File-level outcome.
    pub status: ConfigFileStatus,
}

impl ConfigDocument {
    /// Parses a canonical/string-valued settings object without numeric enum migration.
    pub fn from_json_str(text: &str) -> Result<Self, ConfigError> {
        Self::from_value(serde_json::from_str(text)?)
    }

    /// Parses a settings object after applying the upstream migrations.
    pub fn from_json_str_with_codec(
        text: &str,
        codec: &dyn EnumCodec,
    ) -> Result<Self, ConfigError> {
        let mut value: Value = serde_json::from_str(text)?;
        migrate_settings(&mut value, codec);
        Self::from_value(value)
    }

    /// Parses a settings object and repairs a renderer not present in `available_renderers`.
    pub fn from_json_str_with_renderers(
        text: &str,
        available_renderers: &[&str],
    ) -> Result<Self, ConfigError> {
        let mut document = Self::from_json_str(text)?;
        document.settings.sanitize_renderer(available_renderers);
        Ok(document)
    }

    /// Loads a path. Missing and empty files produce defaults with an explicit status.
    pub fn load_path(path: impl AsRef<Path>) -> Result<ConfigLoad, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ConfigLoad {
                    document: Self::default(),
                    status: ConfigFileStatus::FileMissing,
                });
            }
            Err(error) => return Err(ConfigError::Io(error)),
        };
        if text.trim().is_empty() {
            return Ok(ConfigLoad {
                document: Self::default(),
                status: ConfigFileStatus::Empty,
            });
        }
        Ok(ConfigLoad {
            document: Self::from_json_str(&text)?,
            status: ConfigFileStatus::Loaded,
        })
    }

    /// Returns a compact, sorted-key JSON representation.
    pub fn to_json_string(&self) -> Result<String, ConfigError> {
        Ok(serde_json::to_string(&self.to_value())?)
    }

    /// Saves the canonical representation to a path.
    pub fn save_path(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        fs::write(path, self.to_json_string()?)?;
        Ok(())
    }

    /// Converts the retained document to its canonical JSON value.
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        for (key, value) in &self.unknown {
            object.insert(key.clone(), value.clone());
        }
        for spec in SETTING_REGISTRY {
            if let Some(value) = self.settings.get(spec.key) {
                object.insert(spec.key.to_owned(), setting_to_json(value));
            }
        }
        Value::Object(object)
    }

    fn from_value(mut value: Value) -> Result<Self, ConfigError> {
        let Some(object) = value.as_object_mut() else {
            return Err(ConfigError::RootType);
        };
        let mut document = Self::default();
        for (key, value) in object.iter() {
            let Some(spec) = SETTING_REGISTRY.iter().find(|spec| spec.key == key) else {
                document.unknown.insert(key.clone(), value.clone());
                continue;
            };
            let Some(setting_value) = json_to_setting(spec.value_type, value) else {
                document.issues.push(ConfigIssue {
                    key: key.clone(),
                    reason: "wrong JSON type or invalid enum value".to_owned(),
                });
                continue;
            };
            if let Err(error) = document.settings.set(key, setting_value) {
                document.issues.push(ConfigIssue {
                    key: key.clone(),
                    reason: error.to_string(),
                });
            }
        }
        Ok(document)
    }
}

fn json_to_setting(value_type: SettingType, value: &Value) -> Option<SettingValue> {
    match value_type {
        SettingType::String | SettingType::Enum => {
            value.as_str().map(|v| SettingValue::Text(v.to_owned()))
        }
        SettingType::Boolean => value.as_bool().map(SettingValue::Bool),
        SettingType::Integer => value.as_i64().map(SettingValue::Int),
        SettingType::Object if value.is_object() => {
            Some(SettingValue::Json(normalize_touch_layout(value)))
        }
        SettingType::Object => None,
    }
}

fn normalize_touch_layout(value: &Value) -> Value {
    let mut controls = Map::new();
    if let Some(entries) = value.get("controls").and_then(Value::as_object) {
        for (id, entry) in entries {
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(anchor) = entry.get("anchor").and_then(Value::as_str) else {
                continue;
            };
            if !matches!(
                anchor,
                "topLeft" | "top" | "topRight" | "bottomLeft" | "bottom" | "bottomRight"
            ) {
                continue;
            }
            let x = finite_f32(entry.get("x")).unwrap_or(0.0);
            let y = finite_f32(entry.get("y")).unwrap_or(0.0);
            let scale = finite_f32(entry.get("scale"))
                .unwrap_or(1.0)
                .clamp(0.5, 2.5);
            let mut placement = Map::new();
            placement.insert("x".to_owned(), json_number(x));
            placement.insert("y".to_owned(), json_number(y));
            placement.insert("scale".to_owned(), json_number(scale));
            placement.insert("anchor".to_owned(), Value::String(anchor.to_owned()));
            controls.insert(id.clone(), Value::Object(placement));
        }
    }
    let mut layout = Map::new();
    layout.insert("version".to_owned(), Value::Number(1.into()));
    layout.insert("controls".to_owned(), Value::Object(controls));
    Value::Object(layout)
}

fn finite_f32(value: Option<&Value>) -> Option<f32> {
    let value = value?.as_f64()? as f32;
    value.is_finite().then_some(value)
}

fn json_number(value: f32) -> Value {
    match serde_json::Number::from_f64(f64::from(value)) {
        Some(number) => Value::Number(number),
        None => Value::Number(0.into()),
    }
}

fn setting_to_json(value: SettingValue) -> Value {
    match value {
        SettingValue::Int(value) => Value::Number(value.into()),
        SettingValue::Bool(value) => Value::Bool(value),
        SettingValue::Text(value) => Value::String(value),
        SettingValue::Json(value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SETTING_REGISTRY;

    #[test]
    fn registry_is_complete_and_canonical_output_is_stable() {
        assert_eq!(SETTING_REGISTRY.len(), 30);
        let document = ConfigDocument::default();
        let first = document
            .to_json_string()
            .expect("static settings serialize");
        let reparsed = ConfigDocument::from_json_str(&first).expect("canonical JSON parses");
        assert_eq!(
            first,
            reparsed
                .to_json_string()
                .expect("canonical JSON reserializes")
        );
        assert!(first.contains("\"input.touchLayout\":{\"controls\":{},\"version\":1}"));
    }

    #[test]
    fn unknown_values_survive_and_wrong_types_are_reported() {
        let document = ConfigDocument::from_json_str(
            r#"{"future":{"x":[true,3.5]},"video.windowWidth":99999,"audio.masterVolume":"bad"}"#,
        )
        .expect("object parses");
        assert_eq!(document.settings.video.window_width, 16_384);
        assert_eq!(document.settings.audio.master_volume, 100);
        assert_eq!(document.issues.len(), 1);
        let output = document.to_json_string().expect("output serializes");
        assert!(output.contains(r#""future":{"x":[true,3.5]}"#));
    }

    #[test]
    fn malformed_and_non_object_documents_are_typed_errors() {
        assert!(matches!(
            ConfigDocument::from_json_str("{broken"),
            Err(ConfigError::Json(_))
        ));
        assert!(matches!(
            ConfigDocument::from_json_str("[]"),
            Err(ConfigError::RootType)
        ));
    }

    #[test]
    fn touch_layout_uses_the_upstream_shape_and_scale_clamp() {
        let document = ConfigDocument::from_json_str(
            r#"{"input.touchLayout":{"controls":{"a":{"x":4,"y":5,"scale":9,"anchor":"top"},"bad":{"anchor":"invalid"}}}}"#,
        )
        .expect("object parses");
        let output = document.to_json_string().expect("output serializes");
        assert!(output.contains(r#""a":{"anchor":"top","scale":2.5,"x":4.0,"y":5.0}"#));
        assert!(!output.contains("bad"));
    }
}
