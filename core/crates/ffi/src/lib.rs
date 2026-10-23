//! The UniFFI surface of the Vaultday core.
//!
//! Every exported type is a plain record or an error. The objects are
//! [`WorkspaceHandle`] (an opened vault workspace),
//! [`app_state::AppStateHandle`] and [`app_state::WorkspaceStateHandle`]
//! (the encrypted app state). The platform supplies the ports as foreign
//! traits: [`secrets::SecretStorePort`] (the Keychain),
//! [`files::SyncFilesPort`] (coordinated file access inside granted folders)
//! and [`calendar::CalendarPort`] (EventKit). The Swift package maps the
//! records into `@Observable` models, so views never see them.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app_state;
pub mod browse;
pub mod calendar;
pub mod files;
pub mod home;
pub mod onboarding;
pub mod preferences;
pub mod secrets;
pub mod side_panel;
pub mod timer;
pub mod workspace_form;

use std::sync::Arc;

use vault_base::{
    Date, PlainFileAccess, Task, TaskStatus, Time, TodayStory, Workspace, ops, ops::OpError,
};

uniffi::setup_scaffolding!();

/// Why a call failed. Flat: the foreign side gets a message per case.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    /// The path is not inside a workspace: no `.vaultday/settings.json` found.
    #[error("{0}")]
    NotAWorkspace(String),
    /// The manifest was refused by the settings schema.
    #[error("{0}")]
    InvalidSettings(String),
    /// The file system failed.
    #[error("{0}")]
    FileAccess(String),
    /// A date or time argument is malformed.
    #[error("{0}")]
    InvalidDate(String),
    /// An argument is not acceptable.
    #[error("{0}")]
    InvalidInput(String),
    /// A story, task or report subject does not exist.
    #[error("{0}")]
    NotFound(String),
    /// The vault changed under the call or the request clashes with it.
    #[error("{0}")]
    Conflict(String),
    /// The platform's secret store failed.
    #[error("{0}")]
    SecretStore(String),
    /// A synchronisable key has not reached this device yet.
    #[error("{0}")]
    KeyNotArrived(String),
    /// The app state refused the operation.
    #[error("{0}")]
    AppState(String),
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
    /// Open tasks that wait on someone else, oldest first.
    pub waiting_on: Vec<WaitingOnView>,
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

vault_base::binding_records!(derive(Debug, Clone, PartialEq, Eq, uniffi::Record));

/// Opens the workspace rooted exactly at `path`.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] without a manifest, [`CoreError::InvalidSettings`]
/// when the manifest is refused, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn open_workspace(path: String) -> Result<Arc<WorkspaceHandle>, CoreError> {
    let workspace = Workspace::open(path.as_ref(), &PlainFileAccess).map_err(OpError::from)?;
    Ok(Arc::new(WorkspaceHandle { workspace }))
}

/// Opens the workspace holding `path` (a file or folder): the deepest
/// `.vaultday/` at or above it wins.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] outside any workspace, otherwise as [`open_workspace`].
#[uniffi::export]
pub fn resolve_workspace(path: String) -> Result<Arc<WorkspaceHandle>, CoreError> {
    let workspace = ops::resolve_workspace(path.as_ref(), &PlainFileAccess)?;
    Ok(Arc::new(WorkspaceHandle { workspace }))
}

/// Builds the Today view: every story with open work, the day's log lines and
/// the waiting-on items. `date` is `YYYY-MM-DD`; `None` means the host's local
/// day.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::FileAccess`]
/// when the vault cannot be read.
#[uniffi::export(default(date = None))]
pub fn today(workspace: &WorkspaceHandle, date: Option<String>) -> Result<TodayView, CoreError> {
    let date = day_or_today(date.as_deref())?;
    Ok(build_today(&workspace.workspace, date)?)
}

/// The task carrying `[id:: id]`, in any state.
///
/// # Errors
/// [`CoreError::NotFound`] without such a task.
#[uniffi::export]
pub fn task_by_id(workspace: &WorkspaceHandle, id: String) -> Result<TaskDetailView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(ops::task_by_id(&vault, &id)?.into())
}

/// Ticks the task `id` done as of `date` (default: the host's local day),
/// changing only that line.
///
/// # Errors
/// [`CoreError::NotFound`] without such a task, [`CoreError::Conflict`] when
/// the line changed while it was written.
#[uniffi::export(default(date = None))]
pub fn tick_task_done(
    workspace: &WorkspaceHandle,
    id: String,
    date: Option<String>,
) -> Result<ChangeView, CoreError> {
    let date = day_or_today(date.as_deref())?;
    let change = ops::tick_task_done(&workspace.workspace, &PlainFileAccess, &id, date)?;
    Ok(change.into())
}

/// One change to a task line, for [`edit_task`]. Dates are `YYYY-MM-DD`;
/// `None` removes the field.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TaskEditInput {
    /// `[x]` with `[completion:: date]`.
    Done {
        /// The completion day.
        date: String,
    },
    /// `[ ]` without a completion date.
    Reopen,
    /// Sets or removes `[scheduled:: …]`.
    Schedule {
        /// The scheduled day.
        date: Option<String>,
    },
    /// Sets or removes `[due:: …]`.
    Due {
        /// The due day.
        date: Option<String>,
    },
    /// Sets or removes `[estimate:: …]` (`2h`, `45m`).
    Estimate {
        /// The estimate text.
        estimate: Option<String>,
    },
    /// Replaces the description, keeping the fields.
    Text {
        /// The new description.
        text: String,
    },
}

impl TaskEditInput {
    fn into_edit(self) -> Result<vault_base::writer::TaskEdit, CoreError> {
        use vault_base::writer::TaskEdit;
        let day = |text: Option<String>| -> Result<Option<Date>, CoreError> {
            text.map(|text| ops::parse_date(&text))
                .transpose()
                .map_err(CoreError::from)
        };
        Ok(match self {
            TaskEditInput::Done { date } => TaskEdit::Done(ops::parse_date(&date)?),
            TaskEditInput::Reopen => TaskEdit::Reopen,
            TaskEditInput::Schedule { date } => TaskEdit::Schedule(day(date)?),
            TaskEditInput::Due { date } => TaskEdit::Due(day(date)?),
            TaskEditInput::Estimate { estimate } => TaskEdit::Estimate(estimate),
            TaskEditInput::Text { text } => TaskEdit::Text(text),
        })
    }
}

/// Applies one edit to the task `id`, changing only its line. The apps'
/// register operations and their undo run through it.
///
/// # Errors
/// [`CoreError::NotFound`] without such a task, [`CoreError::InvalidInput`]
/// for an unusable value, [`CoreError::Conflict`] when the line changed.
#[uniffi::export]
pub fn edit_task(
    workspace: &WorkspaceHandle,
    id: String,
    edit: TaskEditInput,
) -> Result<ChangeView, CoreError> {
    let edit = edit.into_edit()?;
    let change = ops::edit_task(&workspace.workspace, &PlainFileAccess, &id, &edit)?;
    Ok(change.into())
}

/// Puts the line of task `id` back to `line` while it still reads
/// `expected`: the exact undo and redo of [`edit_task`] from its
/// [`ChangeView`].
///
/// # Errors
/// [`CoreError::NotFound`] without such a task, [`CoreError::Conflict`]
/// when the line changed since, [`CoreError::InvalidInput`] for a line that
/// is not one line or drops the id.
#[uniffi::export]
pub fn restore_task_line(
    workspace: &WorkspaceHandle,
    id: String,
    expected: String,
    line: String,
) -> Result<ChangeView, CoreError> {
    let change = ops::restore_task_line(
        &workspace.workspace,
        &PlainFileAccess,
        &id,
        &expected,
        &line,
    )?;
    Ok(change.into())
}

/// The next free `task-N` id.
///
/// # Errors
/// [`CoreError::FileAccess`] for an unreadable vault.
#[uniffi::export]
pub fn next_task_id(workspace: &WorkspaceHandle) -> Result<String, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(ops::next_task_id(&vault))
}

/// The line `add_task` writes for these arguments, so a caller can fingerprint
/// it before asking for the write. `created` defaults to the host's local day.
///
/// # Errors
/// [`CoreError::InvalidInput`] for an unusable text, id, assignee or date.
#[uniffi::export(default(created = None, assigned_to = None))]
pub fn format_task_line(
    text: String,
    id: String,
    created: Option<String>,
    assigned_to: Option<String>,
) -> Result<String, CoreError> {
    let created = day_or_today(created.as_deref())?;
    Ok(ops::format_task_line(
        &text,
        &id,
        created,
        assigned_to.as_deref(),
    )?)
}

/// Adds a task under `## Next Steps` or `## Pending Input` of a story.
///
/// # Errors
/// [`CoreError::NotFound`] for an unknown story, [`CoreError::InvalidInput`]
/// for an unknown section or an unusable value, [`CoreError::Conflict`] when
/// the id is taken.
#[uniffi::export]
pub fn add_task(
    workspace: &WorkspaceHandle,
    input: AddTaskInput,
) -> Result<AddedTaskView, CoreError> {
    let request = input.into_request(local_today()?)?;
    let added = ops::add_story_task(&workspace.workspace, &PlainFileAccess, &request)?;
    Ok(added.into())
}

/// Appends one line to a day log.
///
/// # Errors
/// [`CoreError::InvalidInput`] when the text, kind, date or time is unusable,
/// [`CoreError::NotFound`] for an unknown story or task.
#[uniffi::export]
pub fn append_log(workspace: &WorkspaceHandle, input: LogInput) -> Result<ChangeView, CoreError> {
    let request = input.into_request(local_today()?, local_time()?)?;
    let change = ops::append_day_log(&workspace.workspace, &PlainFileAccess, &request)?;
    Ok(change.into())
}

/// The report for a story number or a client over `from..=to` (`YYYY-MM-DD`).
///
/// # Errors
/// [`CoreError::InvalidInput`] for a malformed or reversed range,
/// [`CoreError::NotFound`] when the subject matches no story and no client.
#[uniffi::export]
pub fn report(
    workspace: &WorkspaceHandle,
    subject: String,
    from: String,
    to: String,
) -> Result<ReportView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let report = ops::build_report(
        &workspace.workspace,
        &vault,
        &subject,
        ops::parse_date(&from)?,
        ops::parse_date(&to)?,
    )?;
    Ok(report.into())
}

fn day_or_today(text: Option<&str>) -> Result<Date, OpError> {
    match text {
        Some(text) => ops::parse_date(text),
        None => local_today(),
    }
}

fn local_today() -> Result<Date, OpError> {
    let today = jiff::Zoned::now().date();
    Date::new(
        i32::from(today.year()),
        today.month().unsigned_abs(),
        today.day().unsigned_abs(),
    )
    .ok_or_else(|| OpError::InvalidInput("the host clock is out of range".to_owned()))
}

fn local_time() -> Result<Time, OpError> {
    let now = jiff::Zoned::now();
    Time::new(now.hour().unsigned_abs(), now.minute().unsigned_abs())
        .ok_or_else(|| OpError::InvalidInput("the host clock is out of range".to_owned()))
}

fn build_today(workspace: &Workspace, date: Date) -> Result<TodayView, OpError> {
    let vault = ops::open_vault(workspace, &PlainFileAccess)?;
    let view = vault.today(date);
    Ok(TodayView {
        workspace_name: workspace.settings.name.clone(),
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
        waiting_on: ops::waiting_items(&vault, date)
            .into_iter()
            .map(Into::into)
            .collect(),
    })
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

impl From<OpError> for CoreError {
    fn from(error: OpError) -> Self {
        let message = error.to_string();
        match error {
            OpError::NotAWorkspace(_) => CoreError::NotAWorkspace(message),
            OpError::InvalidSettings(_) => CoreError::InvalidSettings(message),
            OpError::InvalidDate(_) => CoreError::InvalidDate(message),
            OpError::InvalidInput(_) => CoreError::InvalidInput(message),
            OpError::NotFound(_) => CoreError::NotFound(message),
            OpError::Conflict(_) => CoreError::Conflict(message),
            OpError::FileAccess(_) => CoreError::FileAccess(message),
        }
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
        assert_eq!(view.date, local_today().unwrap().to_string());
    }

    #[test]
    fn today_lists_the_waiting_on_items() {
        let workspace = open_workspace(mock_vault()).unwrap();
        let view = today(&workspace, Some("2026-10-22".to_owned())).unwrap();
        let who: Vec<&str> = view
            .waiting_on
            .iter()
            .map(|item| item.assigned_to.as_str())
            .collect();
        assert!(who.contains(&"Dana") && who.contains(&"Morgan"));
    }

    #[test]
    fn a_story_folder_resolves_to_its_workspace() {
        let folder = format!(
            "{}/Projects/Website/WEB-142 Checkout redesign",
            mock_vault()
        );
        let workspace = resolve_workspace(folder).unwrap();
        assert_eq!(workspace.name(), "Mock Studio");
        let error = resolve_workspace(env!("CARGO_MANIFEST_DIR").to_owned()).unwrap_err();
        assert!(matches!(error, CoreError::NotAWorkspace(_)));
    }

    #[test]
    fn a_task_and_a_client_report_come_back_as_records() {
        let workspace = open_workspace(mock_vault()).unwrap();
        let task = task_by_id(&workspace, "task-6".to_owned()).unwrap();
        assert_eq!(task.section.as_deref(), Some("pending-input"));
        let missing = task_by_id(&workspace, "task-999".to_owned()).unwrap_err();
        assert!(matches!(missing, CoreError::NotFound(_)));

        let report = report(
            &workspace,
            "Website".to_owned(),
            "2026-10-21".to_owned(),
            "2026-10-22".to_owned(),
        )
        .unwrap();
        assert_eq!(report.subject_kind, "client");
        assert_eq!(report.stories.len(), 2);
        assert_eq!(report.log.len(), 8);
        let reversed = super::report(
            &workspace,
            "Website".to_owned(),
            "2026-10-22".to_owned(),
            "2026-10-21".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(reversed, CoreError::InvalidInput(_)));
    }
}
