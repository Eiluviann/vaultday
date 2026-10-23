//! The operations the CLI, the MCP server and the apps share: resolve a
//! workspace from a path, ask the vault for stories, tasks, waiting-on items
//! and reports, and make the few writes a session needs (tick a task, add a
//! task, append a day-log line).
//!
//! Everything here takes and returns owned plain records (strings, numbers,
//! options and lists) so the napi and UniFFI bindings copy them field by field
//! and hold no logic of their own. The clock is the caller's: dates and times
//! arrive as values, so these functions stay pure and testable.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;
use thiserror::Error;

use crate::date::{Date, Time};
use crate::file_access::FileAccess;
use crate::index::IndexScope;
use crate::log::{LogAppend, LogFormatError};
use crate::note::{LogKind, Story};
use crate::scan::{Task, TaskStatus};
use crate::vault::{ME, Note, Vault, VaultError};
use crate::workspace::{Workspace, WorkspaceError, find_workspace_root};
use crate::writer::{
    NewTask, TaskSection, TaskTarget, VaultWriter, WriteError, WriteReceipt, task_line,
};

const TASK_ID_PREFIX: &str = "task-";

/// Why an operation failed. The kind says what a caller can do about it.
#[derive(Debug, Error)]
pub enum OpError {
    /// The path is not inside a workspace.
    #[error(
        "{} is not a workspace: no .vaultday/settings.json at or above it",
        .0.display()
    )]
    NotAWorkspace(PathBuf),
    /// The workspace manifest was refused.
    #[error("the workspace settings are invalid: {0}")]
    InvalidSettings(String),
    /// A date or time argument is malformed.
    #[error("{0}")]
    InvalidDate(String),
    /// An argument is not acceptable.
    #[error("{0}")]
    InvalidInput(String),
    /// A story, task or report subject does not exist.
    #[error("{0}")]
    NotFound(String),
    /// The vault changed under the caller or the request clashes with it.
    #[error("{0}")]
    Conflict(String),
    /// The file system failed.
    #[error("file access failed: {0}")]
    FileAccess(String),
}

impl OpError {
    /// A stable kebab-case code for the kind, which the Node binding puts on
    /// the JavaScript error so callers can branch without parsing messages.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            OpError::NotAWorkspace(_) => "not-a-workspace",
            OpError::InvalidSettings(_) => "invalid-settings",
            OpError::InvalidDate(_) => "invalid-date",
            OpError::InvalidInput(_) => "invalid-input",
            OpError::NotFound(_) => "not-found",
            OpError::Conflict(_) => "conflict",
            OpError::FileAccess(_) => "file-access",
        }
    }
}

/// Parses `YYYY-MM-DD`.
///
/// # Errors
/// [`OpError::InvalidDate`] for anything else.
pub fn parse_date(text: &str) -> Result<Date, OpError> {
    Date::parse(text).ok_or_else(|| {
        OpError::InvalidDate(format!(
            "{text} is not a calendar date; expected YYYY-MM-DD"
        ))
    })
}

/// Parses `HH:MM`.
///
/// # Errors
/// [`OpError::InvalidDate`] for anything else.
pub fn parse_time(text: &str) -> Result<Time, OpError> {
    Time::parse(text)
        .ok_or_else(|| OpError::InvalidDate(format!("{text} is not a time; expected HH:MM")))
}

/// An open task assigned to someone else, with how long it has waited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingItem {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// The story number when the note is a story.
    pub story_number: Option<String>,
    /// The `[id:: …]` field, when present.
    pub task_id: Option<String>,
    /// The description without its inline fields.
    pub text: String,
    /// Who it waits on.
    pub assigned_to: String,
    /// Days since `[created:: …]`, when the field is present and parses.
    pub age_days: Option<i64>,
}

/// A task line with where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// The story number when the note is a story.
    pub story_number: Option<String>,
    /// `next-steps` or `pending-input` when the task sits in one of them.
    pub section: Option<String>,
    /// 1-based line number.
    pub line_number: u32,
    /// The `[id:: …]` field, when present.
    pub id: Option<String>,
    /// The description without its inline fields.
    pub text: String,
    /// `open`, `in-progress`, `done`, `cancelled` or `other`.
    pub status: String,
    /// Nesting depth, 0 for a top-level task.
    pub depth: u32,
    /// The `[assigned_to:: …]` field, when present.
    pub assigned_to: Option<String>,
    /// The `[created:: …]` field, when present.
    pub created: Option<String>,
    /// The `[scheduled:: …]` field, when present.
    pub scheduled: Option<String>,
    /// The `[due:: …]` field, when present.
    pub due: Option<String>,
    /// The `[estimate:: …]` field, when present.
    pub estimate: Option<String>,
    /// The `[completion:: …]` field, when present.
    pub completion: Option<String>,
}

/// One line a write changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRecord {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// 1-based line number in the new file.
    pub line_number: u32,
    /// The line as it was; `None` when the line is new.
    pub old_line: Option<String>,
    /// The line as it is now.
    pub new_line: String,
}

/// A task that was added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedTask {
    /// The id the new task carries.
    pub id: String,
    /// The line that was written.
    pub change: ChangeRecord,
}

/// What `add_story_task` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTaskRequest {
    /// The story number, such as `WEB-142`.
    pub story_number: String,
    /// `next-steps` or `pending-input`.
    pub section: String,
    /// The description, one line without inline fields.
    pub text: String,
    /// Who the task waits on.
    pub assigned_to: Option<String>,
    /// The id to use; allocated (`task-N`, one past the highest in the vault)
    /// when absent.
    pub id: Option<String>,
    /// The creation date.
    pub created: Date,
    /// The day the task is planned for.
    pub scheduled: Option<Date>,
    /// The estimate as written, such as `45m`.
    pub estimate: Option<String>,
    /// The `%% ai:begin … %%` line that wraps a model-written task.
    pub begin_marker: Option<String>,
}

/// What `append_day_log` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRequest {
    /// The day whose log gets the line.
    pub date: Date,
    /// The time prefix.
    pub time: Time,
    /// `session`, `done`, `decision`, `blocker`, `status` or `note`.
    pub kind: String,
    /// What happened, on one line.
    pub text: String,
    /// The story number the line is about.
    pub story: Option<String>,
    /// The task id the line is about.
    pub task: Option<String>,
    /// The model that wrote the line; absent for facts.
    pub agent: Option<String>,
}

/// One frontmatter key with its values (a scalar has one, a list several).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterEntry {
    /// The key.
    pub key: String,
    /// The values as text.
    pub values: Vec<String>,
}

/// One story in a report.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryReport {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// The ticket number, when the frontmatter has one.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The story status as written.
    pub status: Option<String>,
    /// Every frontmatter key in source order.
    pub frontmatter: Vec<FrontmatterEntry>,
    /// Tasks under `## Next Steps`, open and closed.
    pub next_steps: Vec<TaskRecord>,
    /// Tasks under `## Pending Input`, open and closed.
    pub pending_input: Vec<TaskRecord>,
    /// The note's text, for the authorship summary.
    pub note_text: String,
}

/// One day-log line in a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLogLine {
    /// The log note, relative to the workspace root.
    pub relative_path: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM`.
    pub time: String,
    /// The `[kind:: …]` field as written.
    pub kind: Option<String>,
    /// The text without its inline fields.
    pub text: String,
    /// The `[story:: …]` field.
    pub story: Option<String>,
    /// The `[project:: …]` field.
    pub project: Option<String>,
    /// The `[task:: …]` field.
    pub task: Option<String>,
    /// The `[agent:: …]` field: the model that wrote the line.
    pub agent: Option<String>,
}

/// Everything about one story or client over a date range.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// What the caller asked for.
    pub subject: String,
    /// `story` or `client`.
    pub subject_kind: String,
    /// First day, `YYYY-MM-DD`.
    pub from: String,
    /// Last day, `YYYY-MM-DD`.
    pub to: String,
    /// The stories the subject covers.
    pub stories: Vec<StoryReport>,
    /// Their day-log lines in range, oldest first.
    pub log: Vec<ReportLogLine>,
}

/// Resolves the workspace holding `start` (a file or folder): the deepest
/// `.vaultday/` at or above it wins.
///
/// # Errors
/// [`OpError::NotAWorkspace`] outside any workspace, [`OpError::InvalidSettings`]
/// for a refused manifest, [`OpError::FileAccess`] otherwise.
pub fn resolve_workspace(start: &Path, file_access: &dyn FileAccess) -> Result<Workspace, OpError> {
    let root = find_workspace_root(start, file_access)?
        .ok_or_else(|| OpError::NotAWorkspace(crate::workspace::normalize_path(start)))?;
    Ok(Workspace::open(&root, file_access)?)
}

/// Loads the vault of `workspace` with the default index scope.
///
/// # Errors
/// [`OpError::FileAccess`] when a note cannot be read.
pub fn open_vault(workspace: &Workspace, file_access: &dyn FileAccess) -> Result<Vault, OpError> {
    Ok(Vault::open(
        workspace,
        IndexScope::default(),
        file_access,
        SystemTime::now(),
    )?)
}

/// Open tasks assigned to someone other than the vault's owner, oldest first,
/// with their age as of `today`.
#[must_use]
pub fn waiting_items(vault: &Vault, today: Date) -> Vec<WaitingItem> {
    vault
        .waiting_on(ME, today)
        .into_iter()
        .map(|item| WaitingItem {
            relative_path: path_text(item.path),
            story_number: item.story_number.map(str::to_owned),
            task_id: item.task.id().map(str::to_owned),
            text: item.task.text.clone(),
            assigned_to: item.assigned_to.to_owned(),
            age_days: item.age_days,
        })
        .collect()
}

/// The task carrying `[id:: id]`.
///
/// # Errors
/// [`OpError::NotFound`] when no task has the id.
pub fn task_by_id(vault: &Vault, id: &str) -> Result<TaskRecord, OpError> {
    let found = vault
        .task_by_id(id)
        .ok_or_else(|| OpError::NotFound(format!("no task with [id:: {id}] in the workspace")))?;
    let note = vault.note(found.path);
    Ok(task_record(found.path, note, found.task))
}

/// Ticks the task `id` done as of `date`, changing only that line.
///
/// # Errors
/// [`OpError::NotFound`] without such a task, [`OpError::Conflict`] when the
/// line changed while it was written, [`OpError::FileAccess`] otherwise.
pub fn tick_task_done(
    workspace: &Workspace,
    file_access: &dyn FileAccess,
    id: &str,
    date: Date,
) -> Result<ChangeRecord, OpError> {
    let vault = open_vault(workspace, file_access)?;
    let found = vault
        .task_by_id(id)
        .ok_or_else(|| OpError::NotFound(format!("no task with [id:: {id}] in the workspace")))?;
    let target = TaskTarget::new(found.path, id);
    let receipt = VaultWriter::new(file_access, workspace).tick_done(&target, date)?;
    Ok(change_record(receipt))
}

/// The next free `task-N` id: one past the highest in the vault.
#[must_use]
pub fn next_task_id(vault: &Vault) -> String {
    let highest = vault
        .tasks()
        .filter_map(|found| found.task.id())
        .filter_map(|id| id.strip_prefix(TASK_ID_PREFIX)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("{TASK_ID_PREFIX}{}", highest.saturating_add(1))
}

/// The line `add_story_task` writes for these arguments, so a caller can
/// fingerprint it before asking for the write.
///
/// # Errors
/// [`OpError::InvalidInput`] for an unusable text, id or assignee.
pub fn format_task_line(
    text: &str,
    id: &str,
    created: Date,
    assigned_to: Option<&str>,
) -> Result<String, OpError> {
    let task = NewTask {
        text: text.to_owned(),
        id: id.to_owned(),
        created,
        assigned_to: assigned_to.map(str::to_owned),
        scheduled: None,
        estimate: None,
        due: None,
        reminder: None,
    };
    Ok(task_line(&task)?)
}

/// Adds a task under `## Next Steps` or `## Pending Input` of a story.
///
/// # Errors
/// [`OpError::NotFound`] for an unknown story, [`OpError::InvalidInput`] for an
/// unknown section or an unusable value, [`OpError::Conflict`] when the id is
/// taken, [`OpError::FileAccess`] otherwise.
pub fn add_story_task(
    workspace: &Workspace,
    file_access: &dyn FileAccess,
    request: &AddTaskRequest,
) -> Result<AddedTask, OpError> {
    let section = parse_section(&request.section)?;
    let vault = open_vault(workspace, file_access)?;
    let story = vault
        .story_by_number(&request.story_number)
        .ok_or_else(|| story_not_found(&request.story_number))?;
    let id = request.id.clone().unwrap_or_else(|| next_task_id(&vault));
    let task = NewTask {
        text: request.text.clone(),
        id: id.clone(),
        created: request.created,
        assigned_to: request.assigned_to.clone(),
        scheduled: request.scheduled,
        estimate: request.estimate.clone(),
        due: None,
        reminder: None,
    };
    let writer = VaultWriter::new(file_access, workspace);
    let receipt = match &request.begin_marker {
        Some(marker) => writer.add_model_task(story.path, section, &task, marker)?,
        None => writer.add_task(story.path, section, &task)?,
    };
    Ok(AddedTask {
        id,
        change: change_record(receipt),
    })
}

/// Appends one line to the day log of `request.date`.
///
/// # Errors
/// [`OpError::InvalidInput`] when the text or kind breaks the day-log grammar,
/// [`OpError::NotFound`] for an unknown story or task, [`OpError::FileAccess`]
/// otherwise.
pub fn append_day_log(
    workspace: &Workspace,
    file_access: &dyn FileAccess,
    request: &LogRequest,
) -> Result<ChangeRecord, OpError> {
    let kind = LogKind::parse(&request.kind);
    if let LogKind::Other(other) = &kind {
        return Err(LogFormatError::UnknownKind(other.clone()).into());
    }
    if request.story.is_some() || request.task.is_some() {
        let vault = open_vault(workspace, file_access)?;
        if let Some(number) = &request.story
            && vault.story_by_number(number).is_none()
        {
            return Err(story_not_found(number));
        }
        if let Some(id) = &request.task
            && vault.task_by_id(id).is_none()
        {
            return Err(OpError::NotFound(format!(
                "no task with [id:: {id}] in the workspace"
            )));
        }
    }
    let mut entry = LogAppend::new(request.time, request.text.clone()).with_kind(kind);
    if let Some(story) = &request.story {
        entry = entry.with_story(story.clone());
    }
    if let Some(task) = &request.task {
        entry = entry.with_task(task.clone());
    }
    if let Some(agent) = &request.agent {
        entry = entry.with_agent(agent.clone());
    }
    let receipt = VaultWriter::new(file_access, workspace).append_log(request.date, &entry)?;
    Ok(change_record(receipt))
}

/// Builds the report for a story number or a client over `from..=to`.
///
/// `subject` is a story number (`WEB-142`) or a client: a project hub's key,
/// title or folder name (`WEB`, `Website`), or the workspace's own name for
/// every story in it. Log lines count when they name one of the stories, one of
/// their tasks or, for a client, its project.
///
/// # Errors
/// [`OpError::InvalidInput`] for a reversed range, [`OpError::NotFound`] when
/// the subject matches nothing.
pub fn build_report(
    workspace: &Workspace,
    vault: &Vault,
    subject: &str,
    from: Date,
    to: Date,
) -> Result<Report, OpError> {
    if to < from {
        return Err(OpError::InvalidInput(format!(
            "the range is reversed: {from} is after {to}"
        )));
    }
    let scope = resolve_subject(workspace, vault, subject)?;
    let stories: Vec<StoryReport> = vault
        .stories()
        .into_iter()
        .filter(|entry| scope.covers(entry.path))
        .map(|entry| story_report(vault, entry.path, entry.story))
        .collect();
    let numbers: Vec<&str> = stories
        .iter()
        .filter_map(|story| story.number.as_deref())
        .collect();
    let task_ids: Vec<&str> = stories
        .iter()
        .flat_map(|story| story.next_steps.iter().chain(&story.pending_input))
        .filter_map(|task| task.id.as_deref())
        .collect();
    let mut log: Vec<ReportLogLine> = Vec::new();
    for note in vault.notes() {
        let Some(day_log) = &note.day_log else {
            continue;
        };
        if day_log.date < from || to < day_log.date {
            continue;
        }
        for line in &day_log.lines {
            let belongs = line.story().is_some_and(|story| numbers.contains(&story))
                || line.task().is_some_and(|task| task_ids.contains(&task))
                || line
                    .project()
                    .is_some_and(|project| scope.names_project(project));
            if belongs {
                log.push(ReportLogLine {
                    relative_path: path_text(&note.relative_path),
                    date: day_log.date.to_string(),
                    time: line.time.to_string(),
                    kind: line.kind.as_ref().map(|kind| kind.as_str().to_owned()),
                    text: line.text.clone(),
                    story: line.story().map(str::to_owned),
                    project: line.project().map(str::to_owned),
                    task: line.task().map(str::to_owned),
                    agent: line.field("agent").map(str::to_owned),
                });
            }
        }
    }
    log.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
    Ok(Report {
        subject: subject.to_owned(),
        subject_kind: scope.kind().to_owned(),
        from: from.to_string(),
        to: to.to_string(),
        stories,
        log,
    })
}

/// Which stories a report covers and which project names its log lines may use.
enum Scope {
    Story { path: PathBuf },
    Project { folder: PathBuf, names: Vec<String> },
    Workspace,
}

impl Scope {
    fn kind(&self) -> &'static str {
        match self {
            Scope::Story { .. } => "story",
            Scope::Project { .. } | Scope::Workspace => "client",
        }
    }

    fn covers(&self, story_path: &Path) -> bool {
        match self {
            Scope::Story { path } => story_path == path,
            Scope::Project { folder, .. } => story_path.starts_with(folder),
            Scope::Workspace => true,
        }
    }

    fn names_project(&self, project: &str) -> bool {
        match self {
            Scope::Story { .. } => false,
            Scope::Project { names, .. } => names.iter().any(|name| same_name(name, project)),
            Scope::Workspace => true,
        }
    }
}

fn resolve_subject(workspace: &Workspace, vault: &Vault, subject: &str) -> Result<Scope, OpError> {
    if let Some(entry) = vault.story_by_number(subject) {
        return Ok(Scope::Story {
            path: entry.path.to_path_buf(),
        });
    }
    for (path, hub) in vault.projects() {
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let folder_name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let names: Vec<String> = [hub.key.clone(), hub.title.clone(), folder_name]
            .into_iter()
            .flatten()
            .collect();
        if names.iter().any(|name| same_name(name, subject)) {
            return Ok(Scope::Project { folder, names });
        }
    }
    if same_name(&workspace.settings.name, subject) {
        return Ok(Scope::Workspace);
    }
    Err(OpError::NotFound(format!(
        "{subject} is neither a story number nor a client (project key, title or folder) in this workspace"
    )))
}

fn story_report(vault: &Vault, path: &Path, story: &Story) -> StoryReport {
    let note = vault.note(path);
    let records = |tasks: &[Task]| -> Vec<TaskRecord> {
        tasks
            .iter()
            .map(|task| task_record(path, note, task))
            .collect()
    };
    StoryReport {
        relative_path: path_text(path),
        number: story.number.clone(),
        title: story.title.clone(),
        status: story.raw_status.clone(),
        frontmatter: note.map(frontmatter_entries).unwrap_or_default(),
        next_steps: records(&story.next_steps),
        pending_input: records(&story.pending_input),
        note_text: note.map(|note| note.document.render()).unwrap_or_default(),
    }
}

fn frontmatter_entries(note: &Note) -> Vec<FrontmatterEntry> {
    let Some(frontmatter) = note.document.frontmatter() else {
        return Vec::new();
    };
    frontmatter
        .fields()
        .iter()
        .map(|(key, value)| FrontmatterEntry {
            key: key.clone(),
            values: match value {
                Value::Null => Vec::new(),
                Value::Array(items) => items.iter().map(value_text).collect(),
                other => vec![value_text(other)],
            },
        })
        .collect()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn task_record(path: &Path, note: Option<&Note>, task: &Task) -> TaskRecord {
    let story = note.and_then(|note| note.story.as_ref());
    let section = story.and_then(|story| {
        if story
            .next_steps
            .iter()
            .any(|candidate| candidate.line == task.line)
        {
            Some("next-steps")
        } else if story
            .pending_input
            .iter()
            .any(|candidate| candidate.line == task.line)
        {
            Some("pending-input")
        } else {
            None
        }
    });
    let field = |key: &str| task.field(key).map(str::to_owned);
    TaskRecord {
        relative_path: path_text(path),
        story_number: story.and_then(|story| story.number.clone()),
        section: section.map(str::to_owned),
        line_number: u32::try_from(task.line + 1).unwrap_or(u32::MAX),
        id: field("id"),
        text: task.text.clone(),
        status: status_name(task.status).to_owned(),
        depth: u32::try_from(task.depth).unwrap_or(u32::MAX),
        assigned_to: field("assigned_to"),
        created: field("created"),
        scheduled: field("scheduled"),
        due: field("due"),
        estimate: field("estimate"),
        completion: field("completion"),
    }
}

fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in-progress",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Other(_) => "other",
    }
}

fn parse_section(section: &str) -> Result<TaskSection, OpError> {
    match section {
        "next-steps" => Ok(TaskSection::NextSteps),
        "pending-input" => Ok(TaskSection::PendingInput),
        other => Err(OpError::InvalidInput(format!(
            "unknown section {other:?}; expected next-steps or pending-input"
        ))),
    }
}

fn story_not_found(number: &str) -> OpError {
    OpError::NotFound(format!("no story numbered {number} in the workspace"))
}

fn change_record(receipt: WriteReceipt) -> ChangeRecord {
    ChangeRecord {
        relative_path: path_text(&receipt.path),
        line_number: u32::try_from(receipt.line_number).unwrap_or(u32::MAX),
        old_line: receipt.old_line,
        new_line: receipt.new_line,
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl From<WorkspaceError> for OpError {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::NotAWorkspace(path) => OpError::NotAWorkspace(path),
            WorkspaceError::Settings { .. } => OpError::InvalidSettings(error.to_string()),
            other => OpError::FileAccess(other.to_string()),
        }
    }
}

impl From<VaultError> for OpError {
    fn from(error: VaultError) -> Self {
        OpError::FileAccess(error.to_string())
    }
}

impl From<LogFormatError> for OpError {
    fn from(error: LogFormatError) -> Self {
        OpError::InvalidInput(error.to_string())
    }
}

impl From<WriteError> for OpError {
    fn from(error: WriteError) -> Self {
        let message = error.to_string();
        if error.is_not_found() {
            return OpError::NotFound(message);
        }
        if error.is_conflict() {
            return OpError::Conflict(message);
        }
        match error {
            WriteError::InvalidInput(_) | WriteError::Log(_) | WriteError::OutsideWorkspace(_) => {
                OpError::InvalidInput(message)
            }
            _ => OpError::FileAccess(message),
        }
    }
}

impl From<crate::file_access::FileAccessError> for OpError {
    fn from(error: crate::file_access::FileAccessError) -> Self {
        OpError::FileAccess(error.to_string())
    }
}

/// Defines the plain records the bindings export (`WaitingOnView`,
/// `TaskDetailView`, `ChangeView`, `AddedTaskView`, `AddTaskInput`,
/// `LogInput`, `FrontmatterEntryView`, `StoryReportView`,
/// `ReportLogLineView`, `ReportView`) with their conversions from the
/// records of this module, so the napi and UniFFI crates share one
/// definition. `$record` is the attribute that makes each struct a record of
/// the binding, such as `napi(object)`.
#[macro_export]
macro_rules! binding_records {
    ($record:meta) => {
        /// An open task assigned to someone else, with how long it has waited.
        #[$record]
        pub struct WaitingOnView {
            /// The note, relative to the workspace root.
            pub relative_path: String,
            /// The story number when the note is a story.
            pub story_number: Option<String>,
            /// The `[id:: …]` field, when present.
            pub task_id: Option<String>,
            /// The description without its inline fields.
            pub text: String,
            /// Who it waits on.
            pub assigned_to: String,
            /// Days since `[created:: …]`, when the field is present and parses.
            pub age_days: Option<i64>,
        }

        /// A task line with where it lives, in any state.
        #[$record]
        pub struct TaskDetailView {
            /// The note, relative to the workspace root.
            pub relative_path: String,
            /// The story number when the note is a story.
            pub story_number: Option<String>,
            /// `next-steps` or `pending-input` when the task sits in one of them.
            pub section: Option<String>,
            /// 1-based line number.
            pub line_number: u32,
            /// The `[id:: …]` field, when present.
            pub id: Option<String>,
            /// The description without its inline fields.
            pub text: String,
            /// `open`, `in-progress`, `done`, `cancelled` or `other`.
            pub status: String,
            /// Nesting depth, 0 for a top-level task.
            pub depth: u32,
            /// The `[assigned_to:: …]` field, when present.
            pub assigned_to: Option<String>,
            /// The `[created:: …]` field, when present.
            pub created: Option<String>,
            /// The `[scheduled:: …]` field, when present.
            pub scheduled: Option<String>,
            /// The `[due:: …]` field, when present.
            pub due: Option<String>,
            /// The `[estimate:: …]` field, when present.
            pub estimate: Option<String>,
            /// The `[completion:: …]` field, when present.
            pub completion: Option<String>,
        }

        /// One line a write changed.
        #[$record]
        pub struct ChangeView {
            /// The note, relative to the workspace root.
            pub relative_path: String,
            /// 1-based line number in the new file.
            pub line_number: u32,
            /// The line as it was; absent when the line is new.
            pub old_line: Option<String>,
            /// The line as it is now.
            pub new_line: String,
        }

        /// A task that was added.
        #[$record]
        pub struct AddedTaskView {
            /// The id the new task carries.
            pub id: String,
            /// The line that was written.
            pub change: ChangeView,
        }

        /// What `add_task` needs.
        #[$record]
        pub struct AddTaskInput {
            /// The story number, such as `WEB-142`.
            pub story_number: String,
            /// `next-steps` or `pending-input`.
            pub section: String,
            /// The description, one line without inline fields.
            pub text: String,
            /// Who the task waits on.
            pub assigned_to: Option<String>,
            /// The id to use; allocated when absent.
            pub id: Option<String>,
            /// The creation date, `YYYY-MM-DD`; the host's local day when absent.
            pub created: Option<String>,
            /// The `%% ai:begin … %%` line that wraps a model-written task.
            pub begin_marker: Option<String>,
        }

        /// What `append_log` needs.
        #[$record]
        pub struct LogInput {
            /// The day whose log gets the line, `YYYY-MM-DD`; the host's local day when absent.
            pub date: Option<String>,
            /// The time prefix, `HH:MM`; the host's local time when absent.
            pub time: Option<String>,
            /// `session`, `done`, `decision`, `blocker`, `status` or `note`.
            pub kind: String,
            /// What happened, on one line.
            pub text: String,
            /// The story number the line is about.
            pub story: Option<String>,
            /// The task id the line is about.
            pub task: Option<String>,
            /// The model that wrote the line; absent for facts.
            pub agent: Option<String>,
        }

        /// One frontmatter key with its values (a scalar has one, a list several).
        #[$record]
        pub struct FrontmatterEntryView {
            /// The key.
            pub key: String,
            /// The values as text.
            pub values: Vec<String>,
        }

        /// One story in a report.
        #[$record]
        pub struct StoryReportView {
            /// The note, relative to the workspace root.
            pub relative_path: String,
            /// The ticket number, when the frontmatter has one.
            pub number: Option<String>,
            /// The note's first heading.
            pub title: Option<String>,
            /// The story status as written.
            pub status: Option<String>,
            /// Every frontmatter key in source order.
            pub frontmatter: Vec<FrontmatterEntryView>,
            /// Tasks under `## Next Steps`, open and closed.
            pub next_steps: Vec<TaskDetailView>,
            /// Tasks under `## Pending Input`, open and closed.
            pub pending_input: Vec<TaskDetailView>,
            /// The note's text, for the authorship summary.
            pub note_text: String,
        }

        /// One day-log line in a report.
        #[$record]
        pub struct ReportLogLineView {
            /// The log note, relative to the workspace root.
            pub relative_path: String,
            /// `YYYY-MM-DD`.
            pub date: String,
            /// `HH:MM`.
            pub time: String,
            /// The `[kind:: …]` field as written.
            pub kind: Option<String>,
            /// The text without its inline fields.
            pub text: String,
            /// The `[story:: …]` field.
            pub story: Option<String>,
            /// The `[project:: …]` field.
            pub project: Option<String>,
            /// The `[task:: …]` field.
            pub task: Option<String>,
            /// The `[agent:: …]` field: the model that wrote the line.
            pub agent: Option<String>,
        }

        /// Everything about one story or client over a date range.
        #[$record]
        pub struct ReportView {
            /// What the caller asked for.
            pub subject: String,
            /// `story` or `client`.
            pub subject_kind: String,
            /// First day, `YYYY-MM-DD`.
            pub from: String,
            /// Last day, `YYYY-MM-DD`.
            pub to: String,
            /// The stories the subject covers.
            pub stories: Vec<StoryReportView>,
            /// Their day-log lines in range, oldest first.
            pub log: Vec<ReportLogLineView>,
        }

        $crate::__mirror_record!($crate::ops::WaitingItem => WaitingOnView {
            relative_path, story_number, task_id, text, assigned_to, age_days
        });
        $crate::__mirror_record!($crate::ops::TaskRecord => TaskDetailView {
            relative_path, story_number, section, line_number, id, text, status, depth,
            assigned_to, created, scheduled, due, estimate, completion
        });
        $crate::__mirror_record!($crate::ops::ChangeRecord => ChangeView { relative_path, line_number, old_line, new_line });
        $crate::__mirror_record!($crate::ops::FrontmatterEntry => FrontmatterEntryView { key, values });
        $crate::__mirror_record!($crate::ops::ReportLogLine => ReportLogLineView {
            relative_path, date, time, kind, text, story, project, task, agent
        });

        impl From<$crate::ops::AddedTask> for AddedTaskView {
            fn from(value: $crate::ops::AddedTask) -> Self {
                AddedTaskView {
                    id: value.id,
                    change: value.change.into(),
                }
            }
        }

        impl From<$crate::ops::StoryReport> for StoryReportView {
            fn from(value: $crate::ops::StoryReport) -> Self {
                StoryReportView {
                    relative_path: value.relative_path,
                    number: value.number,
                    title: value.title,
                    status: value.status,
                    frontmatter: value.frontmatter.into_iter().map(Into::into).collect(),
                    next_steps: value.next_steps.into_iter().map(Into::into).collect(),
                    pending_input: value.pending_input.into_iter().map(Into::into).collect(),
                    note_text: value.note_text,
                }
            }
        }

        impl From<$crate::ops::Report> for ReportView {
            fn from(value: $crate::ops::Report) -> Self {
                ReportView {
                    subject: value.subject,
                    subject_kind: value.subject_kind,
                    from: value.from,
                    to: value.to,
                    stories: value.stories.into_iter().map(Into::into).collect(),
                    log: value.log.into_iter().map(Into::into).collect(),
                }
            }
        }

        impl AddTaskInput {
            /// Converts to the shared request; `today` stands in for an absent date.
            ///
            /// # Errors
            /// [`OpError::InvalidDate`] for a malformed date.
            pub fn into_request(
                self,
                today: $crate::Date,
            ) -> Result<$crate::ops::AddTaskRequest, $crate::ops::OpError> {
                Ok($crate::ops::AddTaskRequest {
                    story_number: self.story_number,
                    section: self.section,
                    text: self.text,
                    assigned_to: self.assigned_to,
                    id: self.id,
                    created: match self.created.as_deref() {
                        Some(text) => $crate::ops::parse_date(text)?,
                        None => today,
                    },
                    scheduled: None,
                    estimate: None,
                    begin_marker: self.begin_marker,
                })
            }
        }

        impl LogInput {
            /// Converts to the shared request; `today` and `now` stand in for an absent date and time.
            ///
            /// # Errors
            /// [`OpError::InvalidDate`] for a malformed date or time.
            pub fn into_request(
                self,
                today: $crate::Date,
                now: $crate::Time,
            ) -> Result<$crate::ops::LogRequest, $crate::ops::OpError> {
                Ok($crate::ops::LogRequest {
                    date: match self.date.as_deref() {
                        Some(text) => $crate::ops::parse_date(text)?,
                        None => today,
                    },
                    time: match self.time.as_deref() {
                        Some(text) => $crate::ops::parse_time(text)?,
                        None => now,
                    },
                    kind: self.kind,
                    text: self.text,
                    story: self.story,
                    task: self.task,
                    agent: self.agent,
                })
            }
        }


    };
}

/// Copies a shared record into a binding record field by field.
#[doc(hidden)]
#[macro_export]
macro_rules! __mirror_record {
    ($from:ty => $to:ident { $($field:ident),* $(,)? }) => {
        impl From<$from> for $to {
            fn from(value: $from) -> Self {
                $to { $($field: value.$field),* }
            }
        }
    };
}
