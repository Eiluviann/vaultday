//! The side panel's Calendar and Hours tabs and the Reminders intake.
//!
//! - The day grid ([`WorkspaceStateHandle::calendar_day`]) lays out the
//!   picked calendars' events and the stored time boxes through
//!   [`domain::calendar_day`]; unlocked boxes are the scheduler's output
//!   ([`WorkspaceStateHandle::schedule_day`]), never placed here.
//! - A pin writes the box to the private calendar through the platform's
//!   [`CalendarPort`] before the box is stored as locked, so a refused write
//!   (iCloud Calendar off) never leaves a locked box without its event.
//! - Ended events become meeting sessions through the domain's session rules.
//! - The Hours tab is the domain's estimate maths per story.
//! - A Reminders item becomes a task line carrying `[reminder:: <id>]`; the
//!   platform deletes the item only after the line is written, and a retry
//!   finds the line by that field instead of writing it twice.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::calendar_day::{self, Block, BlockKind, TaskBlockInput};
use domain::day_plan::{Lock, TimeBox};
use domain::estimate::{self, BudgetSource, Estimate};
use domain::ports::{CalendarError, CalendarSource};
use domain::quick_add;
use domain::scheduler::{DEFAULT_UNESTIMATED_DURATION, Event, Span};
use domain::session::{CalendarEvent as MeetingEvent, SessionTarget};
use domain::task::{StoryNumber, TaskId};
use domain::ticket::TicketPattern;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use vault_base::ops::{self, OpError};
use vault_base::{NewTask, PlainFileAccess, StoryStatus, TaskSection, Vault, VaultWriter};

use app_state::PlannedTimeBox;

use crate::app_state::{WorkspaceStateHandle, lock, parse_date};
use crate::calendar::{CalendarEvent, CalendarPort, ForeignCalendar};
use crate::home::{
    TimeBoxView, clock, date_error, local_midnight, settled_session_book, time_box_view,
    whole_minutes,
};
use crate::{CoreError, WorkspaceHandle};

/// The private calendar's name; the platform's calendar source writes only to
/// the calendar of this name in the iCloud account.
pub const PRIVATE_CALENDAR: &str = "Vaultday";

/// Where Reminders items from a list without a story are filed.
pub const REMINDERS_INTAKE_NOTE: &str = "Inbox/Reminders.md";

/// What a block of the day grid stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CalendarBlockKind {
    /// An event of one of the workspace's calendars; never moved or written.
    Event,
    /// A pinned task: its box is in the private calendar and keeps its slot.
    LockedTask,
    /// A task's projection from the scheduler; it floats as the day changes.
    UnlockedTask,
}

/// One block of the day grid, placed in minutes since the date's midnight.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CalendarBlockView {
    /// The event's or the box's id.
    pub id: String,
    /// Event, locked or unlocked task.
    pub kind: CalendarBlockKind,
    /// The task, for a task block.
    pub task: Option<String>,
    /// The item's full name.
    pub title: String,
    /// The second line: time range and state tokens.
    pub detail: String,
    /// Where the block starts on the date's grid.
    pub start_minute: u32,
    /// Where it ends, at most 1440.
    pub end_minute: u32,
    /// The tracked share of the block in thousandths, drawn from its top.
    pub tracked_permille: u32,
    /// Its column among the blocks it overlaps, from 0.
    pub lane: u32,
    /// How many columns its overlapping group needs.
    pub lanes: u32,
    /// A done task's block.
    pub is_done: bool,
}

/// The day grid of one date.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CalendarDayView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Every block, by start.
    pub blocks: Vec<CalendarBlockView>,
    /// Now, in minutes since midnight, when the date is today; the now line
    /// and the past wash end there.
    pub now_minute: Option<u32>,
    /// The date lies before today: the whole grid is past.
    pub is_past: bool,
}

/// Where a story's budget comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BudgetSourceView {
    /// The story's own `estimate`.
    Story,
    /// The sum of its tasks' estimates.
    Tasks,
    /// No estimate anywhere.
    None,
}

/// One story's row of the Hours tab.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryHoursView {
    /// The ticket number.
    pub number: String,
    /// The note's title, else the number.
    pub title: String,
    /// Real timer time on the story, private.
    pub tracked_minutes: u32,
    /// The budget, when there is one.
    pub estimate_minutes: Option<u32>,
    /// Budget left, never below zero.
    pub remaining_minutes: u32,
    /// Tracked time beyond the budget.
    pub overtime_minutes: u32,
    /// Tracked as a share of the budget, rounded down.
    pub percent_used: Option<u32>,
    /// Where the budget comes from.
    pub budget: BudgetSourceView,
}

/// The story screen's Hours tab: the three numbers kept apart. Tracked is the
/// timer's real time and private; logged is what stopped sessions allocated
/// to the story; remaining is the estimate less the logged time.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryHoursDetailView {
    /// The ticket number.
    pub number: String,
    /// Real timer time on the story, private.
    pub tracked_minutes: u32,
    /// Time stopped sessions logged to the story.
    pub logged_minutes: u32,
    /// The budget, when there is one.
    pub estimate_minutes: Option<u32>,
    /// Estimate less logged, never below zero.
    pub remaining_minutes: u32,
    /// Logged time beyond the budget.
    pub overtime_minutes: u32,
    /// Where the budget comes from.
    pub budget: BudgetSourceView,
}

/// The Hours tab: today's tracked time against the daily target, and every
/// open story with an estimate or tracked time.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HoursView {
    /// The daily target from the workspace's `hours_per_day`.
    pub target_minutes: u32,
    /// Tracked inside the date, up to now.
    pub tracked_minutes: u32,
    /// The stories, in the vault's story order.
    pub stories: Vec<StoryHoursView>,
}

/// One Reminders item, as EventKit reads it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReminderItemView {
    /// The item's external identifier, the same on every device; it is
    /// written to the task line as `[reminder:: ]`.
    pub external_id: String,
    /// The title.
    pub title: String,
    /// The due day, `YYYY-MM-DD`, when it has one.
    pub due_date: Option<String>,
    /// Whether the due day carries a time of day.
    pub has_due_time: bool,
    /// Ticked in Reminders.
    pub is_completed: bool,
    /// Repeats on a rule.
    pub is_recurring: bool,
    /// Alerts on arriving at or leaving a place.
    pub has_location_alarm: bool,
}

/// What the intake did with one item; the platform deletes the item from
/// Reminders only for [`ReminderIntake::Imported`] and
/// [`ReminderIntake::AlreadyImported`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ReminderIntake {
    /// A task line was written.
    Imported {
        /// The new task's id.
        task_id: String,
        /// The note it went to, relative to the workspace root.
        note: String,
    },
    /// A line carrying the item's id exists already, from an import that was
    /// interrupted before the item was deleted.
    AlreadyImported {
        /// That line's task id.
        task_id: String,
    },
    /// The item stays in Reminders.
    Left {
        /// Why, in words for the user.
        reason: String,
    },
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// The day grid of `date`: the events `calendar` reads for it and the
    /// stored boxes, with each task block's tracked share of that day.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::FileAccess`]
    /// when the vault cannot be read, [`CoreError::Conflict`] when the
    /// calendar cannot be read.
    pub fn calendar_day(
        &self,
        workspace: &WorkspaceHandle,
        calendar: Arc<dyn CalendarPort>,
        date: String,
    ) -> Result<CalendarDayView, CoreError> {
        let date = parse_date(&date)?;
        let events = ForeignCalendar::new(calendar)
            .events(date, date)
            .map_err(calendar_error)?;
        self.calendar_day_at(workspace, &events, date, &Zoned::now())
    }

    /// Pins or unpins the task's box on `date`: a pin writes the box to the
    /// private calendar, then stores it locked; an unpin deletes that event,
    /// then stores the box unlocked. Returns the box as stored.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the task holds no box that day,
    /// [`CoreError::Conflict`] when another pin holds the time or the
    /// calendar refused (the message says why), or the document refused.
    pub fn toggle_pin(
        &self,
        workspace: &WorkspaceHandle,
        calendar: Arc<dyn CalendarPort>,
        date: String,
        task: String,
    ) -> Result<TimeBoxView, CoreError> {
        let date = parse_date(&date)?;
        let task = TaskId::new(task);
        // The calendar port calls into the platform's store, so the state
        // lock is not held across it.
        let stored = lock(&self.sync).state().time_boxes_on(date);
        let held = stored
            .iter()
            .find(|planned| planned.task == task)
            .ok_or_else(|| {
                CoreError::NotFound(
                    "This task has no time on the calendar yet; drag it onto the calendar or auto-schedule the day".to_owned(),
                )
            })?;
        let mut port = ForeignCalendar::new(calendar);
        let mut time_box = held.time_box.clone();
        match time_box.lock {
            Lock::Unlocked => {
                let span = box_span(&time_box)?;
                calendar_day::ensure_lockable(
                    &task,
                    &span,
                    stored
                        .iter()
                        .map(|planned| (&planned.task, &planned.time_box)),
                )?;
                time_box.lock = Lock::Locked;
                let title = task_title(workspace, &task)?;
                port.write_time_box(PRIVATE_CALENDAR, &title, &time_box)
                    .map_err(calendar_error)?;
            }
            Lock::Locked => {
                // Without the private calendar (iCloud Calendar turned off
                // since), its event is gone with it; the box floats again.
                match port.delete_time_box(PRIVATE_CALENDAR, &time_box.id) {
                    Ok(())
                    | Err(
                        CalendarError::TimeBoxNotFound { .. }
                        | CalendarError::UnknownCalendar { .. },
                    ) => {}
                    Err(error) => return Err(calendar_error(error)),
                }
                time_box.lock = Lock::Unlocked;
            }
        }
        lock(&self.sync).edit(Instant::now(), |state| state.set_time_box(&task, &time_box))?;
        Ok(time_box_view(&task, &time_box))
    }

    /// A task dropped on the grid of `date` at `start_minute`: its box, from
    /// whichever day, moves there (keeping its length, else the task's
    /// estimate, else an hour),
    /// is written to the private calendar and stored locked.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date or minute,
    /// [`CoreError::Conflict`] when another pin holds the time or the
    /// calendar refused, or the document refused.
    pub fn pin_at(
        &self,
        workspace: &WorkspaceHandle,
        calendar: Arc<dyn CalendarPort>,
        date: String,
        task: String,
        start_minute: u32,
    ) -> Result<TimeBoxView, CoreError> {
        let date = parse_date(&date)?;
        let task = TaskId::new(task);
        let start = date.to_datetime(clock(start_minute)?);
        let title = task_title(workspace, &task)?;
        let estimate = task_estimate(workspace, &task)?;
        // The task's box on any day moves, so a drag to another day never
        // leaves a second box (or a second event) behind.
        let (stored, held) = {
            let mut sync = lock(&self.sync);
            let stored = sync.state().time_boxes_on(date);
            let held = match sync.state().time_boxes_of(&task).into_iter().next() {
                Some(planned) => planned,
                None => PlannedTimeBox {
                    task: task.clone(),
                    time_box: TimeBox {
                        id: sync.edit(Instant::now(), |state| state.mint_time_box_id())?,
                        start,
                        duration: estimate
                            .map_or(DEFAULT_UNESTIMATED_DURATION, Estimate::as_duration),
                        lock: Lock::Unlocked,
                    },
                },
            };
            (stored, held)
        };
        let time_box = TimeBox {
            id: held.time_box.id,
            start,
            duration: held.time_box.duration,
            lock: Lock::Locked,
        };
        calendar_day::ensure_lockable(
            &task,
            &box_span(&time_box)?,
            stored
                .iter()
                .map(|planned| (&planned.task, &planned.time_box)),
        )?;
        ForeignCalendar::new(calendar)
            .write_time_box(PRIVATE_CALENDAR, &title, &time_box)
            .map_err(calendar_error)?;
        lock(&self.sync).edit(Instant::now(), |state| state.set_time_box(&task, &time_box))?;
        Ok(time_box_view(&task, &time_box))
    }

    /// Records a meeting session for every event of `date` that has ended,
    /// once: the session's id derives from the event, so another call or
    /// another Mac adds nothing. A ticket number in the title files the time
    /// under that story. Returns how many sessions were added.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::Conflict`]
    /// when the calendar cannot be read, or the document refused.
    pub fn record_ended_meetings(
        &self,
        workspace: &WorkspaceHandle,
        calendar: Arc<dyn CalendarPort>,
        date: String,
    ) -> Result<u32, CoreError> {
        let date = parse_date(&date)?;
        let events = calendar
            .events(date.to_string(), date.to_string())
            .map_err(|failure| calendar_error(crate::calendar::calendar_error(failure)))?;
        self.record_meetings_at(workspace, &events, Timestamp::now())
    }

    /// The Hours tab for `date`.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date, [`CoreError::FileAccess`]
    /// when the vault cannot be read, [`CoreError::InvalidInput`] when stored
    /// sessions break their rules.
    pub fn hours(&self, workspace: &WorkspaceHandle, date: String) -> Result<HoursView, CoreError> {
        self.hours_at(workspace, parse_date(&date)?, Timestamp::now())
    }

    /// The story screen's Hours for the story numbered `number`.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] without such a story, [`CoreError::FileAccess`]
    /// when the vault cannot be read, [`CoreError::InvalidInput`] when stored
    /// sessions break their rules.
    pub fn story_hours(
        &self,
        workspace: &WorkspaceHandle,
        number: String,
    ) -> Result<StoryHoursDetailView, CoreError> {
        self.story_hours_at(workspace, &number, Timestamp::now())
    }

    /// Files one Reminders item as a task line: into the Next Steps of
    /// `story`, else of the intake note. The due day becomes `scheduled` when
    /// it has a time and `due` when it is a day alone. Completed, recurring
    /// and location-based items stay in Reminders. `today` is the creation
    /// day and defaults to the host's.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for an unknown story, [`CoreError::FileAccess`]
    /// when the vault cannot be read or written, or the document refused.
    #[uniffi::method(default(today = None))]
    pub fn import_reminder(
        &self,
        workspace: &WorkspaceHandle,
        item: ReminderItemView,
        story: Option<String>,
        today: Option<String>,
    ) -> Result<ReminderIntake, CoreError> {
        if let Some(reason) = left_in_reminders(&item) {
            return Ok(ReminderIntake::Left {
                reason: reason.to_owned(),
            });
        }
        let mut batch = ReminderBatch::open(workspace, today.as_deref())?;
        self.import_into(&mut batch, workspace, &item, story.as_deref())
    }

    /// Files a batch of Reminders items as [`Self::import_reminder`] files
    /// one, reading the vault once for the whole batch rather than once per
    /// item. Each item gets its own outcome, in order; one item's failure
    /// (an unknown story, a refused write) does not stop the rest.
    ///
    /// # Errors
    /// [`CoreError::FileAccess`] when the vault cannot be read, or
    /// [`CoreError::InvalidDate`] for a malformed `today`.
    #[uniffi::method(default(today = None))]
    pub fn import_reminders(
        &self,
        workspace: &WorkspaceHandle,
        batch: Vec<ReminderImport>,
        today: Option<String>,
    ) -> Result<Vec<ReminderImportOutcome>, CoreError> {
        let mut opened = ReminderBatch::open(workspace, today.as_deref())?;
        Ok(batch
            .into_iter()
            .map(|entry| {
                let external_id = entry.item.external_id.clone();
                let result = match left_in_reminders(&entry.item) {
                    Some(reason) => Ok(ReminderIntake::Left {
                        reason: reason.to_owned(),
                    }),
                    None => self.import_into(
                        &mut opened,
                        workspace,
                        &entry.item,
                        entry.story.as_deref(),
                    ),
                };
                match result {
                    Ok(intake) => ReminderImportOutcome {
                        external_id,
                        intake: Some(intake),
                        error: None,
                    },
                    Err(error) => ReminderImportOutcome {
                        external_id,
                        intake: None,
                        error: Some(error.to_string()),
                    },
                }
            })
            .collect())
    }
}

/// One item of a Reminders batch and the story its list files into.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReminderImport {
    /// The item.
    pub item: ReminderItemView,
    /// The story whose Next Steps take it; `None` for the intake note.
    pub story: Option<String>,
}

/// What filing one item of a batch did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReminderImportOutcome {
    /// The item's external id.
    pub external_id: String,
    /// What happened to it; `None` when it failed.
    pub intake: Option<ReminderIntake>,
    /// Why it failed, in words for the user; the item is tried again next run.
    pub error: Option<String>,
}

/// The vault as read once for a batch, with what the batch wrote since.
struct ReminderBatch {
    vault: Vault,
    created: vault_base::Date,
    /// Task ids in the vault or minted by this batch.
    taken_ids: std::collections::HashSet<String>,
    /// External ids filed by this batch, with their task ids.
    filed: HashMap<String, String>,
}

impl ReminderBatch {
    fn open(workspace: &WorkspaceHandle, today: Option<&str>) -> Result<ReminderBatch, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let created = match today {
            Some(text) => ops::parse_date(text)?,
            None => vault_today()?,
        };
        let taken_ids = vault
            .tasks()
            .filter_map(|found| found.task.id().map(str::to_owned))
            .collect();
        Ok(ReminderBatch {
            vault,
            created,
            taken_ids,
            filed: HashMap::new(),
        })
    }
}

impl WorkspaceStateHandle {
    /// Files one item that is not left in Reminders into the batch's vault.
    fn import_into(
        &self,
        batch: &mut ReminderBatch,
        workspace: &WorkspaceHandle,
        item: &ReminderItemView,
        story: Option<&str>,
    ) -> Result<ReminderIntake, CoreError> {
        let already = batch
            .filed
            .get(&item.external_id)
            .cloned()
            .or_else(|| line_of_reminder(&batch.vault, &item.external_id));
        if let Some(task_id) = already {
            return Ok(ReminderIntake::AlreadyImported { task_id });
        }
        let writer = VaultWriter::new(&PlainFileAccess, &workspace.workspace);
        let note = match story {
            Some(number) => batch
                .vault
                .story_by_number(number)
                .map(|entry| entry.path.to_path_buf())
                .ok_or_else(|| {
                    CoreError::NotFound(format!("No story {number} in this workspace"))
                })?,
            None => {
                let path = Path::new(REMINDERS_INTAKE_NOTE);
                writer
                    .create_intake_note(path, "Reminders")
                    .map_err(OpError::from)?;
                path.to_path_buf()
            }
        };
        let due = item.due_date.as_deref().map(ops::parse_date).transpose()?;
        let taken = &batch.taken_ids;
        let task_id = lock(&self.sync)
            .edit(Instant::now(), |state| {
                state.mint_task_id(&|id| taken.contains(id))
            })?
            .as_str()
            .to_owned();
        batch.taken_ids.insert(task_id.clone());
        let task = NewTask {
            text: reminder_text(&item.title),
            id: task_id.clone(),
            created: batch.created,
            assigned_to: None,
            scheduled: due.filter(|_| item.has_due_time),
            estimate: None,
            due: due.filter(|_| !item.has_due_time),
            reminder: Some(item.external_id.clone()),
            source: None,
        };
        writer
            .add_task(&note, TaskSection::NextSteps, &task)
            .map_err(OpError::from)?;
        batch
            .filed
            .insert(item.external_id.clone(), task_id.clone());
        Ok(ReminderIntake::Imported {
            task_id,
            note: note.to_string_lossy().into_owned(),
        })
    }
}

impl WorkspaceStateHandle {
    fn calendar_day_at(
        &self,
        workspace: &WorkspaceHandle,
        events: &[Event],
        date: Date,
        now: &Zoned,
    ) -> Result<CalendarDayView, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let tasks = task_facts(&vault);
        let from = local_midnight(date)?;
        let to = local_midnight(date.tomorrow().map_err(date_error)?)?;
        let mut sync = lock(&self.sync);
        let book = settled_session_book(&mut sync)?;
        let inputs: Vec<TaskBlockInput> = sync
            .state()
            .time_boxes_on(date)
            .into_iter()
            .map(|planned| {
                let facts = tasks.get(&planned.task);
                TaskBlockInput {
                    tracked: book.spent_on_task_within(&planned.task, from, to, now.timestamp()),
                    title: facts
                        .map_or_else(|| planned.task.to_string(), |facts| facts.text.clone()),
                    has_estimate: facts.is_some_and(|facts| facts.estimate.is_some()),
                    is_done: facts.is_some_and(|facts| facts.is_done),
                    task: planned.task,
                    time_box: planned.time_box,
                }
            })
            .collect();
        let today = now.date();
        Ok(CalendarDayView {
            date: date.to_string(),
            blocks: calendar_day::day_blocks(date, events, &inputs)
                .iter()
                .map(|block| block_view(date, block))
                .collect(),
            now_minute: (date == today).then(|| minute_of(now.datetime())),
            is_past: date < today,
        })
    }

    fn record_meetings_at(
        &self,
        workspace: &WorkspaceHandle,
        events: &[CalendarEvent],
        now: Timestamp,
    ) -> Result<u32, CoreError> {
        let pattern = workspace
            .workspace
            .settings
            .ticket_pattern
            .as_deref()
            .map(TicketPattern::new)
            .transpose()?;
        // Declined invitations and free time are no meeting the user sat in.
        let meetings: Vec<MeetingEvent> = events
            .iter()
            .filter(|event| event.is_attended)
            .filter_map(|event| {
                Some(MeetingEvent {
                    key: event.meeting_key.clone(),
                    title: event.title.clone(),
                    start: instant(event.start.parse().ok()?)?,
                    end: instant(event.end.parse().ok()?)?,
                })
            })
            .collect();
        let mut sync = lock(&self.sync);
        let mut book = settled_session_book(&mut sync)?;
        let recorded = book.record_ended_meetings(&meetings, now, |event| SessionTarget {
            task: None,
            story: pattern
                .as_ref()
                .and_then(|pattern| pattern.find_all(&event.title).first().copied())
                .map(StoryNumber::new),
        })?;
        if !recorded.is_empty() {
            sync.edit(Instant::now(), |state| {
                recorded
                    .iter()
                    .try_for_each(|record| state.put_session(record))
            })?;
        }
        Ok(u32::try_from(recorded.len()).unwrap_or(u32::MAX))
    }

    fn story_hours_at(
        &self,
        workspace: &WorkspaceHandle,
        number: &str,
        now: Timestamp,
    ) -> Result<StoryHoursDetailView, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let entry = vault
            .story_by_number(number)
            .ok_or_else(|| CoreError::NotFound(format!("No story {number} in this workspace")))?;
        let story = entry.story;
        let target = estimate::daily_target(workspace.workspace.settings.hours_per_day);
        let working_day = target.unwrap_or(Duration::from_secs(8 * 3600));
        let tasks: Vec<&vault_base::Task> = story
            .next_steps
            .iter()
            .chain(&story.pending_input)
            .collect();
        let task_stories: HashMap<TaskId, StoryNumber> = tasks
            .iter()
            .filter_map(|task| task.id())
            .map(|id| (TaskId::new(id), StoryNumber::new(number)))
            .collect();
        let (budget_estimate, budget) = estimate::story_budget(
            story
                .estimate
                .as_deref()
                .and_then(|text| estimate::parse_story_budget(text, working_day)),
            tasks
                .iter()
                .filter(|task| task.parent.is_none())
                .map(|task| task.field("estimate").and_then(quick_add::parse_estimate)),
        );
        let story_number = StoryNumber::new(number);
        let mut sync = lock(&self.sync);
        let book = settled_session_book(&mut sync)?;
        let tracked = book
            .spent_by_story(&task_stories, now)
            .get(&story_number)
            .copied()
            .unwrap_or_default();
        let logged =
            estimate::logged_report(book.logged_on_story(&story_number, now), budget_estimate);
        Ok(StoryHoursDetailView {
            number: number.to_owned(),
            tracked_minutes: whole_minutes(tracked.as_duration()),
            logged_minutes: whole_minutes(logged.logged.as_duration()),
            estimate_minutes: budget_estimate.map(|estimate| whole_minutes(estimate.as_duration())),
            remaining_minutes: whole_minutes(logged.remaining),
            overtime_minutes: whole_minutes(logged.overtime),
            budget: budget_source_view(budget),
        })
    }

    fn hours_at(
        &self,
        workspace: &WorkspaceHandle,
        date: Date,
        now: Timestamp,
    ) -> Result<HoursView, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let target = estimate::daily_target(workspace.workspace.settings.hours_per_day);
        // A story budget in days counts each as the daily target.
        let working_day = target.unwrap_or(Duration::from_secs(8 * 3600));
        let mut task_stories: HashMap<TaskId, StoryNumber> = HashMap::new();
        let mut rows = Vec::new();
        for entry in vault.stories() {
            let story = entry.story;
            let Some(number) = story.number.as_deref() else {
                continue;
            };
            let tasks: Vec<&vault_base::Task> = story
                .next_steps
                .iter()
                .chain(&story.pending_input)
                .collect();
            for task in &tasks {
                if let Some(id) = task.id() {
                    task_stories.insert(TaskId::new(id), StoryNumber::new(number));
                }
            }
            let is_open = !matches!(
                story.status,
                Some(StoryStatus::Done | StoryStatus::Cancelled)
            );
            if is_open {
                rows.push((
                    StoryNumber::new(number),
                    story.title.clone().unwrap_or_else(|| number.to_owned()),
                    story
                        .estimate
                        .as_deref()
                        .and_then(|text| estimate::parse_story_budget(text, working_day)),
                    tasks
                        .iter()
                        .filter(|task| task.parent.is_none())
                        .map(|task| task.field("estimate").and_then(quick_add::parse_estimate))
                        .collect::<Vec<_>>(),
                ));
            }
        }
        let mut sync = lock(&self.sync);
        let book = settled_session_book(&mut sync)?;
        let tracked = book.spent_by_story(&task_stories, now);
        let day = book.day_time(
            local_midnight(date)?,
            local_midnight(date.tomorrow().map_err(date_error)?)?,
            now,
        );
        let stories = rows
            .into_iter()
            .map(|(number, title, story_estimate, task_estimates)| {
                let spent = tracked.get(&number).copied().unwrap_or_default();
                (
                    title,
                    estimate::story_hours(number, story_estimate, task_estimates, spent),
                )
            })
            .filter(|(_, hours)| {
                hours.report.estimate.is_some() || !hours.report.reported.is_zero()
            })
            .map(|(title, hours)| story_hours_view(title, &hours))
            .collect();
        Ok(HoursView {
            target_minutes: target.map_or(0, whole_minutes),
            tracked_minutes: whole_minutes(day.spent.as_duration()),
            stories,
        })
    }
}

/// A task line's text, estimate and state, by id.
struct TaskFacts {
    text: String,
    estimate: Option<Estimate>,
    is_done: bool,
}

fn task_facts(vault: &Vault) -> HashMap<TaskId, TaskFacts> {
    let mut facts = HashMap::new();
    for found in vault.tasks() {
        let Some(id) = found.task.id() else { continue };
        facts.entry(TaskId::new(id)).or_insert_with(|| TaskFacts {
            text: found.task.text.clone(),
            estimate: found
                .task
                .field("estimate")
                .and_then(quick_add::parse_estimate),
            is_done: matches!(
                found.task.status,
                vault_base::TaskStatus::Done | vault_base::TaskStatus::Cancelled
            ),
        });
    }
    facts
}

fn task_title(workspace: &WorkspaceHandle, task: &TaskId) -> Result<String, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(task_facts(&vault)
        .remove(task)
        .map_or_else(|| task.to_string(), |facts| facts.text))
}

fn task_estimate(
    workspace: &WorkspaceHandle,
    task: &TaskId,
) -> Result<Option<Estimate>, CoreError> {
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    Ok(task_facts(&vault)
        .remove(task)
        .and_then(|facts| facts.estimate))
}

fn block_view(date: Date, block: &Block) -> CalendarBlockView {
    let (kind, task, is_done) = match &block.kind {
        BlockKind::Event { .. } => (CalendarBlockKind::Event, None, false),
        BlockKind::Task {
            task,
            lock,
            is_done,
            ..
        } => (
            match lock {
                Lock::Locked => CalendarBlockKind::LockedTask,
                Lock::Unlocked => CalendarBlockKind::UnlockedTask,
            },
            Some(task.as_str().to_owned()),
            *is_done,
        ),
    };
    CalendarBlockView {
        id: block.id.clone(),
        kind,
        task,
        title: block.title.clone(),
        detail: block.detail(),
        start_minute: minute_on(date, block.span.start()),
        end_minute: minute_on(date, block.span.end()),
        tracked_permille: block.tracked_permille(),
        lane: u32::try_from(block.lane).unwrap_or(0),
        lanes: u32::try_from(block.lanes).unwrap_or(1),
        is_done,
    }
}

fn story_hours_view(title: String, hours: &estimate::StoryHours) -> StoryHoursView {
    let report = &hours.report;
    StoryHoursView {
        number: hours.story.as_str().to_owned(),
        title,
        tracked_minutes: whole_minutes(report.reported.as_duration()),
        estimate_minutes: report
            .estimate
            .map(|estimate| whole_minutes(estimate.as_duration())),
        remaining_minutes: whole_minutes(report.remaining),
        overtime_minutes: whole_minutes(report.overtime),
        percent_used: report
            .percent_used()
            .map(|percent| u32::try_from(percent).unwrap_or(u32::MAX)),
        budget: budget_source_view(hours.budget),
    }
}

fn budget_source_view(budget: BudgetSource) -> BudgetSourceView {
    match budget {
        BudgetSource::Story => BudgetSourceView::Story,
        BudgetSource::Tasks => BudgetSourceView::Tasks,
        BudgetSource::None => BudgetSourceView::None,
    }
}

/// Why an item stays in Reminders, or `None` when it is imported. Deleting a
/// recurring item would end the series, and a place-based alert cannot be
/// kept by a task line.
fn left_in_reminders(item: &ReminderItemView) -> Option<&'static str> {
    if item.is_completed {
        Some("Completed in Reminders")
    } else if item.is_recurring {
        Some("Repeats in Reminders; repeat rules are not imported yet")
    } else if item.has_location_alarm {
        Some("Alerts at a place, which only Reminders can do")
    } else {
        None
    }
}

/// The task carrying `[reminder:: external_id]`, if a line has it already.
fn line_of_reminder(vault: &Vault, external_id: &str) -> Option<String> {
    vault
        .tasks()
        .find(|found| found.task.field("reminder") == Some(external_id))
        .and_then(|found| found.task.id().map(str::to_owned))
}

/// A Reminders title as one task text: one line, without the brackets that
/// would read as inline fields.
fn reminder_text(title: &str) -> String {
    let text = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('[', "(")
        .replace(']', ")");
    if text.is_empty() {
        "Untitled reminder".to_owned()
    } else {
        text
    }
}

fn vault_today() -> Result<vault_base::Date, CoreError> {
    let today = Zoned::now().date();
    ops::parse_date(&today.to_string()).map_err(CoreError::from)
}

fn box_span(time_box: &TimeBox) -> Result<Span, CoreError> {
    Ok(Span::starting_at(time_box.start, time_box.duration)?)
}

fn instant(at: DateTime) -> Option<Timestamp> {
    at.to_zoned(TimeZone::system())
        .ok()
        .map(|zoned| zoned.timestamp())
}

/// Minutes since `date`'s midnight; the next midnight is 1440.
fn minute_on(date: Date, at: DateTime) -> u32 {
    if at.date() > date {
        return 24 * 60;
    }
    minute_of(at)
}

fn minute_of(at: DateTime) -> u32 {
    u32::from(at.hour().unsigned_abs()) * 60 + u32::from(at.minute().unsigned_abs())
}

fn calendar_error(error: CalendarError) -> CoreError {
    match error {
        CalendarError::Unavailable { reason } => CoreError::Conflict(reason),
        CalendarError::UnknownCalendar { name } => {
            CoreError::Conflict(format!("There is no writable calendar named {name}"))
        }
        CalendarError::TimeBoxNotFound { calendar, id } => {
            CoreError::NotFound(format!("Calendar {calendar} holds no event for {id}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use domain::session::Session;

    use super::*;
    use crate::app_state::tests::Keychain;
    use crate::app_state::{create_workspace_state, load_device_id};
    use crate::calendar::{CalendarEvent, CalendarFailure, CalendarTimeBox};
    use crate::files::plain_sync_files;
    use crate::open_workspace;

    const DAY: &str = "2026-10-22";
    const ICLOUD_OFF: &str = "iCloud Calendar is off on this Mac";

    /// The platform port as the Mac app's source behaves: it writes only to
    /// the private calendar, and can be told to refuse as with iCloud off.
    #[derive(Default)]
    struct FakePort {
        written: Mutex<Vec<(String, String, CalendarTimeBox)>>,
        deleted: Mutex<Vec<(String, String)>>,
        refusal: Option<String>,
        /// The private calendar is gone, as when iCloud Calendar was turned off.
        is_calendar_gone: bool,
    }

    impl CalendarPort for FakePort {
        fn events(
            &self,
            _from: String,
            _to: String,
        ) -> Result<Vec<CalendarEvent>, CalendarFailure> {
            Ok(Vec::new())
        }

        fn write_time_box(
            &self,
            calendar: String,
            title: String,
            time_box: CalendarTimeBox,
        ) -> Result<(), CalendarFailure> {
            if let Some(reason) = &self.refusal {
                return Err(CalendarFailure::Unavailable {
                    reason: reason.clone(),
                });
            }
            if calendar != PRIVATE_CALENDAR {
                return Err(CalendarFailure::UnknownCalendar { name: calendar });
            }
            self.written
                .lock()
                .unwrap()
                .push((calendar, title, time_box));
            Ok(())
        }

        fn delete_time_box(&self, calendar: String, id: String) -> Result<(), CalendarFailure> {
            if self.is_calendar_gone {
                return Err(CalendarFailure::UnknownCalendar { name: calendar });
            }
            self.deleted.lock().unwrap().push((calendar, id));
            Ok(())
        }
    }

    fn sandbox() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        copy_dir(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mock-vault"),
            temp.path(),
        );
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

    fn open(root: &Path) -> (Arc<WorkspaceHandle>, Arc<WorkspaceStateHandle>) {
        let keychain = Arc::new(Keychain::default());
        let device = load_device_id(keychain.clone()).unwrap();
        let state = create_workspace_state(
            plain_sync_files(),
            root.to_string_lossy().into_owned(),
            "mock-studio".to_owned(),
            device,
            keychain,
        )
        .unwrap();
        (
            open_workspace(root.to_string_lossy().into_owned(), Vec::new()).unwrap(),
            state,
        )
    }

    fn local(hour: i8, minute: i8) -> Zoned {
        parse_date(DAY)
            .unwrap()
            .at(hour, minute, 0, 0)
            .to_zoned(TimeZone::system())
            .unwrap()
    }

    fn event(title: &str, from: (i8, i8), to: (i8, i8)) -> Event {
        let span = Span::new(
            local(from.0, from.1).datetime(),
            local(to.0, to.1).datetime(),
        )
        .unwrap();
        Event::new(format!("ev-{}", from.0), "Work", title, span)
    }

    fn port_event(title: &str, from: (i8, i8), to: (i8, i8)) -> CalendarEvent {
        let text = |at: (i8, i8)| {
            local(at.0, at.1)
                .datetime()
                .strftime("%Y-%m-%dT%H:%M:%S")
                .to_string()
        };
        CalendarEvent {
            id: format!("ev-{}", from.0),
            calendar: "Work".to_owned(),
            title: title.to_owned(),
            start: text(from),
            end: text(to),
            meeting_key: Some(format!("uid-{title}")),
            is_attended: true,
        }
    }

    fn store_session(
        state: &WorkspaceStateHandle,
        id: &str,
        target: SessionTarget,
        from: (i8, i8),
        to: (i8, i8),
    ) {
        let mut session = Session::start(id, target, local(from.0, from.1).timestamp());
        session.stop(local(to.0, to.1).timestamp()).unwrap();
        lock(&state.sync)
            .edit(Instant::now(), |doc| doc.put_session(&session.to_record()))
            .unwrap();
    }

    #[test]
    fn a_pin_writes_only_the_private_calendar_then_stores_the_box_locked() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let port = Arc::new(FakePort::default());

        let pinned = state
            .pin_at(
                &workspace,
                port.clone(),
                DAY.to_owned(),
                "task-2".to_owned(),
                9 * 60 + 30,
            )
            .unwrap();
        assert!(pinned.is_locked);
        assert_eq!(
            (pinned.start_minute, pinned.minutes),
            (570, 120),
            "the 2h estimate"
        );
        let written = port.written.lock().unwrap().clone();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "Vaultday");
        assert_eq!(written[0].1, "Wire the payment form to the gateway sandbox");
        assert_eq!(written[0].2.start, "2026-10-22T09:30:00");
        assert_eq!(state.time_boxes(DAY.to_owned()).unwrap(), vec![pinned]);

        let clash = state.pin_at(
            &workspace,
            port.clone(),
            DAY.to_owned(),
            "task-5".to_owned(),
            10 * 60,
        );
        assert!(matches!(clash, Err(CoreError::Conflict(_))), "{clash:?}");
        assert_eq!(
            port.written.lock().unwrap().len(),
            1,
            "a refused pin writes nothing"
        );
    }

    #[test]
    fn a_refused_calendar_write_leaves_no_locked_box() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let port = Arc::new(FakePort {
            refusal: Some(ICLOUD_OFF.to_owned()),
            ..FakePort::default()
        });
        let refused = state.pin_at(&workspace, port, DAY.to_owned(), "task-2".to_owned(), 600);
        match refused {
            Err(CoreError::Conflict(message)) => assert_eq!(message, ICLOUD_OFF),
            other => panic!("expected the decided message, got {other:?}"),
        }
        assert!(state.time_boxes(DAY.to_owned()).unwrap().is_empty());
    }

    #[test]
    fn unpinning_deletes_the_event_before_the_box_floats() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let port = Arc::new(FakePort::default());
        let missing = state.toggle_pin(
            &workspace,
            port.clone(),
            DAY.to_owned(),
            "task-2".to_owned(),
        );
        assert!(matches!(missing, Err(CoreError::NotFound(_))));

        let pinned = state
            .pin_at(
                &workspace,
                port.clone(),
                DAY.to_owned(),
                "task-2".to_owned(),
                600,
            )
            .unwrap();
        let unpinned = state
            .toggle_pin(
                &workspace,
                port.clone(),
                DAY.to_owned(),
                "task-2".to_owned(),
            )
            .unwrap();
        assert!(!unpinned.is_locked);
        assert_eq!(
            unpinned.start_minute, pinned.start_minute,
            "the box keeps its slot"
        );
        assert_eq!(
            port.deleted.lock().unwrap().clone(),
            vec![("Vaultday".to_owned(), pinned.id.clone())]
        );

        let again = state
            .toggle_pin(
                &workspace,
                port.clone(),
                DAY.to_owned(),
                "task-2".to_owned(),
            )
            .unwrap();
        assert!(again.is_locked);
        assert_eq!(
            port.written.lock().unwrap().len(),
            2,
            "pinned again, written again"
        );
    }

    #[test]
    fn a_box_dragged_to_another_day_moves_with_its_event() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let port = Arc::new(FakePort::default());
        let monday = state
            .pin_at(
                &workspace,
                port.clone(),
                "2026-10-19".to_owned(),
                "task-2".to_owned(),
                600,
            )
            .unwrap();
        let wednesday = state
            .pin_at(
                &workspace,
                port.clone(),
                "2026-10-21".to_owned(),
                "task-2".to_owned(),
                840,
            )
            .unwrap();
        assert_eq!(
            wednesday.id, monday.id,
            "the same box, so the same event moves"
        );
        assert!(
            state
                .time_boxes("2026-10-19".to_owned())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            state.time_boxes("2026-10-21".to_owned()).unwrap(),
            vec![wednesday]
        );
        let written = port.written.lock().unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(written[1].2.id, monday.id);
        assert_eq!(written[1].2.start, "2026-10-21T14:00:00");
    }

    #[test]
    fn a_pin_whose_private_calendar_is_gone_can_still_be_unpinned() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        state
            .pin_at(
                &workspace,
                Arc::new(FakePort::default()),
                DAY.to_owned(),
                "task-2".to_owned(),
                600,
            )
            .unwrap();
        let gone = Arc::new(FakePort {
            is_calendar_gone: true,
            ..FakePort::default()
        });
        let unpinned = state
            .toggle_pin(&workspace, gone, DAY.to_owned(), "task-2".to_owned())
            .unwrap();
        assert!(!unpinned.is_locked);
    }

    #[test]
    fn the_day_grid_places_events_and_boxes_with_the_tracked_fill() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let port = Arc::new(FakePort::default());
        state
            .pin_at(
                &workspace,
                port,
                DAY.to_owned(),
                "task-2".to_owned(),
                9 * 60 + 30,
            )
            .unwrap();
        store_session(
            &state,
            "ses-1",
            SessionTarget::task("task-2"),
            (9, 30),
            (10, 17),
        );
        let events = [event("Team stand-up", (9, 0), (9, 15))];

        let grid = state
            .calendar_day_at(
                &workspace,
                &events,
                parse_date(DAY).unwrap(),
                &local(10, 17),
            )
            .unwrap();
        assert_eq!(grid.now_minute, Some(617));
        assert!(!grid.is_past);
        let summary: Vec<(CalendarBlockKind, &str, u32, u32, &str)> = grid
            .blocks
            .iter()
            .map(|block| {
                (
                    block.kind,
                    block.title.as_str(),
                    block.start_minute,
                    block.end_minute,
                    block.detail.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    CalendarBlockKind::Event,
                    "Team stand-up",
                    540,
                    555,
                    "09:00 – 09:15 · Work"
                ),
                (
                    CalendarBlockKind::LockedTask,
                    "Wire the payment form to the gateway sandbox",
                    570,
                    690,
                    "09:30 – 11:30 · locked · tracked 0:47"
                ),
            ]
        );
        assert_eq!(grid.blocks[1].tracked_permille, 391, "47 of 120 minutes");

        let next_day = state
            .calendar_day_at(
                &workspace,
                &[],
                parse_date("2026-10-21").unwrap(),
                &local(10, 17),
            )
            .unwrap();
        assert!(next_day.is_past && next_day.now_minute.is_none());
    }

    #[test]
    fn ended_events_become_meeting_sessions_once() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let events = [
            port_event("WEB-142 design review", (9, 0), (10, 0)),
            port_event("Lunch with the team", (12, 0), (13, 0)),
        ];
        let now = local(11, 0).timestamp();
        assert_eq!(
            state.record_meetings_at(&workspace, &events, now).unwrap(),
            1
        );
        assert_eq!(
            state.record_meetings_at(&workspace, &events, now).unwrap(),
            0
        );
        let sessions = lock(&state.sync).state().sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].target.story, Some(StoryNumber::new("WEB-142")));
        assert!(sessions[0].id.as_str().starts_with("mtg-"));
        assert_eq!(
            state
                .record_meetings_at(&workspace, &events, local(13, 0).timestamp())
                .unwrap(),
            1,
            "lunch has ended by 13:00"
        );
    }

    #[test]
    fn declined_and_free_events_are_no_meeting() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let events = [CalendarEvent {
            is_attended: false,
            ..port_event("Declined planning", (9, 0), (10, 0))
        }];
        assert_eq!(
            state
                .record_meetings_at(&workspace, &events, local(11, 0).timestamp())
                .unwrap(),
            0
        );
        assert!(lock(&state.sync).state().sessions().is_empty());
    }

    #[test]
    fn a_renamed_or_moved_meeting_keeps_its_one_session() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let now = local(12, 0).timestamp();
        let first = port_event("Design review", (9, 0), (10, 0));
        assert_eq!(
            state
                .record_meetings_at(&workspace, std::slice::from_ref(&first), now)
                .unwrap(),
            1
        );
        let renamed_and_moved = CalendarEvent {
            title: "Design review (moved)".to_owned(),
            ..port_event("Design review", (10, 0), (11, 0))
        };
        assert_eq!(
            state
                .record_meetings_at(
                    &workspace,
                    &[CalendarEvent {
                        meeting_key: first.meeting_key.clone(),
                        ..renamed_and_moved
                    }],
                    now
                )
                .unwrap(),
            0
        );
        assert_eq!(lock(&state.sync).state().sessions().len(), 1);
    }

    #[test]
    fn story_hours_keep_tracked_logged_and_remaining_apart() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        // Tracked through its task only: real time, not logged to the story.
        store_session(
            &state,
            "ses-1",
            SessionTarget::task("task-2"),
            (9, 0),
            (10, 0),
        );
        store_session(
            &state,
            "ses-2",
            SessionTarget::task_in_story("task-5", "WEB-142"),
            (10, 0),
            (10, 30),
        );

        let hours = state
            .story_hours_at(&workspace, "WEB-142", local(12, 0).timestamp())
            .unwrap();
        assert_eq!(hours.tracked_minutes, 90);
        assert_eq!(hours.logged_minutes, 30);
        assert_eq!(
            hours.estimate_minutes,
            Some(3 * 480),
            "3d at hours_per_day 8.0"
        );
        assert_eq!(
            hours.remaining_minutes,
            3 * 480 - 30,
            "the estimate less the logged time"
        );
        assert_eq!(hours.overtime_minutes, 0);
        assert_eq!(hours.budget, BudgetSourceView::Story);
        assert!(matches!(
            state.story_hours_at(&workspace, "WEB-999", local(12, 0).timestamp()),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn hours_show_tracked_estimate_and_remaining_per_open_story() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        store_session(
            &state,
            "ses-1",
            SessionTarget::task("task-2"),
            (9, 0),
            (10, 0),
        );
        store_session(
            &state,
            "ses-2",
            SessionTarget::story("WEB-157"),
            (10, 0),
            (10, 30),
        );

        let hours = state
            .hours_at(
                &workspace,
                parse_date(DAY).unwrap(),
                local(12, 0).timestamp(),
            )
            .unwrap();
        assert_eq!(hours.target_minutes, 480, "hours_per_day 8.0");
        assert_eq!(hours.tracked_minutes, 90);
        let rows: Vec<(&str, u32, Option<u32>, u32, BudgetSourceView)> = hours
            .stories
            .iter()
            .map(|row| {
                (
                    row.number.as_str(),
                    row.tracked_minutes,
                    row.estimate_minutes,
                    row.remaining_minutes,
                    row.budget,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("WEB-142", 60, Some(1440), 1380, BudgetSourceView::Story),
                ("WEB-157", 30, Some(480), 450, BudgetSourceView::Story),
            ],
            "task-2's time counts for its story; ACME-12 is done and ACME-9 has neither budget nor time"
        );
    }

    fn reminder(id: &str, title: &str) -> ReminderItemView {
        ReminderItemView {
            external_id: id.to_owned(),
            title: title.to_owned(),
            due_date: None,
            has_due_time: false,
            is_completed: false,
            is_recurring: false,
            has_location_alarm: false,
        }
    }

    #[test]
    fn a_reminder_becomes_one_task_line_even_when_the_import_is_retried() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let item = ReminderItemView {
            due_date: Some("2026-10-30".to_owned()),
            ..reminder("rem-1", "Ask Lee about\nthe [provider]")
        };
        let first = state
            .import_reminder(
                &workspace,
                item.clone(),
                Some("WEB-157".to_owned()),
                Some(DAY.to_owned()),
            )
            .unwrap();
        let ReminderIntake::Imported { task_id, note } = first else {
            panic!("expected an import, got {first:?}");
        };
        let text = fs::read_to_string(sandbox.path().join(&note)).unwrap();
        let line = text
            .lines()
            .find(|line| line.contains("[reminder:: rem-1]"))
            .unwrap();
        assert_eq!(
            line,
            format!(
                "- [ ] Ask Lee about the (provider) [id:: {task_id}] [created:: 2026-10-22] [due:: 2026-10-30] [reminder:: rem-1]"
            )
        );

        // The app quit before Reminders deleted the item: the retry finds the line.
        let retry = state
            .import_reminder(
                &workspace,
                item,
                Some("WEB-157".to_owned()),
                Some(DAY.to_owned()),
            )
            .unwrap();
        assert_eq!(retry, ReminderIntake::AlreadyImported { task_id });
        let again = fs::read_to_string(sandbox.path().join(&note)).unwrap();
        assert_eq!(again.matches("rem-1").count(), 1);
    }

    #[test]
    fn a_list_without_a_story_files_into_the_intake_note() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let item = ReminderItemView {
            due_date: Some("2026-10-24".to_owned()),
            has_due_time: true,
            ..reminder("rem-2", "Call the bank")
        };
        let imported = state
            .import_reminder(&workspace, item, None, Some(DAY.to_owned()))
            .unwrap();
        assert!(
            matches!(&imported, ReminderIntake::Imported { note, .. } if note == REMINDERS_INTAKE_NOTE)
        );
        let text = fs::read_to_string(sandbox.path().join(REMINDERS_INTAKE_NOTE)).unwrap();
        assert!(text.contains("Call the bank [id:: "));
        assert!(
            text.contains("[scheduled:: 2026-10-24] [reminder:: rem-2]"),
            "a time makes it scheduled: {text}"
        );

        let unknown = state.import_reminder(
            &workspace,
            reminder("rem-3", "x"),
            Some("WEB-999".to_owned()),
            None,
        );
        assert!(matches!(unknown, Err(CoreError::NotFound(_))));
    }

    #[test]
    fn a_batch_files_every_item_from_one_read_of_the_vault() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let batch = vec![
            ReminderImport {
                item: reminder("rem-10", "First"),
                story: None,
            },
            ReminderImport {
                item: reminder("rem-11", "Second"),
                story: Some("WEB-157".to_owned()),
            },
            ReminderImport {
                item: reminder("rem-10", "First again"),
                story: None,
            },
            ReminderImport {
                item: ReminderItemView {
                    is_recurring: true,
                    ..reminder("rem-12", "Water the plants")
                },
                story: None,
            },
            ReminderImport {
                item: reminder("rem-13", "Lost"),
                story: Some("WEB-999".to_owned()),
            },
        ];
        let outcomes = state
            .import_reminders(&workspace, batch, Some(DAY.to_owned()))
            .unwrap();
        let ids: Vec<&str> = outcomes.iter().map(|o| o.external_id.as_str()).collect();
        assert_eq!(ids, ["rem-10", "rem-11", "rem-10", "rem-12", "rem-13"]);
        let Some(ReminderIntake::Imported { task_id: first, .. }) = &outcomes[0].intake else {
            panic!("{:?}", outcomes[0]);
        };
        let Some(ReminderIntake::Imported {
            task_id: second, ..
        }) = &outcomes[1].intake
        else {
            panic!("{:?}", outcomes[1]);
        };
        assert_ne!(first, second, "two ids minted from one read stay apart");
        assert_eq!(
            outcomes[2].intake,
            Some(ReminderIntake::AlreadyImported {
                task_id: first.clone()
            })
        );
        assert!(matches!(
            outcomes[3].intake,
            Some(ReminderIntake::Left { .. })
        ));
        assert!(outcomes[4].error.is_some() && outcomes[4].intake.is_none());
        let intake = fs::read_to_string(sandbox.path().join(REMINDERS_INTAKE_NOTE)).unwrap();
        assert_eq!(intake.matches("rem-10").count(), 1);
    }

    #[test]
    fn completed_recurring_and_place_based_items_stay_in_reminders() {
        let sandbox = sandbox();
        let (workspace, state) = open(sandbox.path());
        let left = [
            ReminderItemView {
                is_completed: true,
                ..reminder("a", "Done already")
            },
            ReminderItemView {
                is_recurring: true,
                ..reminder("b", "Water the plants")
            },
            ReminderItemView {
                has_location_alarm: true,
                ..reminder("c", "Buy milk")
            },
        ];
        for item in left {
            let outcome = state.import_reminder(&workspace, item, None, None).unwrap();
            assert!(
                matches!(outcome, ReminderIntake::Left { .. }),
                "{outcome:?}"
            );
        }
        assert!(
            !sandbox.path().join(REMINDERS_INTAKE_NOTE).exists(),
            "nothing was written"
        );
    }
}
