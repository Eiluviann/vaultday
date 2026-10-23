//! The napi-rs surface of the Vaultday core.
//!
//! Every exported type is a plain object; the only class is
//! [`WorkspaceHandle`], which holds an opened workspace. The TypeScript types
//! in `index.d.ts` are generated from these declarations by `@napi-rs/cli`.
//!
//! The functions are thin wrappers over `vault_base::ops`; the records that
//! carry its results are defined once, by `vault_base::binding_records!`, and
//! shared with the UniFFI crate. A failed call throws an `Error` whose `code`
//! is `not-a-workspace`, `invalid-settings`, `invalid-input`, `not-found`,
//! `conflict` or `file-access`.

use napi_derive::napi;
use vault_base::{
    Date, PlainFileAccess, Task, TaskStatus, Time, TodayStory, Workspace, ops, ops::OpError,
};

/// An opened workspace: its root and its manifest, ready for queries.
#[napi]
pub struct WorkspaceHandle {
    workspace: Workspace,
}

/// What a surface shows for one day.
#[napi(object)]
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
#[napi(object)]
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
#[napi(object)]
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
#[napi(string_enum = "kebab-case")]
pub enum TaskState {
    /// `[ ]`
    Open,
    /// `[/]`
    InProgress,
}

/// One `- HH:MM text [kind:: …]` line of the day log.
#[napi(object)]
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

#[napi]
impl WorkspaceHandle {
    /// The normalised workspace root.
    #[napi(getter)]
    pub fn root(&self) -> String {
        self.workspace.root.to_string_lossy().into_owned()
    }

    /// The workspace name from the manifest.
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.workspace.settings.name.clone()
    }
}

vault_base::binding_records!(napi(object));

/// Opens the workspace rooted exactly at `path`.
///
/// # Errors
/// `not-a-workspace` without a manifest, `invalid-settings` for a refused
/// manifest, `file-access` otherwise.
#[napi]
pub fn open_workspace(path: String) -> Result<WorkspaceHandle, napi::Error<&'static str>> {
    let workspace = Workspace::open(path.as_ref(), &PlainFileAccess)
        .map_err(OpError::from)
        .map_err(fail)?;
    Ok(WorkspaceHandle { workspace })
}

/// Opens the workspace holding `path` (a file or folder): the deepest
/// `.vaultday/` at or above it wins.
///
/// # Errors
/// `not-a-workspace` outside any workspace, otherwise as [`open_workspace`].
#[napi]
pub fn resolve_workspace(path: String) -> Result<WorkspaceHandle, napi::Error<&'static str>> {
    let workspace = ops::resolve_workspace(path.as_ref(), &PlainFileAccess).map_err(fail)?;
    Ok(WorkspaceHandle { workspace })
}

/// Builds the Today view: every story with open work, the day's log lines and
/// the waiting-on items. `date` is `YYYY-MM-DD`; omitted means the host's
/// local day.
///
/// # Errors
/// `invalid-input` for a malformed date, `file-access` for an unreadable vault.
#[napi]
pub fn today(
    workspace: &WorkspaceHandle,
    date: Option<String>,
) -> Result<TodayView, napi::Error<&'static str>> {
    let date = day_or_today(date.as_deref()).map_err(fail)?;
    build_today(&workspace.workspace, date).map_err(fail)
}

/// The task carrying `[id:: id]`, in any state.
///
/// # Errors
/// `not-found` without such a task.
#[napi]
pub fn task_by_id(
    workspace: &WorkspaceHandle,
    id: String,
) -> Result<TaskDetailView, napi::Error<&'static str>> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess).map_err(fail)?;
    Ok(ops::task_by_id(&vault, &id).map_err(fail)?.into())
}

/// Ticks the task `id` done as of `date` (default: the host's local day),
/// changing only that line.
///
/// # Errors
/// `not-found` without such a task, `conflict` when the line changed while it
/// was written, `invalid-input` for a malformed date.
#[napi]
pub fn tick_task_done(
    workspace: &WorkspaceHandle,
    id: String,
    date: Option<String>,
) -> Result<ChangeView, napi::Error<&'static str>> {
    let date = day_or_today(date.as_deref()).map_err(fail)?;
    let change =
        ops::tick_task_done(&workspace.workspace, &PlainFileAccess, &id, date).map_err(fail)?;
    Ok(change.into())
}

/// The next free `task-N` id.
///
/// # Errors
/// `file-access` for an unreadable vault.
#[napi]
pub fn next_task_id(workspace: &WorkspaceHandle) -> Result<String, napi::Error<&'static str>> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess).map_err(fail)?;
    Ok(ops::next_task_id(&vault))
}

/// The line `add_task` writes for these arguments, so a caller can fingerprint
/// it before asking for the write. `created` defaults to the host's local day.
///
/// # Errors
/// `invalid-input` for an unusable text, id, assignee or date.
#[napi]
pub fn format_task_line(
    text: String,
    id: String,
    created: Option<String>,
    assigned_to: Option<String>,
) -> Result<String, napi::Error<&'static str>> {
    let created = day_or_today(created.as_deref()).map_err(fail)?;
    ops::format_task_line(&text, &id, created, assigned_to.as_deref()).map_err(fail)
}

/// Adds a task under `## Next Steps` or `## Pending Input` of a story.
///
/// # Errors
/// `not-found` for an unknown story, `invalid-input` for an unknown section or
/// an unusable value, `conflict` when the id is taken.
#[napi]
pub fn add_task(
    workspace: &WorkspaceHandle,
    input: AddTaskInput,
) -> Result<AddedTaskView, napi::Error<&'static str>> {
    let request = input
        .into_request(local_today().map_err(fail)?)
        .map_err(fail)?;
    let added =
        ops::add_story_task(&workspace.workspace, &PlainFileAccess, &request).map_err(fail)?;
    Ok(added.into())
}

/// Appends one line to a day log.
///
/// # Errors
/// `invalid-input` when the text, kind, date or time is unusable, `not-found`
/// for an unknown story or task.
#[napi]
pub fn append_log(
    workspace: &WorkspaceHandle,
    input: LogInput,
) -> Result<ChangeView, napi::Error<&'static str>> {
    let request = input
        .into_request(local_today().map_err(fail)?, local_time().map_err(fail)?)
        .map_err(fail)?;
    let change =
        ops::append_day_log(&workspace.workspace, &PlainFileAccess, &request).map_err(fail)?;
    Ok(change.into())
}

/// The report for a story number or a client over `from..=to` (`YYYY-MM-DD`).
///
/// # Errors
/// `invalid-input` for a malformed or reversed range, `not-found` when the
/// subject matches no story and no client.
#[napi]
pub fn report(
    workspace: &WorkspaceHandle,
    subject: String,
    from: String,
    to: String,
) -> Result<ReportView, napi::Error<&'static str>> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess).map_err(fail)?;
    let report = ops::build_report(
        &workspace.workspace,
        &vault,
        &subject,
        ops::parse_date(&from).map_err(fail)?,
        ops::parse_date(&to).map_err(fail)?,
    )
    .map_err(fail)?;
    Ok(report.into())
}

fn fail(error: OpError) -> napi::Error<&'static str> {
    napi::Error::new(error.code(), error.to_string())
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
