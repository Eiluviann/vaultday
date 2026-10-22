//! The UniFFI surface of the Vaultday core.
//!
//! Every exported type is a plain record or a flat error; the only object is
//! [`WorkspaceHandle`], which holds an opened workspace. The Swift package maps
//! these records into `@Observable` models, so views never see them.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::sync::Arc;
use std::time::SystemTime;

use vault_base::{
    Date, IndexScope, PlainFileAccess, Task, TaskStatus, TodayStory, Vault, VaultError, Workspace,
    WorkspaceError,
};

uniffi::setup_scaffolding!();

/// Why a call failed. Flat: the foreign side gets a message per case.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    /// The path holds no `.vaultday/settings.json`.
    #[error("{0} is not a workspace: no .vaultday/settings.json found")]
    NotAWorkspace(String),
    /// The manifest was refused by the settings schema.
    #[error("the workspace settings are invalid: {0}")]
    InvalidSettings(String),
    /// The file system failed.
    #[error("file access failed: {0}")]
    FileAccess(String),
    /// The date argument is not `YYYY-MM-DD`.
    #[error("{0} is not a calendar date; expected YYYY-MM-DD")]
    InvalidDate(String),
}

/// An opened workspace: its root and its manifest, ready for queries.
#[derive(Debug, uniffi::Object)]
pub struct WorkspaceHandle {
    workspace: Workspace,
}

/// What a surface shows for one day.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TodayView {
    /// The workspace name from the manifest.
    pub workspace_name: String,
    /// The day the view was built for, `YYYY-MM-DD`.
    pub date: String,
    /// Stories with open work.
    pub stories: Vec<StoryView>,
    /// The day log's lines for `date`.
    pub log: Vec<LogLineView>,
}

/// One story's open work.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryView {
    /// Path of the story note, relative to the workspace root.
    pub relative_path: String,
    /// The ticket number, when the frontmatter has one.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The story status as written, when recognised.
    pub status: Option<String>,
    /// Open tasks under `## Next Steps`.
    pub next_steps: Vec<TaskView>,
    /// Open tasks under `## Pending Input`.
    pub pending_input: Vec<TaskView>,
}

/// An open task line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TaskView {
    /// The `[id:: …]` field, when present.
    pub id: Option<String>,
    /// The description without its inline fields.
    pub text: String,
    /// Whether the task is untouched or in progress.
    pub state: TaskState,
    /// Nesting depth, 0 for a top-level task.
    pub depth: u32,
    /// The `[assigned_to:: …]` field, when present.
    pub assigned_to: Option<String>,
    /// The `[due:: …]` field, when present.
    pub due: Option<String>,
    /// The `[estimate:: …]` field, when present.
    pub estimate: Option<String>,
}

/// The two open task states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TaskState {
    /// `[ ]`
    Open,
    /// `[/]`
    InProgress,
}

/// One `- HH:MM text [kind:: …]` line of the day log.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LogLineView {
    /// `HH:MM`.
    pub time: String,
    /// The `[kind:: …]` field as written, when present.
    pub kind: Option<String>,
    /// The text without its inline fields.
    pub text: String,
    /// The `[story:: …]` field, when present.
    pub story: Option<String>,
    /// The `[task:: …]` field, when present.
    pub task: Option<String>,
}

#[uniffi::export]
impl WorkspaceHandle {
    /// The normalised workspace root.
    pub fn root(&self) -> String {
        self.workspace.root.to_string_lossy().into_owned()
    }

    /// The workspace name from the manifest.
    pub fn name(&self) -> String {
        self.workspace.settings.name.clone()
    }
}

/// Opens the workspace rooted at `path`.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] without a manifest, [`CoreError::InvalidSettings`]
/// when the manifest is refused, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn open_workspace(path: String) -> Result<Arc<WorkspaceHandle>, CoreError> {
    let workspace = Workspace::open(path.as_ref(), &PlainFileAccess)?;
    Ok(Arc::new(WorkspaceHandle { workspace }))
}

/// Builds the Today view: every story with open work and the day's log lines.
/// `date` is `YYYY-MM-DD`; `None` means the host's local day.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::FileAccess`]
/// when the vault cannot be read.
#[uniffi::export(default(date = None))]
pub fn today(workspace: &WorkspaceHandle, date: Option<String>) -> Result<TodayView, CoreError> {
    let date = resolve_date(date)?;
    let vault = Vault::open(
        &workspace.workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )?;
    let view = vault.today(date);
    Ok(TodayView {
        workspace_name: workspace.workspace.settings.name.clone(),
        date: date.to_string(),
        stories: view.stories.iter().map(story_view).collect(),
        log: view
            .log
            .iter()
            .map(|line| LogLineView {
                time: line.time.to_string(),
                kind: line.kind.as_ref().map(|kind| kind.as_str().to_owned()),
                text: line.text.clone(),
                story: line.story().map(str::to_owned),
                task: line.task().map(str::to_owned),
            })
            .collect(),
    })
}

fn resolve_date(date: Option<String>) -> Result<Date, CoreError> {
    match date {
        Some(text) => Date::parse(&text).ok_or(CoreError::InvalidDate(text)),
        None => Ok(local_today()),
    }
}

fn local_today() -> Date {
    let today = jiff::Zoned::now().date();
    // The civil date of a real clock always fits; a `None` here would be a jiff bug.
    Date::new(
        i32::from(today.year()),
        today.month().unsigned_abs(),
        today.day().unsigned_abs(),
    )
    .unwrap_or_else(|| Date::from_system_time_utc(SystemTime::now()))
}

fn story_view(story: &TodayStory) -> StoryView {
    StoryView {
        relative_path: story.relative_path.to_string_lossy().into_owned(),
        number: story.number.clone(),
        title: story.title.clone(),
        status: story.status.map(|status| status.as_str().to_owned()),
        next_steps: story.next_steps.iter().map(task_view).collect(),
        pending_input: story.pending_input.iter().map(task_view).collect(),
    }
}

fn task_view(task: &Task) -> TaskView {
    TaskView {
        id: task.id().map(str::to_owned),
        text: task.text.clone(),
        state: match task.status {
            TaskStatus::InProgress => TaskState::InProgress,
            _ => TaskState::Open,
        },
        depth: u32::try_from(task.depth).unwrap_or(u32::MAX),
        assigned_to: task.field("assigned_to").map(str::to_owned),
        due: task.field("due").map(str::to_owned),
        estimate: task.field("estimate").map(str::to_owned),
    }
}

impl From<WorkspaceError> for CoreError {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::NotAWorkspace(path) => {
                CoreError::NotAWorkspace(path.to_string_lossy().into_owned())
            }
            WorkspaceError::Settings { .. } => CoreError::InvalidSettings(error.to_string()),
            other => CoreError::FileAccess(other.to_string()),
        }
    }
}

impl From<VaultError> for CoreError {
    fn from(error: VaultError) -> Self {
        CoreError::FileAccess(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn mock_vault() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/mock-vault")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn the_mock_vault_opens_and_lists_open_work() {
        let workspace = open_workspace(mock_vault()).unwrap();
        assert_eq!(workspace.name(), "Mock Studio");
        let view = today(&workspace, Some("2026-10-22".to_owned())).unwrap();
        let numbers: Vec<Option<&str>> = view
            .stories
            .iter()
            .map(|story| story.number.as_deref())
            .collect();
        assert_eq!(numbers, [Some("WEB-142"), Some("WEB-157"), Some("ACME-9")]);
        assert_eq!(view.stories[0].next_steps[2].state, TaskState::InProgress);
        assert_eq!(view.stories[0].next_steps[2].depth, 1);
        assert_eq!(
            view.stories[0].pending_input[0].assigned_to.as_deref(),
            Some("Dana")
        );
        assert_eq!(view.log.len(), 4);
        assert_eq!(view.log[0].kind.as_deref(), Some("done"));
        assert_eq!(view.log[0].story.as_deref(), Some("WEB-142"));
    }

    #[test]
    fn a_folder_without_a_manifest_is_refused() {
        let error = open_workspace(env!("CARGO_MANIFEST_DIR").to_owned()).unwrap_err();
        assert!(matches!(error, CoreError::NotAWorkspace(_)));
    }

    #[test]
    fn a_malformed_date_is_refused() {
        let workspace = open_workspace(mock_vault()).unwrap();
        let error = today(&workspace, Some("yesterday".to_owned())).unwrap_err();
        assert!(matches!(error, CoreError::InvalidDate(_)));
    }

    #[test]
    fn the_local_day_is_the_default() {
        let workspace = open_workspace(mock_vault()).unwrap();
        let view = today(&workspace, None).unwrap();
        assert_eq!(view.date, local_today().to_string());
    }
}
