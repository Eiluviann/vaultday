//! Home over the bridge: the day's list, quick add, the scheduler, the timer
//! and the vault's health. The rules live in `domain` and `vault-base`; this
//! module reads the vault's lines into domain values and hands the results
//! back as plain records for the Swift models.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use domain::DomainError;
use domain::day_plan::{DayPlan, Lock, TimeBox};
use domain::estimate::{Estimate, format_hours_minutes};
use domain::quick_add::{
    self, EntryKind, ParseWarning, QuickAddContext, StoryMatch, StoryRef, WorkspaceRef,
};
use domain::scheduler::{self, ScheduleOptions, Span, WorkingDay};
use domain::session::{Session, SessionBook, SessionTarget};
use domain::task::{Section, Task, TaskId, TaskStatus, TimeBoxId, VaultTask};
use domain::ticket::TicketPattern;
use domain::today_list::{self, NewTaskPlacement};
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use vault_base::ops::{self, AddTaskRequest};
use vault_base::{Document, PlainFileAccess, Vault, conflict_copy_original};

use app_state::WorkspaceSync;

use crate::app_state::{SessionView, WorkspaceStateHandle, lock, parse_date, session_view};
use crate::{AddedTaskView, CoreError, WorkspaceHandle};

/// The longest estimate a task takes: a day.
const MAXIMUM_ESTIMATE_MINUTES: u32 = 24 * 60;

/// Where a task on the day's list stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlanItemState {
    /// `[ ]`
    Open,
    /// `[/]`
    InProgress,
    /// `[x]`, completed on the list's day.
    Done,
}

/// Where tasks the stored order does not name yet join the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NewTaskPlacementView {
    /// Above the ordered ones.
    Top,
    /// Below the ordered ones.
    Bottom,
}

/// A subtask line inside its parent's card.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SubtaskView {
    /// The `[id:: …]` field, when present.
    pub id: Option<String>,
    /// The description without its inline fields.
    pub text: String,
    /// Ticked done.
    pub is_done: bool,
    /// The estimate in minutes, when it reads as one.
    pub estimate_minutes: Option<u32>,
}

/// One task card of the day's list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanItemView {
    /// The `[id:: …]` field.
    pub id: String,
    /// The description without its inline fields.
    pub text: String,
    /// The story the task's note belongs to.
    pub story_number: Option<String>,
    /// The story's title.
    pub story_title: Option<String>,
    /// Whether the line sits under `## Pending Input`.
    pub is_pending_input: bool,
    /// Open, in progress or done.
    pub state: PlanItemState,
    /// The estimate in minutes, when it reads as one.
    pub estimate_minutes: Option<u32>,
    /// Who the task waits on.
    pub assigned_to: Option<String>,
    /// Days a Pending Input task has waited since it was created.
    pub waiting_days: Option<u32>,
    /// The direct subtasks, in note order.
    pub subtasks: Vec<SubtaskView>,
}

/// The list of one date.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DayListView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Whether the date is the host's local day.
    pub is_today: bool,
    /// Unfinished tasks in list order, then the ones done on the date.
    pub items: Vec<PlanItemView>,
    /// The planned estimate of every task on the list, done ones included.
    pub planned_minutes: u32,
    /// Unfinished tasks without an estimate.
    pub unestimated: u32,
    /// Every open top-level task line of the workspace's stories, for Browse.
    pub open_task_count: u32,
}

/// A task or a calendar event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EntryKindView {
    /// A task line in a story note.
    Task,
    /// A calendar event.
    Event,
}

/// A workspace `/name` may route a quick-add line to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickAddWorkspaceView {
    /// The workspace id.
    pub id: String,
    /// The display name.
    pub name: String,
}

/// What one quick-add line was understood as: the confirmation popover's contents.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickAddView {
    /// Task or event.
    pub kind: EntryKindView,
    /// The text with the recognised tokens taken out.
    pub text: String,
    /// The story the entry routes to.
    pub story_number: Option<String>,
    /// That story's title, when it is in the workspace.
    pub story_title: Option<String>,
    /// Whether the number names a story of the workspace.
    pub is_story_known: bool,
    /// Whether the story was found by its name or an alias rather than its number.
    pub is_story_from_name: bool,
    /// The workspace a `/name` routed to.
    pub workspace: Option<String>,
    /// `YYYY-MM-DD`: a day word in the line, else today.
    pub day: String,
    /// `HH:MM`, events only.
    pub start_time: Option<String>,
    /// `HH:MM`, events only, when known.
    pub end_time: Option<String>,
    /// The estimate in minutes.
    pub estimate_minutes: Option<u32>,
    /// Whether the line names someone to wait on, which files it under Pending Input.
    pub is_pending_input: bool,
    /// Who it waits on.
    pub waiting_on: Option<String>,
    /// The id the task gets when saved.
    pub id: String,
    /// What was understood but not used, one line each.
    pub warnings: Vec<String>,
}

/// A story the confirmation's Adjust can route to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryChoiceView {
    /// The ticket number.
    pub number: String,
    /// The note's title, else the number.
    pub title: String,
}

/// What Save writes for a confirmed quick-add entry.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickAddSaveInput {
    /// The id the popover showed.
    pub id: String,
    /// The task's text.
    pub text: String,
    /// The story whose note gets the line; required.
    pub story_number: Option<String>,
    /// Under Pending Input rather than Next Steps.
    pub is_pending_input: bool,
    /// Who it waits on.
    pub waiting_on: Option<String>,
    /// `YYYY-MM-DD`, written as `[scheduled:: ]`.
    pub day: String,
    /// Written as `[estimate:: ]`.
    pub estimate_minutes: Option<u32>,
}

/// `=` lays the list from the start of the working day; `r` from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ScheduleModeView {
    /// `=`: every unfinished, unlocked task from the working day's start.
    AutoSchedule,
    /// `r`: the same, from now.
    Replan,
}

/// One task of the list, as the scheduler needs it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduleItemView {
    /// The task id.
    pub task: String,
    /// The estimate in minutes.
    pub estimate_minutes: Option<u32>,
    /// Done or cancelled: its box stays where it is.
    pub is_finished: bool,
}

/// A busy span of the day, such as a calendar event, in minutes since midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BusySpanView {
    /// Inclusive start.
    pub start_minute: u32,
    /// Exclusive end.
    pub end_minute: u32,
}

/// What the scheduler needs for one date.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduleDayInput {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `=` or `r`.
    pub mode: ScheduleModeView,
    /// Now, in minutes since midnight; where `r` starts.
    pub now_minute: u32,
    /// The working day's start, in minutes since midnight.
    pub working_start_minute: u32,
    /// The working day's end, in minutes since midnight.
    pub working_end_minute: u32,
    /// The list, in list order.
    pub items: Vec<ScheduleItemView>,
    /// Calendar events and other busy spans.
    pub busy: Vec<BusySpanView>,
    /// The gap left after each block.
    pub buffer_minutes: u32,
}

/// A time box of the day.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TimeBoxView {
    /// The box id, `box-…`.
    pub id: String,
    /// The task it holds.
    pub task: String,
    /// Start, in minutes since midnight.
    pub start_minute: u32,
    /// Length in minutes.
    pub minutes: u32,
    /// Locked boxes never move on a re-plan.
    pub is_locked: bool,
}

/// What a schedule did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduleView {
    /// Every box of the day after the schedule, by start.
    pub boxes: Vec<TimeBoxView>,
    /// Tasks that found no slot before the day ends.
    pub unplaced: Vec<String>,
    /// Free time between the start and the end of the day.
    pub free_minutes: u32,
    /// How far the work exceeds that free time.
    pub over_minutes: u32,
}

/// One day's timer time, the two kinds kept apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct DayTimeView {
    /// Real time the timer counted inside the day.
    pub spent_seconds: u64,
    /// The part of it allocated to a story by stopped sessions.
    pub logged_seconds: u64,
}

/// What the footer's health dot summarises.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VaultHealthView {
    /// Notes read.
    pub files: u32,
    /// Conflict copies a sync service left beside a note, relative paths.
    pub conflict_copies: Vec<String>,
    /// Unfinished task lines without an `[id:: ]`.
    pub tasks_without_id: u32,
    /// Notes that could not be read, relative paths.
    pub unreadable: Vec<String>,
}

/// A task line of a story with what its card needs.
pub(crate) struct StoryLine {
    pub(crate) task: VaultTask,
    pub(crate) story_title: Option<String>,
    /// The story note's status token, lower case: the vocabulary's word, else the word as written.
    pub(crate) story_status: Option<String>,
    /// The line's `[source:: ]` field.
    pub(crate) source: Option<String>,
    is_in_progress: bool,
    created: Option<Date>,
    subtasks: Vec<SubtaskView>,
}

/// The list of `date`: the tasks scheduled for it, the overdue ones when it
/// is today, and those `stored_order` names, in that order; see
/// [`domain::today_list`]. `today` defaults to the host's local day.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::FileAccess`]
/// when the vault cannot be read.
#[uniffi::export(default(today = None))]
pub fn day_list(
    workspace: &WorkspaceHandle,
    date: String,
    stored_order: Vec<String>,
    placement: NewTaskPlacementView,
    today: Option<String>,
) -> Result<DayListView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let today = civil_date(crate::day_or_today(today.as_deref())?)?;
    let date = parse_date(&date)?;
    let lines = story_lines(&vault);
    let tasks: Vec<VaultTask> = lines.iter().map(|line| line.task.clone()).collect();
    let order: Vec<TaskId> = stored_order.into_iter().map(TaskId::new).collect();
    let list = today_list::today_list(&tasks, &order, date, today, placement.into());
    // A repeated id shows its first line, the one the list counted.
    let mut by_id: HashMap<&TaskId, &StoryLine> = HashMap::new();
    for line in &lines {
        by_id.entry(&line.task.id).or_insert(line);
    }
    let listed: Vec<&StoryLine> = list
        .order()
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect();
    let totals = plan_of(date, listed.iter().map(|line| line.task.clone()))?.totals(Duration::ZERO);
    Ok(DayListView {
        date: date.to_string(),
        is_today: date == today,
        items: listed.iter().map(|line| plan_item(line, today)).collect(),
        planned_minutes: whole_minutes(totals.planned.as_duration()),
        unestimated: count(totals.unestimated),
        open_task_count: count(
            lines
                .iter()
                .filter(|line| line.task.parent.is_none() && line.task.status.is_unfinished())
                .count(),
        ),
    })
}

/// Reads one quick-add line with the workspace's ticket pattern and stories.
/// `next_id` is the id the entry shows and later saves with.
///
/// # Errors
/// [`CoreError::InvalidInput`] when nothing is left to add once the
/// recognised tokens are taken out, or when the ticket pattern is unusable.
#[uniffi::export(default(today = None))]
pub fn parse_quick_add(
    workspace: &WorkspaceHandle,
    line: String,
    next_id: String,
    workspaces: Vec<QuickAddWorkspaceView>,
    today: Option<String>,
) -> Result<QuickAddView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let today = civil_date(crate::day_or_today(today.as_deref())?)?;
    let patterns: Vec<TicketPattern> = workspace
        .workspace
        .settings
        .ticket_pattern
        .as_deref()
        .map(TicketPattern::new)
        .transpose()?
        .into_iter()
        .collect();
    let stories = story_refs(&vault);
    let workspaces: Vec<WorkspaceRef> = workspaces
        .into_iter()
        .map(|workspace| WorkspaceRef {
            id: WorkspaceId::new(workspace.id),
            name: workspace.name,
        })
        .collect();
    let context = QuickAddContext {
        today,
        ticket_patterns: &patterns,
        stories: &stories,
        workspaces: &workspaces,
        next_id: TaskId::new(next_id),
    };
    let entry = quick_add::parse(&line, &context)?;
    let story_title = entry.story.as_ref().and_then(|route| {
        stories
            .iter()
            .find(|story| story.number == route.number)
            .map(|story| story.name.clone())
    });
    Ok(QuickAddView {
        kind: match entry.kind {
            EntryKind::Task => EntryKindView::Task,
            EntryKind::Event => EntryKindView::Event,
        },
        text: entry.text,
        story_number: entry
            .story
            .as_ref()
            .map(|route| route.number.as_str().to_owned()),
        story_title,
        is_story_known: entry.story.as_ref().is_some_and(|route| route.is_known),
        is_story_from_name: entry
            .story
            .as_ref()
            .is_some_and(|route| route.matched_by == StoryMatch::Name),
        workspace: entry.workspace.map(|id| id.as_str().to_owned()),
        day: entry.day.to_string(),
        start_time: entry.time.map(|range| clock_text(range.start)),
        end_time: entry.time.and_then(|range| range.end).map(clock_text),
        estimate_minutes: entry
            .estimate
            .map(|estimate| whole_minutes(estimate.as_duration())),
        is_pending_input: entry.section == Section::PendingInput,
        waiting_on: entry.waiting_on,
        id: entry.id.as_str().to_owned(),
        warnings: entry.warnings.iter().map(warning_text).collect(),
    })
}

/// The stories the confirmation's Adjust offers, in the vault's story order.
///
/// # Errors
/// [`CoreError::FileAccess`] when the vault cannot be read.
#[uniffi::export]
pub fn story_choices(workspace: &WorkspaceHandle) -> Result<Vec<StoryChoiceView>, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(story_refs(&vault)
        .into_iter()
        .map(|story| StoryChoiceView {
            number: story.number.as_str().to_owned(),
            title: story.name,
        })
        .collect())
}

/// Writes a confirmed quick-add entry as one task line in its story's note,
/// with its day and estimate. `created` defaults to the host's local day.
///
/// # Errors
/// [`CoreError::InvalidInput`] without a story or with an unusable value,
/// [`CoreError::NotFound`] for an unknown story, [`CoreError::Conflict`] when
/// the id is taken.
#[uniffi::export(default(created = None))]
pub fn save_quick_add(
    workspace: &WorkspaceHandle,
    input: QuickAddSaveInput,
    created: Option<String>,
) -> Result<AddedTaskView, CoreError> {
    let story_number = input
        .story_number
        .ok_or_else(|| CoreError::InvalidInput("Pick a story for the task".to_owned()))?;
    let day = ops::parse_date(&input.day)?;
    let request = AddTaskRequest {
        story_number,
        section: if input.is_pending_input {
            "pending-input"
        } else {
            "next-steps"
        }
        .to_owned(),
        text: input.text,
        assigned_to: input.waiting_on,
        id: Some(input.id),
        created: crate::day_or_today(created.as_deref())?,
        scheduled: Some(day),
        estimate: input
            .estimate_minutes
            .map(|minutes| format_hours_minutes(Duration::from_secs(u64::from(minutes) * 60))),
        begin_marker: None,
    };
    let added = ops::add_story_task(&workspace.workspace, &PlainFileAccess, &request)?;
    Ok(added.into())
}

/// The free time of `date`'s working day from `from_minute` on: the working
/// hours minus the busy spans and the part of the day already past.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed date or minute,
/// [`CoreError::InvalidInput`] for a working day that ends before it starts.
#[uniffi::export]
pub fn free_minutes(
    date: String,
    from_minute: u32,
    working_start_minute: u32,
    working_end_minute: u32,
    busy: Vec<BusySpanView>,
) -> Result<u32, CoreError> {
    let date = parse_date(&date)?;
    let day = WorkingDay::new(
        date,
        clock(working_start_minute)?,
        clock(working_end_minute)?,
    )?;
    let past = BusySpanView {
        start_minute: 0,
        end_minute: from_minute,
    };
    let spans: Vec<Span> = busy
        .iter()
        .chain(std::iter::once(&past))
        .filter_map(|span| busy_span(date, *span))
        .collect();
    let slots = scheduler::free_slots(&day, spans);
    Ok(whole_minutes(scheduler::free_time(&slots)))
}

/// The minutes of an estimate typed on its own, as in the confirmation's
/// Adjust field (`45m`, `1h 30m`, `1.5h`), read by the quick-add grammar.
/// `None` for anything else, for no time at all, or for more than a day.
#[uniffi::export]
pub fn parse_estimate_minutes(text: String) -> Option<u32> {
    let compact: String = text.split_whitespace().collect();
    let minutes = whole_minutes(quick_add::parse_estimate(&compact)?.as_duration());
    (1..=MAXIMUM_ESTIMATE_MINUTES)
        .contains(&minutes)
        .then_some(minutes)
}

/// What the footer's health dot needs: conflict copies, unreadable notes and
/// unfinished task lines without an id.
///
/// # Errors
/// [`CoreError::FileAccess`] when the vault cannot be read.
#[uniffi::export]
pub fn vault_health(workspace: &WorkspaceHandle) -> Result<VaultHealthView, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(VaultHealthView {
        files: count(vault.len()),
        conflict_copies: vault
            .notes()
            .filter(|note| conflict_copy_original(&note.relative_path).is_some())
            .map(|note| note.relative_path.to_string_lossy().into_owned())
            .collect(),
        tasks_without_id: count(
            vault
                .tasks()
                .filter(|found| found.task.id().is_none())
                .filter(|found| {
                    matches!(
                        found.task.status,
                        vault_base::TaskStatus::Open | vault_base::TaskStatus::InProgress
                    )
                })
                .count(),
        ),
        unreadable: vault
            .unreadable()
            .iter()
            .map(|note| note.relative_path.to_string_lossy().into_owned())
            .collect(),
    })
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// The time boxes starting on `date`, by start.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date.
    pub fn time_boxes(&self, date: String) -> Result<Vec<TimeBoxView>, CoreError> {
        let date = parse_date(&date)?;
        Ok(lock(&self.sync)
            .state()
            .time_boxes_on(date)
            .iter()
            .map(|planned| time_box_view(&planned.task, &planned.time_box))
            .collect())
    }

    /// Runs `=` or `r` over the list and stores the boxes it lays: projected
    /// boxes are set, an unplaced task loses the unlocked box it held, locked
    /// boxes stay. New boxes get ids from this device's range.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date or minute,
    /// [`CoreError::InvalidInput`] for a working day that ends before it
    /// starts or an unusable list, or the document refused the edit.
    pub fn schedule_day(&self, input: ScheduleDayInput) -> Result<ScheduleView, CoreError> {
        let date = parse_date(&input.date)?;
        let day = WorkingDay::new(
            date,
            clock(input.working_start_minute)?,
            clock(input.working_end_minute)?,
        )?;
        let mut sync = lock(&self.sync);
        let stored = sync.state().time_boxes_on(date);
        let mut plan = plan_of(date, input.items.iter().map(schedule_task))?;
        for planned in &stored {
            if plan.task(&planned.task).is_some() {
                // A stored box that clashes with another is dropped from this
                // run; the schedule lays the task again.
                let _ = plan.set_time_box(&planned.task, planned.time_box.clone());
            }
        }
        let needing_ids: Vec<TaskId> = plan
            .entries()
            .iter()
            .filter(|entry| entry.task().is_unfinished() && entry.time_box().is_none())
            .map(|entry| entry.task().id().clone())
            .collect();
        let fresh_ids: HashMap<TaskId, TimeBoxId> = sync.edit(Instant::now(), |state| {
            needing_ids
                .iter()
                .map(|task| Ok((task.clone(), state.mint_time_box_id()?)))
                .collect()
        })?;
        let start = date.to_datetime(match input.mode {
            ScheduleModeView::AutoSchedule => clock(input.working_start_minute)?,
            ScheduleModeView::Replan => clock(input.now_minute)?,
        });
        let busy: Vec<Span> = input
            .busy
            .iter()
            .filter_map(|span| busy_span(date, *span))
            .collect();
        let options = ScheduleOptions {
            buffer: Duration::from_secs(u64::from(input.buffer_minutes) * 60),
            ..ScheduleOptions::default()
        };
        let mut new_id = |task: &TaskId| {
            fresh_ids
                .get(task)
                .cloned()
                .unwrap_or_else(|| TimeBoxId::new(format!("box-{task}")))
        };
        let schedule = match input.mode {
            ScheduleModeView::AutoSchedule => {
                scheduler::auto_schedule(&plan, &day, &busy, start, &options, &mut new_id)
            }
            ScheduleModeView::Replan => {
                scheduler::replan_from_now(&plan, &day, &busy, start, &options, &mut new_id)
            }
        };
        scheduler::apply(&mut plan, &schedule)?;
        let kept: Vec<(TaskId, TimeBox)> = plan
            .entries()
            .iter()
            .filter_map(|entry| {
                entry
                    .time_box()
                    .map(|time_box| (entry.task().id().clone(), time_box.clone()))
            })
            .collect();
        let kept_ids: HashSet<&TimeBoxId> = kept.iter().map(|(_, time_box)| &time_box.id).collect();
        let dropped: Vec<TimeBoxId> = stored
            .iter()
            .filter(|planned| plan.task(&planned.task).is_some())
            .filter(|planned| planned.time_box.lock == Lock::Unlocked)
            .map(|planned| planned.time_box.id.clone())
            .filter(|id| !kept_ids.contains(id))
            .collect();
        sync.edit(Instant::now(), |state| {
            for (task, time_box) in &kept {
                state.set_time_box(task, time_box)?;
            }
            for id in &dropped {
                state.remove_time_box(id)?;
            }
            Ok(())
        })?;
        let mut boxes: Vec<TimeBoxView> = kept
            .iter()
            .map(|(task, time_box)| time_box_view(task, time_box))
            .collect();
        boxes.sort_by_key(|time_box| (time_box.start_minute, time_box.id.clone()));
        Ok(ScheduleView {
            boxes,
            unplaced: schedule
                .unplaced
                .iter()
                .map(|task| task.as_str().to_owned())
                .collect(),
            free_minutes: whole_minutes(schedule.free_time),
            over_minutes: whole_minutes(schedule.over_free_time),
        })
    }

    /// Starts the timer on a task or a story. Only one session runs at a time.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] while another session runs or is paused, or the
    /// document refused the edit.
    pub fn start_session(
        &self,
        task: Option<String>,
        story: Option<String>,
    ) -> Result<SessionView, CoreError> {
        let mut sync = lock(&self.sync);
        let mut book = settled_session_book(&mut sync)?;
        let target = SessionTarget {
            task: task.map(TaskId::new),
            story: story.map(Into::into),
        };
        let now = Timestamp::now();
        let id = sync.edit(Instant::now(), |state| state.mint_session_id())?;
        let record = book.start(id, target, now)?.to_record();
        sync.edit(Instant::now(), |state| state.put_session(&record))?;
        Ok(session_view(record))
    }

    /// Pauses the running session.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] when no session runs, or the document refused the edit.
    pub fn pause_session(&self) -> Result<SessionView, CoreError> {
        self.step_session(|book, now| book.pause(now).map(Session::to_record))
    }

    /// Resumes the paused session.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] when no session is paused, or the document refused the edit.
    pub fn resume_session(&self) -> Result<SessionView, CoreError> {
        self.step_session(|book, now| book.resume(now).map(Session::to_record))
    }

    /// Stops the running or paused session.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] when no session is active, or the document refused the edit.
    pub fn stop_session(&self) -> Result<SessionView, CoreError> {
        self.step_session(|book, now| book.stop(now).map(Session::to_record))
    }

    /// The timer time inside the host's local `date`, up to now.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::InvalidInput`]
    /// when the stored sessions break their rules.
    pub fn day_time(&self, date: String) -> Result<DayTimeView, CoreError> {
        let date = parse_date(&date)?;
        let book = settled_session_book(&mut lock(&self.sync))?;
        let day = book.day_time(
            local_midnight(date)?,
            local_midnight(date.tomorrow().map_err(date_error)?)?,
            Timestamp::now(),
        );
        Ok(DayTimeView {
            spent_seconds: day.spent.as_secs(),
            logged_seconds: day.logged.as_secs(),
        })
    }

    /// Mints the id a new task gets from this device's range, skipping ids the
    /// vault already uses, so two Macs adding at once never collide.
    ///
    /// # Errors
    /// [`CoreError::FileAccess`] when the vault cannot be read, or the document refused the edit.
    pub fn mint_task_id(&self, workspace: &WorkspaceHandle) -> Result<String, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let taken: HashSet<String> = vault
            .tasks()
            .filter_map(|found| found.task.id().map(str::to_owned))
            .collect();
        let id = lock(&self.sync).edit(Instant::now(), |state| {
            state.mint_task_id(&|id| taken.contains(id))
        })?;
        Ok(id.as_str().to_owned())
    }
}

impl WorkspaceStateHandle {
    /// Applies one step to the session book and stores the changed session.
    fn step_session(
        &self,
        step: impl FnOnce(&mut SessionBook, Timestamp) -> domain::Result<domain::session::SessionRecord>,
    ) -> Result<SessionView, CoreError> {
        let mut sync = lock(&self.sync);
        let mut book = settled_session_book(&mut sync)?;
        let record = step(&mut book, Timestamp::now())?;
        sync.edit(Instant::now(), |state| state.put_session(&record))?;
        Ok(session_view(record))
    }
}

impl From<NewTaskPlacementView> for NewTaskPlacement {
    fn from(view: NewTaskPlacementView) -> Self {
        match view {
            NewTaskPlacementView::Top => NewTaskPlacement::Top,
            NewTaskPlacementView::Bottom => NewTaskPlacement::Bottom,
        }
    }
}

impl From<DomainError> for CoreError {
    fn from(error: DomainError) -> Self {
        let message = error.to_string();
        match error {
            DomainError::SessionAlreadyRunning { .. }
            | DomainError::NoRunningSession
            | DomainError::SessionNotRunning { .. }
            | DomainError::SessionNotPaused { .. }
            | DomainError::SessionAlreadyStopped { .. }
            | DomainError::MultipleActiveSessions { .. }
            | DomainError::LockedTimeTaken { .. } => CoreError::Conflict(message),
            DomainError::EmptyEntry => CoreError::InvalidInput("Type what to add first".to_owned()),
            _ => CoreError::InvalidInput(message),
        }
    }
}

/// Every task line with an id under a story's Next Steps or Pending Input,
/// in the vault's story order, then note order.
pub(crate) fn story_lines(vault: &Vault) -> Vec<StoryLine> {
    let mut lines = Vec::new();
    for entry in vault.stories() {
        if is_conflict_copy(vault, entry.path) {
            continue;
        }
        let Some(note) = vault.note(entry.path) else {
            continue;
        };
        let story = entry.story;
        let sections = [
            (Section::NextSteps, &story.next_steps),
            (Section::PendingInput, &story.pending_input),
        ];
        for (section, tasks) in sections {
            for task in tasks {
                if let Some(line) = story_line(task, section, story, &note.document) {
                    lines.push(line);
                }
            }
        }
    }
    lines
}

fn story_line(
    task: &vault_base::Task,
    section: Section,
    story: &vault_base::Story,
    document: &Document,
) -> Option<StoryLine> {
    let id = task.id().filter(|id| is_task_id(id))?;
    let status = task_status(task.status)?;
    let position = document
        .tasks()
        .iter()
        .position(|candidate| candidate.line == task.line)?;
    let parent = task
        .parent
        .and_then(|index| document.tasks().get(index))
        .and_then(vault_base::Task::id)
        .map(TaskId::from);
    Some(StoryLine {
        task: VaultTask {
            id: TaskId::from(id),
            text: task.text.clone(),
            status,
            section,
            story: story.number.as_deref().map(Into::into),
            created: field_date(task, "created"),
            scheduled: field_date(task, "scheduled"),
            due: field_date(task, "due"),
            estimate: task.field("estimate").and_then(quick_add::parse_estimate),
            assigned_to: task.field("assigned_to").map(str::to_owned),
            completion: field_date(task, "completion"),
            parent,
        },
        story_title: story.title.clone().or_else(|| story.number.clone()),
        story_status: story
            .status
            .map(|status| status.as_str().to_owned())
            .or_else(|| story.raw_status.as_deref().map(str::to_lowercase)),
        source: task.field("source").map(str::to_owned),
        is_in_progress: status == TaskStatus::InProgress,
        created: field_date(task, "created"),
        subtasks: document
            .subtasks(position)
            .into_iter()
            .map(|subtask| SubtaskView {
                id: subtask.id().map(str::to_owned),
                text: subtask.text.clone(),
                is_done: subtask.status == vault_base::TaskStatus::Done,
                estimate_minutes: subtask
                    .field("estimate")
                    .and_then(quick_add::parse_estimate)
                    .map(|estimate| whole_minutes(estimate.as_duration())),
            })
            .collect(),
    })
}

/// A sync service's copy (`Plan 2.md`) beside a note the vault also holds:
/// its lines are the original's, so they are left to the health dot.
fn is_conflict_copy(vault: &Vault, path: &std::path::Path) -> bool {
    conflict_copy_original(path).is_some_and(|(original, _)| vault.note(&original).is_some())
}

/// An id the app can store and order by: one token without spaces or
/// brackets, as the writer mints them.
fn is_task_id(id: &str) -> bool {
    !id.is_empty() && !id.contains(|c: char| c.is_whitespace() || c == '[' || c == ']')
}

fn plan_item(line: &StoryLine, today: Date) -> PlanItemView {
    let task = &line.task;
    let is_waiting = task.section == Section::PendingInput && task.status.is_unfinished();
    PlanItemView {
        id: task.id.as_str().to_owned(),
        text: task.text.clone(),
        story_number: task.story.as_ref().map(|story| story.as_str().to_owned()),
        story_title: line.story_title.clone(),
        is_pending_input: task.section == Section::PendingInput,
        state: match task.status {
            TaskStatus::Done | TaskStatus::Cancelled => PlanItemState::Done,
            _ if line.is_in_progress => PlanItemState::InProgress,
            _ => PlanItemState::Open,
        },
        estimate_minutes: task
            .estimate
            .map(|estimate| whole_minutes(estimate.as_duration())),
        assigned_to: task.assigned_to.clone(),
        waiting_days: line
            .created
            .filter(|_| is_waiting)
            .and_then(|created| created.until(today).ok())
            .and_then(|span| u32::try_from(span.get_days()).ok()),
        subtasks: line.subtasks.clone(),
    }
}

fn story_refs(vault: &Vault) -> Vec<StoryRef> {
    vault
        .stories()
        .into_iter()
        .filter(|entry| !is_conflict_copy(vault, entry.path))
        .filter_map(|entry| {
            let number = entry.story.number.clone()?;
            Some(StoryRef {
                name: entry.story.title.clone().unwrap_or_else(|| number.clone()),
                number: number.into(),
                aliases: entry.story.aliases.clone(),
                workspace: None,
            })
        })
        .collect()
}

/// A plan holding `tasks` in order, for the totals and the scheduler.
fn plan_of(date: Date, tasks: impl IntoIterator<Item = VaultTask>) -> Result<DayPlan, CoreError> {
    let mut plan = DayPlan::new(date);
    for task in tasks {
        plan.add(Task::new(task))?;
    }
    Ok(plan)
}

fn schedule_task(item: &ScheduleItemView) -> VaultTask {
    let mut task = VaultTask::open(item.task.as_str(), "");
    if item.is_finished {
        task.status = TaskStatus::Done;
    }
    task.estimate = item
        .estimate_minutes
        .map(|minutes| Estimate::from_minutes(u64::from(minutes)));
    task
}

/// The workspace's sessions as a book. Two Macs that each started a timer
/// before they synced leave two open sessions; only one may run, so the
/// older ones are stopped where the newest began, and that is stored.
pub(crate) fn settled_session_book(sync: &mut WorkspaceSync) -> Result<SessionBook, CoreError> {
    let mut sessions = sync
        .state()
        .sessions()
        .into_iter()
        .map(Session::from_record)
        .collect::<Result<Vec<Session>, DomainError>>()?;
    let newest_start = sessions
        .iter()
        .filter(|session| session.is_active())
        .map(Session::started_at)
        .max();
    let mut settled = Vec::new();
    for session in &mut sessions {
        let is_older_open = session.is_active() && Some(session.started_at()) != newest_start;
        if !is_older_open {
            continue;
        }
        let stop_at = newest_start.unwrap_or_else(Timestamp::now);
        if session.stop(stop_at).is_err() {
            session.stop(Timestamp::now())?;
        }
        settled.push(session.to_record());
    }
    if !settled.is_empty() {
        sync.edit(Instant::now(), |state| {
            settled
                .iter()
                .try_for_each(|record| state.put_session(record))
        })?;
    }
    Ok(SessionBook::from_sessions(sessions)?)
}

pub(crate) fn time_box_view(task: &TaskId, time_box: &TimeBox) -> TimeBoxView {
    let start = time_box.start.time();
    TimeBoxView {
        id: time_box.id.as_str().to_owned(),
        task: task.as_str().to_owned(),
        start_minute: u32::from(start.hour().unsigned_abs()) * 60
            + u32::from(start.minute().unsigned_abs()),
        minutes: whole_minutes(time_box.duration),
        is_locked: time_box.lock == Lock::Locked,
    }
}

fn busy_span(date: Date, span: BusySpanView) -> Option<Span> {
    let start: DateTime = date.to_datetime(clock(span.start_minute).ok()?);
    let end = if span.end_minute >= 24 * 60 {
        date.tomorrow().ok()?.to_datetime(Time::midnight())
    } else {
        date.to_datetime(clock(span.end_minute).ok()?)
    };
    Span::new(start, end).ok()
}

fn task_status(status: vault_base::TaskStatus) -> Option<TaskStatus> {
    match status {
        vault_base::TaskStatus::Open => Some(TaskStatus::Open),
        vault_base::TaskStatus::Done => Some(TaskStatus::Done),
        vault_base::TaskStatus::Cancelled => Some(TaskStatus::Cancelled),
        vault_base::TaskStatus::InProgress => Some(TaskStatus::InProgress),
        vault_base::TaskStatus::Other(_) => None,
    }
}

fn field_date(task: &vault_base::Task, key: &str) -> Option<Date> {
    task.field(key)?.parse().ok()
}

pub(crate) fn civil_date(date: vault_base::Date) -> Result<Date, CoreError> {
    Date::new(
        i16::try_from(date.year()).map_err(|_| out_of_range(date))?,
        i8::try_from(date.month()).map_err(|_| out_of_range(date))?,
        i8::try_from(date.day()).map_err(|_| out_of_range(date))?,
    )
    .map_err(|_| out_of_range(date))
}

fn out_of_range(date: vault_base::Date) -> CoreError {
    CoreError::InvalidDate(format!("{date} is out of range"))
}

/// The time `minute` minutes after midnight.
pub(crate) fn clock(minute: u32) -> Result<Time, CoreError> {
    let hour = i8::try_from(minute / 60).map_err(|_| minute_error(minute))?;
    let minute_of_hour = i8::try_from(minute % 60).map_err(|_| minute_error(minute))?;
    Time::new(hour, minute_of_hour, 0, 0).map_err(|_| minute_error(minute))
}

fn minute_error(minute: u32) -> CoreError {
    CoreError::InvalidDate(format!("{minute} minutes is not a time of day"))
}

fn clock_text(time: Time) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

pub(crate) fn local_midnight(date: Date) -> Result<Timestamp, CoreError> {
    Ok(date
        .to_zoned(TimeZone::system())
        .map_err(date_error)?
        .timestamp())
}

pub(crate) fn date_error(error: jiff::Error) -> CoreError {
    CoreError::InvalidDate(error.to_string())
}

fn warning_text(warning: &ParseWarning) -> String {
    match warning {
        ParseWarning::UnknownTicket { number } => format!("{number} matches no story here"),
        ParseWarning::UnknownWorkspace { name } => format!("No workspace is called {name}"),
        ParseWarning::ExtraWorkspaceDropped { name } => {
            format!("Second workspace {name} left out")
        }
        ParseWarning::ExtraEstimateDropped { token } => format!("Second estimate {token} left out"),
        ParseWarning::ExtraTimeDropped { token } => format!("Second time {token} left out"),
        ParseWarning::ExtraDayDropped { token } => format!("Second day {token} left out"),
    }
}

pub(crate) fn whole_minutes(duration: Duration) -> u32 {
    u32::try_from(duration.as_secs() / 60).unwrap_or(u32::MAX)
}

pub(crate) fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::*;
    use crate::app_state::tests::Keychain;
    use crate::app_state::{create_workspace_state, load_device_id};
    use crate::files::plain_sync_files;
    use crate::open_workspace;

    const WEB_142: &str = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md";

    fn mock_vault() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mock-vault")
    }

    /// A writable copy of the mock vault.
    pub(crate) fn sandbox() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        copy_dir(&mock_vault(), temp.path());
        temp
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    pub(crate) fn open(root: &Path) -> Arc<WorkspaceHandle> {
        open_workspace(root.to_string_lossy().into_owned()).unwrap()
    }

    pub(crate) fn state(root: &Path) -> Arc<WorkspaceStateHandle> {
        let keychain = Arc::new(Keychain::default());
        let device = load_device_id(keychain.clone()).unwrap();
        create_workspace_state(
            plain_sync_files(),
            root.to_string_lossy().into_owned(),
            "mock-studio".to_owned(),
            device,
            keychain,
        )
        .unwrap()
    }

    fn list(workspace: &WorkspaceHandle, date: &str, order: &[&str]) -> DayListView {
        day_list(
            workspace,
            date.to_owned(),
            order.iter().map(|id| (*id).to_owned()).collect(),
            NewTaskPlacementView::Bottom,
            Some("2026-10-23".to_owned()),
        )
        .unwrap()
    }

    fn ids(view: &DayListView) -> Vec<&str> {
        view.items.iter().map(|item| item.id.as_str()).collect()
    }

    fn quick_add(workspace: &WorkspaceHandle, line: &str) -> QuickAddView {
        parse_quick_add(
            workspace,
            line.to_owned(),
            "task-40".to_owned(),
            Vec::new(),
            Some("2026-10-23".to_owned()),
        )
        .unwrap()
    }

    #[test]
    fn today_holds_the_overdue_task_with_its_subtasks_after_the_stored_order() {
        let workspace = open(&mock_vault());
        let view = list(&workspace, "2026-10-23", &["task-5"]);
        assert!(view.is_today);
        assert_eq!(ids(&view), ["task-5", "task-2"]);
        let overdue = &view.items[1];
        assert_eq!(overdue.story_number.as_deref(), Some("WEB-142"));
        assert_eq!(overdue.story_title.as_deref(), Some("Checkout redesign"));
        assert_eq!(overdue.estimate_minutes, Some(120));
        let subtasks: Vec<(&str, Option<u32>)> = overdue
            .subtasks
            .iter()
            .map(|subtask| (subtask.text.as_str(), subtask.estimate_minutes))
            .collect();
        assert_eq!(
            subtasks,
            [
                ("Add the gateway test keys to the `.env.example`", None),
                ("Handle the declined-card response", Some(45)),
            ]
        );
        assert_eq!(view.planned_minutes, 120);
        assert_eq!(view.unestimated, 1);
        assert!(view.open_task_count >= 4);
    }

    #[test]
    fn another_day_holds_only_what_is_scheduled_or_stored_for_it() {
        let workspace = open(&mock_vault());
        let tomorrow = list(&workspace, "2026-10-24", &[]);
        assert!(!tomorrow.is_today);
        assert!(tomorrow.items.is_empty());
        let waiting = list(&workspace, "2026-10-24", &["task-6"]);
        let item = &waiting.items[0];
        assert!(item.is_pending_input);
        assert_eq!(item.assigned_to.as_deref(), Some("Dana"));
        assert_eq!(item.waiting_days, Some(13));
    }

    #[test]
    fn a_quick_add_line_is_understood_and_saved_as_one_planned_line() {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let entry = quick_add(&workspace, "Review the coupon copy tomorrow 30m WEB-142");
        assert_eq!(entry.kind, EntryKindView::Task);
        assert_eq!(entry.text, "Review the coupon copy");
        assert_eq!(entry.story_number.as_deref(), Some("WEB-142"));
        assert_eq!(entry.story_title.as_deref(), Some("Checkout redesign"));
        assert!(entry.is_story_known);
        assert_eq!(entry.day, "2026-10-24");
        assert_eq!(entry.estimate_minutes, Some(30));
        assert_eq!(entry.id, "task-40");
        assert!(!entry.is_pending_input);

        let note = sandbox.path().join(WEB_142);
        let before = fs::read_to_string(&note).unwrap();
        let added = save_quick_add(
            &workspace,
            QuickAddSaveInput {
                id: entry.id,
                text: entry.text,
                story_number: entry.story_number,
                is_pending_input: entry.is_pending_input,
                waiting_on: entry.waiting_on,
                day: entry.day,
                estimate_minutes: entry.estimate_minutes,
            },
            Some("2026-10-23".to_owned()),
        )
        .unwrap();
        assert_eq!(
            added.change.new_line,
            "- [ ] Review the coupon copy [id:: task-40] [created:: 2026-10-23] [scheduled:: 2026-10-24] [estimate:: 30m]"
        );
        let after = fs::read_to_string(&note).unwrap();
        assert_eq!(after.lines().count(), before.lines().count() + 1);
        assert_eq!(ids(&list(&workspace, "2026-10-24", &[])), ["task-40"]);
    }

    #[test]
    fn a_line_by_story_name_an_event_and_an_unknown_ticket_are_told_apart() {
        let workspace = open(&mock_vault());
        let by_name = quick_add(&workspace, "Sketch the summary for Cart redesign 1h");
        assert_eq!(by_name.story_number.as_deref(), Some("WEB-142"));
        assert!(by_name.is_story_from_name);
        let event = quick_add(&workspace, "Standup 9:30-9:45");
        assert_eq!(event.kind, EntryKindView::Event);
        assert_eq!(event.start_time.as_deref(), Some("09:30"));
        assert_eq!(event.end_time.as_deref(), Some("09:45"));
        let unknown = quick_add(&workspace, "Check WEB-999 later");
        assert!(!unknown.is_story_known);
        assert_eq!(unknown.warnings, ["WEB-999 matches no story here"]);
        let empty = parse_quick_add(
            &workspace,
            "   ".to_owned(),
            "task-40".to_owned(),
            Vec::new(),
            None,
        )
        .unwrap_err();
        assert!(matches!(empty, CoreError::InvalidInput(_)));
    }

    #[test]
    fn saving_without_a_story_writes_nothing() {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let before = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        let refused = save_quick_add(
            &workspace,
            QuickAddSaveInput {
                id: "task-41".to_owned(),
                text: "Loose thought".to_owned(),
                story_number: None,
                is_pending_input: false,
                waiting_on: None,
                day: "2026-10-23".to_owned(),
                estimate_minutes: None,
            },
            None,
        )
        .unwrap_err();
        assert!(matches!(refused, CoreError::InvalidInput(_)));
        assert_eq!(
            fs::read_to_string(sandbox.path().join(WEB_142)).unwrap(),
            before
        );
        let stories = story_choices(&workspace).unwrap();
        assert!(stories.iter().any(|story| story.number == "WEB-142"));
    }

    fn schedule_input(mode: ScheduleModeView) -> ScheduleDayInput {
        ScheduleDayInput {
            date: "2026-10-23".to_owned(),
            mode,
            now_minute: 13 * 60,
            working_start_minute: 9 * 60,
            working_end_minute: 17 * 60,
            items: vec![
                ScheduleItemView {
                    task: "task-2".to_owned(),
                    estimate_minutes: Some(120),
                    is_finished: false,
                },
                ScheduleItemView {
                    task: "task-5".to_owned(),
                    estimate_minutes: Some(5 * 60),
                    is_finished: false,
                },
            ],
            busy: vec![BusySpanView {
                start_minute: 11 * 60,
                end_minute: 12 * 60,
            }],
            buffer_minutes: 0,
        }
    }

    #[test]
    fn auto_schedule_lays_the_list_around_events_and_keeps_its_boxes() {
        let temp = tempfile::tempdir().unwrap();
        let state = state(temp.path());
        let laid = state
            .schedule_day(schedule_input(ScheduleModeView::AutoSchedule))
            .unwrap();
        let starts: Vec<(&str, u32, u32)> = laid
            .boxes
            .iter()
            .map(|time_box| {
                (
                    time_box.task.as_str(),
                    time_box.start_minute,
                    time_box.minutes,
                )
            })
            .collect();
        assert_eq!(
            starts,
            [("task-2", 9 * 60, 120), ("task-5", 12 * 60, 5 * 60)]
        );
        assert_eq!(laid.free_minutes, 7 * 60);
        assert!(laid.unplaced.is_empty());
        assert!(laid.over_minutes == 0);
        assert_eq!(
            state.time_boxes("2026-10-23".to_owned()).unwrap(),
            laid.boxes
        );

        let replanned = state
            .schedule_day(schedule_input(ScheduleModeView::Replan))
            .unwrap();
        assert_eq!(replanned.free_minutes, 4 * 60);
        assert_eq!(replanned.over_minutes, 3 * 60);
        assert_eq!(
            replanned.boxes[0].id, laid.boxes[0].id,
            "a box keeps its id"
        );
        assert_eq!(replanned.unplaced, ["task-5"]);
        assert_eq!(
            state.time_boxes("2026-10-23".to_owned()).unwrap().len(),
            1,
            "the unplaced task's box is gone"
        );
    }

    #[test]
    fn the_timer_runs_pauses_and_stops_with_one_session_at_a_time() {
        let temp = tempfile::tempdir().unwrap();
        let state = state(temp.path());
        let started = state
            .start_session(Some("task-2".to_owned()), Some("WEB-142".to_owned()))
            .unwrap();
        assert!(started.stopped_at.is_none());
        let second = state
            .start_session(Some("task-5".to_owned()), None)
            .unwrap_err();
        assert!(matches!(second, CoreError::Conflict(_)));
        assert!(state.pause_session().unwrap().is_paused);
        assert!(!state.resume_session().unwrap().is_paused);
        let stopped = state.stop_session().unwrap();
        assert_eq!(stopped.id, started.id);
        assert!(stopped.stopped_at.is_some());
        assert!(matches!(
            state.stop_session().unwrap_err(),
            CoreError::Conflict(_)
        ));
        let today = jiff::Zoned::now().date().to_string();
        let day = state.day_time(today).unwrap();
        assert_eq!(day.spent_seconds, day.logged_seconds);
    }

    #[test]
    fn a_minted_task_id_is_never_one_the_vault_uses() {
        let temp = sandbox();
        let workspace = open(temp.path());
        let state = state(temp.path());
        let first = state.mint_task_id(&workspace).unwrap();
        let second = state.mint_task_id(&workspace).unwrap();
        assert_ne!(first, second);
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess).unwrap();
        assert!(vault.task_by_id(&first).is_none());
    }

    #[test]
    fn free_time_is_the_working_day_minus_events_and_the_past() {
        let meeting = BusySpanView {
            start_minute: 11 * 60,
            end_minute: 12 * 60,
        };
        let free = |from: u32| {
            free_minutes(
                "2026-10-23".to_owned(),
                from,
                9 * 60,
                17 * 60,
                vec![meeting],
            )
            .unwrap()
        };
        assert_eq!(free(0), 7 * 60);
        assert_eq!(free(13 * 60), 4 * 60);
        assert_eq!(free(24 * 60), 0);
        let reversed = free_minutes("2026-10-23".to_owned(), 0, 17 * 60, 9 * 60, Vec::new());
        assert!(matches!(reversed.unwrap_err(), CoreError::InvalidInput(_)));
    }

    #[test]
    fn a_conflict_copy_and_a_repeated_id_leave_one_card_per_task() {
        let sandbox = sandbox();
        let note = sandbox.path().join(WEB_142);
        fs::copy(&note, note.with_file_name("WEB-142 Checkout redesign 2.md")).unwrap();
        let other = sandbox
            .path()
            .join("Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup.md");
        let mut text = fs::read_to_string(&other).unwrap();
        text.push_str(
            "\n## Next Steps\n- [ ] Pasted twice [id:: task-2] [scheduled:: 2026-10-23]\n",
        );
        fs::write(&other, text).unwrap();
        let view = list(&open(sandbox.path()), "2026-10-23", &[]);
        assert_eq!(ids(&view), ["task-2"]);
        assert_eq!(view.items[0].story_number.as_deref(), Some("WEB-142"));
    }

    #[test]
    fn an_estimate_typed_in_adjust_is_read_by_the_grammar_up_to_a_day() {
        let read = |text: &str| parse_estimate_minutes(text.to_owned());
        assert_eq!(read("45m"), Some(45));
        assert_eq!(read("45min"), Some(45));
        assert_eq!(read("2h"), Some(120));
        assert_eq!(read("1h 30m"), Some(90));
        assert_eq!(read("1.5h"), Some(90));
        assert_eq!(read("24h"), Some(24 * 60));
        for refused in [
            "",
            "later",
            "0m",
            "1h75m",
            "1.5h30",
            "1e1h",
            "infh",
            "25h",
            "5000000000m",
        ] {
            assert_eq!(read(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_repeated_id_shows_the_line_the_list_counted() {
        let sandbox = sandbox();
        let other = sandbox
            .path()
            .join("Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup.md");
        let text = fs::read_to_string(&other).unwrap().replace(
            "## Pending Input\n",
            "## Pending Input\n- [x] Old copy [id:: task-2] [scheduled:: 2026-10-23] [completion:: 2026-10-23]\n",
        );
        fs::write(&other, text).unwrap();
        let view = list(&open(sandbox.path()), "2026-10-23", &[]);
        assert_eq!(ids(&view), ["task-2"]);
        assert!(view.items[0].text.starts_with("Wire the payment form"));
        assert!(matches!(view.items[0].state, PlanItemState::Open));
    }

    #[test]
    fn a_tick_beside_a_conflict_copy_changes_the_note_the_card_shows() {
        let sandbox = sandbox();
        let note = sandbox.path().join(WEB_142);
        let copy = note.with_file_name("WEB-142 Checkout redesign 2.md");
        fs::copy(&note, &copy).unwrap();
        let copy_before = fs::read_to_string(&copy).unwrap();
        let workspace = open(sandbox.path());
        crate::tick_task_done(
            &workspace,
            "task-2".to_owned(),
            Some("2026-10-23".to_owned()),
        )
        .unwrap();
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("- [x] Wire the payment form")
        );
        assert_eq!(fs::read_to_string(&copy).unwrap(), copy_before);
        let view = list(&workspace, "2026-10-23", &[]);
        assert!(matches!(view.items[0].state, PlanItemState::Done));
    }

    #[test]
    fn a_quick_add_beside_a_conflict_copy_lands_in_the_note_and_on_the_list() {
        let sandbox = sandbox();
        let note = sandbox.path().join(WEB_142);
        let copy = note.with_file_name("WEB-142 Checkout redesign 2.md");
        fs::copy(&note, &copy).unwrap();
        let copy_before = fs::read_to_string(&copy).unwrap();
        let workspace = open(sandbox.path());
        let numbers: Vec<String> = story_choices(&workspace)
            .unwrap()
            .into_iter()
            .map(|choice| choice.number)
            .filter(|number| number == "WEB-142")
            .collect();
        assert_eq!(numbers, ["WEB-142"]);
        let entry = quick_add(&workspace, "Review the coupon copy tomorrow 30m WEB-142");
        save_quick_add(
            &workspace,
            QuickAddSaveInput {
                id: entry.id,
                text: entry.text,
                story_number: entry.story_number,
                is_pending_input: entry.is_pending_input,
                waiting_on: entry.waiting_on,
                day: entry.day,
                estimate_minutes: entry.estimate_minutes,
            },
            Some("2026-10-23".to_owned()),
        )
        .unwrap();
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("Review the coupon copy [id:: task-40]")
        );
        assert_eq!(fs::read_to_string(&copy).unwrap(), copy_before);
        assert_eq!(ids(&list(&workspace, "2026-10-24", &[])), ["task-40"]);
    }

    #[test]
    fn a_line_whose_id_is_no_task_id_stays_off_the_list() {
        let sandbox = sandbox();
        let note = sandbox.path().join(WEB_142);
        let text = fs::read_to_string(&note).unwrap().replace(
            "## Pending Input\n",
            "## Pending Input\n- [ ] Odd one [id:: my task] [scheduled:: 2026-10-23]\n",
        );
        fs::write(&note, text).unwrap();
        let view = list(&open(sandbox.path()), "2026-10-23", &[]);
        assert_eq!(ids(&view), ["task-2"]);
    }

    #[test]
    fn two_timers_from_two_macs_are_settled_and_the_timer_works_again() {
        let temp = tempfile::tempdir().unwrap();
        let state = state(temp.path());
        let now = Timestamp::now();
        let earlier = Session::start(
            "ses-mac-a",
            SessionTarget::task("task-2"),
            now.checked_sub(jiff::SignedDuration::from_mins(30))
                .unwrap(),
        )
        .to_record();
        let later = Session::start(
            "ses-mac-b",
            SessionTarget::task("task-5"),
            now.checked_sub(jiff::SignedDuration::from_mins(10))
                .unwrap(),
        )
        .to_record();
        lock(&state.sync)
            .edit(Instant::now(), |doc| {
                doc.put_session(&earlier)?;
                doc.put_session(&later)
            })
            .unwrap();

        let today = jiff::Zoned::now().date().to_string();
        assert!(state.day_time(today).is_ok());
        let stopped = state.stop_session().unwrap();
        assert_eq!(
            stopped.id, "ses-mac-b",
            "the newer timer is the one running"
        );
        let settled = state
            .sessions()
            .into_iter()
            .find(|session| session.id == "ses-mac-a")
            .unwrap();
        assert!(
            settled.stopped_at.is_some(),
            "the older one stopped where the newer began"
        );
        assert!(state.start_session(Some("task-8".to_owned()), None).is_ok());
    }

    #[test]
    fn the_health_dot_counts_lines_without_an_id() {
        let health = vault_health(&open(&mock_vault())).unwrap();
        assert!(health.files > 10);
        assert!(health.tasks_without_id >= 1);
        assert!(health.conflict_copies.is_empty());
    }
}
