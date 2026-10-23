//! The feature's manifest, its contributions to the registry's slots, and
//! [`TogglFeature`], the proof that the feature is on for a workspace that
//! every operation of this crate takes.

use domain::DomainError;
use domain::registry::{
    Contribution, Enablement, FeatureId, FeatureManifest, FeatureScope, Registry, Slot,
};
use domain::workspace::WorkspaceId;

use crate::settings::TogglSettings;

/// The feature's id, under which enablement and settings are stored.
pub const FEATURE_ID: &str = "toggl-billing";

/// Story note keys the feature reads: a task's `billable::` inline field.
/// A story's Toggl project comes from the app-level mapping, not the note.
const OWNED_KEYS: [&str; 1] = ["billable"];

/// The feature is on for one workspace. Only [`TogglFeature::for_workspace`]
/// makes one, so holding it means the registry said yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglFeature {
    workspace: WorkspaceId,
}

impl TogglFeature {
    /// The feature for `workspace`, `None` unless the registry holds it and
    /// it is switched on for the app and for the workspace.
    #[must_use]
    pub fn for_workspace(
        registry: &Registry,
        enablement: &Enablement,
        workspace: &WorkspaceId,
    ) -> Option<TogglFeature> {
        registry
            .is_enabled(&feature_id(), enablement, workspace)
            .then(|| TogglFeature {
                workspace: workspace.clone(),
            })
    }

    /// The workspace it is on for.
    #[must_use]
    pub fn workspace(&self) -> &WorkspaceId {
        &self.workspace
    }
}

/// The feature's id.
#[must_use]
pub fn feature_id() -> FeatureId {
    FeatureId::new(FEATURE_ID)
}

/// The JSON Schema of the feature's entry in `settings.json`, as text.
#[must_use]
pub fn settings_schema() -> String {
    schemars::schema_for!(TogglSettings).to_value().to_string()
}

/// What the feature declares about itself: per workspace, no requirements,
/// the story note keys it reads, its settings schema, and egress to Toggl.
#[must_use]
pub fn manifest() -> FeatureManifest {
    FeatureManifest {
        owned_keys: OWNED_KEYS.iter().map(|key| (*key).to_owned()).collect(),
        settings_schema: settings_schema(),
        raises_egress: true,
        ..FeatureManifest::new(FEATURE_ID, FeatureScope::Workspace)
    }
}

/// The feature's contributions: the time sink (the entry queue), the
/// Settings panel and the close-out step for the FTE flat week.
#[must_use]
pub fn contributions() -> Vec<Contribution> {
    vec![
        Contribution::new(FEATURE_ID, Slot::TimeSink, "Toggl queue"),
        Contribution::new(FEATURE_ID, Slot::SettingsPanel, "Toggl and billing"),
        Contribution::new(FEATURE_ID, Slot::CloseOutStep, "Toggl flat entries"),
    ]
}

/// Registers the feature into `registry`.
///
/// # Errors
/// The registry's refusal: the id is taken, or another feature owns one of its keys.
pub fn register(registry: &mut Registry) -> Result<(), DomainError> {
    registry.register(manifest(), contributions())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::registry::PLANNED_FEATURE_SETS;

    fn acme() -> WorkspaceId {
        WorkspaceId::new("acme")
    }

    fn registry() -> Registry {
        let mut registry = Registry::new();
        register(&mut registry).unwrap();
        registry
    }

    const SLOTS: [Slot; 6] = [
        Slot::TicketProvider,
        Slot::TimeSink,
        Slot::SettingsPanel,
        Slot::CliCommand,
        Slot::RailTab,
        Slot::CloseOutStep,
    ];

    #[test]
    fn the_manifest_matches_the_planned_tile() {
        let manifest = manifest();
        let planned = PLANNED_FEATURE_SETS
            .iter()
            .find(|set| set.id == FEATURE_ID)
            .unwrap();
        assert_eq!(manifest.id, feature_id());
        assert_eq!(manifest.scope, FeatureScope::Workspace);
        assert!(manifest.requires.is_empty() && manifest.enhances.is_empty());
        assert_eq!(manifest.owned_keys, ["billable"]);
        assert!(manifest.raises_egress && planned.raises_egress);
        let schema: serde_json::Value = serde_json::from_str(&manifest.settings_schema).unwrap();
        assert_eq!(schema["title"], "toggl-billing");
        assert!(schema["properties"]["tags"].is_object());
    }

    #[test]
    fn enablement_is_the_app_switch_and_the_workspace_switch() {
        let registry = registry();
        let id = feature_id();
        let mut enablement = Enablement::new();
        assert!(TogglFeature::for_workspace(&registry, &enablement, &acme()).is_none());

        enablement.enable_for_workspace("acme", FEATURE_ID);
        assert!(
            TogglFeature::for_workspace(&registry, &enablement, &acme()).is_none(),
            "the workspace switch alone is not enough"
        );
        enablement.disable_for_workspace(&acme(), &id);
        enablement.enable_for_app(FEATURE_ID);
        assert!(
            TogglFeature::for_workspace(&registry, &enablement, &acme()).is_none(),
            "the app switch alone is not enough"
        );
        enablement.enable_for_workspace("acme", FEATURE_ID);
        let feature = TogglFeature::for_workspace(&registry, &enablement, &acme()).unwrap();
        assert_eq!(feature.workspace(), &acme());
        assert!(
            TogglFeature::for_workspace(&registry, &enablement, &WorkspaceId::new("home"))
                .is_none()
        );
        assert!(registry.raises_egress(&enablement, &acme()));
        assert!(!registry.raises_egress(&enablement, &WorkspaceId::new("home")));
    }

    #[test]
    fn its_slots_fill_only_while_it_is_on() {
        let registry = registry();
        let names = |enablement: &Enablement, slot: Slot| -> Vec<String> {
            registry
                .contributions(slot, enablement, &acme())
                .iter()
                .map(|contribution| contribution.name.clone())
                .collect()
        };
        let off = Enablement::new();
        let mut app_only = Enablement::new();
        app_only.enable_for_app(FEATURE_ID);
        let mut workspace_only = Enablement::new();
        workspace_only.enable_for_workspace("acme", FEATURE_ID);
        for enablement in [&off, &app_only, &workspace_only] {
            for slot in SLOTS {
                assert!(names(enablement, slot).is_empty(), "{slot:?}");
            }
            assert!(registry.enabled_features(enablement, &acme()).is_empty());
        }
        let mut on = app_only.clone();
        on.enable_for_workspace("acme", FEATURE_ID);
        assert_eq!(names(&on, Slot::TimeSink), ["Toggl queue"]);
        assert_eq!(names(&on, Slot::SettingsPanel), ["Toggl and billing"]);
        assert_eq!(names(&on, Slot::CloseOutStep), ["Toggl flat entries"]);
        for slot in [Slot::TicketProvider, Slot::CliCommand, Slot::RailTab] {
            assert!(names(&on, slot).is_empty());
        }
    }

    #[test]
    fn it_registers_once_and_owns_its_keys() {
        let mut registry = registry();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.owner_of_key("billable"), Some(&feature_id()));
        assert!(matches!(
            register(&mut registry),
            Err(DomainError::DuplicateFeature { .. })
        ));
    }
}
