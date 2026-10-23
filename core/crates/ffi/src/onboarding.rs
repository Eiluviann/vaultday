//! The setup steps' choices stored in the app-level document, as plain
//! records, and the static lists the steps draw: the planned feature sets
//! and the `.gitignore` snippet for vaults that are git repositories, which
//! is added to a workspace folder's `.gitignore` here rather than by the app.

use app_state::{
    AccountPart, FeatureScopeChoice, FeatureScopeMode, GITIGNORE_SNIPPET, OnboardingProgress,
    SyncChoice, SyncHome, SyncService,
};
use std::path::Path;

use domain::registry::{FeatureId, PLANNED_FEATURE_SETS};
use domain::workspace::WorkspaceId;
use vault_base::{FileAccess, FileAccessError, PlainFileAccess};

use crate::CoreError;
use crate::app_state::{AppStateHandle, SourceView};

/// Where the user is in the setup flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct OnboardingProgressView {
    /// The step shown, 1 to 12.
    pub current_step: u32,
    /// The furthest step reached.
    pub furthest_step: u32,
    /// Whether the last step was completed.
    pub is_finished: bool,
}

/// Where the working copy of the notes lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncHomeView {
    /// Only on this Mac.
    Local,
    /// A local folder, synced to the cloud.
    LocalAndCloud,
    /// Directly in a cloud folder.
    Cloud,
}

/// The service that carries the notes between devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncServiceView {
    /// iCloud Drive.
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

/// The sync mode and service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SyncChoiceView {
    /// Where the working copy lives.
    pub home: SyncHomeView,
    /// The service picked.
    pub service: SyncServiceView,
    /// Whether the user confirmed the service.
    pub is_service_chosen: bool,
}

/// A part of an account a workspace wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AccountPartView {
    /// Mail.
    Mail,
    /// Calendars.
    Calendar,
    /// Contacts.
    Contacts,
    /// Reminders.
    Reminders,
}

/// The four-way switch of a feature tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FeatureScopeModeView {
    /// Off.
    Off,
    /// On for every workspace.
    Everywhere,
    /// On for the chosen spheres.
    PerSphere,
    /// On for the chosen workspaces.
    PerWorkspace,
}

/// Where a planned feature set is switched on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FeatureScopeView {
    /// The switch.
    pub mode: FeatureScopeModeView,
    /// Sphere names (`work`, `life`) or workspace ids, by mode.
    pub targets: Vec<String>,
}

/// A feature set the product plans; not in the base version.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlannedFeatureView {
    /// The id its enablement is stored under.
    pub id: String,
    /// The tile's title.
    pub title: String,
    /// The tile's description.
    pub description: String,
    /// Whether switching it on will send data to a new third party.
    pub raises_egress: bool,
}

/// The planned feature sets, in the order the step draws them.
#[uniffi::export]
pub fn planned_feature_sets() -> Vec<PlannedFeatureView> {
    PLANNED_FEATURE_SETS
        .iter()
        .map(|planned| PlannedFeatureView {
            id: planned.id.to_owned(),
            title: planned.title.to_owned(),
            description: planned.description.to_owned(),
            raises_egress: planned.raises_egress,
        })
        .collect()
}

/// The lines the Sync and backup step offers for a vault that is a git repository.
#[uniffi::export]
pub fn gitignore_snippet() -> String {
    GITIGNORE_SNIPPET.to_owned()
}

/// Whether the workspace folder's `.gitignore` already holds the snippet's rules.
///
/// # Errors
/// [`CoreError::FileAccess`] when the file exists but cannot be read.
#[uniffi::export]
pub fn gitignore_has_snippet(workspace_root: String) -> Result<bool, CoreError> {
    Ok(read_gitignore(&gitignore_path(&workspace_root))?
        .is_some_and(|text| holds_snippet_rules(&text)))
}

/// Appends the snippet to the workspace folder's `.gitignore`, creating the
/// file, unless its rules are already there; the existing lines are kept.
///
/// # Errors
/// [`CoreError::FileAccess`] when the file cannot be read or written.
#[uniffi::export]
pub fn add_gitignore_snippet(workspace_root: String) -> Result<bool, CoreError> {
    let path = gitignore_path(&workspace_root);
    let mut text = read_gitignore(&path)?.unwrap_or_default();
    if holds_snippet_rules(&text) {
        return Ok(false);
    }
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(GITIGNORE_SNIPPET);
    PlainFileAccess
        .write_atomic(&path, &text)
        .map_err(|error| CoreError::FileAccess(error.to_string()))?;
    Ok(true)
}

#[uniffi::export]
impl AppStateHandle {
    /// Every source of every workspace.
    pub fn all_sources(&self) -> Vec<SourceView> {
        self.read_state(|state| state.all_sources())
            .into_iter()
            .map(|source| SourceView {
                workspace: source.workspace.as_str().to_owned(),
                connection: source.vault.as_str().to_owned(),
                subpath: source.subpath,
            })
            .collect()
    }

    /// Where the setup flow stands.
    pub fn onboarding_progress(&self) -> OnboardingProgressView {
        let OnboardingProgress {
            current_step,
            furthest_step,
            is_finished,
        } = self.read_state(|state| state.onboarding_progress());
        OnboardingProgressView {
            current_step: u32::from(current_step),
            furthest_step: u32::from(furthest_step),
            is_finished,
        }
    }

    /// Shows a step and raises the furthest step reached.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_onboarding_step(&self, step: u32) -> Result<(), CoreError> {
        let step = u8::try_from(step).unwrap_or(u8::MAX);
        self.edit_state(|state| state.set_onboarding_step(step))
    }

    /// Marks the setup flow finished or not.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_onboarding_finished(&self, is_finished: bool) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_onboarding_finished(is_finished))
    }

    /// The sync mode and service.
    pub fn sync_choice(&self) -> SyncChoiceView {
        let choice = self.read_state(|state| state.sync_choice());
        SyncChoiceView {
            home: match choice.home {
                SyncHome::Local => SyncHomeView::Local,
                SyncHome::LocalAndCloud => SyncHomeView::LocalAndCloud,
                SyncHome::Cloud => SyncHomeView::Cloud,
            },
            service: match choice.service {
                SyncService::IcloudDrive => SyncServiceView::IcloudDrive,
                SyncService::Dropbox => SyncServiceView::Dropbox,
                SyncService::GoogleDrive => SyncServiceView::GoogleDrive,
                SyncService::Syncthing => SyncServiceView::Syncthing,
                SyncService::Folder => SyncServiceView::Folder,
            },
            is_service_chosen: choice.is_service_chosen,
        }
    }

    /// Stores the sync mode and service.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_sync_choice(&self, choice: SyncChoiceView) -> Result<(), CoreError> {
        let choice = SyncChoice {
            home: match choice.home {
                SyncHomeView::Local => SyncHome::Local,
                SyncHomeView::LocalAndCloud => SyncHome::LocalAndCloud,
                SyncHomeView::Cloud => SyncHome::Cloud,
            },
            service: match choice.service {
                SyncServiceView::IcloudDrive => SyncService::IcloudDrive,
                SyncServiceView::Dropbox => SyncService::Dropbox,
                SyncServiceView::GoogleDrive => SyncService::GoogleDrive,
                SyncServiceView::Syncthing => SyncService::Syncthing,
                SyncServiceView::Folder => SyncService::Folder,
            },
            is_service_chosen: choice.is_service_chosen,
        };
        self.edit_state(|state| state.set_sync_choice(choice))
    }

    /// The parts of an account a workspace wants.
    pub fn account_parts(&self, workspace: String) -> Vec<AccountPartView> {
        let workspace = WorkspaceId::new(workspace);
        self.read_state(|state| state.account_parts(&workspace))
            .into_iter()
            .map(|part| match part {
                AccountPart::Mail => AccountPartView::Mail,
                AccountPart::Calendar => AccountPartView::Calendar,
                AccountPart::Contacts => AccountPartView::Contacts,
                AccountPart::Reminders => AccountPartView::Reminders,
            })
            .collect()
    }

    /// Stores the parts of an account a workspace wants.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_account_parts(
        &self,
        workspace: String,
        parts: Vec<AccountPartView>,
    ) -> Result<(), CoreError> {
        let workspace = WorkspaceId::new(workspace);
        let parts: Vec<AccountPart> = parts
            .into_iter()
            .map(|part| match part {
                AccountPartView::Mail => AccountPart::Mail,
                AccountPartView::Calendar => AccountPart::Calendar,
                AccountPartView::Contacts => AccountPart::Contacts,
                AccountPartView::Reminders => AccountPart::Reminders,
            })
            .collect();
        self.edit_state(|state| state.set_account_parts(&workspace, &parts))
    }

    /// Where a feature set is switched on.
    pub fn feature_scope(&self, feature: String) -> FeatureScopeView {
        let feature = FeatureId::new(feature);
        let choice = self.read_state(|state| state.feature_scope(&feature));
        FeatureScopeView {
            mode: match choice.mode {
                FeatureScopeMode::Off => FeatureScopeModeView::Off,
                FeatureScopeMode::Everywhere => FeatureScopeModeView::Everywhere,
                FeatureScopeMode::PerSphere => FeatureScopeModeView::PerSphere,
                FeatureScopeMode::PerWorkspace => FeatureScopeModeView::PerWorkspace,
            },
            targets: choice.targets,
        }
    }

    /// Stores where a feature set is on and writes the registry's switches:
    /// `enabled_workspaces` are the workspace ids it is on for.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_feature_scope(
        &self,
        feature: String,
        scope: FeatureScopeView,
        enabled_workspaces: Vec<String>,
    ) -> Result<(), CoreError> {
        let feature = FeatureId::new(feature);
        let choice = FeatureScopeChoice {
            mode: match scope.mode {
                FeatureScopeModeView::Off => FeatureScopeMode::Off,
                FeatureScopeModeView::Everywhere => FeatureScopeMode::Everywhere,
                FeatureScopeModeView::PerSphere => FeatureScopeMode::PerSphere,
                FeatureScopeModeView::PerWorkspace => FeatureScopeMode::PerWorkspace,
            },
            targets: scope.targets,
        };
        let workspaces: Vec<WorkspaceId> = enabled_workspaces
            .into_iter()
            .map(WorkspaceId::new)
            .collect();
        self.edit_state(|state| state.set_feature_scope(&feature, &choice, &workspaces))
    }
}

fn gitignore_path(workspace_root: &str) -> std::path::PathBuf {
    Path::new(workspace_root).join(".gitignore")
}

fn read_gitignore(path: &Path) -> Result<Option<String>, CoreError> {
    match PlainFileAccess.read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(FileAccessError::NotFound(_)) => Ok(None),
        Err(error) => Err(CoreError::FileAccess(error.to_string())),
    }
}

fn holds_snippet_rules(text: &str) -> bool {
    GITIGNORE_SNIPPET
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .all(|rule| text.lines().any(|line| line.trim() == rule))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gitignore_snippet_is_added_once_and_keeps_the_existing_lines() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().to_string_lossy().into_owned();
        let ignore = folder.path().join(".gitignore");
        assert!(!gitignore_has_snippet(root.clone()).expect("no file"));
        assert!(add_gitignore_snippet(root.clone()).expect("created"));
        assert_eq!(
            std::fs::read_to_string(&ignore).expect("read"),
            GITIGNORE_SNIPPET
        );

        std::fs::write(&ignore, ".obsidian/workspace.json").expect("seed");
        assert!(!gitignore_has_snippet(root.clone()).expect("not yet"));
        assert!(add_gitignore_snippet(root.clone()).expect("appended"));
        let appended = std::fs::read_to_string(&ignore).expect("read");
        assert_eq!(
            appended,
            format!(".obsidian/workspace.json\n\n{GITIGNORE_SNIPPET}")
        );
        assert!(gitignore_has_snippet(root.clone()).expect("has it"));
        assert!(!add_gitignore_snippet(root).expect("already there"));
        assert_eq!(std::fs::read_to_string(&ignore).expect("read"), appended);
    }

    #[test]
    fn the_planned_sets_and_the_snippet_are_served() {
        let planned = planned_feature_sets();
        assert_eq!(planned.len(), 5);
        assert!(
            planned
                .iter()
                .any(|set| set.id == "toggl-billing" && set.raises_egress)
        );
        assert!(planned.iter().all(|set| !set.title.contains("Outlook")));
        assert!(gitignore_snippet().contains(".vaultday/sync"));
    }
}
