//! The week close-out ritual and its monthly variant, per workspace with a
//! `log_deadline`.
//!
//! [`close_out`] computes, from the workspace document and the period's work,
//! what the ritual shows and what is still missing; the other functions are
//! its steps. The steps depend on the billing model:
//!
//! - FTE: (1) queue the flat `flat_hours` Toggl entry of each working day
//!   through the Toggl feature set (a dry run while it is off); (2) confirm
//!   the allocation of `hours_per_day` over the stories touched each day
//!   ([`domain::allocation`]); (3) the time-sheet table, ticked row by row;
//!   (4) close: the `status` line "week closed" in the day log.
//! - Fixed price: (1) every tracked session queued to Toggl; (2) review the
//!   cap per work item and the unpaid overrun; (3) the completed-work table;
//!   (4) close.
//! - Hourly: (1) every tracked session queued to Toggl; (2) review the
//!   rounded, billable hours; (3) the billable table; (4) close.
//!
//! Nothing here sends anything: queueing writes to the workspace document
//! only, and the app's drain sends through the Toggl live guard.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod error;
pub mod schedule;
mod store;
pub mod table;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use app_state::WorkspaceState;
use domain::allocation::{
    Allotment, DayAllocation, DayWork, StoryDay, allocate_days, rebalance_day,
};
use domain::estimate::daily_target;
use domain::ports::{Clock, Notifier};
use domain::task::StoryNumber;
use domain::week::{DayKind, DaysOff, Period, PeriodDay, PeriodKind};
use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Weekday};
use vault_base::log::VERB_SEPARATOR;
use vault_base::{
    BillingModel, LeaveNote, LogAppend, LogKind, VaultWriter, WorkspaceKind, WorkspaceSettings,
};
use vaultday_feature_toggl::{
    FixedRow, FlatDayStatus, FlatJobInput, HourlyRow, TogglError, TogglFeature, WorkItemFacts,
    WorkSession, reconcile_fixed, reconcile_hourly,
};

pub use error::CloseOutError;
pub use store::RECORDS;
pub use table::{PLAIN_SINK, PlainTsv, Table, TableFormatter, TableRow};

/// The text of the closing `status` line of a week, as the design note has it.
pub const WEEK_CLOSED: &str = "week closed";
/// The text of the closing `status` line of a month.
pub const MONTH_CLOSED: &str = "month closed";

/// A story the period touched, as the vault has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryFacts {
    /// The story's title.
    pub title: String,
    /// Its budget in minutes (its estimate, else its tasks'); `None` without one.
    pub budget_minutes: Option<u32>,
}

/// A story worked on a day: a session ran on it, a meeting counted for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Touch {
    /// The day.
    pub date: Date,
    /// The story.
    pub story: StoryNumber,
    /// Minutes tracked on it that day; 0 for a touch without a timer.
    pub tracked_minutes: u32,
}

/// What the period's work was, read from the vault and the sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeriodWork {
    /// The stories named below, by number.
    pub stories: BTreeMap<StoryNumber, StoryFacts>,
    /// FTE: the stories touched, by day.
    pub touched: Vec<Touch>,
    /// Hourly and fixed price: the period's stopped sessions.
    pub sessions: Vec<WorkSession>,
    /// Hourly and fixed price: each work item's flag, estimate and earlier time.
    pub facts: BTreeMap<String, WorkItemFacts>,
}

/// Everything a close-out is computed from besides the workspace document.
#[derive(Debug, Clone, Copy)]
pub struct CloseOutContext<'a> {
    /// The workspace's id, for the alert.
    pub workspace: &'a str,
    /// The week or month.
    pub period: Period,
    /// The workspace's settings: billing, `hours_per_day`, `log_deadline`.
    pub settings: &'a WorkspaceSettings,
    /// Leave and public holidays.
    pub days_off: &'a DaysOff,
    /// The period's work.
    pub work: &'a PeriodWork,
    /// The Toggl feature, when it is on for the workspace.
    pub toggl: Option<&'a TogglFeature>,
    /// Tags of the workspace's Toggl entries.
    pub toggl_tags: &'a [String],
    /// Days the user adjusted and has not confirmed yet.
    pub draft: &'a BTreeMap<Date, Vec<Allotment>>,
}

/// Which steps the close-out has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseOutModel {
    /// FTE, or no billing model: flat entries and the allocation.
    Fte,
    /// Hourly billing.
    Hourly,
    /// Fixed price per task.
    Fixed,
}

/// One of the four steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    /// Entries to Toggl: the flat days, or the sessions.
    Toggl,
    /// FTE: the allocation.
    Allocate,
    /// Hourly and fixed price: the review of the billed hours.
    Review,
    /// The copyable table.
    Table,
    /// The closing log line.
    Close,
}

/// Where a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepStatus {
    /// Something is left to do.
    Open,
    /// Done.
    Done,
    /// Nothing to do here: Toggl is off, or the workspace bills nothing.
    Skipped,
}

/// A step and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Which step.
    pub kind: StepKind,
    /// Its title.
    pub title: String,
    /// Where it stands.
    pub status: StepStatus,
}

/// Something the close-out still misses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Missing {
    /// Working days whose flat Toggl entry is not queued.
    FlatEntries(usize),
    /// Sessions not queued to Toggl.
    SessionsNotQueued(usize),
    /// Working days whose allocation is not confirmed.
    AllocationNotConfirmed(usize),
    /// The review is not confirmed.
    ReviewNotConfirmed,
    /// Table rows not ticked as entered.
    RowsNotTicked(usize),
    /// The closing line is not written.
    NotClosed,
}

/// One day of the flat week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatLine {
    /// The day.
    pub date: Date,
    /// What happens or happened to its entry; [`FlatDayStatus::WillCreate`]
    /// while Toggl is off is the dry run.
    pub status: FlatDayStatus,
    /// The entry's length.
    pub seconds: u64,
}

/// One working day of the allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationDay {
    /// The allocation, proposed or kept.
    pub allocation: DayAllocation,
    /// Tracked minutes per story that day, private.
    pub tracked: BTreeMap<StoryNumber, u32>,
    /// Whether the day is confirmed in the workspace document.
    pub is_confirmed: bool,
    /// Whether the user adjusted it and has not confirmed yet.
    pub is_draft: bool,
    /// Stories worked that day without an estimate: they get no minutes, so
    /// the ritual flags them for the user to give them an estimate.
    pub unestimated: BTreeSet<StoryNumber>,
}

/// What the close-out shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseOut {
    /// The week or month.
    pub period: Period,
    /// Which steps.
    pub model: CloseOutModel,
    /// When it becomes due.
    pub opens_at: DateTime,
    /// When the hours must be logged by.
    pub deadline: DateTime,
    /// Every day of the period and what it is.
    pub days: Vec<PeriodDay>,
    /// Whether the Toggl feature is on for the workspace.
    pub is_toggl_on: bool,
    /// FTE: the flat days, working days and days off.
    pub flat: Vec<FlatLine>,
    /// Hourly and fixed price: sessions not queued to Toggl yet.
    pub unqueued_sessions: Vec<String>,
    /// FTE: the allocation of each working day.
    pub allocation: Vec<AllocationDay>,
    /// Story titles, by number.
    pub titles: BTreeMap<StoryNumber, String>,
    /// Hourly: the review rows.
    pub hourly: Vec<HourlyRow>,
    /// Fixed price: the review rows.
    pub fixed: Vec<FixedRow>,
    /// The table of the table step.
    pub table: Table,
    /// The table as the sink takes it.
    pub table_text: String,
    /// The four steps.
    pub steps: Vec<Step>,
    /// What is still missing, in step order.
    pub missing: Vec<Missing>,
    /// When the review was confirmed.
    pub reviewed_at: Option<Timestamp>,
    /// When the period was closed.
    pub closed_at: Option<Timestamp>,
}

/// What closing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// The line appended, `None` when the period was closed before.
    pub line: Option<String>,
    /// The day log it went to.
    pub date: Date,
}

/// A workspace the leave note may come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveCandidate {
    /// Its name.
    pub name: String,
    /// Work or life.
    pub kind: WorkspaceKind,
    /// Its root folder.
    pub root: String,
}

impl Missing {
    /// The line the ritual and the alert show, for a period of `kind`.
    #[must_use]
    pub fn text(&self, kind: PeriodKind) -> String {
        let noun = period_noun(kind);
        match *self {
            Missing::FlatEntries(days) => format!("Toggl flat entries for {}", days_text(days)),
            Missing::SessionsNotQueued(1) => "1 session not sent to Toggl".to_owned(),
            Missing::SessionsNotQueued(count) => format!("{count} sessions not sent to Toggl"),
            Missing::AllocationNotConfirmed(days) => {
                format!("Allocation not confirmed for {}", days_text(days))
            }
            Missing::ReviewNotConfirmed => "Hours not reviewed".to_owned(),
            Missing::RowsNotTicked(1) => "1 table row not ticked".to_owned(),
            Missing::RowsNotTicked(count) => format!("{count} table rows not ticked"),
            Missing::NotClosed => format!("The {noun} is not closed"),
        }
    }
}

impl CloseOut {
    /// Whether the period is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed_at.is_some()
    }

    /// What is missing as lines, for the alert.
    #[must_use]
    pub fn missing_texts(&self) -> Vec<String> {
        self.missing
            .iter()
            .map(|missing| missing.text(self.period.kind()))
            .collect()
    }

    /// Whether every step but the closing one is done or skipped.
    #[must_use]
    pub fn can_close(&self) -> bool {
        self.missing
            .iter()
            .all(|missing| *missing == Missing::NotClosed)
    }
}

/// The close-out of `ctx.period` as the workspace document stands.
///
/// # Errors
/// [`CloseOutError::NoDeadline`] without a `log_deadline`,
/// [`CloseOutError::InvalidInput`] for a bad `hours_per_day` or deadline, or
/// the Toggl feature refused the flat preview.
pub fn close_out(
    state: &WorkspaceState,
    ctx: &CloseOutContext<'_>,
) -> Result<CloseOut, CloseOutError> {
    let log_deadline = ctx
        .settings
        .log_deadline
        .as_ref()
        .ok_or(CloseOutError::NoDeadline)?;
    let model = model_of(ctx.settings);
    let period = ctx.period;
    let days = period.days(ctx.days_off);
    let titles = titles(ctx.work);
    let reviewed_at = store::reviewed_at(state, &period);
    let closed_at = store::closed_at(state, &period);

    let mut missing = Vec::new();
    let mut flat = Vec::new();
    let mut unqueued_sessions = Vec::new();
    let mut allocation = Vec::new();
    let mut hourly = Vec::new();
    let mut fixed = Vec::new();
    let is_billing_fte = ctx
        .settings
        .billing
        .as_ref()
        .is_some_and(|billing| billing.model == BillingModel::Fte);

    let toggl_status = match model {
        CloseOutModel::Fte if is_billing_fte => {
            flat = flat_lines(state, ctx)?;
            let unqueued = flat
                .iter()
                .filter(|line| line.status == FlatDayStatus::WillCreate)
                .count();
            if ctx.toggl.is_none() {
                StepStatus::Skipped
            } else if unqueued > 0 {
                missing.push(Missing::FlatEntries(unqueued));
                StepStatus::Open
            } else {
                StepStatus::Done
            }
        }
        CloseOutModel::Fte => StepStatus::Skipped,
        CloseOutModel::Hourly | CloseOutModel::Fixed => {
            unqueued_sessions = unqueued(state, ctx);
            if ctx.toggl.is_none() {
                StepStatus::Skipped
            } else if unqueued_sessions.is_empty() {
                StepStatus::Done
            } else {
                missing.push(Missing::SessionsNotQueued(unqueued_sessions.len()));
                StepStatus::Open
            }
        }
    };

    let second_status = match model {
        CloseOutModel::Fte => {
            allocation = allocation_days(state, ctx)?;
            let open = allocation.iter().filter(|day| !day.is_confirmed).count();
            if open > 0 {
                missing.push(Missing::AllocationNotConfirmed(open));
                StepStatus::Open
            } else {
                StepStatus::Done
            }
        }
        CloseOutModel::Hourly | CloseOutModel::Fixed => {
            if model == CloseOutModel::Hourly {
                let billing = ctx.settings.billing.as_ref();
                hourly = reconcile_hourly(
                    &ctx.work.sessions,
                    &ctx.work.facts,
                    billing.map_or(15, |billing| billing.rounding_minutes_or_default()),
                    billing.is_none_or(|billing| billing.billable_default_or_default()),
                );
            } else {
                fixed = reconcile_fixed(&ctx.work.sessions, &ctx.work.facts);
            }
            if reviewed_at.is_some() {
                StepStatus::Done
            } else {
                missing.push(Missing::ReviewNotConfirmed);
                StepStatus::Open
            }
        }
    };

    let mut table = match model {
        CloseOutModel::Fte => fte_table(&period, ctx.days_off, &allocation, &titles),
        CloseOutModel::Hourly => hourly_table(&hourly),
        CloseOutModel::Fixed => fixed_table(&fixed),
    };
    for row in &mut table.rows {
        row.is_ticked = store::is_ticked(state, &period, &table.sink, &row.key);
    }
    let open_rows = table.open_rows();
    let table_status = if open_rows > 0 {
        missing.push(Missing::RowsNotTicked(open_rows));
        StepStatus::Open
    } else {
        StepStatus::Done
    };
    let close_status = if closed_at.is_some() {
        StepStatus::Done
    } else {
        missing.push(Missing::NotClosed);
        StepStatus::Open
    };
    let steps = steps(
        model,
        &period,
        ctx.settings,
        &table,
        [toggl_status, second_status, table_status, close_status],
    );
    let table_text = PlainTsv.format(&table);
    Ok(CloseOut {
        period,
        model,
        opens_at: schedule::opens_at(&period, ctx.days_off),
        deadline: schedule::deadline(&period, ctx.days_off, log_deadline)?,
        days,
        is_toggl_on: ctx.toggl.is_some(),
        flat,
        unqueued_sessions,
        allocation,
        titles,
        hourly,
        fixed,
        table,
        table_text,
        steps,
        missing,
        reviewed_at,
        closed_at,
    })
}

/// Step 1 for FTE: queues the flat entry of every working day that has none
/// through the Toggl feature set; sends nothing. Returns the days queued.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] for a workspace that is not FTE, or the
/// feature or the document refused.
pub fn queue_flat_entries(
    state: &mut WorkspaceState,
    ctx: &CloseOutContext<'_>,
    feature: &TogglFeature,
    start_of_day: &dyn Fn(Date) -> Result<Timestamp, TogglError>,
) -> Result<Vec<Date>, CloseOutError> {
    let input = flat_input(ctx)?;
    Ok(feature.queue_flat_job(state, &input, start_of_day)?)
}

/// The allocation with `story` on `date` set to `minutes` and the rest of
/// that day re-balanced to keep its total; returns the new draft. Days
/// before `date` keep what they have; days after it are proposed afresh
/// unless confirmed or adjusted.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] for a day that is not a working day of the
/// period, a story not touched that day, or a bad `hours_per_day`.
pub fn rebalance(
    state: &WorkspaceState,
    ctx: &CloseOutContext<'_>,
    date: Date,
    story: &StoryNumber,
    minutes: u32,
) -> Result<BTreeMap<Date, Vec<Allotment>>, CloseOutError> {
    let days = allocation_days(state, ctx)?;
    let Some(index) = days.iter().position(|day| day.allocation.date == date) else {
        return Err(CloseOutError::InvalidInput(format!(
            "{date} is not a working day of this close-out"
        )));
    };
    let day = &days[index];
    if !day.tracked.contains_key(story) {
        return Err(CloseOutError::InvalidInput(format!(
            "{story} was not worked on {date}; the close-out never adds work to a story"
        )));
    }
    let used_before: BTreeMap<StoryNumber, u32> = days[..index]
        .iter()
        .flat_map(|day| day.allocation.allotments.iter())
        .fold(BTreeMap::new(), |mut used, allotment| {
            *used.entry(allotment.story.clone()).or_default() += allotment.minutes;
            used
        });
    let start = remaining_at_start(state, ctx);
    let stories: Vec<StoryDay> = day
        .tracked
        .iter()
        .map(|(number, tracked)| StoryDay {
            story: number.clone(),
            tracked_minutes: *tracked,
            remaining_minutes: start
                .get(number)
                .copied()
                .unwrap_or(0)
                .saturating_sub(used_before.get(number).copied().unwrap_or(0)),
        })
        .collect();
    let mut draft = ctx.draft.clone();
    draft.insert(
        date,
        rebalance_day(day.allocation.target_minutes, &stories, story, minutes),
    );
    Ok(draft)
}

/// Step 2 for FTE: stores the allocation of each given day as confirmed.
/// Each day must be a working day of the period, name only stories touched
/// that day, and stay within its target and the stories' budgets.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] when a day breaks those rules, or the
/// document refused the edit.
pub fn confirm_allocation(
    state: &mut WorkspaceState,
    ctx: &CloseOutContext<'_>,
    days: &BTreeMap<Date, Vec<Allotment>>,
) -> Result<(), CloseOutError> {
    let target = target_minutes(ctx.settings)?;
    let working: BTreeSet<Date> = ctx.period.working_days(ctx.days_off).into_iter().collect();
    let touched = touched_by_day(ctx.work);
    let start = remaining_at_start(state, ctx);
    let mut used: BTreeMap<StoryNumber, u32> = BTreeMap::new();
    let confirmed = store::confirmed_days(state);
    for (date, allotments) in confirmed
        .iter()
        .filter(|(date, _)| ctx.period.contains(**date))
    {
        if days.contains_key(date) {
            continue;
        }
        for allotment in allotments {
            *used.entry(allotment.story.clone()).or_default() += allotment.minutes;
        }
    }
    for (date, allotments) in days {
        if !working.contains(date) {
            return Err(CloseOutError::InvalidInput(format!(
                "{date} is not a working day of this close-out"
            )));
        }
        let worked = touched.get(date);
        let mut total = 0_u32;
        for allotment in allotments {
            let is_worked = worked.is_some_and(|stories| stories.contains_key(&allotment.story));
            if allotment.minutes > 0 && !is_worked {
                return Err(CloseOutError::InvalidInput(format!(
                    "{} was not worked on {date}; the close-out never adds work to a story",
                    allotment.story
                )));
            }
            total = total.saturating_add(allotment.minutes);
            *used.entry(allotment.story.clone()).or_default() += allotment.minutes;
        }
        if total > target {
            return Err(CloseOutError::InvalidInput(format!(
                "{date} allocates {total} minutes, more than the day's {target}"
            )));
        }
    }
    if let Some((story, minutes)) = used
        .iter()
        .find(|(story, minutes)| **minutes > start.get(*story).copied().unwrap_or(0))
    {
        return Err(CloseOutError::InvalidInput(format!(
            "{story} would get {minutes} minutes, more than its remaining estimate"
        )));
    }
    for (date, allotments) in days {
        let kept: Vec<Allotment> = allotments
            .iter()
            .filter(|allotment| allotment.minutes > 0)
            .cloned()
            .collect();
        store::store_day(state, *date, &kept)?;
    }
    Ok(())
}

/// Step 2 for hourly and fixed price: records that the user reviewed the hours.
///
/// # Errors
/// The document refused the edit.
pub fn confirm_review(
    state: &mut WorkspaceState,
    period: &Period,
    now: Timestamp,
) -> Result<(), CloseOutError> {
    store::store_reviewed(state, period, now)?;
    Ok(())
}

/// Step 3: ticks or unticks a row of a sink's table.
///
/// # Errors
/// The document refused the edit.
pub fn set_row_ticked(
    state: &mut WorkspaceState,
    period: &Period,
    sink: &str,
    key: &str,
    is_ticked: bool,
) -> Result<(), CloseOutError> {
    store::set_ticked(state, period, sink, key, is_ticked)?;
    Ok(())
}

/// Step 4: records the closing and appends the `status` line "week closed"
/// (or "month closed") to the day log of the period's last working day.
/// The line carries the time of closing; closing after that day puts it at
/// 23:59, the end of the day it belongs to. Closing twice writes once, and
/// so does a retry after the record was lost (a failed edit, a crash before
/// the document was saved): a day log that already holds the line gets no
/// second one.
///
/// # Errors
/// [`CloseOutError::NotReady`] while another step is open, or the day log or
/// the document refused the write.
pub fn close(
    state: &mut WorkspaceState,
    writer: &VaultWriter<'_>,
    ctx: &CloseOutContext<'_>,
    now_local: DateTime,
    now: Timestamp,
) -> Result<Closed, CloseOutError> {
    let current = close_out(state, ctx)?;
    let date = ctx
        .period
        .last_working_day(ctx.days_off)
        .unwrap_or(ctx.period.end());
    if current.is_closed() {
        return Ok(Closed { line: None, date });
    }
    if !current.can_close() {
        let open = current
            .missing
            .iter()
            .filter(|missing| **missing != Missing::NotClosed)
            .map(|missing| missing.text(ctx.period.kind()))
            .collect();
        return Err(CloseOutError::NotReady(open));
    }
    let (hour, minute) = if now_local.date() == date {
        (now_local.hour(), now_local.minute())
    } else {
        (23, 59)
    };
    let time = vault_base::Time::new(
        u8::try_from(hour).unwrap_or(23),
        u8::try_from(minute).unwrap_or(59),
    )
    .ok_or_else(|| CloseOutError::InvalidInput("the time is out of range".to_owned()))?;
    let day = vault_base::Date::new(
        i32::from(date.year()),
        u8::try_from(date.month()).unwrap_or(1),
        u8::try_from(date.day()).unwrap_or(1),
    )
    .ok_or_else(|| CloseOutError::InvalidInput(format!("{date} is outside the day log")))?;
    let text = match ctx.period.kind() {
        PeriodKind::Week => WEEK_CLOSED,
        PeriodKind::Month => MONTH_CLOSED,
    };
    store::store_closed(state, &ctx.period, now)?;
    if has_closing_line(writer, day, text)? {
        return Ok(Closed { line: None, date });
    }
    let receipt = writer.append_log(day, &LogAppend::new(time, text).with_kind(LogKind::Status))?;
    Ok(Closed {
        line: Some(receipt.new_line),
        date,
    })
}

/// The Monday 07:30 alert: when `ctx.period` is a week, the clock is in its
/// alert window, something is missing and the alert has not fired for that
/// week, tells `notifier` and records it so it fires once per week on every
/// device. Returns whether it fired.
///
/// # Errors
/// As [`close_out`], or the document refused the record.
pub fn run_alert(
    state: &mut WorkspaceState,
    ctx: &CloseOutContext<'_>,
    clock: &dyn Clock,
    notifier: &dyn Notifier,
) -> Result<bool, CloseOutError> {
    let key = schedule::alert_key(&ctx.period);
    if state.notified_at(&key).is_some()
        || !schedule::is_alert_window(&ctx.period, clock.local_now())?
    {
        return Ok(false);
    }
    let current = close_out(state, ctx)?;
    if current.missing.is_empty() {
        return Ok(false);
    }
    notifier.close_out_missing(ctx.workspace, ctx.period.start(), &current.missing_texts());
    state.mark_notified(&key, clock.now())?;
    Ok(true)
}

/// The root of the workspace whose `Leave.md` counts for a workspace with
/// `settings`: the one it names as `leave_source`, else the first life
/// workspace. `None` when there is none.
#[must_use]
pub fn leave_source<'a>(
    settings: &WorkspaceSettings,
    candidates: &'a [LeaveCandidate],
) -> Option<&'a LeaveCandidate> {
    match &settings.leave_source {
        Some(name) => candidates
            .iter()
            .find(|candidate| candidate.name.trim() == name.trim()),
        None => candidates
            .iter()
            .find(|candidate| candidate.kind == WorkspaceKind::Life),
    }
}

/// The days off a `Leave.md` lists.
#[must_use]
pub fn days_off(note: &LeaveNote) -> DaysOff {
    let civil = |day: &vault_base::Date| {
        jiff::civil::Date::new(
            i16::try_from(day.year()).ok()?,
            i8::try_from(day.month()).ok()?,
            i8::try_from(day.day()).ok()?,
        )
        .ok()
    };
    DaysOff {
        leave: note.leave_days().iter().filter_map(civil).collect(),
        holidays: note.holidays().iter().filter_map(civil).collect(),
    }
}

/// Which steps a workspace's close-out has.
#[must_use]
pub fn model_of(settings: &WorkspaceSettings) -> CloseOutModel {
    match settings.billing.as_ref().map(|billing| billing.model) {
        Some(BillingModel::Hourly) => CloseOutModel::Hourly,
        Some(BillingModel::Fixed) => CloseOutModel::Fixed,
        Some(BillingModel::Fte) | None => CloseOutModel::Fte,
    }
}

fn flat_input(ctx: &CloseOutContext<'_>) -> Result<FlatJobInput, CloseOutError> {
    let Some(billing) = ctx
        .settings
        .billing
        .as_ref()
        .filter(|billing| billing.model == BillingModel::Fte)
    else {
        return Err(CloseOutError::InvalidInput(
            "The flat entries are for FTE workspaces".to_owned(),
        ));
    };
    let mut input = FlatJobInput::weekdays(
        ctx.period.start(),
        ctx.period.end(),
        billing.flat_hours_or_default(),
    );
    input.leave = ctx.days_off.leave.iter().copied().collect();
    input.holidays = ctx.days_off.holidays.iter().copied().collect();
    input.tags = ctx.toggl_tags.to_vec();
    Ok(input)
}

/// The flat days: the feature's preview while it is on, the dry run otherwise.
fn flat_lines(
    state: &WorkspaceState,
    ctx: &CloseOutContext<'_>,
) -> Result<Vec<FlatLine>, CloseOutError> {
    let input = flat_input(ctx)?;
    if let Some(feature) = ctx.toggl {
        return Ok(feature
            .preview_flat_job(state, &input)?
            .into_iter()
            .filter(|day| day.status != FlatDayStatus::NotWorkingDay)
            .map(|day| FlatLine {
                date: day.date,
                status: day.status,
                seconds: day.duration_secs,
            })
            .collect());
    }
    // In range: `FlatJobInput` holds at most 24 hours.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seconds = (input.flat_hours * 3600.0).round() as u64;
    Ok(ctx
        .period
        .days(ctx.days_off)
        .into_iter()
        .filter_map(|day| {
            let (status, seconds) = match day.kind {
                DayKind::Working => (FlatDayStatus::WillCreate, seconds),
                DayKind::Leave => (FlatDayStatus::Leave, 0),
                DayKind::Holiday => (FlatDayStatus::Holiday, 0),
                DayKind::Weekend => return None,
            };
            Some(FlatLine {
                date: day.date,
                status,
                seconds,
            })
        })
        .collect())
}

/// Sessions of the period with time that are not in the Toggl queue.
fn unqueued(state: &WorkspaceState, ctx: &CloseOutContext<'_>) -> Vec<String> {
    let queued: BTreeSet<String> = ctx
        .toggl
        .map(|feature| {
            feature
                .queue(state)
                .into_iter()
                .filter_map(|item| item.session)
                .collect()
        })
        .unwrap_or_default();
    ctx.work
        .sessions
        .iter()
        .filter(|session| session.duration >= Duration::from_secs(1))
        .filter(|session| !queued.contains(&session.session))
        .map(|session| session.session.clone())
        .collect()
}

fn target_minutes(settings: &WorkspaceSettings) -> Result<u32, CloseOutError> {
    let target = daily_target(settings.hours_per_day).ok_or_else(|| {
        CloseOutError::InvalidInput(format!(
            "hours_per_day {} is not a working day",
            settings.hours_per_day
        ))
    })?;
    Ok(u32::try_from(target.as_secs() / 60).unwrap_or(u32::MAX))
}

/// Each story's budget minus every allocation confirmed before the period.
fn remaining_at_start(
    state: &WorkspaceState,
    ctx: &CloseOutContext<'_>,
) -> BTreeMap<StoryNumber, u32> {
    let mut used: BTreeMap<StoryNumber, u32> = BTreeMap::new();
    for (_, allotments) in store::confirmed_days(state)
        .into_iter()
        .filter(|(date, _)| *date < ctx.period.start())
    {
        for allotment in allotments {
            *used.entry(allotment.story).or_default() += allotment.minutes;
        }
    }
    ctx.work
        .stories
        .iter()
        .map(|(story, facts)| {
            let budget = facts.budget_minutes.unwrap_or(0);
            (
                story.clone(),
                budget.saturating_sub(used.get(story).copied().unwrap_or(0)),
            )
        })
        .collect()
}

fn touched_by_day(work: &PeriodWork) -> BTreeMap<Date, BTreeMap<StoryNumber, u32>> {
    work.touched
        .iter()
        .fold(BTreeMap::new(), |mut days, touch| {
            let stories: &mut BTreeMap<StoryNumber, u32> = days.entry(touch.date).or_default();
            let tracked = stories.entry(touch.story.clone()).or_default();
            *tracked = tracked.saturating_add(touch.tracked_minutes);
            days
        })
}

fn allocation_days(
    state: &WorkspaceState,
    ctx: &CloseOutContext<'_>,
) -> Result<Vec<AllocationDay>, CloseOutError> {
    let target = target_minutes(ctx.settings)?;
    let confirmed = store::confirmed_days(state);
    let touched = touched_by_day(ctx.work);
    let work: Vec<DayWork> = ctx
        .period
        .working_days(ctx.days_off)
        .into_iter()
        .map(|date| DayWork {
            date,
            target_minutes: target,
            touched: touched
                .get(&date)
                .map(|stories| {
                    stories
                        .iter()
                        .map(|(story, tracked)| (story.clone(), *tracked))
                        .collect()
                })
                .unwrap_or_default(),
            fixed: confirmed
                .get(&date)
                .or_else(|| ctx.draft.get(&date))
                .cloned(),
        })
        .collect();
    let start = remaining_at_start(state, ctx);
    Ok(allocate_days(&work, &start)
        .into_iter()
        .map(|allocation| {
            let date = allocation.date;
            let tracked = touched.get(&date).cloned().unwrap_or_default();
            let unestimated = tracked
                .keys()
                .filter(|story| {
                    ctx.work
                        .stories
                        .get(*story)
                        .and_then(|facts| facts.budget_minutes)
                        .is_none()
                })
                .cloned()
                .collect();
            AllocationDay {
                tracked,
                unestimated,
                is_confirmed: confirmed.contains_key(&date),
                is_draft: !confirmed.contains_key(&date) && ctx.draft.contains_key(&date),
                allocation,
            }
        })
        .collect())
}

fn titles(work: &PeriodWork) -> BTreeMap<StoryNumber, String> {
    work.stories
        .iter()
        .map(|(story, facts)| (story.clone(), facts.title.clone()))
        .collect()
}

fn fte_table(
    period: &Period,
    off: &DaysOff,
    allocation: &[AllocationDay],
    titles: &BTreeMap<StoryNumber, String>,
) -> Table {
    let is_week = period.kind() == PeriodKind::Week;
    let working = period.working_days(off);
    let mut columns = vec!["Story".to_owned(), "Title".to_owned()];
    if is_week {
        columns.extend(working.iter().map(ToString::to_string));
    }
    columns.push("Total".to_owned());
    let mut by_story: BTreeMap<StoryNumber, BTreeMap<Date, u32>> = BTreeMap::new();
    for day in allocation {
        for allotment in &day.allocation.allotments {
            if allotment.minutes > 0 {
                *by_story
                    .entry(allotment.story.clone())
                    .or_default()
                    .entry(day.allocation.date)
                    .or_default() += allotment.minutes;
            }
        }
    }
    let rows = by_story
        .into_iter()
        .map(|(story, days)| {
            let mut cells = vec![
                story.as_str().to_owned(),
                titles
                    .get(&story)
                    .cloned()
                    .unwrap_or_else(|| story.as_str().to_owned()),
            ];
            if is_week {
                cells.extend(working.iter().map(|date| {
                    table::hours_or_blank(u64::from(days.get(date).copied().unwrap_or(0)))
                }));
            }
            cells.push(table::hours(days.values().map(|m| u64::from(*m)).sum()));
            TableRow {
                key: story.as_str().to_owned(),
                cells,
                is_ticked: false,
            }
        })
        .collect();
    Table {
        sink: PLAIN_SINK.to_owned(),
        title: "Time sheet".to_owned(),
        columns,
        rows,
    }
}

fn hourly_table(rows: &[HourlyRow]) -> Table {
    Table {
        sink: PLAIN_SINK.to_owned(),
        title: "Billable hours".to_owned(),
        columns: ["Story", "Item", "Tracked", "Billed"]
            .map(str::to_owned)
            .to_vec(),
        rows: rows
            .iter()
            .filter(|row| row.is_billable && !row.billed.is_zero())
            .map(|row| TableRow {
                key: row.key.clone(),
                cells: vec![
                    row.story.clone().unwrap_or_default(),
                    row.title.clone(),
                    table::hours(row.tracked.as_secs() / 60),
                    table::hours(row.billed.as_secs() / 60),
                ],
                is_ticked: false,
            })
            .collect(),
    }
}

fn fixed_table(rows: &[FixedRow]) -> Table {
    Table {
        sink: PLAIN_SINK.to_owned(),
        title: "Completed work".to_owned(),
        columns: ["Story", "Item", "Estimate", "Billed"]
            .map(str::to_owned)
            .to_vec(),
        rows: rows
            .iter()
            .filter(|row| !row.billable.is_zero())
            .map(|row| TableRow {
                key: row.key.clone(),
                cells: vec![
                    row.story.clone().unwrap_or_default(),
                    row.title.clone(),
                    row.estimate
                        .map(|estimate| table::hours(estimate.as_secs() / 60))
                        .unwrap_or_default(),
                    table::hours(row.billable.as_secs() / 60),
                ],
                is_ticked: false,
            })
            .collect(),
    }
}

fn steps(
    model: CloseOutModel,
    period: &Period,
    settings: &WorkspaceSettings,
    table: &Table,
    statuses: [StepStatus; 4],
) -> Vec<Step> {
    let first = match model {
        CloseOutModel::Fte => "Toggl flat entries".to_owned(),
        CloseOutModel::Hourly | CloseOutModel::Fixed => "Toggl entries".to_owned(),
    };
    let (second_kind, second) = match model {
        CloseOutModel::Fte => (
            StepKind::Allocate,
            format!("Allocate {} h per day", hours_text(settings.hours_per_day)),
        ),
        CloseOutModel::Hourly => (StepKind::Review, "Review billable hours".to_owned()),
        CloseOutModel::Fixed => (StepKind::Review, "Review the caps".to_owned()),
    };
    let close = format!("Close the {}", period_noun(period.kind()));
    [
        (StepKind::Toggl, first),
        (second_kind, second),
        (StepKind::Table, table.title.clone()),
        (StepKind::Close, close),
    ]
    .into_iter()
    .zip(statuses)
    .map(|((kind, title), status)| Step {
        kind,
        title,
        status,
    })
    .collect()
}

/// Whether `day`'s log already holds the `status` line `text`, written by a
/// closing whose record did not survive.
fn has_closing_line(
    writer: &VaultWriter<'_>,
    day: vault_base::Date,
    text: &str,
) -> Result<bool, CloseOutError> {
    let closing = format!("{} {VERB_SEPARATOR} {text}", LogKind::Status.as_str());
    Ok(writer
        .day_log_lines(day)?
        .iter()
        .any(|line| line.kind == Some(LogKind::Status) && line.text == closing))
}

fn hours_text(hours: f64) -> String {
    let text = format!("{hours:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

const fn period_noun(kind: PeriodKind) -> &'static str {
    match kind {
        PeriodKind::Week => "week",
        PeriodKind::Month => "month",
    }
}

fn days_text(days: usize) -> String {
    if days == 1 {
        "1 day".to_owned()
    } else {
        format!("{days} days")
    }
}

/// Whether `date` is a weekday, for callers building touches.
#[must_use]
pub fn is_weekday(date: Date) -> bool {
    !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}
