//! The napi-rs surface of the Vaultday core.
//!
//! Every exported type is a plain object; the only class is
//! [`WorkspaceHandle`], which holds an opened workspace. The TypeScript types
//! in `index.d.ts` are generated from these declarations by `@napi-rs/cli`.

use std::time::SystemTime;

use napi_derive::napi;
use vault_base::{
    Date, IndexScope, PlainFileAccess, Task, TaskStatus, TodayStory, Vault, VaultError, Workspace,
    WorkspaceError,
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

/// Opens the workspace rooted at `path`.
///
/// # Errors
/// A JavaScript error whose message says why: no manifest, a refused manifest
/// or a file system failure.
#[napi]
pub fn open_workspace(path: String) -> napi::Result<WorkspaceHandle> {
    let workspace = Workspace::open(path.as_ref(), &PlainFileAccess).map_err(workspace_error)?;
    Ok(WorkspaceHandle { workspace })
}

/// Builds the Today view: every story with open work and the day's log lines.
/// `date` is `YYYY-MM-DD`; omitted means the host's local day.
///
/// # Errors
/// A JavaScript error for a malformed date or an unreadable vault.
#[napi]
pub fn today(workspace: &WorkspaceHandle, date: Option<String>) -> napi::Result<TodayView> {
    let date = resolve_date(date)?;
    let vault = Vault::open(
        &workspace.workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .map_err(vault_error)?;
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

fn resolve_date(date: Option<String>) -> napi::Result<Date> {
    match date {
        Some(text) => Date::parse(&text).ok_or_else(|| {
            napi::Error::from_reason(format!(
                "{text} is not a calendar date; expected YYYY-MM-DD"
            ))
        }),
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

fn workspace_error(error: WorkspaceError) -> napi::Error {
    let message = match &error {
        WorkspaceError::NotAWorkspace(path) => {
            format!(
                "{} is not a workspace: no .vaultday/settings.json found",
                path.display()
            )
        }
        WorkspaceError::Settings { .. } => format!("the workspace settings are invalid: {error}"),
        _ => format!("file access failed: {error}"),
    };
    napi::Error::from_reason(message)
}

fn vault_error(error: VaultError) -> napi::Error {
    napi::Error::from_reason(format!("file access failed: {error}"))
}
