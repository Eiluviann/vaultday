//! The app-level choices the setup steps make: where the user stopped, how
//! the notes sync, which parts of an account a workspace wants, and where
//! each planned feature set is switched on. All of it lives in the
//! document's `settings` map and the feature switches; nothing here is
//! enforced yet, and nothing secret belongs here.

use automerge::transaction::Transactable;
use domain::registry::FeatureId;
use domain::workspace::WorkspaceId;

use super::{AppState, EnablementScope, WORKSPACE_SCOPE};
use crate::doc::{
    KEY_SEPARATOR, composite_key, delete_if_present, get_bool, get_str, get_u64, keys,
};
use crate::error::AppStateError;

/// The number of setup steps.
pub const ONBOARDING_STEP_COUNT: u8 = 12;

const STEP_KEY: &str = "onboarding_step";
const FURTHEST_KEY: &str = "onboarding_furthest";
const FINISHED_KEY: &str = "onboarding_finished";
const SYNC_HOME_KEY: &str = "sync_home";
const SYNC_SERVICE_KEY: &str = "sync_service";
const SYNC_SERVICE_CHOSEN_KEY: &str = "sync_service_chosen";
const FEATURE_SCOPE_PREFIX: &str = "feature_scope";
const FEATURE_TARGETS_PREFIX: &str = "feature_targets";
const ACCOUNT_PARTS_PREFIX: &str = "account_parts";

/// Where the user is in the setup flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnboardingProgress {
    /// The step shown, 1 to [`ONBOARDING_STEP_COUNT`].
    pub current_step: u8,
    /// The furthest step reached; steps before it are done and linked back.
    pub furthest_step: u8,
    /// Whether the last step was completed.
    pub is_finished: bool,
}

/// Where the working copy of the notes lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncHome {
    /// Only on this Mac.
    Local,
    /// A local folder, synced to the cloud.
    #[default]
    LocalAndCloud,
    /// Directly in a cloud folder.
    Cloud,
}

/// The service that carries the notes between devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncService {
    /// iCloud Drive.
    #[default]
    IcloudDrive,
    /// Dropbox.
    Dropbox,
    /// Google Drive.
    GoogleDrive,
    /// Syncthing.
    Syncthing,
    /// Any other folder the user syncs.
    Folder,
}

/// The sync mode and service, as the Sync and backup step stores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncChoice {
    /// Where the working copy lives.
    pub home: SyncHome,
    /// The service picked.
    pub service: SyncService,
    /// Whether the user confirmed the service; picking in the control alone is not that.
    pub is_service_chosen: bool,
}

/// The four-way switch of a feature tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeatureScopeMode {
    /// Off.
    #[default]
    Off,
    /// On for every workspace.
    Everywhere,
    /// On for the chosen spheres (`work`, `life`).
    PerSphere,
    /// On for the chosen workspaces.
    PerWorkspace,
}

/// Where a planned feature set is switched on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureScopeChoice {
    /// The switch.
    pub mode: FeatureScopeMode,
    /// Sphere names for [`FeatureScopeMode::PerSphere`], workspace ids for
    /// [`FeatureScopeMode::PerWorkspace`], empty otherwise.
    pub targets: Vec<String>,
}

/// A part of an account a workspace wants Vaultday to bring in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountPart {
    /// Mail.
    Mail,
    /// Calendars.
    Calendar,
    /// Contacts.
    Contacts,
    /// Reminders.
    Reminders,
}

impl AccountPart {
    const ALL: [AccountPart; 4] = [
        AccountPart::Mail,
        AccountPart::Calendar,
        AccountPart::Contacts,
        AccountPart::Reminders,
    ];

    const fn label(self) -> &'static str {
        match self {
            AccountPart::Mail => "mail",
            AccountPart::Calendar => "calendar",
            AccountPart::Contacts => "contacts",
            AccountPart::Reminders => "reminders",
        }
    }
}

impl SyncHome {
    const fn label(self) -> &'static str {
        match self {
            SyncHome::Local => "local",
            SyncHome::LocalAndCloud => "local-and-cloud",
            SyncHome::Cloud => "cloud",
        }
    }
}

impl SyncService {
    const fn label(self) -> &'static str {
        match self {
            SyncService::IcloudDrive => "icloud-drive",
            SyncService::Dropbox => "dropbox",
            SyncService::GoogleDrive => "google-drive",
            SyncService::Syncthing => "syncthing",
            SyncService::Folder => "folder",
        }
    }
}

impl FeatureScopeMode {
    const fn label(self) -> &'static str {
        match self {
            FeatureScopeMode::Off => "off",
            FeatureScopeMode::Everywhere => "everywhere",
            FeatureScopeMode::PerSphere => "per-sphere",
            FeatureScopeMode::PerWorkspace => "per-workspace",
        }
    }
}

impl AppState {
    /// Where the setup flow stands; step 1, nothing done, until it is moved.
    #[must_use]
    pub fn onboarding_progress(&self) -> OnboardingProgress {
        let settings = &self.objects.settings;
        let clamp = |value: Option<u64>| {
            let step = value
                .unwrap_or(1)
                .clamp(1, u64::from(ONBOARDING_STEP_COUNT));
            u8::try_from(step).unwrap_or(1)
        };
        let current_step = clamp(get_u64(&self.doc, settings, STEP_KEY));
        let furthest_step = clamp(get_u64(&self.doc, settings, FURTHEST_KEY)).max(current_step);
        OnboardingProgress {
            current_step,
            furthest_step,
            is_finished: get_bool(&self.doc, settings, FINISHED_KEY).unwrap_or(false),
        }
    }

    /// Shows `step` (1 to 12, clamped) and raises the furthest step reached.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_onboarding_step(&mut self, step: u8) -> Result<(), AppStateError> {
        let step = step.clamp(1, ONBOARDING_STEP_COUNT);
        let furthest = self.onboarding_progress().furthest_step.max(step);
        self.doc
            .put(&self.objects.settings, STEP_KEY, u64::from(step))?;
        self.doc
            .put(&self.objects.settings, FURTHEST_KEY, u64::from(furthest))?;
        Ok(())
    }

    /// Marks the setup flow finished or not.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_onboarding_finished(&mut self, is_finished: bool) -> Result<(), AppStateError> {
        self.doc
            .put(&self.objects.settings, FINISHED_KEY, is_finished)?;
        Ok(())
    }

    /// The sync mode and service; local plus iCloud Drive, not yet chosen, by default.
    #[must_use]
    pub fn sync_choice(&self) -> SyncChoice {
        let settings = &self.objects.settings;
        let home = match get_str(&self.doc, settings, SYNC_HOME_KEY).as_deref() {
            Some("local") => SyncHome::Local,
            Some("cloud") => SyncHome::Cloud,
            _ => SyncHome::LocalAndCloud,
        };
        let service = match get_str(&self.doc, settings, SYNC_SERVICE_KEY).as_deref() {
            Some("dropbox") => SyncService::Dropbox,
            Some("google-drive") => SyncService::GoogleDrive,
            Some("syncthing") => SyncService::Syncthing,
            Some("folder") => SyncService::Folder,
            _ => SyncService::IcloudDrive,
        };
        SyncChoice {
            home,
            service,
            is_service_chosen: get_bool(&self.doc, settings, SYNC_SERVICE_CHOSEN_KEY)
                .unwrap_or(false),
        }
    }

    /// Stores the sync mode and service.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_sync_choice(&mut self, choice: SyncChoice) -> Result<(), AppStateError> {
        let settings = self.objects.settings.clone();
        self.doc
            .put(&settings, SYNC_HOME_KEY, choice.home.label())?;
        self.doc
            .put(&settings, SYNC_SERVICE_KEY, choice.service.label())?;
        self.doc
            .put(&settings, SYNC_SERVICE_CHOSEN_KEY, choice.is_service_chosen)?;
        Ok(())
    }

    /// The parts of an account a workspace wants, in a fixed order.
    #[must_use]
    pub fn account_parts(&self, workspace: &WorkspaceId) -> Vec<AccountPart> {
        let key = composite_key(&[ACCOUNT_PARTS_PREFIX, workspace.as_str()]);
        let stored = get_str(&self.doc, &self.objects.settings, &key).unwrap_or_default();
        AccountPart::ALL
            .into_iter()
            .filter(|part| stored.split(',').any(|label| label == part.label()))
            .collect()
    }

    /// Stores the parts of an account a workspace wants; an empty set clears them.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_account_parts(
        &mut self,
        workspace: &WorkspaceId,
        parts: &[AccountPart],
    ) -> Result<(), AppStateError> {
        let key = composite_key(&[ACCOUNT_PARTS_PREFIX, workspace.as_str()]);
        let labels: Vec<&str> = AccountPart::ALL
            .into_iter()
            .filter(|part| parts.contains(part))
            .map(AccountPart::label)
            .collect();
        let settings = self.objects.settings.clone();
        if labels.is_empty() {
            return delete_if_present(&mut self.doc, &settings, &key);
        }
        self.doc.put(&settings, key, labels.join(","))?;
        Ok(())
    }

    /// Where a feature set is switched on; off until set.
    #[must_use]
    pub fn feature_scope(&self, feature: &FeatureId) -> FeatureScopeChoice {
        let settings = &self.objects.settings;
        let mode_key = composite_key(&[FEATURE_SCOPE_PREFIX, feature.as_str()]);
        let mode = match get_str(&self.doc, settings, &mode_key).as_deref() {
            Some("everywhere") => FeatureScopeMode::Everywhere,
            Some("per-sphere") => FeatureScopeMode::PerSphere,
            Some("per-workspace") => FeatureScopeMode::PerWorkspace,
            _ => FeatureScopeMode::Off,
        };
        let targets_key = composite_key(&[FEATURE_TARGETS_PREFIX, feature.as_str()]);
        let targets = get_str(&self.doc, settings, &targets_key)
            .map(|text| {
                text.split(',')
                    .filter(|target| !target.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        FeatureScopeChoice { mode, targets }
    }

    /// Stores where a feature set is on, and writes the registry's switches
    /// to match: the app switch is on unless the mode is off, and exactly the
    /// workspaces in `enabled_workspaces` (the caller resolves everywhere,
    /// spheres and chosen workspaces to them) are on for it.
    ///
    /// Nothing is plugged into the registry in the base version, so the
    /// switches change no behaviour.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_feature_scope(
        &mut self,
        feature: &FeatureId,
        choice: &FeatureScopeChoice,
        enabled_workspaces: &[WorkspaceId],
    ) -> Result<(), AppStateError> {
        let settings = self.objects.settings.clone();
        let mode_key = composite_key(&[FEATURE_SCOPE_PREFIX, feature.as_str()]);
        let targets_key = composite_key(&[FEATURE_TARGETS_PREFIX, feature.as_str()]);
        self.doc.put(&settings, mode_key, choice.mode.label())?;
        if choice.targets.is_empty() {
            delete_if_present(&mut self.doc, &settings, &targets_key)?;
        } else {
            self.doc
                .put(&settings, targets_key, choice.targets.join(","))?;
        }
        let is_on = choice.mode != FeatureScopeMode::Off;
        self.set_feature_enabled(&EnablementScope::App, feature, is_on)?;
        for workspace in self.workspaces_switched_for(feature) {
            self.set_feature_enabled(&EnablementScope::Workspace(workspace), feature, false)?;
        }
        if is_on {
            for workspace in enabled_workspaces {
                self.set_feature_enabled(
                    &EnablementScope::Workspace(workspace.clone()),
                    feature,
                    true,
                )?;
            }
        }
        Ok(())
    }

    /// The workspaces with a switch stored for `feature`, on or off.
    fn workspaces_switched_for(&self, feature: &FeatureId) -> Vec<WorkspaceId> {
        keys(&self.doc, &self.objects.features)
            .iter()
            .filter_map(|key| {
                let parts: Vec<&str> = key.split(KEY_SEPARATOR).collect();
                match parts.as_slice() {
                    [WORKSPACE_SCOPE, workspace, name] if *name == feature.as_str() => {
                        Some(WorkspaceId::new(*workspace))
                    }
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;

    fn fresh() -> AppState {
        AppState::new(DeviceId::new("dev-a").expect("device id")).expect("document")
    }

    #[test]
    fn progress_starts_at_the_first_step_and_only_moves_the_furthest_forward() {
        let mut state = fresh();
        assert_eq!(
            state.onboarding_progress(),
            OnboardingProgress {
                current_step: 1,
                furthest_step: 1,
                is_finished: false
            }
        );
        state.set_onboarding_step(4).expect("step");
        state.set_onboarding_step(2).expect("back");
        let progress = state.onboarding_progress();
        assert_eq!((progress.current_step, progress.furthest_step), (2, 4));
        state.set_onboarding_step(99).expect("clamped");
        assert_eq!(
            state.onboarding_progress().current_step,
            ONBOARDING_STEP_COUNT
        );
        state.set_onboarding_finished(true).expect("finished");
        assert!(state.onboarding_progress().is_finished);
    }

    #[test]
    fn the_sync_choice_defaults_to_local_and_cloud_and_round_trips() {
        let mut state = fresh();
        assert_eq!(state.sync_choice(), SyncChoice::default());
        assert_eq!(state.sync_choice().home, SyncHome::LocalAndCloud);
        let choice = SyncChoice {
            home: SyncHome::Cloud,
            service: SyncService::Syncthing,
            is_service_chosen: true,
        };
        state.set_sync_choice(choice).expect("stored");
        assert_eq!(state.sync_choice(), choice);
    }

    #[test]
    fn account_parts_are_stored_per_workspace_in_a_fixed_order() {
        let mut state = fresh();
        let work = WorkspaceId::new("ws-1");
        let life = WorkspaceId::new("ws-2");
        assert!(state.account_parts(&work).is_empty());
        state
            .set_account_parts(&work, &[AccountPart::Reminders, AccountPart::Mail])
            .expect("stored");
        assert_eq!(
            state.account_parts(&work),
            [AccountPart::Mail, AccountPart::Reminders]
        );
        assert!(state.account_parts(&life).is_empty());
        state.set_account_parts(&work, &[]).expect("cleared");
        assert!(state.account_parts(&work).is_empty());
    }

    #[test]
    fn a_feature_scope_writes_the_registry_switches_and_replaces_the_old_ones() {
        let mut state = fresh();
        let feature = FeatureId::new("toggl-billing");
        let one = WorkspaceId::new("ws-1");
        let two = WorkspaceId::new("ws-2");
        assert_eq!(state.feature_scope(&feature), FeatureScopeChoice::default());

        let per_workspace = FeatureScopeChoice {
            mode: FeatureScopeMode::PerWorkspace,
            targets: vec!["ws-1".to_owned()],
        };
        state
            .set_feature_scope(&feature, &per_workspace, std::slice::from_ref(&one))
            .expect("stored");
        assert_eq!(state.feature_scope(&feature), per_workspace);
        let enablement = state.enablement();
        assert!(enablement.is_on_for_app(&feature));
        assert!(enablement.is_on_for_workspace(&one, &feature));
        assert!(!enablement.is_on_for_workspace(&two, &feature));

        let per_sphere = FeatureScopeChoice {
            mode: FeatureScopeMode::PerSphere,
            targets: vec!["life".to_owned()],
        };
        state
            .set_feature_scope(&feature, &per_sphere, std::slice::from_ref(&two))
            .expect("replaced");
        let enablement = state.enablement();
        assert!(!enablement.is_on_for_workspace(&one, &feature));
        assert!(enablement.is_on_for_workspace(&two, &feature));

        state
            .set_feature_scope(
                &feature,
                &FeatureScopeChoice::default(),
                std::slice::from_ref(&two),
            )
            .expect("off");
        let enablement = state.enablement();
        assert!(!enablement.is_on_for_app(&feature));
        assert!(!enablement.is_on_for_workspace(&two, &feature));
        assert_eq!(state.feature_scope(&feature).mode, FeatureScopeMode::Off);
    }
}
