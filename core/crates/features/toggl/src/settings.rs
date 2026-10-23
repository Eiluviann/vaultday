//! The feature's entry under `features.toggl-billing` in `settings.json`:
//! what a workspace's entries carry besides the task (a client name and
//! tags). It holds no secret and no money; unknown keys are kept.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::TogglError;

/// The workspace's Toggl settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(
    title = "toggl-billing",
    description = "The Toggl and billing feature's settings for one workspace. No token, rate or currency belongs here: the token is in the Keychain, rates in the app's signed store."
)]
pub struct TogglSettings {
    /// The Toggl client the workspace's projects belong to, for display and
    /// for picking a project; Toggl derives an entry's client from its project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Tags every entry of the workspace carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Keys this build does not know, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl TogglSettings {
    /// Reads the feature's entry; an absent entry is the defaults.
    ///
    /// # Errors
    /// [`TogglError::InvalidSettings`] when the entry does not match the schema
    /// or a tag is blank.
    pub fn from_entry(entry: Option<&Value>) -> Result<TogglSettings, TogglError> {
        let Some(entry) = entry else {
            return Ok(TogglSettings::default());
        };
        let settings: TogglSettings = serde_json::from_value(entry.clone())
            .map_err(|error| TogglError::InvalidSettings(error.to_string()))?;
        if settings.tags.iter().any(|tag| tag.trim().is_empty()) {
            return Err(TogglError::InvalidSettings(
                "a tag must not be blank".to_owned(),
            ));
        }
        Ok(settings)
    }

    /// The entry as stored under `features.toggl-billing`.
    ///
    /// # Errors
    /// [`TogglError::InvalidSettings`] when serialisation fails.
    pub fn to_entry(&self) -> Result<Value, TogglError> {
        serde_json::to_value(self).map_err(|error| TogglError::InvalidSettings(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_defaults_tags_and_keeps_unknown_keys() {
        assert_eq!(
            TogglSettings::from_entry(None).unwrap(),
            TogglSettings::default()
        );
        let entry = json!({"client": "Mock Client", "tags": ["vaultday", "web"], "x_future": 1});
        let settings = TogglSettings::from_entry(Some(&entry)).unwrap();
        assert_eq!(settings.client.as_deref(), Some("Mock Client"));
        assert_eq!(settings.tags, ["vaultday", "web"]);
        assert_eq!(settings.to_entry().unwrap(), entry);
    }

    #[test]
    fn refuses_a_wrong_shape_and_blank_tags() {
        for entry in [json!({"tags": "web"}), json!({"tags": [" "]}), json!([1])] {
            assert!(matches!(
                TogglSettings::from_entry(Some(&entry)),
                Err(TogglError::InvalidSettings(_))
            ));
        }
    }
}
