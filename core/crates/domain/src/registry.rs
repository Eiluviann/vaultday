//! The feature registry: plumbing for the feature sets that arrive after v1,
//! with nothing plugged in.
//!
//! A feature set ships a [`FeatureManifest`] and the [`Contribution`]s it
//! makes to the app's [`Slot`]s. The base version registers none, so a
//! [`Registry::new`] is empty and stays so; what this module fixes are the
//! rules: a feature is on for a workspace when the app and the workspace both
//! enable it (an app-scoped feature needs only the app), a feature with unmet
//! `requires` is off however it is switched, and a contribution that enhances
//! another feature shows only while both are on. No feature code may depend
//! on another feature: the registry is the only place that knows them both.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DomainError, Result};
use crate::workspace::WorkspaceId;

id_type! {
    /// Identifies a feature set, such as `toggl-billing`.
    FeatureId
}

/// Where a feature is switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureScope {
    /// Once for the whole app; workspaces cannot opt out.
    App,
    /// Per workspace, under the app's switch.
    Workspace,
}

/// The places a feature set can plug into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// Recognises and links ticket numbers for a workspace.
    TicketProvider,
    /// Receives finished sessions (a time tracker, a billing queue).
    TimeSink,
    /// A panel in Settings.
    SettingsPanel,
    /// A `vaultday` CLI subcommand.
    CliCommand,
    /// A tab on the side panel's rail.
    RailTab,
    /// A step of the week's close-out (a flat Toggl week, time cards).
    CloseOutStep,
}

/// What a feature set declares about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureManifest {
    /// The feature's id.
    pub id: FeatureId,
    /// App-wide or per workspace.
    pub scope: FeatureScope,
    /// Features that must be on for this one to work at all.
    pub requires: Vec<FeatureId>,
    /// Features this one adds to when they are on, and works without.
    pub enhances: Vec<FeatureId>,
    /// Keys this feature reads from story notes, as frontmatter or as a
    /// task's inline field; no two features share one.
    pub owned_keys: Vec<String>,
    /// JSON Schema, as text, for the feature's entry under `features` in `settings.json`.
    pub settings_schema: String,
    /// Whether enabling the feature sends data to a new third party.
    pub raises_egress: bool,
}

impl FeatureManifest {
    /// A workspace-scoped manifest with no dependencies, no keys, an empty
    /// schema and no egress; fields are public for the rest.
    pub fn new(id: impl Into<FeatureId>, scope: FeatureScope) -> Self {
        Self {
            id: id.into(),
            scope,
            requires: Vec::new(),
            enhances: Vec::new(),
            owned_keys: Vec::new(),
            settings_schema: String::new(),
            raises_egress: false,
        }
    }
}

/// One thing a feature plugs into a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contribution {
    /// The contributing feature.
    pub feature: FeatureId,
    /// The slot.
    pub slot: Slot,
    /// The contribution's name within the slot (a command name, a tab title).
    pub name: String,
    /// When set, the contribution shows only while this feature is on too.
    /// It must be listed in the manifest's `enhances`.
    pub enhances: Option<FeatureId>,
}

impl Contribution {
    /// A plain contribution of `feature` to `slot`.
    pub fn new(feature: impl Into<FeatureId>, slot: Slot, name: impl Into<String>) -> Self {
        Self {
            feature: feature.into(),
            slot,
            name: name.into(),
            enhances: None,
        }
    }

    /// The same contribution, shown only while `other` is on as well.
    #[must_use]
    pub fn enhancing(self, other: impl Into<FeatureId>) -> Self {
        Self {
            enhances: Some(other.into()),
            ..self
        }
    }
}

/// Which features are switched on for the app and for each workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Enablement {
    app: BTreeSet<FeatureId>,
    workspaces: BTreeMap<WorkspaceId, BTreeSet<FeatureId>>,
}

impl Enablement {
    /// Nothing switched on.
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches a feature on for the app.
    pub fn enable_for_app(&mut self, feature: impl Into<FeatureId>) {
        self.app.insert(feature.into());
    }

    /// Switches a feature off for the app.
    pub fn disable_for_app(&mut self, feature: &FeatureId) {
        self.app.remove(feature);
    }

    /// Switches a feature on for a workspace.
    pub fn enable_for_workspace(
        &mut self,
        workspace: impl Into<WorkspaceId>,
        feature: impl Into<FeatureId>,
    ) {
        self.workspaces
            .entry(workspace.into())
            .or_default()
            .insert(feature.into());
    }

    /// Switches a feature off for a workspace.
    pub fn disable_for_workspace(&mut self, workspace: &WorkspaceId, feature: &FeatureId) {
        if let Some(features) = self.workspaces.get_mut(workspace) {
            features.remove(feature);
        }
    }

    /// Whether the app switch is on.
    pub fn is_on_for_app(&self, feature: &FeatureId) -> bool {
        self.app.contains(feature)
    }

    /// Whether the workspace switch is on.
    pub fn is_on_for_workspace(&self, workspace: &WorkspaceId, feature: &FeatureId) -> bool {
        self.workspaces
            .get(workspace)
            .is_some_and(|features| features.contains(feature))
    }
}

/// The registered features and their contributions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    manifests: BTreeMap<FeatureId, FeatureManifest>,
    contributions: Vec<Contribution>,
}

impl Registry {
    /// An empty registry: what the base version runs with.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many features are registered.
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Registers a feature with its contributions.
    ///
    /// # Errors
    /// [`DomainError::DuplicateFeature`] for an id already registered,
    /// [`DomainError::OwnedKeyClaimed`] when another feature owns one of its
    /// keys, [`DomainError::UndeclaredEnhancement`] when a contribution
    /// enhances a feature the manifest does not list.
    pub fn register(
        &mut self,
        manifest: FeatureManifest,
        contributions: impl IntoIterator<Item = Contribution>,
    ) -> Result<()> {
        if self.manifests.contains_key(&manifest.id) {
            return Err(DomainError::DuplicateFeature { id: manifest.id });
        }
        if let Some((key, holder)) = self.claimed_key(&manifest) {
            return Err(DomainError::OwnedKeyClaimed {
                feature: manifest.id,
                key,
                holder,
            });
        }
        let contributions: Vec<Contribution> = contributions
            .into_iter()
            .map(|contribution| Contribution {
                feature: manifest.id.clone(),
                ..contribution
            })
            .collect();
        if let Some(undeclared) = contributions.iter().find_map(|contribution| {
            contribution
                .enhances
                .as_ref()
                .filter(|other| !manifest.enhances.contains(other))
        }) {
            return Err(DomainError::UndeclaredEnhancement {
                feature: manifest.id,
                enhances: undeclared.clone(),
            });
        }
        self.contributions.extend(contributions);
        self.manifests.insert(manifest.id.clone(), manifest);
        Ok(())
    }

    /// The manifest of a registered feature.
    pub fn manifest(&self, id: &FeatureId) -> Option<&FeatureManifest> {
        self.manifests.get(id)
    }

    /// Every registered manifest, by id.
    pub fn manifests(&self) -> impl Iterator<Item = &FeatureManifest> {
        self.manifests.values()
    }

    /// The feature that owns a frontmatter key, if any.
    pub fn owner_of_key(&self, key: &str) -> Option<&FeatureId> {
        self.manifests
            .values()
            .find(|manifest| manifest.owned_keys.iter().any(|owned| owned == key))
            .map(|manifest| &manifest.id)
    }

    /// Whether a feature is on in `workspace`: registered, switched on for the
    /// app and (unless app-scoped) for the workspace, with every feature it
    /// requires on by the same rule. Features that require each other in a
    /// cycle count as satisfied once each is switched on.
    pub fn is_enabled(
        &self,
        id: &FeatureId,
        enablement: &Enablement,
        workspace: &WorkspaceId,
    ) -> bool {
        self.is_enabled_visiting(id, enablement, workspace, &mut BTreeSet::new())
    }

    /// The features on in `workspace`, by id.
    pub fn enabled_features(
        &self,
        enablement: &Enablement,
        workspace: &WorkspaceId,
    ) -> Vec<&FeatureManifest> {
        self.manifests
            .values()
            .filter(|manifest| self.is_enabled(&manifest.id, enablement, workspace))
            .collect()
    }

    /// The contributions to a slot that show in `workspace`: those of enabled
    /// features, minus the ones enhancing a feature that is off.
    pub fn contributions(
        &self,
        slot: Slot,
        enablement: &Enablement,
        workspace: &WorkspaceId,
    ) -> Vec<&Contribution> {
        self.contributions
            .iter()
            .filter(|contribution| contribution.slot == slot)
            .filter(|contribution| self.is_enabled(&contribution.feature, enablement, workspace))
            .filter(|contribution| {
                contribution
                    .enhances
                    .as_ref()
                    .is_none_or(|other| self.is_enabled(other, enablement, workspace))
            })
            .collect()
    }

    /// Whether any feature on in `workspace` raises egress.
    pub fn raises_egress(&self, enablement: &Enablement, workspace: &WorkspaceId) -> bool {
        self.enabled_features(enablement, workspace)
            .iter()
            .any(|manifest| manifest.raises_egress)
    }

    fn claimed_key(&self, manifest: &FeatureManifest) -> Option<(String, FeatureId)> {
        manifest.owned_keys.iter().find_map(|key| {
            self.owner_of_key(key)
                .map(|holder| (key.clone(), holder.clone()))
        })
    }

    fn is_enabled_visiting(
        &self,
        id: &FeatureId,
        enablement: &Enablement,
        workspace: &WorkspaceId,
        visiting: &mut BTreeSet<FeatureId>,
    ) -> bool {
        let Some(manifest) = self.manifests.get(id) else {
            return false;
        };
        let is_switched_on = enablement.is_on_for_app(id)
            && match manifest.scope {
                FeatureScope::App => true,
                FeatureScope::Workspace => enablement.is_on_for_workspace(workspace, id),
            };
        if !is_switched_on {
            return false;
        }
        if !visiting.insert(id.clone()) {
            return true;
        }
        let requirements_met = manifest
            .requires
            .iter()
            .all(|required| self.is_enabled_visiting(required, enablement, workspace, visiting));
        visiting.remove(id);
        requirements_met
    }
}

/// A feature set the product plans but the base version does not ship. The
/// onboarding step lists these as inert tiles with a "Planned" tag; the
/// registry itself stays empty until a feature crate registers a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedFeatureSet {
    /// The id the feature will register under; enablement is stored under it today.
    pub id: &'static str,
    /// The tile's title.
    pub title: &'static str,
    /// The tile's one-line description.
    pub description: &'static str,
    /// Whether switching it on will send data to a new third party.
    pub raises_egress: bool,
}

/// The planned feature sets, in the order the Feature sets step draws them.
pub const PLANNED_FEATURE_SETS: [PlannedFeatureSet; 5] = [
    PlannedFeatureSet {
        id: "github",
        title: "GitHub",
        description: "Review requests, assigned issues, failing checks",
        raises_egress: true,
    },
    PlannedFeatureSet {
        id: "servicenow",
        title: "ServiceNow",
        description: "Capture tickets from the browser, link stories",
        raises_egress: false,
    },
    PlannedFeatureSet {
        id: "azure-devops",
        title: "Azure DevOps",
        description: "Work items and pull requests, linked to stories",
        raises_egress: false,
    },
    PlannedFeatureSet {
        id: "teams-work-mail",
        title: "Teams and work mail",
        description: "Messages and mail from a work tenant as signals",
        raises_egress: false,
    },
    PlannedFeatureSet {
        id: "toggl-billing",
        title: "Toggl and billing",
        description: "Send logged hours, hourly or fixed-price billing",
        raises_egress: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn acme() -> WorkspaceId {
        WorkspaceId::new("acme")
    }

    fn time_sink() -> FeatureManifest {
        FeatureManifest {
            owned_keys: vec!["sink_project".to_owned()],
            settings_schema: r#"{"type":"object"}"#.to_owned(),
            raises_egress: true,
            ..FeatureManifest::new("time-sink", FeatureScope::Workspace)
        }
    }

    fn fully_on(registry: &Registry, ids: &[&str]) -> Enablement {
        let mut enablement = Enablement::new();
        for id in ids {
            assert!(
                registry.manifest(&FeatureId::new(*id)).is_some(),
                "{id} is not registered"
            );
            enablement.enable_for_app(*id);
            enablement.enable_for_workspace(acme(), *id);
        }
        enablement
    }

    #[test]
    fn the_base_version_registers_nothing() {
        let registry = Registry::new();
        let enablement = Enablement::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.manifests().next().is_none());
        assert!(!registry.is_enabled(&FeatureId::new("time-sink"), &enablement, &acme()));
        for slot in [
            Slot::TicketProvider,
            Slot::TimeSink,
            Slot::SettingsPanel,
            Slot::CliCommand,
            Slot::RailTab,
            Slot::CloseOutStep,
        ] {
            assert!(
                registry
                    .contributions(slot, &enablement, &acme())
                    .is_empty()
            );
        }
        assert!(!registry.raises_egress(&enablement, &acme()));
    }

    #[test]
    fn a_workspace_feature_needs_the_app_and_the_workspace_switch() {
        let mut registry = Registry::new();
        registry.register(time_sink(), []).unwrap();
        let id = FeatureId::new("time-sink");
        let mut enablement = Enablement::new();
        assert!(!registry.is_enabled(&id, &enablement, &acme()));

        enablement.enable_for_app("time-sink");
        assert!(!registry.is_enabled(&id, &enablement, &acme()));

        enablement.enable_for_workspace("acme", "time-sink");
        assert!(registry.is_enabled(&id, &enablement, &acme()));
        assert!(!registry.is_enabled(&id, &enablement, &WorkspaceId::new("home")));
        assert!(registry.raises_egress(&enablement, &acme()));

        enablement.disable_for_app(&id);
        assert!(!registry.is_enabled(&id, &enablement, &acme()));
        enablement.enable_for_app("time-sink");
        enablement.disable_for_workspace(&acme(), &id);
        assert!(!registry.is_enabled(&id, &enablement, &acme()));
    }

    #[test]
    fn an_app_feature_ignores_the_workspace_switch() {
        let mut registry = Registry::new();
        registry
            .register(
                FeatureManifest::new("palette-extras", FeatureScope::App),
                [],
            )
            .unwrap();
        let id = FeatureId::new("palette-extras");
        let mut enablement = Enablement::new();
        enablement.enable_for_app("palette-extras");
        assert!(registry.is_enabled(&id, &enablement, &acme()));
        assert!(registry.is_enabled(&id, &enablement, &WorkspaceId::new("home")));
        assert!(!registry.raises_egress(&enablement, &acme()));
    }

    #[test]
    fn requires_must_be_on_through_the_whole_chain() {
        let mut registry = Registry::new();
        registry.register(time_sink(), []).unwrap();
        registry
            .register(
                FeatureManifest {
                    requires: vec![FeatureId::new("time-sink")],
                    ..FeatureManifest::new("billing", FeatureScope::Workspace)
                },
                [],
            )
            .unwrap();
        registry
            .register(
                FeatureManifest {
                    requires: vec![FeatureId::new("billing")],
                    ..FeatureManifest::new("invoices", FeatureScope::Workspace)
                },
                [],
            )
            .unwrap();
        let invoices = FeatureId::new("invoices");

        let all = fully_on(&registry, &["time-sink", "billing", "invoices"]);
        assert!(registry.is_enabled(&invoices, &all, &acme()));
        assert_eq!(registry.enabled_features(&all, &acme()).len(), 3);

        let mut without_sink = all.clone();
        without_sink.disable_for_workspace(&acme(), &FeatureId::new("time-sink"));
        assert!(!registry.is_enabled(&FeatureId::new("billing"), &without_sink, &acme()));
        assert!(!registry.is_enabled(&invoices, &without_sink, &acme()));
        assert_eq!(registry.enabled_features(&without_sink, &acme()).len(), 0);

        // A requirement that is not registered at all is unmet.
        registry
            .register(
                FeatureManifest {
                    requires: vec![FeatureId::new("ghost")],
                    ..FeatureManifest::new("haunted", FeatureScope::App)
                },
                [],
            )
            .unwrap();
        let mut haunted = Enablement::new();
        haunted.enable_for_app("haunted");
        haunted.enable_for_app("ghost");
        assert!(!registry.is_enabled(&FeatureId::new("haunted"), &haunted, &acme()));
    }

    #[test]
    fn features_requiring_each_other_are_on_once_both_are_switched_on() {
        let mut registry = Registry::new();
        registry
            .register(
                FeatureManifest {
                    requires: vec![FeatureId::new("b")],
                    ..FeatureManifest::new("a", FeatureScope::App)
                },
                [],
            )
            .unwrap();
        registry
            .register(
                FeatureManifest {
                    requires: vec![FeatureId::new("a")],
                    ..FeatureManifest::new("b", FeatureScope::App)
                },
                [],
            )
            .unwrap();
        let mut enablement = Enablement::new();
        enablement.enable_for_app("a");
        assert!(!registry.is_enabled(&FeatureId::new("a"), &enablement, &acme()));
        enablement.enable_for_app("b");
        assert!(registry.is_enabled(&FeatureId::new("a"), &enablement, &acme()));
        assert!(registry.is_enabled(&FeatureId::new("b"), &enablement, &acme()));
    }

    #[test]
    fn enhancing_contributions_show_only_while_both_features_are_on() {
        let mut registry = Registry::new();
        registry
            .register(
                time_sink(),
                [
                    Contribution::new("time-sink", Slot::TimeSink, "queue"),
                    Contribution::new("time-sink", Slot::SettingsPanel, "Time sink"),
                ],
            )
            .unwrap();
        registry
            .register(
                FeatureManifest {
                    enhances: vec![FeatureId::new("time-sink")],
                    ..FeatureManifest::new("tickets", FeatureScope::Workspace)
                },
                [
                    Contribution::new("tickets", Slot::TicketProvider, "desk"),
                    Contribution::new("tickets", Slot::CliCommand, "tickets"),
                    Contribution::new("tickets", Slot::RailTab, "Billing by ticket")
                        .enhancing("time-sink"),
                ],
            )
            .unwrap();

        let names = |slot: Slot, enablement: &Enablement| -> Vec<String> {
            registry
                .contributions(slot, enablement, &acme())
                .iter()
                .map(|contribution| contribution.name.clone())
                .collect()
        };

        let only_tickets = fully_on(&registry, &["tickets"]);
        assert_eq!(names(Slot::TicketProvider, &only_tickets), ["desk"]);
        assert_eq!(names(Slot::CliCommand, &only_tickets), ["tickets"]);
        assert!(names(Slot::RailTab, &only_tickets).is_empty());
        assert!(names(Slot::TimeSink, &only_tickets).is_empty());

        let both = fully_on(&registry, &["tickets", "time-sink"]);
        assert_eq!(names(Slot::RailTab, &both), ["Billing by ticket"]);
        assert_eq!(names(Slot::TimeSink, &both), ["queue"]);
        assert_eq!(names(Slot::SettingsPanel, &both), ["Time sink"]);

        let only_sink = fully_on(&registry, &["time-sink"]);
        assert!(names(Slot::RailTab, &only_sink).is_empty());
        assert!(names(Slot::TicketProvider, &only_sink).is_empty());
    }

    #[test]
    fn registration_refuses_duplicates_claimed_keys_and_undeclared_enhancements() {
        let mut registry = Registry::new();
        registry.register(time_sink(), []).unwrap();
        assert_eq!(
            registry.register(time_sink(), []),
            Err(DomainError::DuplicateFeature {
                id: FeatureId::new("time-sink")
            })
        );
        assert_eq!(
            registry.register(
                FeatureManifest {
                    owned_keys: vec!["other".to_owned(), "sink_project".to_owned()],
                    ..FeatureManifest::new("rival", FeatureScope::Workspace)
                },
                [],
            ),
            Err(DomainError::OwnedKeyClaimed {
                feature: FeatureId::new("rival"),
                key: "sink_project".to_owned(),
                holder: FeatureId::new("time-sink"),
            })
        );
        assert_eq!(
            registry.register(
                FeatureManifest::new("tickets", FeatureScope::Workspace),
                [Contribution::new("tickets", Slot::RailTab, "x").enhancing("time-sink")],
            ),
            Err(DomainError::UndeclaredEnhancement {
                feature: FeatureId::new("tickets"),
                enhances: FeatureId::new("time-sink"),
            })
        );
        // A refused registration leaves nothing behind.
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.owner_of_key("sink_project"),
            Some(&FeatureId::new("time-sink"))
        );
        assert_eq!(registry.owner_of_key("other"), None);
    }

    #[test]
    fn a_contribution_is_stamped_with_the_registering_feature() {
        let mut registry = Registry::new();
        registry
            .register(
                time_sink(),
                [Contribution::new("someone-else", Slot::CliCommand, "sink")],
            )
            .unwrap();
        let enablement = fully_on(&registry, &["time-sink"]);
        let shown = registry.contributions(Slot::CliCommand, &enablement, &acme());
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].feature, FeatureId::new("time-sink"));
    }

    #[test]
    fn planned_feature_sets_are_unique_and_none_is_registered() {
        let mut ids: Vec<&str> = PLANNED_FEATURE_SETS.iter().map(|set| set.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PLANNED_FEATURE_SETS.len());
        let registry = Registry::new();
        assert!(registry.is_empty());
        for planned in PLANNED_FEATURE_SETS {
            assert!(registry.manifest(&FeatureId::new(planned.id)).is_none());
            assert!(!planned.title.is_empty() && !planned.description.ends_with('.'));
        }
    }
}
