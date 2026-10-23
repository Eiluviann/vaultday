//! The week close-out and the leave calendar over the bridge: the week's
//! working days as a dry run, the close-out of a week or month with its four
//! steps, the allocation's adjustments, the ticks, the closing line, the
//! Monday alert and the two settings it needs (`log_deadline`, `leave_source`).
//!
//! The period's work is read here: the stories touched each day from the
//! sessions, their budgets from the vault, the stopped sessions of an hourly
//! or fixed-price week. Nothing is sent to Toggl: the Toggl step queues into
//! the workspace document through the feature set, only while it is on.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_state::{AppStateError, WorkspaceState, WorkspaceSync};
use close_out::{
    CloseOut, CloseOutContext, CloseOutError, CloseOutModel, LeaveCandidate, PeriodWork, StepKind,
    StepStatus, StoryFacts, Touch,
};
use domain::allocation::{Allotment, StoryDay, allocate_day};
use domain::estimate;
use domain::ports::Clock;
use domain::quick_add;
use domain::session::{Session, SessionRecord, SessionSource};
use domain::task::{StoryNumber, TaskId};
use domain::week::{DayKind, DaysOff, Period, PeriodKind};
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use vault_base::{
    LEAVE_NOTE_NAME, LeaveNote, LogDeadline, PlainFileAccess, SettingsWeekday, VaultWriter,
    Workspace, WorkspaceKind, WorkspaceSettings, ops, update_settings_file,
};
use vaultday_feature_toggl::{
    FlatDayStatus, TogglError, TogglFeature, TogglSettings, WorkItemFacts, WorkSession, feature_id,
    work_item_key,
};

use crate::app_state::{AppStateHandle, WorkspaceStateHandle, lock};
use crate::home::{local_midnight, settled_session_book, whole_minutes};
use crate::preferences::WeekdayView;
use crate::timer::{ForeignNotifier, NotifierPort};
use crate::toggl::{
    TogglEnqueueView, current_settings, enabled, enqueue_record, meeting_or_story, task_facts,
    toggl_error,
};
use crate::{CoreError, WorkspaceHandle};

/// Week or month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PeriodKindView {
    /// Monday to Friday.
    Week,
    /// A calendar month.
    Month,
}

/// What a day of a period is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DayKindView {
    /// A working day.
    Working,
    /// Leave.
    Leave,
    /// A public holiday.
    Holiday,
    /// Saturday or Sunday.
    Weekend,
}

/// One day of a period.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PeriodDayView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// What it is.
    pub kind: DayKindView,
}

/// A line of `Leave.md` that was ignored.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LeaveWarningView {
    /// The 1-based line.
    pub line: u32,
    /// The line as written.
    pub text: String,
    /// Why it was ignored.
    pub reason: String,
}

/// The dry run of a week's working days.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkingWeekView {
    /// Monday to Friday with what each is.
    pub days: Vec<PeriodDayView>,
    /// The working days among them.
    pub working_days: Vec<String>,
    /// Whether a `Leave.md` was found.
    pub has_leave_note: bool,
    /// Lines of `Leave.md` that were ignored.
    pub warnings: Vec<LeaveWarningView>,
}

/// A workspace the leave note may come from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LeaveCandidateView {
    /// Its name.
    pub name: String,
    /// Whether it is a life workspace.
    pub is_life: bool,
    /// Its root folder.
    pub root: String,
}

/// Which close-out is asked for.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutRequest {
    /// The workspace's id.
    pub workspace_id: String,
    /// Week or month.
    pub kind: PeriodKindView,
    /// A day inside the period, `YYYY-MM-DD`.
    pub date: String,
    /// The root of the workspace holding `Leave.md`; `None` without one.
    pub leave_root: Option<String>,
    /// Days adjusted and not confirmed yet.
    pub draft: Vec<DraftDayView>,
}

/// One story's minutes on a day.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AllotmentView {
    /// The story number.
    pub story: String,
    /// Whole minutes.
    pub minutes: u32,
}

/// One day of a draft or a confirmation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DraftDayView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// The day's minutes per story.
    pub allotments: Vec<AllotmentView>,
}

/// One story worked on a day, for [`allocate_day_minutes`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryDayView {
    /// The story number.
    pub story: String,
    /// Minutes tracked that day.
    pub tracked_minutes: u32,
    /// Minutes of its estimate left.
    pub remaining_minutes: u32,
}

/// A story's row on one day of the allocation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AllocationRowView {
    /// The story number.
    pub story: String,
    /// Its title.
    pub title: String,
    /// Minutes allocated.
    pub minutes: u32,
    /// Minutes tracked, private.
    pub tracked_minutes: u32,
    /// Whether the story has an estimate; without one it gets no minutes
    /// and the ritual flags it.
    pub has_estimate: bool,
}

/// One working day of the allocation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AllocationDayView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// The day's target in minutes.
    pub target_minutes: u32,
    /// The stories touched that day.
    pub rows: Vec<AllocationRowView>,
    /// Whether it is confirmed.
    pub is_confirmed: bool,
    /// Whether the user adjusted it and has not confirmed yet.
    pub is_draft: bool,
}

/// What happens to a day's flat entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FlatLineStateView {
    /// Not queued; while Toggl is off this is the dry run.
    NotQueued,
    /// Waiting to be sent.
    Queued,
    /// Toggl holds it.
    Sent,
    /// Failed and waits for the user.
    Failed,
    /// Leave: no entry.
    Leave,
    /// A public holiday: no entry.
    Holiday,
}

/// One day of the flat week.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlatLineView {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// What happens to its entry.
    pub state: FlatLineStateView,
    /// The entry's length.
    pub seconds: u64,
}

/// A review row of an hourly or fixed-price week.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReviewRowView {
    /// The work item.
    pub key: String,
    /// Its story.
    pub story: Option<String>,
    /// Its title.
    pub title: String,
    /// Minutes tracked.
    pub tracked_minutes: u32,
    /// Minutes billed.
    pub billed_minutes: u32,
    /// Fixed price: the unpaid overrun in minutes; 0 for hourly.
    pub overrun_minutes: u32,
    /// Fixed price: the estimate in minutes.
    pub estimate_minutes: Option<u32>,
    /// Hourly: whether it is billable; fixed price: true.
    pub is_billable: bool,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutRowView {
    /// What it stands for; ticks are kept by it.
    pub key: String,
    /// One cell per column.
    pub cells: Vec<String>,
    /// Whether it is ticked as entered.
    pub is_ticked: bool,
}

/// The copyable table of the table step.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutTableView {
    /// The sink the ticks are kept for.
    pub sink: String,
    /// Its title.
    pub title: String,
    /// Column headers.
    pub columns: Vec<String>,
    /// The rows.
    pub rows: Vec<CloseOutRowView>,
    /// The table as the sink takes it: plain TSV.
    pub text: String,
}

/// Which step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StepKindView {
    /// Entries to Toggl.
    Toggl,
    /// FTE: the allocation.
    Allocate,
    /// Hourly and fixed price: the review.
    Review,
    /// The copyable table.
    Table,
    /// The closing log line.
    Close,
}

/// Where a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StepStatusView {
    /// Something is left to do.
    Open,
    /// Done.
    Done,
    /// Nothing to do: Toggl is off or nothing is billed.
    Skipped,
}

/// A step.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StepView {
    /// Which step.
    pub kind: StepKindView,
    /// Its title.
    pub title: String,
    /// Where it stands.
    pub status: StepStatusView,
}

/// Which steps the close-out has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CloseOutModelView {
    /// FTE, or no billing model.
    Fte,
    /// Hourly.
    Hourly,
    /// Fixed price.
    Fixed,
}

/// The close-out of a week or month.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutView {
    /// `week-2026-10-19` or `month-2026-10`.
    pub period_id: String,
    /// Week or month.
    pub kind: PeriodKindView,
    /// The first day.
    pub start: String,
    /// The last day.
    pub end: String,
    /// Which steps.
    pub model: CloseOutModelView,
    /// When it becomes due, `YYYY-MM-DDTHH:MM`.
    pub opens_at: String,
    /// The deadline, `YYYY-MM-DDTHH:MM`.
    pub deadline: String,
    /// Every day of the period.
    pub days: Vec<PeriodDayView>,
    /// Whether the Toggl feature is on.
    pub is_toggl_on: bool,
    /// FTE: the flat days.
    pub flat: Vec<FlatLineView>,
    /// Hourly and fixed price: sessions not queued.
    pub unqueued_sessions: u32,
    /// FTE: the allocation.
    pub allocation: Vec<AllocationDayView>,
    /// Hourly and fixed price: the review rows.
    pub review: Vec<ReviewRowView>,
    /// The table.
    pub table: CloseOutTableView,
    /// The four steps.
    pub steps: Vec<StepView>,
    /// What is missing, as lines.
    pub missing: Vec<String>,
    /// Whether every step but the last is done.
    pub can_close: bool,
    /// Whether the period is closed.
    pub is_closed: bool,
}

/// What the header's ritual button offers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutDueView {
    /// The period to open.
    pub kind: PeriodKindView,
    /// A day inside it.
    pub date: String,
    /// "Week close-out" or "Month close-out".
    pub title: String,
    /// The deadline, `YYYY-MM-DDTHH:MM`.
    pub deadline: String,
    /// Whether the deadline has passed.
    pub is_overdue: bool,
}

/// What closing did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutClosedView {
    /// The line appended; `None` when it was closed before.
    pub line: Option<String>,
    /// The day log it went to, `YYYY-MM-DD`.
    pub date: String,
}

/// The close-out keys of a workspace's `settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloseOutSettingsView {
    /// The deadline's weekday; `None` turns the close-out off.
    pub deadline_weekday: Option<WeekdayView>,
    /// The deadline's time, `HH:MM`.
    pub deadline_time: String,
    /// The workspace whose `Leave.md` counts; `None` for the life workspace.
    pub leave_source: Option<String>,
}

/// The week holding `date` with its working days, leave and holidays read
/// from `leave_root`'s `Leave.md` (none without one). Changes nothing.
///
/// # Errors
/// [`CoreError::InvalidDate`] for a malformed day, [`CoreError::FileAccess`]
/// when the note cannot be read.
#[uniffi::export]
pub fn working_week(
    date: String,
    leave_root: Option<String>,
) -> Result<WorkingWeekView, CoreError> {
    let period = Period::week_containing(parse_day(&date)?)?;
    let note = read_leave_note(leave_root.as_deref())?;
    let off = note.as_ref().map(close_out::days_off).unwrap_or_default();
    Ok(WorkingWeekView {
        days: day_views(&period, &off),
        working_days: period
            .working_days(&off)
            .iter()
            .map(ToString::to_string)
            .collect(),
        has_leave_note: note.is_some(),
        warnings: note
            .map(|note| {
                note.warnings()
                    .iter()
                    .map(|warning| LeaveWarningView {
                        line: u32::try_from(warning.line).unwrap_or(u32::MAX),
                        text: warning.text.clone(),
                        reason: warning.reason.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// The root whose `Leave.md` counts for the workspace at `root`: the one its
/// `leave_source` names, else the first life workspace among `candidates`.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] or [`CoreError::InvalidSettings`] as the manifest read fails.
#[uniffi::export]
pub fn resolve_leave_root(
    root: String,
    candidates: Vec<LeaveCandidateView>,
) -> Result<Option<String>, CoreError> {
    let settings = open_workspace(&root)?.settings;
    let candidates: Vec<LeaveCandidate> = candidates
        .into_iter()
        .map(|candidate| LeaveCandidate {
            name: candidate.name,
            kind: if candidate.is_life {
                WorkspaceKind::Life
            } else {
                WorkspaceKind::Work
            },
            root: candidate.root,
        })
        .collect();
    Ok(close_out::leave_source(&settings, &candidates).map(|candidate| candidate.root.clone()))
}

/// The allocation of `target_minutes` over `stories`, by the close-out's rules.
#[uniffi::export]
pub fn allocate_day_minutes(target_minutes: u32, stories: Vec<StoryDayView>) -> Vec<AllotmentView> {
    let stories: Vec<StoryDay> = stories
        .into_iter()
        .map(|story| StoryDay {
            story: StoryNumber::new(story.story),
            tracked_minutes: story.tracked_minutes,
            remaining_minutes: story.remaining_minutes,
        })
        .collect();
    allocate_day(target_minutes, &stories)
        .iter()
        .map(allotment_view)
        .collect()
}

/// Reads the close-out keys of the folder's `settings.json`.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] or [`CoreError::InvalidSettings`] as the read fails.
#[uniffi::export]
pub fn read_close_out_settings(root: String) -> Result<CloseOutSettingsView, CoreError> {
    Ok(settings_view(&open_workspace(&root)?.settings))
}

/// Writes the close-out keys, keeping every other key.
///
/// # Errors
/// [`CoreError::InvalidSettings`] for a malformed time or an empty source name.
#[uniffi::export]
pub fn update_close_out_settings(
    root: String,
    settings: CloseOutSettingsView,
) -> Result<CloseOutSettingsView, CoreError> {
    let written = update_settings_file(Path::new(&root), &PlainFileAccess, |stored| {
        stored.log_deadline = settings.deadline_weekday.map(|weekday| LogDeadline {
            weekday: settings_weekday(weekday),
            time: settings.deadline_time.trim().to_owned(),
        });
        stored.leave_source = settings
            .leave_source
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
    })
    .map_err(|error| CoreError::InvalidSettings(error.to_string()))?;
    Ok(settings_view(&written))
}

/// What the header's ritual button offers at `now_local` (this Mac's time
/// when `None`, `YYYY-MM-DDTHH:MM` otherwise): the week that is due and not
/// closed, else the month; `None` without a `log_deadline` or with both closed.
///
/// # Errors
/// As [`close_out_state`].
#[uniffi::export(default(now_local = None))]
pub fn close_out_due(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    workspace_id: String,
    leave_root: Option<String>,
    now_local: Option<String>,
) -> Result<Option<CloseOutDueView>, CoreError> {
    let settings = current_settings(&workspace.workspace)?;
    if settings.log_deadline.is_none() {
        return Ok(None);
    }
    let (now, _) = local_now(now_local.as_deref())?;
    let off = read_leave_note(leave_root.as_deref())?
        .as_ref()
        .map(close_out::days_off)
        .unwrap_or_default();
    let candidates = [
        (
            PeriodKindView::Week,
            close_out::schedule::due_week(now).map_err(core_error)?,
        ),
        (
            PeriodKindView::Month,
            close_out::schedule::due_month(now, &off).map_err(core_error)?,
        ),
    ];
    for (kind, period) in candidates {
        let request = CloseOutRequest {
            workspace_id: workspace_id.clone(),
            kind,
            date: period.start().to_string(),
            leave_root: leave_root.clone(),
            draft: Vec::new(),
        };
        let close = with_close_out(app, state, workspace, &request, |state, ctx| {
            close_out::close_out(state, ctx)
        })?;
        if close.is_closed() {
            continue;
        }
        return Ok(Some(CloseOutDueView {
            kind,
            date: period.start().to_string(),
            title: match kind {
                PeriodKindView::Week => "Week close-out".to_owned(),
                PeriodKindView::Month => "Month close-out".to_owned(),
            },
            deadline: minute_text(close.deadline),
            is_overdue: now >= close.deadline,
        }));
    }
    Ok(None)
}

/// The close-out of the requested period as it stands.
///
/// # Errors
/// [`CoreError::Conflict`] without a `log_deadline`, [`CoreError::InvalidDate`]
/// for a malformed day, [`CoreError::FileAccess`] when the vault cannot be read.
#[uniffi::export]
pub fn close_out_state(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
) -> Result<CloseOutView, CoreError> {
    let close = with_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::close_out(state, ctx)
    })?;
    Ok(close_out_view(&close))
}

/// Step 1: queues the week's Toggl entries through the feature set, the
/// flat days of an FTE workspace or the unqueued sessions otherwise. Sends
/// nothing. Returns the entries queued.
///
/// # Errors
/// [`CoreError::Conflict`] while the Toggl feature is off, or as [`close_out_state`].
#[uniffi::export]
pub fn close_out_queue_toggl(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
) -> Result<u32, CoreError> {
    let workspace_id = WorkspaceId::new(request.workspace_id.clone());
    let Some(feature) = enabled(app, &workspace_id)? else {
        return Err(CoreError::Conflict(
            "Toggl is off for this workspace; the step shows a dry run".to_owned(),
        ));
    };
    let settings = current_settings(&workspace.workspace)?;
    if close_out::model_of(&settings) == CloseOutModel::Fte {
        let start_of_day = |day: Date| -> Result<Timestamp, TogglError> {
            day.to_zoned(TimeZone::system())
                .map(|zoned| zoned.timestamp())
                .map_err(|error| TogglError::InvalidInput(error.to_string()))
        };
        let queued = edit_close_out(app, state, workspace, &request, |state, ctx| {
            close_out::queue_flat_entries(state, ctx, &feature, &start_of_day)
        })?;
        return Ok(u32::try_from(queued.len()).unwrap_or(u32::MAX));
    }
    let close = with_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::close_out(state, ctx)
    })?;
    let mut sync = lock(&state.sync);
    let mut queued = 0_u32;
    for id in &close.unqueued_sessions {
        let Some(record) = sync
            .state()
            .session(&domain::session::SessionId::new(id.as_str()))
        else {
            continue;
        };
        if let TogglEnqueueView::Queued { entries } =
            enqueue_record(&feature, &mut sync, &workspace.workspace, &record)?
        {
            queued += entries;
        }
    }
    Ok(queued)
}

/// The draft after the user set `story` on `date` to `minutes`; the rest of
/// the day is re-balanced to keep its total.
///
/// # Errors
/// [`CoreError::InvalidInput`] for a day or story outside the allocation, or
/// as [`close_out_state`].
#[uniffi::export]
pub fn close_out_rebalance(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
    date: String,
    story: String,
    minutes: u32,
) -> Result<Vec<DraftDayView>, CoreError> {
    let date = parse_day(&date)?;
    let draft = with_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::rebalance(state, ctx, date, &StoryNumber::new(story.as_str()), minutes)
    })?;
    Ok(draft_views(&draft))
}

/// Step 2 for FTE: confirms the allocation of the given days.
///
/// # Errors
/// [`CoreError::InvalidInput`] for a day that names a story not worked that
/// day or goes over its target or a budget, or as [`close_out_state`].
#[uniffi::export]
pub fn close_out_confirm_allocation(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
    days: Vec<DraftDayView>,
) -> Result<(), CoreError> {
    let days = draft_map(&days)?;
    edit_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::confirm_allocation(state, ctx, &days)
    })
}

/// Step 2 for hourly and fixed price: records the review.
///
/// # Errors
/// As [`close_out_state`].
#[uniffi::export]
pub fn close_out_confirm_review(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
) -> Result<(), CoreError> {
    edit_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::confirm_review(state, &ctx.period, Timestamp::now())
    })
}

/// Step 3: ticks or unticks a table row; kept in the workspace document.
///
/// # Errors
/// As [`close_out_state`].
#[uniffi::export]
pub fn close_out_tick(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
    sink: String,
    key: String,
    is_ticked: bool,
) -> Result<(), CoreError> {
    edit_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::set_row_ticked(state, &ctx.period, &sink, &key, is_ticked)
    })
}

/// Step 4: writes the `status` line "week closed" (or "month closed") to
/// the day log of the period's last working day, once.
///
/// # Errors
/// [`CoreError::Conflict`] while another step is open, [`CoreError::FileAccess`]
/// when the day log cannot be written, or as [`close_out_state`].
#[uniffi::export(default(now_local = None))]
pub fn close_out_close(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: CloseOutRequest,
    now_local: Option<String>,
) -> Result<CloseOutClosedView, CoreError> {
    let (local, now) = local_now(now_local.as_deref())?;
    let writer = VaultWriter::new(&PlainFileAccess, &workspace.workspace);
    let closed = edit_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::close(state, &writer, ctx, local, now)
    })?;
    Ok(CloseOutClosedView {
        line: closed.line,
        date: closed.date.to_string(),
    })
}

/// The Monday 07:30 alert: tells `notifier` once per week what the week
/// before still misses. `now_local` as in [`close_out_due`]. Returns
/// whether it told.
///
/// # Errors
/// As [`close_out_state`].
#[uniffi::export(default(now_local = None))]
pub fn close_out_check_alert(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    workspace_id: String,
    leave_root: Option<String>,
    notifier: Arc<dyn NotifierPort>,
    now_local: Option<String>,
) -> Result<bool, CoreError> {
    let settings = current_settings(&workspace.workspace)?;
    if settings.log_deadline.is_none() {
        return Ok(false);
    }
    let (local, now) = local_now(now_local.as_deref())?;
    let period = close_out::schedule::due_week(local).map_err(core_error)?;
    let request = CloseOutRequest {
        workspace_id,
        kind: PeriodKindView::Week,
        date: period.start().to_string(),
        leave_root,
        draft: Vec::new(),
    };
    let clock = LocalClock { now, local };
    let recorder = domain::ports::RecordingNotifier::new();
    let fired = edit_close_out(app, state, workspace, &request, |state, ctx| {
        close_out::run_alert(state, ctx, &clock, &recorder)
    })?;
    // The platform is told once the document lock is released.
    let port = ForeignNotifier(notifier.as_ref());
    for notification in recorder.sent() {
        crate::timer::forward(&port, &notification);
    }
    Ok(fired)
}

/// A clock standing at one instant, in this Mac's zone.
struct LocalClock {
    now: Timestamp,
    local: DateTime,
}

impl Clock for LocalClock {
    fn now(&self) -> Timestamp {
        self.now
    }

    fn today(&self) -> Date {
        self.local.date()
    }

    fn local_now(&self) -> DateTime {
        self.local
    }
}

/// Builds the context of `request` and runs `action` on a read of the document.
fn with_close_out<R>(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: &CloseOutRequest,
    action: impl FnOnce(&WorkspaceState, &CloseOutContext<'_>) -> Result<R, CloseOutError>,
) -> Result<R, CoreError> {
    let parts = Parts::gather(app, state, workspace, request)?;
    let sync = lock(&state.sync);
    let ctx = parts.context(&request.workspace_id);
    action(sync.state(), &ctx).map_err(core_error)
}

/// As [`with_close_out`], inside one edit of the document.
fn edit_close_out<R>(
    app: &AppStateHandle,
    state: &WorkspaceStateHandle,
    workspace: &WorkspaceHandle,
    request: &CloseOutRequest,
    action: impl FnOnce(&mut WorkspaceState, &CloseOutContext<'_>) -> Result<R, CloseOutError>,
) -> Result<R, CoreError> {
    let parts = Parts::gather(app, state, workspace, request)?;
    let mut sync = lock(&state.sync);
    let ctx = parts.context(&request.workspace_id);
    let mut failure = None;
    let result = sync.edit(Instant::now(), |state| {
        action(state, &ctx).map_err(|error| {
            let message = error.to_string();
            failure = Some(error);
            AppStateError::InvalidSetting(message)
        })
    });
    result.map_err(|error| failure.map_or_else(|| error.into(), core_error))
}

/// What a close-out context borrows.
struct Parts {
    period: Period,
    settings: WorkspaceSettings,
    off: DaysOff,
    work: PeriodWork,
    feature: Option<TogglFeature>,
    tags: Vec<String>,
    draft: BTreeMap<Date, Vec<Allotment>>,
}

impl Parts {
    fn gather(
        app: &AppStateHandle,
        state: &WorkspaceStateHandle,
        workspace: &WorkspaceHandle,
        request: &CloseOutRequest,
    ) -> Result<Parts, CoreError> {
        let settings = current_settings(&workspace.workspace)?;
        if settings.log_deadline.is_none() {
            return Err(CoreError::Conflict(
                "This workspace has no log_deadline, so it has no close-out".to_owned(),
            ));
        }
        let day = parse_day(&request.date)?;
        let period = match request.kind {
            PeriodKindView::Week => Period::week_containing(day)?,
            PeriodKindView::Month => Period::month_containing(day)?,
        };
        let off = read_leave_note(request.leave_root.as_deref())?
            .as_ref()
            .map(close_out::days_off)
            .unwrap_or_default();
        let feature = enabled(app, &WorkspaceId::new(request.workspace_id.clone()))?;
        let tags = TogglSettings::from_entry(settings.features.get(feature_id().as_str()))
            .map_err(toggl_error)?
            .tags;
        let work = {
            let mut sync = lock(&state.sync);
            period_work(&mut sync, &workspace.workspace, &settings, &period)?
        };
        Ok(Parts {
            period,
            settings,
            off,
            work,
            feature,
            tags,
            draft: draft_map(&request.draft)?,
        })
    }

    fn context<'a>(&'a self, workspace: &'a str) -> CloseOutContext<'a> {
        CloseOutContext {
            workspace,
            period: self.period,
            settings: &self.settings,
            days_off: &self.off,
            work: &self.work,
            toggl: self.feature.as_ref(),
            toggl_tags: &self.tags,
            draft: &self.draft,
        }
    }
}

/// The period's work: stories touched per day with their budgets, and the
/// stopped sessions with their work items' facts.
fn period_work(
    sync: &mut WorkspaceSync,
    workspace: &Workspace,
    settings: &WorkspaceSettings,
    period: &Period,
) -> Result<PeriodWork, CoreError> {
    let vault = ops::open_vault(workspace, &PlainFileAccess)?;
    let working_day =
        estimate::daily_target(settings.hours_per_day).unwrap_or(Duration::from_secs(8 * 3600));
    let mut task_stories: HashMap<TaskId, StoryNumber> = HashMap::new();
    let mut budgets: BTreeMap<StoryNumber, StoryFacts> = BTreeMap::new();
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
        let (budget, _) = estimate::story_budget(
            story
                .estimate
                .as_deref()
                .and_then(|text| estimate::parse_story_budget(text, working_day)),
            tasks
                .iter()
                .filter(|task| task.parent.is_none())
                .map(|task| task.field("estimate").and_then(quick_add::parse_estimate)),
        );
        budgets.insert(
            StoryNumber::new(number),
            StoryFacts {
                title: story.title.clone().unwrap_or_else(|| number.to_owned()),
                budget_minutes: budget.map(|budget| whole_minutes(budget.as_duration())),
            },
        );
    }
    let now = Timestamp::now();
    let book = settled_session_book(sync)?;
    let records: Vec<SessionRecord> = sync.state().sessions();
    let story_of = |record: &SessionRecord| -> Option<StoryNumber> {
        record.target.story.clone().or_else(|| {
            record
                .target
                .task
                .as_ref()
                .and_then(|task| task_stories.get(task).cloned())
        })
    };
    drop(book);

    let mut touched = Vec::new();
    for date in period.days(&DaysOff::default()).iter().map(|day| day.date) {
        let from = local_midnight(date)?;
        let to = local_midnight(date.tomorrow().map_err(crate::home::date_error)?)?;
        let until = to.min(now);
        let mut by_story: BTreeMap<StoryNumber, Duration> = BTreeMap::new();
        for record in &records {
            let Some(story) = story_of(record) else {
                continue;
            };
            let session = Session::from_record(record.clone())?;
            let inside = if until <= from {
                Duration::ZERO
            } else {
                session
                    .elapsed(until)
                    .as_duration()
                    .saturating_sub(session.elapsed(from).as_duration())
            };
            if !inside.is_zero() {
                *by_story.entry(story).or_default() += inside;
            }
        }
        touched.extend(by_story.into_iter().map(|(story, tracked)| Touch {
            date,
            story,
            tracked_minutes: whole_minutes(tracked),
        }));
    }

    let start = local_midnight(period.start())?;
    let end = local_midnight(period.end().tomorrow().map_err(crate::home::date_error)?)?;
    let mut sessions = Vec::new();
    let mut facts: BTreeMap<String, WorkItemFacts> = BTreeMap::new();
    let mut before: BTreeMap<String, Duration> = BTreeMap::new();
    for record in &records {
        let Some(stopped_at) = record.stopped_at else {
            continue;
        };
        let session = Session::from_record(record.clone())?;
        let duration = session.elapsed(stopped_at).as_duration();
        let work = WorkSession {
            session: record.id.as_str().to_owned(),
            start: record.started_at,
            story: story_of(record).map(|story| story.as_str().to_owned()),
            task: record
                .target
                .task
                .as_ref()
                .map(|task| task.as_str().to_owned()),
            title: String::new(),
            duration,
        };
        let key = work_item_key(&work);
        if record.started_at < start {
            *before.entry(key).or_default() += duration;
            continue;
        }
        if record.started_at >= end {
            continue;
        }
        let task = match &record.target.task {
            Some(task) => task_facts(workspace, task.as_str())?,
            None => crate::toggl::TaskFacts::default(),
        };
        facts.entry(key).or_insert(WorkItemFacts {
            is_billable: task.is_billable,
            estimate: task.estimate,
            tracked_before: Duration::ZERO,
        });
        sessions.push(WorkSession {
            title: task.text.unwrap_or_else(|| match &record.source {
                SessionSource::Meeting { .. } => meeting_or_story(record),
                SessionSource::Timer => work
                    .story
                    .clone()
                    .unwrap_or_else(|| meeting_or_story(record)),
            }),
            ..work
        });
    }
    for (key, facts) in &mut facts {
        facts.tracked_before = before.get(key).copied().unwrap_or_default();
    }
    let stories = budgets
        .into_iter()
        .filter(|(story, _)| touched.iter().any(|touch: &Touch| &touch.story == story))
        .collect();
    Ok(PeriodWork {
        stories,
        touched,
        sessions,
        facts,
    })
}

fn read_leave_note(root: Option<&str>) -> Result<Option<LeaveNote>, CoreError> {
    let Some(root) = root else {
        return Ok(None);
    };
    let path = Path::new(root).join(LEAVE_NOTE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(LeaveNote::parse(&text))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::FileAccess(format!(
            "{} cannot be read: {error}",
            path.display()
        ))),
    }
}

fn local_now(text: Option<&str>) -> Result<(DateTime, Timestamp), CoreError> {
    match text {
        Some(text) => {
            let local: DateTime = text.parse().map_err(|_| {
                CoreError::InvalidDate(format!("{text} is not a time; expected YYYY-MM-DDTHH:MM"))
            })?;
            let now = local
                .to_zoned(TimeZone::system())
                .map_err(crate::home::date_error)?
                .timestamp();
            Ok((local, now))
        }
        None => {
            let now = Timestamp::now();
            Ok((now.to_zoned(TimeZone::system()).datetime(), now))
        }
    }
}

fn close_out_view(close: &CloseOut) -> CloseOutView {
    let title_of = |story: &StoryNumber| {
        close
            .titles
            .get(story)
            .cloned()
            .unwrap_or_else(|| story.as_str().to_owned())
    };
    let review = close
        .hourly
        .iter()
        .map(|row| ReviewRowView {
            key: row.key.clone(),
            story: row.story.clone(),
            title: row.title.clone(),
            tracked_minutes: whole_minutes(row.tracked),
            billed_minutes: whole_minutes(row.billed),
            overrun_minutes: 0,
            estimate_minutes: None,
            is_billable: row.is_billable,
        })
        .chain(close.fixed.iter().map(|row| ReviewRowView {
            key: row.key.clone(),
            story: row.story.clone(),
            title: row.title.clone(),
            tracked_minutes: whole_minutes(row.tracked),
            billed_minutes: whole_minutes(row.billable),
            overrun_minutes: whole_minutes(row.overrun),
            estimate_minutes: row.estimate.map(whole_minutes),
            is_billable: true,
        }))
        .collect();
    CloseOutView {
        period_id: close.period.id(),
        kind: match close.period.kind() {
            PeriodKind::Week => PeriodKindView::Week,
            PeriodKind::Month => PeriodKindView::Month,
        },
        start: close.period.start().to_string(),
        end: close.period.end().to_string(),
        model: match close.model {
            CloseOutModel::Fte => CloseOutModelView::Fte,
            CloseOutModel::Hourly => CloseOutModelView::Hourly,
            CloseOutModel::Fixed => CloseOutModelView::Fixed,
        },
        opens_at: minute_text(close.opens_at),
        deadline: minute_text(close.deadline),
        days: close
            .days
            .iter()
            .map(|day| PeriodDayView {
                date: day.date.to_string(),
                kind: day_kind_view(day.kind),
            })
            .collect(),
        is_toggl_on: close.is_toggl_on,
        flat: close
            .flat
            .iter()
            .map(|line| FlatLineView {
                date: line.date.to_string(),
                state: match line.status {
                    FlatDayStatus::WillCreate | FlatDayStatus::NotWorkingDay => {
                        FlatLineStateView::NotQueued
                    }
                    FlatDayStatus::Queued => FlatLineStateView::Queued,
                    FlatDayStatus::Sent => FlatLineStateView::Sent,
                    FlatDayStatus::Failed => FlatLineStateView::Failed,
                    FlatDayStatus::Leave => FlatLineStateView::Leave,
                    FlatDayStatus::Holiday => FlatLineStateView::Holiday,
                },
                seconds: line.seconds,
            })
            .collect(),
        unqueued_sessions: u32::try_from(close.unqueued_sessions.len()).unwrap_or(u32::MAX),
        allocation: close
            .allocation
            .iter()
            .map(|day| AllocationDayView {
                date: day.allocation.date.to_string(),
                target_minutes: day.allocation.target_minutes,
                rows: day
                    .allocation
                    .allotments
                    .iter()
                    .map(|allotment| AllocationRowView {
                        story: allotment.story.as_str().to_owned(),
                        title: title_of(&allotment.story),
                        minutes: allotment.minutes,
                        tracked_minutes: day.tracked.get(&allotment.story).copied().unwrap_or(0),
                        has_estimate: !day.unestimated.contains(&allotment.story),
                    })
                    .collect(),
                is_confirmed: day.is_confirmed,
                is_draft: day.is_draft,
            })
            .collect(),
        review,
        table: CloseOutTableView {
            sink: close.table.sink.clone(),
            title: close.table.title.clone(),
            columns: close.table.columns.clone(),
            rows: close
                .table
                .rows
                .iter()
                .map(|row| CloseOutRowView {
                    key: row.key.clone(),
                    cells: row.cells.clone(),
                    is_ticked: row.is_ticked,
                })
                .collect(),
            text: close.table_text.clone(),
        },
        steps: close
            .steps
            .iter()
            .map(|step| StepView {
                kind: match step.kind {
                    StepKind::Toggl => StepKindView::Toggl,
                    StepKind::Allocate => StepKindView::Allocate,
                    StepKind::Review => StepKindView::Review,
                    StepKind::Table => StepKindView::Table,
                    StepKind::Close => StepKindView::Close,
                },
                title: step.title.clone(),
                status: match step.status {
                    StepStatus::Open => StepStatusView::Open,
                    StepStatus::Done => StepStatusView::Done,
                    StepStatus::Skipped => StepStatusView::Skipped,
                },
            })
            .collect(),
        missing: close.missing_texts(),
        can_close: close.can_close(),
        is_closed: close.is_closed(),
    }
}

fn day_views(period: &Period, off: &DaysOff) -> Vec<PeriodDayView> {
    period
        .days(off)
        .iter()
        .map(|day| PeriodDayView {
            date: day.date.to_string(),
            kind: day_kind_view(day.kind),
        })
        .collect()
}

const fn day_kind_view(kind: DayKind) -> DayKindView {
    match kind {
        DayKind::Working => DayKindView::Working,
        DayKind::Leave => DayKindView::Leave,
        DayKind::Holiday => DayKindView::Holiday,
        DayKind::Weekend => DayKindView::Weekend,
    }
}

fn allotment_view(allotment: &Allotment) -> AllotmentView {
    AllotmentView {
        story: allotment.story.as_str().to_owned(),
        minutes: allotment.minutes,
    }
}

fn draft_views(draft: &BTreeMap<Date, Vec<Allotment>>) -> Vec<DraftDayView> {
    draft
        .iter()
        .map(|(date, allotments)| DraftDayView {
            date: date.to_string(),
            allotments: allotments.iter().map(allotment_view).collect(),
        })
        .collect()
}

fn draft_map(days: &[DraftDayView]) -> Result<BTreeMap<Date, Vec<Allotment>>, CoreError> {
    days.iter()
        .map(|day| {
            Ok((
                parse_day(&day.date)?,
                day.allotments
                    .iter()
                    .map(|allotment| Allotment {
                        story: StoryNumber::new(allotment.story.as_str()),
                        minutes: allotment.minutes,
                    })
                    .collect(),
            ))
        })
        .collect()
}

fn settings_view(settings: &WorkspaceSettings) -> CloseOutSettingsView {
    CloseOutSettingsView {
        deadline_weekday: settings
            .log_deadline
            .as_ref()
            .map(|deadline| weekday_view(deadline.weekday)),
        deadline_time: settings
            .log_deadline
            .as_ref()
            .map_or_else(|| "08:00".to_owned(), |deadline| deadline.time.clone()),
        leave_source: settings.leave_source.clone(),
    }
}

const fn weekday_view(day: SettingsWeekday) -> WeekdayView {
    match day {
        SettingsWeekday::Monday => WeekdayView::Monday,
        SettingsWeekday::Tuesday => WeekdayView::Tuesday,
        SettingsWeekday::Wednesday => WeekdayView::Wednesday,
        SettingsWeekday::Thursday => WeekdayView::Thursday,
        SettingsWeekday::Friday => WeekdayView::Friday,
        SettingsWeekday::Saturday => WeekdayView::Saturday,
        SettingsWeekday::Sunday => WeekdayView::Sunday,
    }
}

const fn settings_weekday(day: WeekdayView) -> SettingsWeekday {
    match day {
        WeekdayView::Monday => SettingsWeekday::Monday,
        WeekdayView::Tuesday => SettingsWeekday::Tuesday,
        WeekdayView::Wednesday => SettingsWeekday::Wednesday,
        WeekdayView::Thursday => SettingsWeekday::Thursday,
        WeekdayView::Friday => SettingsWeekday::Friday,
        WeekdayView::Saturday => SettingsWeekday::Saturday,
        WeekdayView::Sunday => SettingsWeekday::Sunday,
    }
}

fn open_workspace(root: &str) -> Result<Workspace, CoreError> {
    Workspace::open(Path::new(root), &PlainFileAccess).map_err(|error| match error {
        vault_base::WorkspaceError::NotAWorkspace(_) => CoreError::NotAWorkspace(error.to_string()),
        other => CoreError::InvalidSettings(other.to_string()),
    })
}

fn minute_text(at: DateTime) -> String {
    format!("{}T{:02}:{:02}", at.date(), at.hour(), at.minute())
}

fn parse_day(text: &str) -> Result<Date, CoreError> {
    text.parse()
        .map_err(|_| CoreError::InvalidDate(format!("{text} is not a date; expected YYYY-MM-DD")))
}

fn core_error(error: CloseOutError) -> CoreError {
    let message = error.to_string();
    match error {
        CloseOutError::InvalidInput(_) => CoreError::InvalidInput(message),
        CloseOutError::NotReady(_) | CloseOutError::NoDeadline => CoreError::Conflict(message),
        CloseOutError::Domain(error) => error.into(),
        CloseOutError::Toggl(error) => toggl_error(error),
        CloseOutError::AppState(error) => error.into(),
        CloseOutError::Write(error) => vault_base::OpError::from(error).into(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::app_state::open_app_state;
    use crate::app_state::tests::Keychain;
    use crate::files::plain_sync_files;
    use crate::home::tests::{open, sandbox, state};
    use crate::toggl::{
        BillingModelView, BillingSettingsView, drain_toggl_queue, update_workspace_billing,
    };
    use domain::session::{SessionId, SessionTarget};

    const WORKSPACE: &str = "mock-studio";

    #[derive(Default)]
    struct Recorded {
        alerts: Mutex<Vec<(String, String, Vec<String>)>>,
    }

    impl NotifierPort for Recorded {
        fn long_timer(&self, _: String, _: u64) {}
        fn threshold_crossed(&self, _: String, _: Option<String>, _: u64, _: u64) {}
        fn new_signals(&self, _: String, _: u32, _: u32) {}
        fn close_out_missing(&self, workspace: String, week_start: String, missing: Vec<String>) {
            self.alerts
                .lock()
                .unwrap()
                .push((workspace, week_start, missing));
        }
    }

    fn app(dir: &Path) -> Arc<AppStateHandle> {
        open_app_state(
            plain_sync_files(),
            dir.join("app").to_string_lossy().into_owned(),
            Arc::new(Keychain::default()),
        )
        .unwrap()
    }

    fn leave_root() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/two-workspaces")
            .to_string_lossy()
            .into_owned()
    }

    fn request() -> CloseOutRequest {
        CloseOutRequest {
            workspace_id: WORKSPACE.to_owned(),
            kind: PeriodKindView::Week,
            date: "2026-10-21".to_owned(),
            leave_root: Some(leave_root()),
            draft: Vec::new(),
        }
    }

    fn fte(root: &Path) {
        update_workspace_billing(
            root.to_string_lossy().into_owned(),
            BillingSettingsView {
                model: Some(BillingModelView::Fte),
                rounding_minutes: 15,
                is_billable_by_default: true,
                flat_hours: 8.0,
            },
        )
        .unwrap();
        update_close_out_settings(
            root.to_string_lossy().into_owned(),
            CloseOutSettingsView {
                deadline_weekday: Some(WeekdayView::Monday),
                deadline_time: "08:00".to_owned(),
                leave_source: None,
            },
        )
        .unwrap();
    }

    /// A stopped two-hour session on WEB-142 on Monday 19 October.
    fn monday_session(state: &WorkspaceStateHandle) {
        let start = date_at(2026, 10, 19)
            .to_zoned(TimeZone::system())
            .unwrap()
            .timestamp();
        let record = SessionRecord {
            id: SessionId::new("ses-closeout-1"),
            target: SessionTarget {
                task: Some(TaskId::new("task-2")),
                story: Some(StoryNumber::new("WEB-142")),
            },
            source: SessionSource::Timer,
            started_at: start,
            stopped_at: Some(start + jiff::SignedDuration::from_hours(2)),
            pauses: Vec::new(),
            has_long_timer_fired: false,
        };
        lock(&state.sync)
            .edit(Instant::now(), |state| state.put_session(&record))
            .unwrap();
    }

    fn date_at(year: i16, month: i8, day: i8) -> DateTime {
        jiff::civil::date(year, month, day).at(10, 0, 0, 0)
    }

    #[test]
    fn the_working_week_reads_leave_and_holidays() {
        let week = working_week("2026-10-21".to_owned(), Some(leave_root())).unwrap();
        assert!(week.has_leave_note);
        assert_eq!(
            week.working_days,
            ["2026-10-19", "2026-10-20", "2026-10-22", "2026-10-23"]
        );
        assert_eq!(week.days[2].kind, DayKindView::Leave);
        let holiday = working_week("2026-11-02".to_owned(), Some(leave_root())).unwrap();
        assert_eq!(holiday.days[0].kind, DayKindView::Holiday);
        let none = working_week("2026-10-21".to_owned(), None).unwrap();
        assert!(!none.has_leave_note && none.working_days.len() == 5);
        assert!(working_week("21 Oct".to_owned(), None).is_err());
    }

    #[test]
    fn the_fte_week_closes_over_the_bridge_without_toggl() {
        let vault = sandbox();
        let app = app(vault.path());
        let state = state(vault.path());
        let workspace = open(vault.path());
        assert!(
            close_out_due(&app, &state, &workspace, WORKSPACE.to_owned(), None, None)
                .unwrap()
                .is_none(),
            "no log_deadline, no close-out"
        );
        fte(vault.path());
        monday_session(&state);

        let close = close_out_state(&app, &state, &workspace, request()).unwrap();
        assert_eq!(close.model, CloseOutModelView::Fte);
        assert!(!close.is_toggl_on);
        assert_eq!(close.steps[0].status, StepStatusView::Skipped);
        assert_eq!(close.flat.len(), 5);
        assert_eq!(close.flat[2].state, FlatLineStateView::Leave);
        assert_eq!(close.allocation.len(), 4);
        let monday = &close.allocation[0];
        assert_eq!(monday.rows.len(), 1);
        assert_eq!(
            (
                monday.rows[0].story.as_str(),
                monday.rows[0].minutes,
                monday.rows[0].tracked_minutes
            ),
            ("WEB-142", 480, 120)
        );
        assert!(monday.rows[0].has_estimate);
        assert!(close.allocation[1..].iter().all(|day| day.rows.is_empty()));
        assert_eq!(
            close.table.text,
            "Story\tTitle\t2026-10-19\t2026-10-20\t2026-10-22\t2026-10-23\tTotal\n\
             WEB-142\tCheckout redesign\t8.00\t\t\t\t8.00\n"
        );
        assert!(matches!(
            close_out_queue_toggl(&app, &state, &workspace, request()),
            Err(CoreError::Conflict(_))
        ));
        let due = close_out_due(
            &app,
            &state,
            &workspace,
            WORKSPACE.to_owned(),
            Some(leave_root()),
            Some("2026-10-23T14:00".to_owned()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                due.title.as_str(),
                due.date.as_str(),
                due.deadline.as_str(),
                due.is_overdue
            ),
            ("Week close-out", "2026-10-19", "2026-10-26T08:00", false)
        );

        // Adjusting can only move minutes between stories worked that day.
        assert!(matches!(
            close_out_rebalance(
                &app,
                &state,
                &workspace,
                request(),
                "2026-10-20".to_owned(),
                "WEB-142".to_owned(),
                60
            ),
            Err(CoreError::InvalidInput(_))
        ));
        let days: Vec<DraftDayView> = close
            .allocation
            .iter()
            .map(|day| DraftDayView {
                date: day.date.clone(),
                allotments: day
                    .rows
                    .iter()
                    .map(|row| AllotmentView {
                        story: row.story.clone(),
                        minutes: row.minutes,
                    })
                    .collect(),
            })
            .collect();
        close_out_confirm_allocation(&app, &state, &workspace, request(), days).unwrap();
        close_out_tick(
            &app,
            &state,
            &workspace,
            request(),
            close.table.sink.clone(),
            "WEB-142".to_owned(),
            true,
        )
        .unwrap();
        state.flush().unwrap();
        let ticked = close_out_state(&app, &state, &workspace, request()).unwrap();
        assert!(ticked.table.rows[0].is_ticked);
        assert_eq!(ticked.missing, ["The week is not closed"]);
        assert!(ticked.can_close);

        let closed = close_out_close(
            &app,
            &state,
            &workspace,
            request(),
            Some("2026-10-23T16:05".to_owned()),
        )
        .unwrap();
        assert_eq!(
            closed.line.as_deref(),
            Some("- 16:05 status · week closed [kind:: status]")
        );
        let log = std::fs::read_to_string(vault.path().join("Log/2026-10-23.md")).unwrap();
        assert_eq!(log.matches("status · week closed").count(), 1);
        let recorded = Arc::new(Recorded::default());
        assert!(
            !close_out_check_alert(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                Some(leave_root()),
                recorded.clone(),
                Some("2026-10-26T07:31".to_owned()),
            )
            .unwrap(),
            "a closed week raises no alert"
        );
        assert!(
            close_out_due(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                Some(leave_root()),
                Some("2026-10-26T07:31".to_owned())
            )
            .unwrap()
            .is_none_or(|due| due.kind == PeriodKindView::Month)
        );
    }

    #[test]
    fn queued_flat_days_stay_behind_the_live_guard() {
        let vault = sandbox();
        let app = app(vault.path());
        let state = state(vault.path());
        let workspace = open(vault.path());
        fte(vault.path());
        app.set_toggl_enabled(WORKSPACE.to_owned(), true).unwrap();
        app.store_toggl_token(
            WORKSPACE.to_owned(),
            "synthetic-toggl-token-0002".to_owned(),
        )
        .unwrap();
        let queued = close_out_queue_toggl(&app, &state, &workspace, request()).unwrap();
        assert_eq!(queued, 4, "four working days, the leave day has no entry");

        // Feature on and a token stored, but allow_live is off: the drain
        // refuses and every flat day stays queued on this machine.
        let drained = drain_toggl_queue(&app, &state, WORKSPACE.to_owned()).unwrap();
        assert_eq!(
            drained.not_connected.as_deref(),
            Some("Toggl is not connected: allow_live is off")
        );
        let close = close_out_state(&app, &state, &workspace, request()).unwrap();
        let states: Vec<FlatLineStateView> = close.flat.iter().map(|line| line.state).collect();
        assert_eq!(
            states,
            [
                FlatLineStateView::Queued,
                FlatLineStateView::Queued,
                FlatLineStateView::Leave,
                FlatLineStateView::Queued,
                FlatLineStateView::Queued,
            ]
        );
    }

    #[test]
    fn the_monday_alert_fires_once() {
        let vault = sandbox();
        let app = app(vault.path());
        let state = state(vault.path());
        let workspace = open(vault.path());
        fte(vault.path());
        monday_session(&state);
        let recorded = Arc::new(Recorded::default());
        let check = |at: &str| {
            close_out_check_alert(
                &app,
                &state,
                &workspace,
                WORKSPACE.to_owned(),
                Some(leave_root()),
                recorded.clone(),
                Some(at.to_owned()),
            )
            .unwrap()
        };
        assert!(!check("2026-10-26T07:29"));
        assert!(check("2026-10-26T07:30"));
        assert!(!check("2026-10-26T12:00"));
        let alerts = recorded.alerts.lock().unwrap().clone();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].0, WORKSPACE);
        assert_eq!(alerts[0].1, "2026-10-19");
        assert_eq!(
            alerts[0].2,
            [
                "Allocation not confirmed for 4 days",
                "1 table row not ticked",
                "The week is not closed"
            ]
        );
    }
}
