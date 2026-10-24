//! Browse, the previews and the day log tab: the queries behind the
//! full task table, the task and story preview and the Peek popover.

use std::sync::Arc;

use domain::browse::{self, BrowseGroup, BrowseQuery, BrowseTask};
use domain::estimate::Estimate;
use domain::task::Section;
use vault_base::ops;
use vault_base::{
    ActivityEntry, NoteBlock, PlainFileAccess, StoryPreview, TaskStatus, WaitingEntry,
    day_log_path, parse_log_shorthand,
};

use crate::home::{civil_date, count, story_lines, whole_minutes};
use crate::{CoreError, TaskDetailView, WorkspaceHandle, day_or_today};

/// A saved query every workspace starts with.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowsePresetView {
    /// What the preset is called.
    pub name: String,
    /// Its query line.
    pub query: String,
}

/// Where a task stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BrowseRowState {
    /// `[ ]`
    Open,
    /// `[/]`
    InProgress,
    /// `[x]`
    Done,
    /// `[-]`
    Cancelled,
}

/// One task row of the table.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseRowView {
    /// The `[id:: …]` field.
    pub id: String,
    /// The description without its fields.
    pub text: String,
    /// The task's state.
    pub state: BrowseRowState,
    /// Who the task waits on, when it is assigned to someone but me.
    pub waiting_on: Option<String>,
    /// Whole days since `[created:: …]`, for a task that waits on someone.
    pub waiting_days: Option<u32>,
    /// `[scheduled:: …]`, `YYYY-MM-DD`.
    pub scheduled: Option<String>,
    /// `[due:: …]`, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// The estimate in whole minutes.
    pub estimate_minutes: Option<u32>,
    /// The story number the line sits under.
    pub story_number: Option<String>,
}

/// A story's task section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BrowseSectionKind {
    /// `## Next Steps`.
    NextSteps,
    /// `## Pending Input`.
    PendingInput,
}

/// The matching tasks of one section of a story.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseSectionView {
    /// Which section; `None` when the query does not group by section, so
    /// the rows of both sit here in the note's order.
    pub section: Option<BrowseSectionKind>,
    /// The rows in query order.
    pub rows: Vec<BrowseRowView>,
}

/// The matching tasks of one story.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseGroupView {
    /// `true` for the one group of a query that does not group by story;
    /// the story fields are then `None`.
    pub spans_stories: bool,
    /// The story number.
    pub story_number: Option<String>,
    /// The story note's title.
    pub story_title: Option<String>,
    /// How many rows the group holds.
    pub task_count: u32,
    /// Next Steps, then Pending Input.
    pub sections: Vec<BrowseSectionView>,
}

/// How much a query matches, without the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BrowseCountView {
    /// The matching tasks.
    pub task_count: u32,
    /// The stories those tasks sit in.
    pub story_count: u32,
}

/// The workspace's task rows read once, for any number of queries. Browse,
/// its summary, the preset counts and the Backlog tab run against one
/// snapshot, so a refresh parses the vault once.
#[derive(Debug, uniffi::Object)]
pub struct BrowseSnapshot {
    rows: Vec<BrowseTask>,
    today: jiff::civil::Date,
}

/// The result of one query.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseResultView {
    /// The groups, stories in the vault's order.
    pub groups: Vec<BrowseGroupView>,
    /// How many rows match in all.
    pub task_count: u32,
}

/// A note block of the preview.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NoteBlockView {
    /// A heading below the title.
    Heading {
        /// 1 to 6.
        level: u32,
        /// The heading text.
        text: String,
    },
    /// A paragraph; inline markdown stays as written.
    Paragraph {
        /// The text.
        text: String,
    },
    /// A list item or a task line.
    Bullet {
        /// 0 for a top-level item.
        depth: u32,
        /// The text without inline fields.
        text: String,
        /// The checkbox of a task line.
        state: Option<BrowseRowState>,
    },
    /// A fenced code block's lines.
    Code {
        /// The lines joined with newlines.
        text: String,
    },
}

/// A task that waits on someone.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WaitingEntryView {
    /// The `[id:: …]` field.
    pub task_id: Option<String>,
    /// The description.
    pub text: String,
    /// Who it waits on.
    pub assigned_to: String,
    /// Days since it was created.
    pub age_days: Option<i64>,
}

/// One log line about a story or task.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityEntryView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM`.
    pub time: String,
    /// The `[kind:: …]` field as written.
    pub kind: Option<String>,
    /// The text without inline fields.
    pub text: String,
    /// The `[task:: …]` field.
    pub task: Option<String>,
    /// The model that wrote the line.
    pub agent: Option<String>,
}

/// A direct subtask.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PreviewSubtaskView {
    /// The `[id:: …]` field.
    pub id: Option<String>,
    /// The description.
    pub text: String,
    /// The checkbox state.
    pub state: BrowseRowState,
    /// The estimate in whole minutes.
    pub estimate_minutes: Option<u32>,
}

/// A story as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryPreviewView {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// The ticket number.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The status as written.
    pub status: Option<String>,
    /// The priority as written.
    pub priority: Option<String>,
    /// The due date, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// The estimate as written.
    pub estimate: Option<String>,
    /// The note without the title, the task sections and the provenance markers.
    pub blocks: Vec<NoteBlockView>,
    /// Open tasks of the story that wait on someone, oldest first.
    pub waiting: Vec<WaitingEntryView>,
    /// The latest log lines about the story and its tasks, newest first.
    pub activity: Vec<ActivityEntryView>,
    /// The `obsidian://` link to the note.
    pub obsidian_link: String,
    /// One line to show beside the link when it had to fall back (the
    /// workspace lies outside its Obsidian vault); `None` otherwise.
    pub obsidian_link_notice: Option<String>,
}

/// A task with its story and subtasks.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TaskPreviewView {
    /// The task line's fields.
    pub task: TaskDetailView,
    /// The story holding the task.
    pub story: Option<StoryPreviewView>,
    /// Direct subtasks in note order.
    pub subtasks: Vec<PreviewSubtaskView>,
    /// The latest log lines about this task, newest first.
    pub task_activity: Vec<ActivityEntryView>,
    /// The `obsidian://` link to the note holding the task.
    pub obsidian_link: String,
    /// One line to show beside the link when it had to fall back (the
    /// workspace lies outside its Obsidian vault); `None` otherwise.
    pub obsidian_link_notice: Option<String>,
}

/// One line of a day log.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DayLogLineView {
    /// `HH:MM`.
    pub time: String,
    /// The `[kind:: …]` field as written.
    pub kind: Option<String>,
    /// The text without inline fields.
    pub text: String,
    /// The `[story:: …]` field.
    pub story: Option<String>,
    /// The `[task:: …]` field.
    pub task: Option<String>,
    /// The `[agent:: …]` field: the model that wrote the line.
    pub agent: Option<String>,
}

/// A day's log.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DayLogView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// The log note, relative to the workspace root.
    pub relative_path: String,
    /// The lines in time order.
    pub lines: Vec<DayLogLineView>,
}

/// A typed log line split into its kind and text.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LogShorthandView {
    /// The kind word, `note` when the line names none.
    pub kind: String,
    /// The text once the kind word is taken out.
    pub text: String,
}

/// The saved queries every workspace starts with.
#[uniffi::export]
pub fn browse_presets() -> Vec<BrowsePresetView> {
    browse::PRESETS
        .iter()
        .map(|(name, query)| BrowsePresetView {
            name: (*name).to_owned(),
            query: (*query).to_owned(),
        })
        .collect()
}

/// Reads the workspace's task rows once. `today` defaults to the host's
/// local day.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed day, [`CoreError::FileAccess`]
/// when the vault cannot be read.
#[uniffi::export(default(today = None))]
pub fn browse_snapshot(
    workspace: &WorkspaceHandle,
    today: Option<String>,
) -> Result<Arc<BrowseSnapshot>, CoreError> {
    let today = civil_date(day_or_today(today.as_deref())?)?;
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let rows = story_lines(&vault)
        .into_iter()
        .filter(|line| line.task.parent.is_none())
        .map(|line| BrowseTask {
            task: line.task,
            story_title: line.story_title,
            story_status: line.story_status,
            source: line.source,
        })
        .collect();
    Ok(Arc::new(BrowseSnapshot { rows, today }))
}

#[uniffi::export]
impl BrowseSnapshot {
    /// Runs a query line over the snapshot.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] with a message naming the term when the
    /// query is not understood.
    pub fn query(&self, query: String) -> Result<BrowseResultView, CoreError> {
        let groups: Vec<BrowseGroupView> = parse_query(&query)?
            .run(&self.rows, self.today)
            .iter()
            .map(|group| group_view(group, self.today))
            .collect();
        Ok(BrowseResultView {
            task_count: groups.iter().map(|group| group.task_count).sum(),
            groups,
        })
    }

    /// How many tasks, and in how many stories, match a query line.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] as for [`BrowseSnapshot::query`].
    pub fn count(&self, query: String) -> Result<BrowseCountView, CoreError> {
        let query = parse_query(&query)?;
        let matching: Vec<&BrowseTask> = self
            .rows
            .iter()
            .filter(|row| query.matches(row, self.today))
            .collect();
        let mut stories: Vec<(Option<&str>, Option<&str>)> = matching
            .iter()
            .map(|row| {
                (
                    row.task.story.as_ref().map(|story| story.as_str()),
                    row.story_title.as_deref(),
                )
            })
            .collect();
        stories.sort_unstable();
        stories.dedup();
        Ok(BrowseCountView {
            task_count: count(matching.len()),
            story_count: count(stories.len()),
        })
    }
}

/// Runs a query line over the workspace's tasks: a snapshot read for one
/// query. `today` defaults to the host's local day.
///
/// # Errors
/// [`CoreError::InvalidInput`] with a message naming the term when the query
/// is not understood, [`CoreError::FileAccess`] when the vault cannot be read.
#[uniffi::export(default(today = None))]
pub fn browse_query(
    workspace: &WorkspaceHandle,
    query: String,
    today: Option<String>,
) -> Result<BrowseResultView, CoreError> {
    parse_query(&query)?;
    browse_snapshot(workspace, today)?.query(query)
}

/// The preview of the task carrying `[id:: id]`.
///
/// # Errors
/// [`CoreError::NotFound`] without such a task.
#[uniffi::export(default(today = None))]
pub fn task_preview(
    workspace: &WorkspaceHandle,
    id: String,
    today: Option<String>,
) -> Result<TaskPreviewView, CoreError> {
    let today = day_or_today(today.as_deref())?;
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let task = ops::task_by_id(&vault, &id)?;
    let preview = vault
        .task_preview(&id, today, ACTIVITY_LIMIT)
        .ok_or_else(|| CoreError::NotFound(format!("no task with [id:: {id}] in the workspace")))?;
    let obsidian_link = workspace
        .workspace
        .obsidian_link(std::path::Path::new(&task.relative_path));
    Ok(TaskPreviewView {
        task: task.into(),
        story: preview
            .story
            .map(|story| story_preview_view(story, workspace)),
        subtasks: preview
            .subtasks
            .into_iter()
            .map(|subtask| PreviewSubtaskView {
                id: subtask.id,
                text: subtask.text,
                state: row_state(subtask.status),
                estimate_minutes: subtask
                    .estimate
                    .as_deref()
                    .and_then(domain::quick_add::parse_estimate)
                    .map(minutes_of),
            })
            .collect(),
        task_activity: preview
            .task_activity
            .into_iter()
            .map(activity_view)
            .collect(),
        obsidian_link,
        obsidian_link_notice: obsidian_link_notice(workspace),
    })
}

/// The preview of the story numbered `number`.
///
/// # Errors
/// [`CoreError::NotFound`] without such a story.
#[uniffi::export(default(today = None))]
pub fn story_preview(
    workspace: &WorkspaceHandle,
    number: String,
    today: Option<String>,
) -> Result<StoryPreviewView, CoreError> {
    let today = day_or_today(today.as_deref())?;
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    vault
        .story_preview(&number, today, ACTIVITY_LIMIT)
        .map(|story| story_preview_view(story, workspace))
        .ok_or_else(|| CoreError::NotFound(format!("no story numbered {number} in the workspace")))
}

/// The log of `date` (`YYYY-MM-DD`; the host's local day when absent).
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed date.
#[uniffi::export(default(date = None))]
pub fn day_log(workspace: &WorkspaceHandle, date: Option<String>) -> Result<DayLogView, CoreError> {
    let date = day_or_today(date.as_deref())?;
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let lines = vault
        .log_lines_on(date)
        .into_iter()
        .map(|entry| DayLogLineView {
            time: entry.line.time.to_string(),
            kind: entry
                .line
                .kind
                .as_ref()
                .map(|kind| kind.as_str().to_owned()),
            text: entry.line.text.clone(),
            story: entry.line.story().map(str::to_owned),
            task: entry.line.task().map(str::to_owned),
            agent: entry.line.field("agent").map(str::to_owned),
        })
        .collect();
    Ok(DayLogView {
        date: date.to_string(),
        relative_path: day_log_path(vault.log_folder(), date)
            .to_string_lossy()
            .into_owned(),
        lines,
    })
}

/// Reads the kind word a typed log line starts with (`decision: …`).
#[uniffi::export]
pub fn parse_log_line_shorthand(text: String) -> LogShorthandView {
    let shorthand = parse_log_shorthand(&text);
    LogShorthandView {
        kind: shorthand.kind.as_str().to_owned(),
        text: shorthand.text,
    }
}

const ACTIVITY_LIMIT: usize = 6;

fn parse_query(query: &str) -> Result<BrowseQuery, CoreError> {
    BrowseQuery::parse(query).map_err(|error| CoreError::InvalidInput(error.to_string()))
}

fn group_view(group: &BrowseGroup, today: jiff::civil::Date) -> BrowseGroupView {
    BrowseGroupView {
        spans_stories: group.spans_stories,
        story_number: group.story_number.clone(),
        story_title: group.story_title.clone(),
        task_count: count(group.task_count()),
        sections: group
            .sections
            .iter()
            .map(|section| BrowseSectionView {
                section: section.section.map(|section| match section {
                    Section::NextSteps => BrowseSectionKind::NextSteps,
                    Section::PendingInput => BrowseSectionKind::PendingInput,
                }),
                rows: section
                    .tasks
                    .iter()
                    .map(|row| row_view(row, today))
                    .collect(),
            })
            .collect(),
    }
}

fn row_view(row: &BrowseTask, today: jiff::civil::Date) -> BrowseRowView {
    let task = &row.task;
    let waiting_on = task
        .assigned_to
        .as_deref()
        .map(|who| who.trim().trim_start_matches('@'))
        .filter(|who| !who.is_empty() && !who.eq_ignore_ascii_case("me"))
        .map(str::to_owned);
    let waiting_days = waiting_on
        .as_ref()
        .and(task.created)
        .and_then(|created| created.until(today).ok())
        .and_then(|span| u32::try_from(span.get_days()).ok());
    BrowseRowView {
        id: task.id.as_str().to_owned(),
        text: task.text.clone(),
        state: match task.status {
            domain::task::TaskStatus::Open => BrowseRowState::Open,
            domain::task::TaskStatus::InProgress => BrowseRowState::InProgress,
            domain::task::TaskStatus::Done => BrowseRowState::Done,
            domain::task::TaskStatus::Cancelled => BrowseRowState::Cancelled,
        },
        waiting_on,
        waiting_days,
        scheduled: task.scheduled.map(|date| date.to_string()),
        due: task.due.map(|date| date.to_string()),
        estimate_minutes: task.estimate.map(minutes_of),
        story_number: task.story.as_ref().map(|story| story.as_str().to_owned()),
    }
}

fn minutes_of(estimate: Estimate) -> u32 {
    whole_minutes(estimate.as_duration())
}

pub(crate) fn row_state(status: TaskStatus) -> BrowseRowState {
    match status {
        TaskStatus::InProgress => BrowseRowState::InProgress,
        TaskStatus::Done => BrowseRowState::Done,
        TaskStatus::Cancelled => BrowseRowState::Cancelled,
        TaskStatus::Open | TaskStatus::Other(_) => BrowseRowState::Open,
    }
}

fn story_preview_view(story: StoryPreview, workspace: &WorkspaceHandle) -> StoryPreviewView {
    let obsidian_link = workspace
        .workspace
        .obsidian_link(std::path::Path::new(&story.relative_path));
    StoryPreviewView {
        relative_path: story.relative_path,
        number: story.number,
        title: story.title,
        status: story.status,
        priority: story.priority,
        due: story.due.map(|date| date.to_string()),
        estimate: story.estimate,
        blocks: story.blocks.into_iter().map(block_view).collect(),
        waiting: story.waiting.into_iter().map(waiting_view).collect(),
        activity: story.activity.into_iter().map(activity_view).collect(),
        obsidian_link,
        obsidian_link_notice: obsidian_link_notice(workspace),
    }
}

fn obsidian_link_notice(workspace: &WorkspaceHandle) -> Option<String> {
    workspace
        .workspace
        .obsidian_link_notice()
        .map(str::to_owned)
}

fn block_view(block: NoteBlock) -> NoteBlockView {
    match block {
        NoteBlock::Heading { level, text } => NoteBlockView::Heading {
            level: u32::from(level),
            text,
        },
        NoteBlock::Paragraph(text) => NoteBlockView::Paragraph { text },
        NoteBlock::Bullet { depth, text, state } => NoteBlockView::Bullet {
            depth: count(depth),
            text,
            state: state.map(row_state),
        },
        NoteBlock::Code(text) => NoteBlockView::Code { text },
    }
}

pub(crate) fn waiting_view(entry: WaitingEntry) -> WaitingEntryView {
    WaitingEntryView {
        task_id: entry.task_id,
        text: entry.text,
        assigned_to: entry.assigned_to,
        age_days: entry.age_days,
    }
}

pub(crate) fn activity_view(entry: ActivityEntry) -> ActivityEntryView {
    ActivityEntryView {
        date: entry.date.to_string(),
        time: entry.time.to_string(),
        kind: entry.kind,
        text: entry.text,
        task: entry.task,
        agent: entry.agent,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{LogInput, append_log, open_workspace};

    const TODAY: &str = "2026-10-22";

    fn mock_vault() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/mock-vault")
            .to_string_lossy()
            .into_owned()
    }

    fn rows(query: &str) -> Vec<String> {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();
        browse_query(&workspace, query.to_owned(), Some(TODAY.to_owned()))
            .unwrap()
            .groups
            .into_iter()
            .flat_map(|group| group.sections)
            .flat_map(|section| section.rows)
            .map(|row| row.id)
            .collect()
    }

    #[test]
    fn browse_groups_the_mock_vault_by_story_then_section() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        let result = browse_query(&workspace, String::new(), Some(TODAY.to_owned())).unwrap();

        type Shape<'a> = Vec<(Option<&'a str>, Vec<(bool, usize)>)>;
        let shape: Shape<'_> = result
            .groups
            .iter()
            .map(|group| {
                (
                    group.story_number.as_deref(),
                    group
                        .sections
                        .iter()
                        .map(|section| {
                            (
                                section.section == Some(BrowseSectionKind::PendingInput),
                                section.rows.len(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        // Subtasks are not rows of their own; they show inside their parent's preview,
        // and a task line without an id is left to the vault health dot.
        assert_eq!(
            shape,
            [
                (Some("WEB-142"), vec![(false, 2), (true, 1)]),
                (Some("WEB-157"), vec![(false, 1), (true, 1)]),
                (Some("ACME-9"), vec![(false, 1)]),
            ]
        );
        assert_eq!(result.task_count, 6);
        assert_eq!(result.groups[0].task_count, 3);
    }

    #[test]
    fn rows_carry_the_columns_of_the_table() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        let result =
            browse_query(&workspace, "story:142".to_owned(), Some(TODAY.to_owned())).unwrap();

        let wire = &result.groups[0].sections[0].rows[0];
        assert_eq!(wire.id, "task-2");
        assert_eq!(wire.scheduled.as_deref(), Some(TODAY));
        assert_eq!(wire.estimate_minutes, Some(120));
        assert_eq!(wire.story_number.as_deref(), Some("WEB-142"));
        let test = &result.groups[0].sections[0].rows[1];
        assert_eq!(test.due.as_deref(), Some("2026-10-28"));
        let copy = &result.groups[0].sections[1].rows[0];
        assert_eq!(copy.waiting_on.as_deref(), Some("Dana"));
        assert_eq!(copy.waiting_days, Some(12));
    }

    #[test]
    fn filters_run_against_the_mock_vault() {
        assert_eq!(rows("waiting:any"), ["task-6", "task-9"]);
        assert_eq!(rows("due:<=week"), Vec::<String>::new());
        assert_eq!(rows("due:>today"), ["task-5"]);
        assert_eq!(rows("story:157 section:next"), ["task-8"]);
        assert_eq!(rows("scheduled:today"), ["task-2"]);
        assert_eq!(rows("estimate:>1h"), ["task-2"]);
        assert_eq!(rows("status:done story:142"), ["task-7"]);
        assert_eq!(rows("payment"), ["task-2"]);
        assert_eq!(rows("section:pending story:157"), ["task-9"]);
    }

    #[test]
    fn a_query_that_is_not_understood_is_refused_with_its_term() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        let error = browse_query(&workspace, "due:soonish".to_owned(), None).unwrap_err();

        assert!(
            matches!(&error, CoreError::InvalidInput(message) if message.starts_with("due:soonish")),
            "{error:?}"
        );
    }

    #[test]
    fn every_preset_runs() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();
        let presets = browse_presets();
        assert_eq!(presets[0].name, "All open");
        assert_eq!(presets[1].name, "Backlog");
        for preset in presets {
            browse_query(&workspace, preset.query.clone(), Some(TODAY.to_owned()))
                .unwrap_or_else(|error| panic!("{}: {error:?}", preset.name));
        }
        assert_eq!(
            rows("status:open section:next scheduled:none"),
            ["task-5", "task-8", "task-12"]
        );
    }

    #[test]
    fn one_snapshot_answers_the_query_the_counts_and_the_presets() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();
        let snapshot = browse_snapshot(&workspace, Some(TODAY.to_owned())).unwrap();

        assert_eq!(
            snapshot.count("status:open".to_owned()).unwrap(),
            BrowseCountView {
                task_count: 6,
                story_count: 3
            }
        );
        let counts: Vec<(String, u32)> = browse_presets()
            .into_iter()
            .map(|preset| {
                let count = snapshot.count(preset.query).unwrap().task_count;
                (preset.name, count)
            })
            .collect();
        assert_eq!(
            counts,
            [
                ("All open".to_owned(), 6),
                ("Backlog".to_owned(), 3),
                ("Chasers".to_owned(), 2),
                ("Active stories".to_owned(), 3),
                ("Due this week".to_owned(), 0),
                ("Braindump".to_owned(), 1),
            ]
        );
        assert_eq!(
            snapshot.query(String::new()).unwrap(),
            browse_query(&workspace, String::new(), Some(TODAY.to_owned())).unwrap()
        );
        assert!(matches!(
            snapshot.count("stroy:x".to_owned()),
            Err(CoreError::InvalidInput(_))
        ));
    }

    #[test]
    fn group_by_section_spans_the_stories_in_one_group() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();
        let snapshot = browse_snapshot(&workspace, Some(TODAY.to_owned())).unwrap();

        let result = snapshot.query("group:section".to_owned()).unwrap();

        assert_eq!(result.groups.len(), 1);
        let group = &result.groups[0];
        assert!(group.spans_stories);
        assert_eq!(group.story_number, None);
        let sections: Vec<Option<BrowseSectionKind>> = group
            .sections
            .iter()
            .map(|section| section.section)
            .collect();
        assert_eq!(
            sections,
            [
                Some(BrowseSectionKind::NextSteps),
                Some(BrowseSectionKind::PendingInput)
            ]
        );
        let flat = snapshot.query("group:none".to_owned()).unwrap();
        assert_eq!(flat.groups[0].sections[0].section, None);
        assert_eq!(flat.task_count, 6);
    }

    #[test]
    fn a_task_preview_cuts_the_story_note_and_lists_the_subtasks() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        let preview =
            task_preview(&workspace, "task-2".to_owned(), Some(TODAY.to_owned())).unwrap();

        assert_eq!(preview.task.id.as_deref(), Some("task-2"));
        assert_eq!(preview.subtasks.len(), 2);
        assert_eq!(preview.subtasks[1].state, BrowseRowState::InProgress);
        assert_eq!(preview.subtasks[1].estimate_minutes, Some(45));
        let story = preview.story.unwrap();
        assert_eq!(story.number.as_deref(), Some("WEB-142"));
        assert!(story.blocks.iter().any(
            |block| matches!(block, NoteBlockView::Heading { level: 2, text } if text == "Implementation")
        ));
        assert!(preview.obsidian_link.starts_with("obsidian://open?vault="));
        assert!(
            preview
                .obsidian_link
                .ends_with("&file=Projects%2FWebsite%2FWEB-142%20Checkout%20redesign%2FWEB-142%20Checkout%20redesign")
        );
        assert_eq!(
            preview.task_activity.len(),
            2,
            "a session and a measured one"
        );
        assert_eq!(preview.obsidian_link_notice, None, "inside its vault");

        let mut outside = workspace.workspace.clone();
        outside.vault_root = std::env::temp_dir().join("another-vault");
        let outside = WorkspaceHandle { workspace: outside };
        let preview = task_preview(&outside, "task-2".to_owned(), Some(TODAY.to_owned())).unwrap();
        let notice = preview.obsidian_link_notice.unwrap();
        assert!(!notice.ends_with('.'), "one line, no period");
        assert_eq!(preview.story.unwrap().obsidian_link_notice, Some(notice));
    }

    #[test]
    fn an_unknown_task_or_story_is_not_found() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        assert!(matches!(
            task_preview(&workspace, "task-999".to_owned(), None),
            Err(CoreError::NotFound(_))
        ));
        assert!(matches!(
            story_preview(&workspace, "WEB-999".to_owned(), None),
            Err(CoreError::NotFound(_))
        ));
        assert!(story_preview(&workspace, "WEB-157".to_owned(), Some(TODAY.to_owned())).is_ok());
    }

    #[test]
    fn the_day_log_lists_the_days_lines_with_their_authors() {
        let workspace = open_workspace(mock_vault(), Vec::new()).unwrap();

        let log = day_log(&workspace, Some(TODAY.to_owned())).unwrap();

        assert_eq!(log.relative_path, "Log/2026-10-22.md");
        let times: Vec<&str> = log.lines.iter().map(|line| line.time.as_str()).collect();
        assert_eq!(times, ["08:45", "09:30", "11:15", "14:00"]);
        assert_eq!(log.lines[2].agent.as_deref(), Some("claude-fable-5-1"));
        let empty = day_log(&workspace, Some("2026-01-01".to_owned())).unwrap();
        assert!(empty.lines.is_empty());
    }

    #[test]
    fn an_appended_line_reads_back_in_the_day_log() {
        let temp = tempfile::tempdir().unwrap();
        copy_dir(&PathBuf::from(mock_vault()), temp.path());
        let workspace =
            open_workspace(temp.path().to_string_lossy().into_owned(), Vec::new()).unwrap();
        let shorthand = parse_log_line_shorthand(
            "decision: keep the QA redirect group dev and test only".to_owned(),
        );
        assert_eq!(shorthand.kind, "decision");

        let change = append_log(
            &workspace,
            LogInput {
                date: Some("2026-10-23".to_owned()),
                time: Some("09:48".to_owned()),
                kind: shorthand.kind,
                text: shorthand.text,
                story: Some("WEB-142".to_owned()),
                task: Some("task-2".to_owned()),
                agent: None,
            },
        )
        .unwrap();

        assert_eq!(
            change.new_line,
            "- 09:48 decision · keep the QA redirect group dev and test only [story:: WEB-142] [kind:: decision] [task:: task-2]"
        );
        let log = day_log(&workspace, Some("2026-10-23".to_owned())).unwrap();
        assert_eq!(log.lines.len(), 1);
        assert_eq!(log.lines[0].kind.as_deref(), Some("decision"));
        assert_eq!(log.lines[0].task.as_deref(), Some("task-2"));
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
