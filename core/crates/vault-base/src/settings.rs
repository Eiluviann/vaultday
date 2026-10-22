//! The `.vaultday/settings.json` schema. Unknown keys survive a read and a
//! write so an older build never refuses a workspace a newer build touched;
//! only the forbidden keys (secrets and the namespaces reserved for the
//! encrypted store) are rejected.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

/// The schema version this build reads and writes.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Keys that never belong in `settings.json` at any depth, compared without
/// case: they hold secrets or state the encrypted app store owns.
pub const FORBIDDEN_SETTINGS_KEYS: [&str; 5] =
    ["security", "tokens", "rates", "models", "integrations"];

/// Key words that mark a value as a secret wherever the key sits. Keys are
/// split into words on `_`, `-`, `.`, spaces and camelCase humps, so
/// `apiToken`, `client-secret` and `max_tokens` are all refused.
pub const SECRET_KEY_WORDS: [&str; 12] = [
    "secret",
    "secrets",
    "password",
    "passwords",
    "passwd",
    "passphrase",
    "token",
    "tokens",
    "credential",
    "credentials",
    "apikey",
    "bearer",
];

/// Adjacent word pairs that mark a secret: `api_key`, `privateKey`, `access-key`.
pub const SECRET_KEY_WORD_PAIRS: [(&str, &str); 3] =
    [("api", "key"), ("private", "key"), ("access", "key")];

/// Most working hours a day may be configured with.
pub const MAX_HOURS_PER_DAY: f64 = 24.0;

/// Whether a workspace holds client work or the user's own life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceKind {
    /// A client workspace; two of these may not nest.
    Work,
    /// A personal workspace.
    Life,
}

/// The contents of `.vaultday/settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(
    title = "Vaultday workspace settings",
    description = "The manifest at .vaultday/settings.json that marks a folder as a Vaultday workspace. Unknown keys are preserved. The keys security, tokens, rates, models and integrations are refused at every depth and without case, as is any key that names a secret (token, password, api_key, ...); the reader enforces this, the schema can only state the top-level rule."
)]
pub struct WorkspaceSettings {
    /// Schema version; this build reads version 1.
    pub schema_version: u32,
    /// Display name of the workspace.
    pub name: String,
    /// Work or life.
    pub kind: WorkspaceKind,
    /// Accent colour token used by the apps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Regular expression that recognises ticket numbers such as `WEB-142`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_pattern: Option<String>,
    /// URL template for a ticket; `{ticket}` is replaced by the number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_url: Option<String>,
    /// Hosts this workspace's links point at.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Browser to open links with; `None` means the system default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    /// Working hours in a day, used by the day plan; above 0 and at most 24.
    #[serde(default = "default_hours_per_day")]
    #[schemars(extend("exclusiveMinimum" = 0.0, "maximum" = 24.0))]
    pub hours_per_day: f64,
    /// Folder that holds the day logs, relative to the workspace root.
    #[serde(default = "default_log_folder")]
    pub log_folder: String,
    /// Template note for new stories, relative to the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_template: Option<String>,
    /// Per-feature settings, keyed by feature id. Opaque in the base version;
    /// each feature validates its own entry against its settings schema.
    #[serde(default)]
    pub features: BTreeMap<String, Value>,
    /// Keys this build does not know, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Why a settings file was refused.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// The file is not valid JSON or does not match the schema.
    #[error("settings.json is not valid: {0}")]
    Json(#[from] serde_json::Error),
    /// The top level is not a JSON object.
    #[error("settings.json must hold a JSON object")]
    NotAnObject,
    /// `schema_version` is missing or not a positive integer.
    #[error("settings.json needs an integer schema_version")]
    MissingSchemaVersion,
    /// `schema_version` is one this build does not read.
    #[error(
        "settings.json schema_version {found} is not supported; this build reads version {supported}"
    )]
    UnsupportedSchemaVersion {
        /// The version in the file.
        found: u64,
        /// The version this build reads.
        supported: u32,
    },
    /// A reserved key is present somewhere in the file.
    #[error("settings.json must not contain the key `{path}`: {reason}")]
    ForbiddenKey {
        /// Dotted path to the offending key.
        path: String,
        /// Why it is refused.
        reason: &'static str,
    },
    /// A known value is out of range or malformed.
    #[error("settings.json `{key}` is invalid: {reason}")]
    InvalidValue {
        /// The offending key.
        key: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// `extra` holds a key the schema already defines, which a write would collapse.
    #[error("the unknown-key map holds `{key}`, which is a schema key; set the field instead")]
    KnownKeyInExtra {
        /// The colliding key.
        key: String,
    },
    /// A key that looks like a secret is present somewhere in the file.
    #[error(
        "settings.json must not hold secrets: the key `{path}` looks like one; keep it in the encrypted store"
    )]
    SecretKey {
        /// Dotted path to the offending key.
        path: String,
    },
}

impl WorkspaceSettings {
    /// A minimal settings document for a new workspace.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: WorkspaceKind) -> WorkspaceSettings {
        WorkspaceSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            name: name.into(),
            kind,
            accent: None,
            ticket_pattern: None,
            ticket_url: None,
            hosts: Vec::new(),
            browser: None,
            hours_per_day: default_hours_per_day(),
            log_folder: default_log_folder(),
            story_template: None,
            features: BTreeMap::new(),
            extra: Map::new(),
        }
    }

    /// Parses `settings.json` text. Unknown keys are kept in `extra`; the
    /// forbidden keys and secret-looking keys anywhere in the file are refused.
    ///
    /// # Errors
    /// Returns a [`SettingsError`] for malformed JSON, a non-object document,
    /// a missing or unsupported `schema_version`, a forbidden key or a secret.
    pub fn from_json(text: &str) -> Result<WorkspaceSettings, SettingsError> {
        let value: Value = serde_json::from_str(text)?;
        let Value::Object(object) = &value else {
            return Err(SettingsError::NotAnObject);
        };
        check_schema_version(object)?;
        check_forbidden_keys(object)?;
        let settings: WorkspaceSettings = serde_json::from_value(value)?;
        settings.validate_values()?;
        Ok(settings)
    }

    /// Serialises the settings as pretty JSON with a trailing newline, known
    /// keys first in schema order and `extra` keys after them.
    ///
    /// # Errors
    /// Returns a [`SettingsError`] when `extra` or `features` would reintroduce
    /// a forbidden or secret key, when `extra` shadows a schema key, when a
    /// value is out of range, or when serialisation fails.
    pub fn to_json(&self) -> Result<String, SettingsError> {
        self.validate_values()?;
        if let Some(key) = self
            .extra
            .keys()
            .find(|key| SCHEMA_KEYS.contains(&key.as_str()))
        {
            return Err(SettingsError::KnownKeyInExtra { key: key.clone() });
        }
        let value = serde_json::to_value(self)?;
        if let Value::Object(object) = &value {
            check_forbidden_keys(object)?;
        }
        let mut text = serde_json::to_string_pretty(&value)?;
        text.push('\n');
        Ok(text)
    }

    /// The JSON Schema of this type, as emitted to `schema/settings.schema.json`.
    /// Besides the derived shape it refuses the top-level forbidden keys
    /// through `propertyNames`, so a schema-aware editor flags them too.
    #[must_use]
    pub fn json_schema() -> Value {
        let mut schema = schemars::schema_for!(WorkspaceSettings).to_value();
        if let Value::Object(root) = &mut schema {
            root.insert(
                "propertyNames".to_owned(),
                serde_json::json!({
                    "not": { "pattern": forbidden_key_pattern() },
                    "description": "The forbidden keys, compared without case.",
                }),
            );
        }
        schema
    }

    /// Range and shape checks on the known values.
    fn validate_values(&self) -> Result<(), SettingsError> {
        if !self.hours_per_day.is_finite()
            || self.hours_per_day <= 0.0
            || self.hours_per_day > MAX_HOURS_PER_DAY
        {
            return Err(SettingsError::InvalidValue {
                key: "hours_per_day",
                reason: format!(
                    "must be a number above 0 and at most {MAX_HOURS_PER_DAY}, got {}",
                    self.hours_per_day
                ),
            });
        }
        let log_folder = Path::new(&self.log_folder);
        let is_plain_relative = !self.log_folder.is_empty()
            && !self.log_folder.ends_with(['/', '\\'])
            && log_folder
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !is_plain_relative {
            return Err(SettingsError::InvalidValue {
                key: "log_folder",
                reason: format!(
                    "must be a relative folder inside the workspace without `.` or `..`, got {:?}",
                    self.log_folder
                ),
            });
        }
        Ok(())
    }
}

/// The keys the schema defines; `extra` may not shadow them.
const SCHEMA_KEYS: [&str; 13] = [
    "schema_version",
    "name",
    "kind",
    "accent",
    "ticket_pattern",
    "ticket_url",
    "hosts",
    "browser",
    "hours_per_day",
    "log_folder",
    "story_template",
    "features",
    "extra",
];

fn default_hours_per_day() -> f64 {
    8.0
}

fn default_log_folder() -> String {
    "Log".to_owned()
}

fn check_schema_version(object: &Map<String, Value>) -> Result<(), SettingsError> {
    let found = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(SettingsError::MissingSchemaVersion)?;
    if found != u64::from(SETTINGS_SCHEMA_VERSION) {
        return Err(SettingsError::UnsupportedSchemaVersion {
            found,
            supported: SETTINGS_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn check_forbidden_keys(object: &Map<String, Value>) -> Result<(), SettingsError> {
    check_keys_at(object, "")
}

fn check_keys_at(object: &Map<String, Value>, prefix: &str) -> Result<(), SettingsError> {
    for (key, value) in object {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if FORBIDDEN_SETTINGS_KEYS.contains(&key.to_lowercase().as_str()) {
            return Err(SettingsError::ForbiddenKey {
                path,
                reason: "that namespace belongs to the encrypted app store, not to the vault",
            });
        }
        if is_secret_key(key) {
            return Err(SettingsError::SecretKey { path });
        }
        check_value_at(value, &path)?;
    }
    Ok(())
}

fn check_value_at(value: &Value, path: &str) -> Result<(), SettingsError> {
    match value {
        Value::Object(nested) => check_keys_at(nested, path),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(index, item)| check_value_at(item, &format!("{path}[{index}]"))),
        _ => Ok(()),
    }
}

/// True when a key name says its value is a secret: one of its words is in
/// [`SECRET_KEY_WORDS`] or two adjacent words form a [`SECRET_KEY_WORD_PAIRS`] entry.
#[must_use]
pub fn is_secret_key(key: &str) -> bool {
    let words = key_words(key);
    words
        .iter()
        .any(|word| SECRET_KEY_WORDS.contains(&word.as_str()))
        || words.windows(2).any(|pair| {
            SECRET_KEY_WORD_PAIRS
                .iter()
                .any(|(first, second)| pair[0] == *first && pair[1] == *second)
        })
}

/// An ECMA-262 pattern that matches exactly a forbidden key, without case:
/// JSON Schema has no case-insensitive flag, so every letter becomes a class.
fn forbidden_key_pattern() -> String {
    let alternatives: Vec<String> = FORBIDDEN_SETTINGS_KEYS
        .iter()
        .map(|key| {
            key.chars()
                .map(|letter| format!("[{}{}]", letter.to_lowercase(), letter.to_uppercase()))
                .collect::<String>()
        })
        .collect();
    format!("^({})$", alternatives.join("|"))
}

/// Lowercase words of a key: split on `_`, `-`, `.`, spaces and camelCase humps.
fn key_words(key: &str) -> Vec<String> {
    let mut spaced = String::with_capacity(key.len() + 4);
    let mut previous_was_lower = false;
    for character in key.chars() {
        if character.is_uppercase() && previous_was_lower {
            spaced.push('_');
        }
        previous_was_lower = character.is_lowercase() || character.is_ascii_digit();
        spaced.push(character);
    }
    spaced
        .to_lowercase()
        .split(['_', '-', '.', ' '])
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        SettingsError, WorkspaceKind, WorkspaceSettings, forbidden_key_pattern, is_secret_key,
    };
    use serde_json::json;

    const FULL: &str = r#"{
  "schema_version": 1,
  "name": "Mock Studio",
  "kind": "work",
  "accent": "teal",
  "ticket_pattern": "(WEB|ACME)-\\d+",
  "ticket_url": "https://tracker.example.test/browse/{ticket}",
  "hosts": ["tracker.example.test"],
  "browser": "default",
  "hours_per_day": 7.5,
  "log_folder": "Log",
  "story_template": "_templates/story.md",
  "features": { "sample_feature": { "enabled": true } },
  "future_key": { "nested": [1, 2] }
}"#;

    #[test]
    fn reads_every_known_key_and_keeps_unknown_ones_through_a_write() {
        let settings = WorkspaceSettings::from_json(FULL).expect("valid settings");
        assert_eq!(settings.name, "Mock Studio");
        assert_eq!(settings.kind, WorkspaceKind::Work);
        assert_eq!(settings.hours_per_day, 7.5);
        assert_eq!(settings.hosts, ["tracker.example.test"]);
        assert_eq!(
            settings.features["sample_feature"],
            json!({ "enabled": true })
        );
        assert_eq!(settings.extra["future_key"], json!({ "nested": [1, 2] }));

        let written = settings.to_json().expect("serialises");
        assert!(written.ends_with('\n'));
        assert!(written.contains("\"future_key\""));
        let reread = WorkspaceSettings::from_json(&written).expect("round trip");
        assert_eq!(reread, settings);
        let first_key_position = written.find("\"schema_version\"").expect("has version");
        let extra_position = written.find("\"future_key\"").expect("has extra");
        assert!(first_key_position < extra_position);
    }

    #[test]
    fn defaults_fill_the_optional_keys() {
        let settings = WorkspaceSettings::from_json(
            r#"{"schema_version": 1, "name": "Home", "kind": "life"}"#,
        )
        .expect("valid");
        assert_eq!(settings.hours_per_day, 8.0);
        assert_eq!(settings.log_folder, "Log");
        assert!(settings.features.is_empty() && settings.extra.is_empty());
        assert_eq!(
            settings,
            WorkspaceSettings::new("Home", WorkspaceKind::Life)
        );
    }

    #[test]
    fn rejects_forbidden_and_secret_keys_with_a_clear_message() {
        for key in ["security", "tokens", "rates", "models", "integrations"] {
            let text =
                format!(r#"{{"schema_version": 1, "name": "x", "kind": "life", "{key}": {{}}}}"#);
            let error = WorkspaceSettings::from_json(&text).expect_err("refused");
            assert!(
                matches!(&error, SettingsError::ForbiddenKey { path, .. } if path == key),
                "{key}: {error}"
            );
            assert!(error.to_string().contains(key));
        }
        let nested_forbidden = r#"{"schema_version": 1, "name": "x", "kind": "life", "features": {"ai": {"Models": ["m"]}}}"#;
        let error = WorkspaceSettings::from_json(nested_forbidden).expect_err("refused");
        assert!(
            matches!(&error, SettingsError::ForbiddenKey { path, .. } if path == "features.ai.Models"),
            "{error}"
        );
        let in_array = r#"{"schema_version": 1, "name": "x", "kind": "life", "features": {"mail": [{"password": "p"}]}}"#;
        let error = WorkspaceSettings::from_json(in_array).expect_err("refused");
        assert!(
            matches!(&error, SettingsError::SecretKey { path } if path == "features.mail[0].password"),
            "{error}"
        );
        let nested_secret = r#"{"schema_version": 1, "name": "x", "kind": "life", "features": {"mail": {"api_token": "abc"}}}"#;
        let error = WorkspaceSettings::from_json(nested_secret).expect_err("refused");
        assert!(
            matches!(&error, SettingsError::SecretKey { path } if path == "features.mail.api_token"),
            "{error}"
        );
        let top_secret =
            r#"{"schema_version": 1, "name": "x", "kind": "life", "Client-Secret": "abc"}"#;
        assert!(matches!(
            WorkspaceSettings::from_json(top_secret),
            Err(SettingsError::SecretKey { .. })
        ));
    }

    #[test]
    fn rejects_missing_wrong_version_and_non_objects() {
        assert!(matches!(
            WorkspaceSettings::from_json(r#"{"name": "x", "kind": "life"}"#),
            Err(SettingsError::MissingSchemaVersion)
        ));
        assert!(matches!(
            WorkspaceSettings::from_json(r#"{"schema_version": 2, "name": "x", "kind": "life"}"#),
            Err(SettingsError::UnsupportedSchemaVersion {
                found: 2,
                supported: 1
            })
        ));
        assert!(matches!(
            WorkspaceSettings::from_json("[]"),
            Err(SettingsError::NotAnObject)
        ));
        assert!(matches!(
            WorkspaceSettings::from_json("{"),
            Err(SettingsError::Json(_))
        ));
        assert!(matches!(
            WorkspaceSettings::from_json(r#"{"schema_version": 1, "name": "x", "kind": "club"}"#),
            Err(SettingsError::Json(_))
        ));
    }

    #[test]
    fn secret_detection_splits_keys_into_words() {
        for secret in [
            "privateKey",
            "api-key",
            "accessKey",
            "clientSecret",
            "AuthToken",
            "max_tokens",
            "passphrase",
            "Client-Secret",
        ] {
            assert!(is_secret_key(secret), "{secret}");
        }
        for plain in [
            "name",
            "keyboard",
            "tokenizer_mode",
            "authority",
            "ticket_pattern",
            "apiVersion",
            "secretary",
        ] {
            assert!(!is_secret_key(plain), "{plain}");
        }
    }

    #[test]
    fn values_are_range_checked_on_read_and_write() {
        for (body, key) in [
            (r#""hours_per_day": 0"#, "hours_per_day"),
            (r#""hours_per_day": 25"#, "hours_per_day"),
            (r#""log_folder": "/etc""#, "log_folder"),
            (r#""log_folder": "../Log""#, "log_folder"),
            (r#""log_folder": "Log/""#, "log_folder"),
            (r#""log_folder": """#, "log_folder"),
        ] {
            let text = format!(r#"{{"schema_version": 1, "name": "x", "kind": "life", {body}}}"#);
            let error = WorkspaceSettings::from_json(&text).expect_err(body);
            assert!(
                matches!(&error, SettingsError::InvalidValue { key: found, .. } if *found == key),
                "{body}: {error}"
            );
        }
        let nested_ok = WorkspaceSettings::from_json(
            r#"{"schema_version": 1, "name": "x", "kind": "life", "log_folder": "Journal/Days"}"#,
        )
        .expect("nested folder");
        assert_eq!(nested_ok.log_folder, "Journal/Days");
        let mut settings = WorkspaceSettings::new("x", WorkspaceKind::Work);
        settings.hours_per_day = f64::NAN;
        assert!(matches!(
            settings.to_json(),
            Err(SettingsError::InvalidValue {
                key: "hours_per_day",
                ..
            })
        ));
        let mut shadowing = WorkspaceSettings::new("x", WorkspaceKind::Work);
        shadowing.extra.insert("name".to_owned(), json!("dup"));
        assert!(
            matches!(shadowing.to_json(), Err(SettingsError::KnownKeyInExtra { key }) if key == "name")
        );
    }

    #[test]
    fn a_write_refuses_a_secret_added_through_extra() {
        let mut settings = WorkspaceSettings::new("x", WorkspaceKind::Work);
        settings
            .extra
            .insert("password".to_owned(), json!("hunter2"));
        assert!(matches!(
            settings.to_json(),
            Err(SettingsError::SecretKey { .. })
        ));
    }

    #[test]
    fn the_schema_names_the_known_keys() {
        let schema = WorkspaceSettings::json_schema();
        let properties = schema["properties"].as_object().expect("properties");
        for key in [
            "schema_version",
            "name",
            "kind",
            "features",
            "hours_per_day",
            "log_folder",
        ] {
            assert!(properties.contains_key(key), "{key}");
        }
        assert_eq!(schema["title"], "Vaultday workspace settings");
        assert_eq!(schema["additionalProperties"], json!(true));
        assert_eq!(properties["hours_per_day"]["exclusiveMinimum"], json!(0.0));
        assert_eq!(properties["hours_per_day"]["maximum"], json!(24.0));
    }

    #[test]
    fn the_schema_refuses_the_top_level_forbidden_keys_without_case() {
        let schema = WorkspaceSettings::json_schema();
        let pattern = schema["propertyNames"]["not"]["pattern"]
            .as_str()
            .expect("propertyNames pattern");
        assert_eq!(forbidden_key_pattern(), pattern);
        assert!(pattern.starts_with("^(") && pattern.ends_with(")$"));
        for key in super::FORBIDDEN_SETTINGS_KEYS {
            let classes: String = key
                .chars()
                .map(|letter| format!("[{letter}{}]", letter.to_ascii_uppercase()))
                .collect();
            assert!(pattern.contains(&classes), "{key}: {pattern}");
        }
        assert_eq!(
            pattern.matches('|').count(),
            super::FORBIDDEN_SETTINGS_KEYS.len() - 1
        );
    }
}
