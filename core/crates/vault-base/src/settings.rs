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

/// Key words that mark a value of the `billing` block as money. Rates and
/// currencies live in the signed app-level document only, never in the vault,
/// so such a key is refused there at any depth, as an unknown key too.
pub const BILLING_MONEY_KEY_WORDS: [&str; 14] = [
    "rate",
    "rates",
    "currency",
    "currencies",
    "price",
    "prices",
    "amount",
    "amounts",
    "fee",
    "fees",
    "cost",
    "costs",
    "money",
    "salary",
];

/// Most working hours a day may be configured with.
pub const MAX_HOURS_PER_DAY: f64 = 24.0;

/// The rounding increment of an hourly workspace until one is set.
pub const DEFAULT_ROUNDING_MINUTES: u32 = 15;
/// The largest rounding increment an hourly workspace may use: an hour.
pub const MAX_ROUNDING_MINUTES: u32 = 60;
/// The flat Toggl hours of an FTE working day until set.
pub const DEFAULT_FLAT_HOURS: f64 = 8.0;

/// Whether a workspace holds client work or the user's own life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceKind {
    /// A client workspace; two of these may not nest.
    Work,
    /// A personal workspace.
    Life,
}

/// How a workspace bills its time. Rates and currencies are not here: they
/// live in the signed app-level document only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BillingModel {
    /// Sessions are billable time, rounded up to the workspace increment.
    Hourly,
    /// Each task's estimate is its quote and cap; an overrun is unpaid.
    Fixed,
    /// Full-time equivalent: tracked time stays private; a flat day goes out.
    Fte,
}

/// A day of the week, as `log_deadline` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SettingsWeekday {
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday.
    Sunday,
}

/// When a week's hours must be logged by: the first `weekday` at or after
/// the week's Friday, at `time`. The week close-out runs for a workspace
/// that has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LogDeadline {
    /// The day of the deadline.
    pub weekday: SettingsWeekday,
    /// The time of day, `HH:MM` in 24 hours.
    #[schemars(regex(pattern = r"^([01][0-9]|2[0-3]):[0-5][0-9]$"))]
    pub time: String,
}

/// The `billing` block of `settings.json`. Each model reads only its own
/// keys; a key of another model is refused so a switch never leaves a stale
/// value behind. Unknown keys are kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BillingSettings {
    /// The billing model.
    pub model: BillingModel,
    /// Hourly only: the increment billable time rounds up to, 1 to 60 minutes; 15 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 60))]
    pub rounding_minutes: Option<u32>,
    /// Hourly only: whether a task is billable until marked otherwise; true when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billable_default: Option<bool>,
    /// FTE only: hours of the flat Toggl entry per working day, above 0 and
    /// at most 24; 8 when absent. Logged time uses `hours_per_day`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0.0, "maximum" = 24.0))]
    pub flat_hours: Option<f64>,
    /// Keys this build does not know, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// How the workspace bills its time; `None` until a model is picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing: Option<BillingSettings>,
    /// When the week's hours must be logged by; the week close-out runs only
    /// for a workspace with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_deadline: Option<LogDeadline>,
    /// The name of the workspace whose `Leave.md` lists leave and public
    /// holidays; the life workspace when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leave_source: Option<String>,
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
            billing: None,
            log_deadline: None,
            leave_source: None,
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
        if let Some(billing) = &self.billing {
            billing.validate()?;
        }
        if let Some(deadline) = &self.log_deadline
            && parse_hh_mm(&deadline.time).is_none()
        {
            return Err(SettingsError::InvalidValue {
                key: "log_deadline.time",
                reason: format!("must be HH:MM in 24 hours, got {:?}", deadline.time),
            });
        }
        if let Some(source) = &self.leave_source
            && source.trim().is_empty()
        {
            return Err(SettingsError::InvalidValue {
                key: "leave_source",
                reason: "must name a workspace".to_owned(),
            });
        }
        Ok(())
    }
}

impl LogDeadline {
    /// The deadline's hour and minute.
    #[must_use]
    pub fn hour_minute(&self) -> Option<(u8, u8)> {
        parse_hh_mm(&self.time)
    }
}

/// `HH:MM` in 24 hours as hour and minute.
fn parse_hh_mm(text: &str) -> Option<(u8, u8)> {
    let (hour, minute) = text.split_once(':')?;
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    let hour: u8 = hour.parse().ok()?;
    let minute: u8 = minute.parse().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

impl BillingSettings {
    /// A block for `model` with every optional key absent, so the defaults apply.
    #[must_use]
    pub fn new(model: BillingModel) -> BillingSettings {
        BillingSettings {
            model,
            rounding_minutes: None,
            billable_default: None,
            flat_hours: None,
            extra: Map::new(),
        }
    }

    /// The rounding increment in minutes; the default when unset.
    #[must_use]
    pub fn rounding_minutes_or_default(&self) -> u32 {
        self.rounding_minutes.unwrap_or(DEFAULT_ROUNDING_MINUTES)
    }

    /// Whether tasks are billable until marked otherwise; true when unset.
    #[must_use]
    pub fn billable_default_or_default(&self) -> bool {
        self.billable_default.unwrap_or(true)
    }

    /// The flat hours of an FTE day; the default when unset.
    #[must_use]
    pub fn flat_hours_or_default(&self) -> f64 {
        self.flat_hours.unwrap_or(DEFAULT_FLAT_HOURS)
    }

    fn validate(&self) -> Result<(), SettingsError> {
        if let Some(path) = money_key_path(&self.extra, "billing") {
            return Err(SettingsError::ForbiddenKey {
                path,
                reason: "rates and currencies belong to the signed app state, not to the vault",
            });
        }
        let foreign = |key: &'static str| SettingsError::InvalidValue {
            key,
            reason: format!("{} billing does not take this key", self.model_label()),
        };
        if self.model != BillingModel::Hourly {
            if self.rounding_minutes.is_some() {
                return Err(foreign("billing.rounding_minutes"));
            }
            if self.billable_default.is_some() {
                return Err(foreign("billing.billable_default"));
            }
        }
        if self.model != BillingModel::Fte && self.flat_hours.is_some() {
            return Err(foreign("billing.flat_hours"));
        }
        if let Some(minutes) = self.rounding_minutes
            && !(1..=MAX_ROUNDING_MINUTES).contains(&minutes)
        {
            return Err(SettingsError::InvalidValue {
                key: "billing.rounding_minutes",
                reason: format!("must be 1 to {MAX_ROUNDING_MINUTES} minutes, got {minutes}"),
            });
        }
        if let Some(hours) = self.flat_hours
            && (!hours.is_finite() || hours <= 0.0 || hours > MAX_HOURS_PER_DAY)
        {
            return Err(SettingsError::InvalidValue {
                key: "billing.flat_hours",
                reason: format!(
                    "must be a number above 0 and at most {MAX_HOURS_PER_DAY}, got {hours}"
                ),
            });
        }
        Ok(())
    }

    const fn model_label(&self) -> &'static str {
        match self.model {
            BillingModel::Hourly => "hourly",
            BillingModel::Fixed => "fixed-price",
            BillingModel::Fte => "FTE",
        }
    }
}

/// The keys the schema defines; `extra` may not shadow them.
const SCHEMA_KEYS: [&str; 16] = [
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
    "billing",
    "log_deadline",
    "leave_source",
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

/// The dotted path of the first key under `object` whose words name money.
fn money_key_path(object: &Map<String, Value>, prefix: &str) -> Option<String> {
    object.iter().find_map(|(key, value)| {
        let path = format!("{prefix}.{key}");
        if key_words(key)
            .iter()
            .any(|word| BILLING_MONEY_KEY_WORDS.contains(&word.as_str()))
        {
            return Some(path);
        }
        money_value_path(value, &path)
    })
}

fn money_value_path(value: &Value, path: &str) -> Option<String> {
    match value {
        Value::Object(nested) => money_key_path(nested, path),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| money_value_path(item, &format!("{path}[{index}]"))),
        _ => None,
    }
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
        BillingModel, BillingSettings, SettingsError, WorkspaceKind, WorkspaceSettings,
        forbidden_key_pattern, is_secret_key,
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

    fn with_billing(block: &str) -> String {
        format!(r#"{{"schema_version": 1, "name": "x", "kind": "work", "billing": {block}}}"#)
    }

    #[test]
    fn each_billing_model_reads_its_keys_with_defaults() {
        let hourly = WorkspaceSettings::from_json(&with_billing(
            r#"{"model": "hourly", "rounding_minutes": 6, "billable_default": false}"#,
        ))
        .expect("hourly");
        let billing = hourly.billing.expect("billing block");
        assert_eq!(billing.model, BillingModel::Hourly);
        assert_eq!(billing.rounding_minutes_or_default(), 6);
        assert!(!billing.billable_default_or_default());

        let bare_hourly =
            WorkspaceSettings::from_json(&with_billing(r#"{"model": "hourly"}"#)).expect("bare");
        let billing = bare_hourly.billing.expect("billing block");
        assert_eq!(billing.rounding_minutes_or_default(), 15);
        assert!(billing.billable_default_or_default());

        let fixed =
            WorkspaceSettings::from_json(&with_billing(r#"{"model": "fixed"}"#)).expect("fixed");
        assert_eq!(
            fixed.billing,
            Some(BillingSettings::new(BillingModel::Fixed))
        );

        let fte =
            WorkspaceSettings::from_json(&with_billing(r#"{"model": "fte", "flat_hours": 7.5}"#))
                .expect("fte");
        assert_eq!(fte.billing.expect("billing").flat_hours_or_default(), 7.5);
        let bare_fte =
            WorkspaceSettings::from_json(&with_billing(r#"{"model": "fte"}"#)).expect("fte");
        assert_eq!(
            bare_fte.billing.expect("billing").flat_hours_or_default(),
            8.0
        );

        let none =
            WorkspaceSettings::from_json(r#"{"schema_version": 1, "name": "x", "kind": "work"}"#)
                .expect("no billing");
        assert_eq!(none.billing, None);
        assert!(!none.to_json().expect("writes").contains("billing"));
    }

    #[test]
    fn the_billing_block_keeps_unknown_keys_through_a_write() {
        let text = with_billing(r#"{"model": "hourly", "x_future": {"a": [1]}}"#);
        let settings = WorkspaceSettings::from_json(&text).expect("valid");
        let billing = settings.billing.clone().expect("billing");
        assert_eq!(billing.extra["x_future"], json!({"a": [1]}));
        let written = settings.to_json().expect("writes");
        assert_eq!(
            WorkspaceSettings::from_json(&written).expect("rereads"),
            settings
        );
        assert!(written.contains("x_future"));
    }

    #[test]
    fn the_billing_block_refuses_rates_tokens_and_other_models_keys() {
        for (block, path) in [
            (
                r#"{"model": "hourly", "rates": {"eur": 90}}"#,
                "billing.rates",
            ),
            (r#"{"model": "fixed", "tokens": []}"#, "billing.tokens"),
            (r#"{"model": "fte", "Rates": 1}"#, "billing.Rates"),
        ] {
            let error = WorkspaceSettings::from_json(&with_billing(block)).expect_err(block);
            assert!(
                matches!(&error, SettingsError::ForbiddenKey { path: found, .. } if found == path),
                "{block}: {error}"
            );
        }
        let secret = with_billing(r#"{"model": "hourly", "toggl_api_token": "abc"}"#);
        assert!(matches!(
            WorkspaceSettings::from_json(&secret),
            Err(SettingsError::SecretKey { path }) if path == "billing.toggl_api_token"
        ));
        for (block, key) in [
            (
                r#"{"model": "fixed", "rounding_minutes": 15}"#,
                "billing.rounding_minutes",
            ),
            (
                r#"{"model": "fte", "billable_default": true}"#,
                "billing.billable_default",
            ),
            (
                r#"{"model": "hourly", "flat_hours": 8}"#,
                "billing.flat_hours",
            ),
            (
                r#"{"model": "hourly", "rounding_minutes": 0}"#,
                "billing.rounding_minutes",
            ),
            (
                r#"{"model": "hourly", "rounding_minutes": 61}"#,
                "billing.rounding_minutes",
            ),
            (r#"{"model": "fte", "flat_hours": 0}"#, "billing.flat_hours"),
            (
                r#"{"model": "fte", "flat_hours": 25}"#,
                "billing.flat_hours",
            ),
        ] {
            let error = WorkspaceSettings::from_json(&with_billing(block)).expect_err(block);
            assert!(
                matches!(&error, SettingsError::InvalidValue { key: found, .. } if *found == key),
                "{block}: {error}"
            );
        }
        assert!(matches!(
            WorkspaceSettings::from_json(&with_billing(r#"{"model": "retainer"}"#)),
            Err(SettingsError::Json(_))
        ));
        let mut settings = WorkspaceSettings::new("x", WorkspaceKind::Work);
        let mut billing = BillingSettings::new(BillingModel::Hourly);
        billing.extra.insert("rates".to_owned(), json!({"eur": 90}));
        settings.billing = Some(billing);
        assert!(matches!(
            settings.to_json(),
            Err(SettingsError::ForbiddenKey { .. })
        ));
    }

    #[test]
    fn the_billing_block_refuses_every_money_key_on_read_and_write() {
        for (block, path) in [
            (r#"{"model": "hourly", "rate": 90}"#, "billing.rate"),
            (
                r#"{"model": "hourly", "hourlyRate": 90}"#,
                "billing.hourlyRate",
            ),
            (
                r#"{"model": "fixed", "currency": "EUR"}"#,
                "billing.currency",
            ),
            (
                r#"{"model": "fixed", "quote_price": 1200}"#,
                "billing.quote_price",
            ),
            (
                r#"{"model": "fte", "x": {"day_rate": 400}}"#,
                "billing.x.day_rate",
            ),
            (
                r#"{"model": "fte", "x": [{"Currency": "CHF"}]}"#,
                "billing.x[0].Currency",
            ),
        ] {
            let error = WorkspaceSettings::from_json(&with_billing(block)).unwrap_err();
            assert!(
                matches!(&error, SettingsError::ForbiddenKey { path: found, .. } if found == path),
                "{block}: {error}"
            );
        }
        let mut settings = WorkspaceSettings::new("x", WorkspaceKind::Work);
        let mut billing = BillingSettings::new(BillingModel::Hourly);
        billing.extra.insert("currency".to_owned(), json!("EUR"));
        settings.billing = Some(billing);
        assert!(matches!(
            settings.to_json(),
            Err(SettingsError::ForbiddenKey { .. })
        ));
        // Words that only contain a money word are not money keys.
        let kept = with_billing(r#"{"model": "hourly", "separate": true, "accurate": 1}"#);
        assert!(WorkspaceSettings::from_json(&kept).is_ok());
    }

    #[test]
    fn the_schema_describes_the_billing_block() {
        let schema = WorkspaceSettings::json_schema();
        let text = schema.to_string();
        assert!(schema["properties"]["billing"].is_object());
        for word in [
            "hourly",
            "fixed",
            "fte",
            "rounding_minutes",
            "flat_hours",
            "billable_default",
        ] {
            assert!(text.contains(word), "{word}");
        }
    }

    #[test]
    fn log_deadline_and_leave_source_are_read_checked_and_kept() {
        let text = r#"{"schema_version": 1, "name": "x", "kind": "work", "log_deadline": {"weekday": "monday", "time": "08:00"}, "leave_source": "Home"}"#;
        let settings = WorkspaceSettings::from_json(text).unwrap();
        let deadline = settings.log_deadline.clone().unwrap();
        assert_eq!(deadline.weekday, super::SettingsWeekday::Monday);
        assert_eq!(deadline.hour_minute(), Some((8, 0)));
        assert_eq!(settings.leave_source.as_deref(), Some("Home"));
        let written = settings.to_json().unwrap();
        assert_eq!(WorkspaceSettings::from_json(&written).unwrap(), settings);
        for bad in [
            r#""log_deadline": {"weekday": "monday", "time": "24:00"}"#,
            r#""log_deadline": {"weekday": "monday", "time": "8:00"}"#,
            r#""log_deadline": {"weekday": "someday", "time": "08:00"}"#,
            r#""leave_source": "  ""#,
        ] {
            let text = format!(r#"{{"schema_version": 1, "name": "x", "kind": "work", {bad}}}"#);
            assert!(WorkspaceSettings::from_json(&text).is_err(), "{bad}");
        }
    }
}
