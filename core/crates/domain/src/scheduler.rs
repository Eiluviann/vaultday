//! The scheduler: free slots of a working day around calendar events, the
//! sequential auto-schedule (`=`), re-plan from now (`r`) and the "over free
//! time" amount.
//!
//! Everything here is a function over values in the user's local civil time.
//! Calendar events arrive as input (the [`crate::ports::CalendarSource`]
//! adapter reads them); the scheduler never moves an event or a locked time
//! box, and it only ever writes unlocked projections for the unfinished tasks
//! of a [`DayPlan`]. Applying a result to the plan is a separate step
//! ([`apply`]), so a caller can preview before it commits.

use std::time::Duration;

use jiff::civil::{Date, DateTime, Time};

use crate::day_plan::{DayPlan, TimeBox};
use crate::error::{DomainError, Result};
use crate::task::{TaskId, TimeBoxId};
use crate::workspace::WorkspaceId;

/// The length a task without an estimate is projected with.
pub const DEFAULT_UNESTIMATED_DURATION: Duration = Duration::from_secs(60 * 60);

/// A half-open span of local civil time: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    start: DateTime,
    end: DateTime,
}

impl Span {
    /// A span from `start` to `end`.
    ///
    /// # Errors
    /// [`DomainError::ReversedSpan`] when `end` is not after `start`.
    pub fn new(start: DateTime, end: DateTime) -> Result<Self> {
        if end <= start {
            return Err(DomainError::ReversedSpan { start, end });
        }
        Ok(Self { start, end })
    }

    /// A span of `duration` starting at `start`.
    ///
    /// # Errors
    /// [`DomainError::ReversedSpan`] for a zero duration,
    /// [`DomainError::TimeOutOfRange`] when the end is unrepresentable.
    pub fn starting_at(start: DateTime, duration: Duration) -> Result<Self> {
        let end = add(start, duration)?;
        Self::new(start, end)
    }

    /// When the span starts.
    pub const fn start(&self) -> DateTime {
        self.start
    }

    /// When the span ends.
    pub const fn end(&self) -> DateTime {
        self.end
    }

    /// How long the span lasts.
    pub fn duration(&self) -> Duration {
        Duration::try_from(self.end.duration_since(self.start)).unwrap_or(Duration::ZERO)
    }

    /// Whether the two spans share any instant.
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Whether `other` lies entirely inside this span.
    pub fn contains(&self, other: &Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The part of this span inside `bounds`, or `None` when they do not overlap.
    pub fn clipped_to(&self, bounds: &Span) -> Option<Span> {
        let start = self.start.max(bounds.start);
        let end = self.end.min(bounds.end);
        (start < end).then_some(Span { start, end })
    }
}

/// The hours of one date the scheduler may place work in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkingDay {
    date: Date,
    start: Time,
    end: Time,
}

impl WorkingDay {
    /// A working day on `date` from `start` to `end`.
    ///
    /// # Errors
    /// [`DomainError::ReversedSpan`] when `end` is not after `start`.
    pub fn new(date: Date, start: Time, end: Time) -> Result<Self> {
        if end <= start {
            return Err(DomainError::ReversedSpan {
                start: date.to_datetime(start),
                end: date.to_datetime(end),
            });
        }
        Ok(Self { date, start, end })
    }

    /// The date.
    pub const fn date(&self) -> Date {
        self.date
    }

    /// The day as a span.
    pub fn span(&self) -> Span {
        Span {
            start: self.date.to_datetime(self.start),
            end: self.date.to_datetime(self.end),
        }
    }
}

/// A calendar event in local time, as planned around.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The event's id in its calendar store.
    pub id: String,
    /// The calendar the event belongs to.
    pub calendar: String,
    /// The title.
    pub title: String,
    /// When the event runs.
    pub span: Span,
}

impl Event {
    /// An event with the given fields.
    pub fn new(
        id: impl Into<String>,
        calendar: impl Into<String>,
        title: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            id: id.into(),
            calendar: calendar.into(),
            title: title.into(),
            span,
        }
    }
}

/// The events of one workspace's calendars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCalendar {
    /// The workspace.
    pub workspace: WorkspaceId,
    /// Its events.
    pub events: Vec<Event>,
}

/// Which workspaces' events count as busy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// One workspace: only its own events.
    Workspace(WorkspaceId),
    /// The All view: every workspace's events.
    All,
}

/// The events visible in a view: the workspace's own, or the union for All.
pub fn events_in_view<'a>(calendars: &'a [WorkspaceCalendar], view: &View) -> Vec<&'a Event> {
    calendars
        .iter()
        .filter(|calendar| match view {
            View::Workspace(id) => calendar.workspace == *id,
            View::All => true,
        })
        .flat_map(|calendar| calendar.events.iter())
        .collect()
}

/// The spans of a set of events.
pub fn spans<'a>(events: impl IntoIterator<Item = &'a Event>) -> Vec<Span> {
    events.into_iter().map(|event| event.span).collect()
}

/// The free slots of the working day: the day minus the busy spans, in time
/// order, with touching slots merged. Busy spans outside the day are ignored.
pub fn free_slots(day: &WorkingDay, busy: impl IntoIterator<Item = Span>) -> Vec<Span> {
    free_slots_within(&day.span(), busy)
}

/// The total length of the slots.
pub fn free_time(slots: &[Span]) -> Duration {
    slots.iter().map(Span::duration).sum()
}

/// What the user chose and the app remembers for the auto-schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleOptions {
    /// The gap left after each projected block before the next one.
    pub buffer: Duration,
    /// The length given to a task without an estimate.
    pub unestimated_duration: Duration,
}

impl Default for ScheduleOptions {
    fn default() -> Self {
        Self {
            buffer: Duration::ZERO,
            unestimated_duration: DEFAULT_UNESTIMATED_DURATION,
        }
    }
}

/// An unlocked time box the scheduler proposes for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    /// The task.
    pub task: TaskId,
    /// Where it lands; always unlocked.
    pub time_box: TimeBox,
}

/// The outcome of an auto-schedule or a re-plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// The instant the scheduling started from.
    pub from: DateTime,
    /// The projected blocks, in list order.
    pub projected: Vec<Projection>,
    /// Unfinished, unlocked tasks that found no slot before the day ends, in list order.
    pub unplaced: Vec<TaskId>,
    /// Free time between `from` and the end of the day around events and kept boxes.
    pub free_time: Duration,
    /// How much the work to place exceeds that free time; zero when it all fits.
    pub over_free_time: Duration,
}

/// `=`: lays every unfinished, unlocked task into the free slots in list order,
/// starting at `start` (or at the working day's start when that is later),
/// leaving `options.buffer` after each block. Locked boxes, events and the
/// boxes of finished tasks stay where they are and count as busy. A task keeps
/// the id of the unlocked box it already holds; `new_id` names a box for a
/// task without one. A block whose end would leave the representable range
/// does not fit anywhere and is unplaced.
///
/// # Panics
/// When `plan` and `day` are not on the same date; that is the caller's bug.
pub fn auto_schedule(
    plan: &DayPlan,
    day: &WorkingDay,
    busy: &[Span],
    start: DateTime,
    options: &ScheduleOptions,
    new_id: &mut dyn FnMut(&TaskId) -> TimeBoxId,
) -> Schedule {
    project(plan, day, busy, start, options, new_id)
}

/// `r`: re-plans from `now`, touching only unfinished, unlocked tasks. Same
/// rules as [`auto_schedule`]; the two differ only in where the start comes from.
///
/// # Panics
/// When `plan` and `day` are not on the same date; that is the caller's bug.
pub fn replan_from_now(
    plan: &DayPlan,
    day: &WorkingDay,
    busy: &[Span],
    now: DateTime,
    options: &ScheduleOptions,
    new_id: &mut dyn FnMut(&TaskId) -> TimeBoxId,
) -> Schedule {
    project(plan, day, busy, now, options, new_id)
}

/// Writes a schedule into the plan: projected boxes are set, and an unplaced
/// task loses the unlocked box it held. Tasks the schedule does not mention
/// are untouched, and so is a task that was locked or finished after the
/// schedule was computed: a stale schedule never moves a locked box. A
/// projected box that now overlaps a box locked or finished since is not
/// written either, and its task keeps the box it holds. The write is all or
/// nothing: on an error the plan is left as it was.
///
/// # Errors
/// [`DomainError::TaskNotInPlan`] when the schedule names a task the plan no
/// longer holds, or any error of [`DayPlan::set_time_box`].
pub fn apply(plan: &mut DayPlan, schedule: &Schedule) -> Result<()> {
    let still_floating = |task: &TaskId| -> Result<bool> {
        let entry = plan.entry(task).ok_or_else(|| DomainError::TaskNotInPlan {
            id: task.clone(),
            date: plan.date(),
        })?;
        let is_locked = entry.time_box().is_some_and(TimeBox::is_locked);
        Ok(entry.task().is_unfinished() && !is_locked)
    };
    let kept = kept_boxes(plan);
    let lands_on_kept_box = |projection: &Projection| {
        box_span(&projection.time_box)
            .is_some_and(|block| kept.iter().any(|span| span.overlaps(&block)))
    };
    let to_set: Vec<&Projection> = schedule
        .projected
        .iter()
        .map(|projection| {
            let is_writable = still_floating(&projection.task)? && !lands_on_kept_box(projection);
            Ok(is_writable.then_some(projection))
        })
        .collect::<Result<Vec<Option<&Projection>>>>()?
        .into_iter()
        .flatten()
        .collect();
    let to_clear: Vec<&TaskId> = schedule
        .unplaced
        .iter()
        .map(|task| {
            let holds_box = plan
                .entry(task)
                .is_some_and(|entry| entry.time_box().is_some());
            Ok((still_floating(task)? && holds_box).then_some(task))
        })
        .collect::<Result<Vec<Option<&TaskId>>>>()?
        .into_iter()
        .flatten()
        .collect();
    // Written into a copy first, so a refused box leaves the plan untouched.
    let mut updated = plan.clone();
    for projection in to_set {
        updated.set_time_box(&projection.task, projection.time_box.clone())?;
    }
    for task in to_clear {
        updated.clear_time_box(task)?;
    }
    *plan = updated;
    Ok(())
}

fn project(
    plan: &DayPlan,
    day: &WorkingDay,
    busy: &[Span],
    from: DateTime,
    options: &ScheduleOptions,
    new_id: &mut dyn FnMut(&TaskId) -> TimeBoxId,
) -> Schedule {
    assert_eq!(
        plan.date(),
        day.date(),
        "project: the plan and the working day are on different dates"
    );
    let day_span = day.span();
    let from = from.max(day_span.start);
    let kept = kept_boxes(plan);
    let remaining_day = Span::new(from, day_span.end).ok();
    let slots = remaining_day.map_or_else(Vec::new, |remaining| {
        free_slots_within(&remaining, busy.iter().chain(&kept).copied())
    });

    let mut projected = Vec::new();
    let mut unplaced = Vec::new();
    let mut needed = Duration::ZERO;
    let mut cursor = from;
    for entry in plan.entries() {
        let task = entry.task();
        let is_locked = entry.time_box().is_some_and(TimeBox::is_locked);
        if !task.is_unfinished() || is_locked {
            continue;
        }
        let duration = block_length(
            task.estimate().map(|estimate| estimate.as_duration()),
            options,
        );
        needed = needed.saturating_add(duration);
        let Some(start) = first_fit(&slots, cursor, duration) else {
            unplaced.push(task.id().clone());
            continue;
        };
        let id = entry
            .time_box()
            .map_or_else(|| new_id(task.id()), |held| held.id.clone());
        projected.push(Projection {
            task: task.id().clone(),
            time_box: TimeBox::unlocked(id, start, duration),
        });
        // A cursor past the day places nothing more, so saturating at the day's end is exact.
        cursor = add(start, duration)
            .and_then(|end| add(end, options.buffer))
            .unwrap_or(day_span.end);
    }

    let free = free_time(&slots);
    Schedule {
        from,
        projected,
        unplaced,
        free_time: free,
        over_free_time: needed.saturating_sub(free),
    }
}

/// The spans the scheduler must not touch: locked boxes and the boxes of
/// finished tasks. An empty box blocks nothing; a box whose end is
/// unrepresentable blocks the rest of time.
fn kept_boxes(plan: &DayPlan) -> Vec<Span> {
    plan.entries()
        .iter()
        .filter_map(|entry| {
            let time_box = entry.time_box()?;
            (time_box.is_locked() || !entry.task().is_unfinished()).then_some(time_box)
        })
        .filter_map(box_span)
        .collect()
}

/// The span a box covers: `None` for an empty box, up to the end of time for
/// one whose end is unrepresentable.
fn box_span(time_box: &TimeBox) -> Option<Span> {
    let end = time_box.end().unwrap_or(DateTime::MAX);
    Span::new(time_box.start, end).ok()
}

fn block_length(estimate: Option<Duration>, options: &ScheduleOptions) -> Duration {
    match estimate {
        Some(duration) if !duration.is_zero() => duration,
        _ => options.unestimated_duration,
    }
}

/// The earliest start at or after `cursor` where a block of `duration` fits in
/// one slot. A block whose end is unrepresentable fits nowhere.
fn first_fit(slots: &[Span], cursor: DateTime, duration: Duration) -> Option<DateTime> {
    slots.iter().find_map(|slot| {
        let start = cursor.max(slot.start);
        if start >= slot.end {
            return None;
        }
        let end = add(start, duration).ok()?;
        (end <= slot.end).then_some(start)
    })
}

fn free_slots_within(bounds: &Span, busy: impl IntoIterator<Item = Span>) -> Vec<Span> {
    let mut clipped: Vec<Span> = busy
        .into_iter()
        .filter_map(|span| span.clipped_to(bounds))
        .collect();
    clipped.sort();
    let mut free = Vec::new();
    let mut cursor = bounds.start;
    for span in clipped {
        if span.start > cursor {
            free.push(Span {
                start: cursor,
                end: span.start,
            });
        }
        cursor = cursor.max(span.end);
    }
    if cursor < bounds.end {
        free.push(Span {
            start: cursor,
            end: bounds.end,
        });
    }
    free
}

fn add(start: DateTime, duration: Duration) -> Result<DateTime> {
    start
        .checked_add(duration)
        .map_err(|_| DomainError::TimeOutOfRange)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::day_plan::Lock;
    use crate::estimate::Estimate;
    use crate::task::{Task, VaultTask};
    use jiff::civil::{date, time};
    use proptest::prelude::*;

    const DAY: Date = date(2026, 10, 22);

    fn at(hour: i8, minute: i8) -> DateTime {
        DAY.at(hour, minute, 0, 0)
    }

    fn span(start: (i8, i8), end: (i8, i8)) -> Span {
        Span::new(at(start.0, start.1), at(end.0, end.1)).unwrap()
    }

    fn working_day() -> WorkingDay {
        WorkingDay::new(DAY, time(9, 0, 0, 0), time(17, 0, 0, 0)).unwrap()
    }

    fn minutes(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn task(id: &str, estimate_minutes: Option<u64>) -> Task {
        let mut vault = VaultTask::open(id, format!("Task {id}"));
        vault.estimate = estimate_minutes.map(Estimate::from_minutes);
        Task::new(vault)
    }

    fn box_id_counter() -> impl FnMut(&TaskId) -> TimeBoxId {
        let mut next = 0;
        move |task: &TaskId| {
            next += 1;
            TimeBoxId::new(format!("box-{task}-{next}"))
        }
    }

    fn projected_spans(schedule: &Schedule) -> Vec<Span> {
        schedule
            .projected
            .iter()
            .map(|projection| {
                Span::new(
                    projection.time_box.start,
                    projection.time_box.end().unwrap(),
                )
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn spans_are_half_open_and_refuse_to_run_backwards() {
        let morning = span((9, 0), (10, 0));
        let next = span((10, 0), (11, 0));
        assert!(!morning.overlaps(&next));
        assert!(morning.overlaps(&span((9, 59), (10, 30))));
        assert!(span((9, 0), (12, 0)).contains(&next));
        assert_eq!(morning.duration(), minutes(60));
        assert_eq!(
            Span::new(at(10, 0), at(10, 0)),
            Err(DomainError::ReversedSpan {
                start: at(10, 0),
                end: at(10, 0),
            })
        );
        assert!(Span::starting_at(at(10, 0), Duration::ZERO).is_err());
        assert_eq!(
            span((8, 0), (9, 30)).clipped_to(&working_day().span()),
            Some(span((9, 0), (9, 30)))
        );
        assert_eq!(span((7, 0), (8, 0)).clipped_to(&working_day().span()), None);
        assert!(WorkingDay::new(DAY, time(9, 0, 0, 0), time(9, 0, 0, 0)).is_err());
    }

    #[test]
    fn free_slots_subtract_and_merge_the_busy_spans() {
        let slots = free_slots(
            &working_day(),
            [
                span((10, 0), (11, 0)),
                span((10, 30), (11, 30)),
                span((13, 0), (14, 0)),
                span((14, 0), (14, 30)),
                span((16, 30), (18, 0)),
                span((6, 0), (7, 0)),
            ],
        );
        assert_eq!(
            slots,
            vec![
                span((9, 0), (10, 0)),
                span((11, 30), (13, 0)),
                span((14, 30), (16, 30)),
            ]
        );
        assert_eq!(free_time(&slots), minutes(60 + 90 + 120));
        assert_eq!(free_slots(&working_day(), []), vec![working_day().span()]);
        assert!(free_slots(&working_day(), [span((8, 0), (18, 0))]).is_empty());
    }

    #[test]
    fn the_all_view_unites_the_workspaces_events() {
        let event = |id: &str, calendar: &str| {
            Event::new(id, calendar, format!("Event {id}"), span((10, 0), (11, 0)))
        };
        let calendars = vec![
            WorkspaceCalendar {
                workspace: WorkspaceId::new("acme"),
                events: vec![event("a1", "Acme"), event("a2", "Acme")],
            },
            WorkspaceCalendar {
                workspace: WorkspaceId::new("home"),
                events: vec![event("h1", "Home")],
            },
        ];
        let ids = |events: Vec<&Event>| -> Vec<String> {
            events.into_iter().map(|event| event.id.clone()).collect()
        };
        assert_eq!(
            ids(events_in_view(
                &calendars,
                &View::Workspace(WorkspaceId::new("acme"))
            )),
            ["a1", "a2"]
        );
        assert_eq!(
            ids(events_in_view(&calendars, &View::All)),
            ["a1", "a2", "h1"]
        );
        assert!(events_in_view(&calendars, &View::Workspace(WorkspaceId::new("none"))).is_empty());
        assert_eq!(spans(events_in_view(&calendars, &View::All)).len(), 3);
    }

    #[test]
    fn auto_schedule_lays_tasks_in_list_order_around_events_and_locked_boxes() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(60))).unwrap();
        plan.add(task("t2", Some(30))).unwrap();
        plan.add(task("t3", None)).unwrap();
        plan.add(task("t4", Some(45))).unwrap();
        plan.set_time_box(
            &TaskId::new("t4"),
            TimeBox {
                id: TimeBoxId::new("box-locked"),
                start: at(11, 0),
                duration: minutes(60),
                lock: Lock::Locked,
            },
        )
        .unwrap();
        let busy = [span((10, 0), (10, 30))];
        let options = ScheduleOptions {
            buffer: minutes(15),
            unestimated_duration: minutes(60),
        };
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &busy,
            at(9, 0),
            &options,
            &mut box_id_counter(),
        );

        // t1 09:00–10:00, buffer to 10:15 but the event runs to 10:30, so t2 10:30–11:00;
        // the locked box fills 11:00–12:00; t3 (no estimate, 1 h) 12:00–13:00.
        assert_eq!(
            projected_spans(&schedule),
            vec![
                span((9, 0), (10, 0)),
                span((10, 30), (11, 0)),
                span((12, 0), (13, 0)),
            ]
        );
        assert!(schedule.unplaced.is_empty());
        assert_eq!(schedule.free_time, minutes(8 * 60 - 30 - 60));
        assert_eq!(schedule.over_free_time, Duration::ZERO);
        assert!(
            schedule
                .projected
                .iter()
                .all(|projection| !projection.time_box.is_locked())
        );
        assert!(
            schedule
                .projected
                .iter()
                .all(|projection| projection.task != TaskId::new("t4"))
        );

        apply(&mut plan, &schedule).unwrap();
        let locked = plan.entry(&TaskId::new("t4")).unwrap().time_box().unwrap();
        assert_eq!(locked.start, at(11, 0));
        assert!(locked.is_locked());
        assert_eq!(
            plan.entry(&TaskId::new("t2"))
                .unwrap()
                .time_box()
                .unwrap()
                .start,
            at(10, 30)
        );
    }

    #[test]
    fn a_chosen_start_before_the_day_is_clamped_to_the_day() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(30))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(6, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(schedule.from, at(9, 0));
        assert_eq!(projected_spans(&schedule), vec![span((9, 0), (9, 30))]);
    }

    #[test]
    fn work_that_does_not_fit_is_unplaced_and_counted_over_free_time() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(120))).unwrap();
        plan.add(task("t2", Some(90))).unwrap();
        plan.add(task("t3", Some(30))).unwrap();
        // Free from 14:00: 14:00–15:00 and 15:30–17:00 (2.5 h); work asks for 4 h.
        let busy = [span((15, 0), (15, 30))];
        let schedule = replan_from_now(
            &plan,
            &working_day(),
            &busy,
            at(14, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        // t1 (2 h) does not fit 14:00–15:00; it fits nowhere, so it is skipped;
        // t2 (1.5 h) fits 15:30–17:00; t3 (30 m) then has no room after it.
        // Blocks are never split and never reordered.
        assert_eq!(projected_spans(&schedule), vec![span((15, 30), (17, 0))]);
        assert_eq!(
            schedule.unplaced,
            vec![TaskId::new("t1"), TaskId::new("t3")]
        );
        assert_eq!(schedule.free_time, minutes(150));
        assert_eq!(schedule.over_free_time, minutes(240 - 150));
    }

    #[test]
    fn replan_from_now_leaves_finished_and_locked_items_alone() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("done", Some(60))).unwrap();
        plan.add(task("open", Some(60))).unwrap();
        plan.add(task("pinned", Some(30))).unwrap();
        plan.set_time_box(
            &TaskId::new("done"),
            TimeBox::unlocked("box-done", at(9, 0), minutes(60)),
        )
        .unwrap();
        plan.complete(&TaskId::new("done"), DAY).unwrap();
        plan.set_time_box(
            &TaskId::new("open"),
            TimeBox::unlocked("box-open", at(10, 0), minutes(60)),
        )
        .unwrap();
        plan.set_time_box(
            &TaskId::new("pinned"),
            TimeBox::unlocked("box-pinned", at(13, 0), minutes(30)),
        )
        .unwrap();
        plan.lock(&TaskId::new("pinned")).unwrap();

        let schedule = replan_from_now(
            &plan,
            &working_day(),
            &[],
            at(12, 45),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        // Only the open task moves: 12:45 has no room before the locked box, so 13:30.
        assert_eq!(schedule.projected.len(), 1);
        assert_eq!(schedule.projected[0].task, TaskId::new("open"));
        assert_eq!(
            schedule.projected[0].time_box.id,
            TimeBoxId::new("box-open")
        );
        assert_eq!(projected_spans(&schedule), vec![span((13, 30), (14, 30))]);

        apply(&mut plan, &schedule).unwrap();
        let done_box = plan
            .entry(&TaskId::new("done"))
            .unwrap()
            .time_box()
            .unwrap();
        assert_eq!(done_box.start, at(9, 0));
        let pinned = plan
            .entry(&TaskId::new("pinned"))
            .unwrap()
            .time_box()
            .unwrap();
        assert_eq!(pinned.start, at(13, 0));
        assert!(pinned.is_locked());
    }

    #[test]
    fn a_task_with_a_zero_estimate_is_projected_as_unestimated() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(0))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(projected_spans(&schedule), vec![span((9, 0), (10, 0))]);
    }

    #[test]
    fn applying_clears_the_unlocked_box_of_an_unplaced_task() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(600))).unwrap();
        plan.set_time_box(
            &TaskId::new("t1"),
            TimeBox::unlocked("box-1", at(9, 0), minutes(60)),
        )
        .unwrap();
        let schedule = replan_from_now(
            &plan,
            &working_day(),
            &[],
            at(16, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(schedule.unplaced, vec![TaskId::new("t1")]);
        apply(&mut plan, &schedule).unwrap();
        assert!(plan.entry(&TaskId::new("t1")).unwrap().time_box().is_none());

        plan.remove(&TaskId::new("t1")).unwrap();
        assert!(matches!(
            apply(&mut plan, &schedule),
            Err(DomainError::TaskNotInPlan { .. })
        ));
    }

    #[test]
    fn a_stale_schedule_never_moves_a_box_locked_or_finished_since() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("a", Some(60))).unwrap();
        plan.add(task("b", Some(60))).unwrap();
        plan.add(task("c", Some(600))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(schedule.projected.len(), 2);
        assert_eq!(schedule.unplaced, vec![TaskId::new("c")]);

        // Meanwhile the user locks `a` at 14:00, finishes `b`, and pins `c` too.
        plan.set_time_box(
            &TaskId::new("a"),
            TimeBox {
                id: TimeBoxId::new("pin-a"),
                start: at(14, 0),
                duration: minutes(60),
                lock: Lock::Locked,
            },
        )
        .unwrap();
        plan.set_time_box(
            &TaskId::new("b"),
            TimeBox::unlocked("box-b", at(15, 0), minutes(60)),
        )
        .unwrap();
        plan.complete(&TaskId::new("b"), DAY).unwrap();
        plan.set_time_box(
            &TaskId::new("c"),
            TimeBox {
                id: TimeBoxId::new("pin-c"),
                start: at(16, 0),
                duration: minutes(60),
                lock: Lock::Locked,
            },
        )
        .unwrap();
        let before = plan.clone();
        apply(&mut plan, &schedule).unwrap();
        assert_eq!(plan, before);
    }

    #[test]
    fn a_stale_schedule_never_writes_over_a_box_locked_since() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("a", Some(60))).unwrap();
        plan.add(task("b", Some(60))).unwrap();
        plan.add(task("pinned", Some(30))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(
            projected_spans(&schedule),
            vec![
                span((9, 0), (10, 0)),
                span((10, 0), (11, 0)),
                span((11, 0), (11, 30)),
            ]
        );

        // Meanwhile the user pins `pinned` at 09:30, inside the block the schedule gave `a`.
        plan.set_time_box(
            &TaskId::new("pinned"),
            TimeBox {
                id: TimeBoxId::new("pin"),
                start: at(9, 30),
                duration: minutes(30),
                lock: Lock::Locked,
            },
        )
        .unwrap();
        apply(&mut plan, &schedule).unwrap();

        assert!(plan.entry(&TaskId::new("a")).unwrap().time_box().is_none());
        assert_eq!(
            plan.entry(&TaskId::new("b"))
                .unwrap()
                .time_box()
                .unwrap()
                .start,
            at(10, 0)
        );
        let pinned = plan
            .entry(&TaskId::new("pinned"))
            .unwrap()
            .time_box()
            .unwrap();
        assert_eq!(pinned.start, at(9, 30));
        assert!(pinned.is_locked());
    }

    #[test]
    fn apply_writes_nothing_when_a_box_id_is_taken() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("a", Some(60))).unwrap();
        plan.add(task("b", Some(60))).unwrap();
        // A caller whose counter hands out the same id twice.
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut |_: &TaskId| TimeBoxId::new("same"),
        );
        let before = plan.clone();
        assert!(matches!(
            apply(&mut plan, &schedule),
            Err(DomainError::DuplicateTimeBoxId { .. })
        ));
        assert_eq!(plan, before);
    }

    #[test]
    fn apply_writes_nothing_when_a_task_has_left_the_plan() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("a", Some(60))).unwrap();
        plan.add(task("b", Some(60))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        plan.remove(&TaskId::new("b")).unwrap();
        let before = plan.clone();
        assert!(matches!(
            apply(&mut plan, &schedule),
            Err(DomainError::TaskNotInPlan { .. })
        ));
        assert_eq!(plan, before);
    }

    #[test]
    fn empty_and_endless_kept_boxes_do_not_fail_the_schedule() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("empty", Some(30))).unwrap();
        plan.add(task("open", Some(30))).unwrap();
        plan.set_time_box(
            &TaskId::new("empty"),
            TimeBox {
                id: TimeBoxId::new("pin-empty"),
                start: at(10, 0),
                duration: Duration::ZERO,
                lock: Lock::Locked,
            },
        )
        .unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(10, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(projected_spans(&schedule), vec![span((10, 0), (10, 30))]);

        plan.add(task("endless", Some(30))).unwrap();
        plan.set_time_box(
            &TaskId::new("endless"),
            TimeBox {
                id: TimeBoxId::new("pin-endless"),
                start: at(12, 0),
                duration: Duration::MAX,
                lock: Lock::Locked,
            },
        )
        .unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(11, 45),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert!(schedule.projected.is_empty());
        assert_eq!(schedule.unplaced, vec![TaskId::new("open")]);
        assert_eq!(schedule.free_time, minutes(15));
    }

    #[test]
    fn an_oversized_estimate_or_buffer_is_unplaced_not_an_error() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("huge", Some(u64::MAX / 60))).unwrap();
        plan.add(task("small", Some(30))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert_eq!(schedule.unplaced, vec![TaskId::new("huge")]);
        assert_eq!(projected_spans(&schedule), vec![span((9, 0), (9, 30))]);

        let options = ScheduleOptions {
            buffer: Duration::MAX,
            ..ScheduleOptions::default()
        };
        plan.remove(&TaskId::new("huge")).unwrap();
        plan.add(task("after", Some(30))).unwrap();
        let schedule = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &options,
            &mut box_id_counter(),
        );
        assert_eq!(projected_spans(&schedule), vec![span((9, 0), (9, 30))]);
        assert_eq!(schedule.unplaced, vec![TaskId::new("after")]);
    }

    #[test]
    #[should_panic(expected = "different dates")]
    fn a_plan_on_another_date_than_the_day_is_a_contract_violation() {
        let plan = DayPlan::new(date(2026, 10, 23));
        let _ = auto_schedule(
            &plan,
            &working_day(),
            &[],
            at(9, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
    }

    #[test]
    fn a_start_after_the_day_places_nothing() {
        let mut plan = DayPlan::new(DAY);
        plan.add(task("t1", Some(30))).unwrap();
        let schedule = replan_from_now(
            &plan,
            &working_day(),
            &[],
            at(17, 0),
            &ScheduleOptions::default(),
            &mut box_id_counter(),
        );
        assert!(schedule.projected.is_empty());
        assert_eq!(schedule.unplaced, vec![TaskId::new("t1")]);
        assert_eq!(schedule.free_time, Duration::ZERO);
        assert_eq!(schedule.over_free_time, minutes(30));
    }

    /// One generated task: estimate, an optional pre-set box and its lock, and whether it is finished.
    #[derive(Debug, Clone)]
    struct GeneratedTask {
        estimate_minutes: Option<u16>,
        preset_box: Option<(u16, u16, bool)>,
        is_finished: bool,
    }

    const DAY_MINUTES: u16 = 8 * 60;

    fn generated_task() -> impl Strategy<Value = GeneratedTask> {
        (
            proptest::option::of(0_u16..=240),
            proptest::option::of((0_u16..DAY_MINUTES, 0_u16..=120, any::<bool>())),
            prop::bool::weighted(0.2),
        )
            .prop_map(
                |(estimate_minutes, preset_box, is_finished)| GeneratedTask {
                    estimate_minutes,
                    preset_box,
                    is_finished,
                },
            )
    }

    fn generated_events() -> impl Strategy<Value = Vec<(u16, u16)>> {
        prop::collection::vec((0_u16..DAY_MINUTES, 5_u16..=120), 0..5)
    }

    fn at_minute(offset: impl Into<i64>) -> DateTime {
        at(9, 0)
            .checked_add(jiff::SignedDuration::from_mins(offset.into()))
            .unwrap()
    }

    fn build_plan(tasks: &[GeneratedTask]) -> DayPlan {
        let mut plan = DayPlan::new(DAY);
        for (index, generated) in tasks.iter().enumerate() {
            let id = format!("t{index}");
            plan.add(task(&id, generated.estimate_minutes.map(u64::from)))
                .unwrap();
            if let Some((start, length, is_locked)) = generated.preset_box {
                let time_box = TimeBox {
                    id: TimeBoxId::new(format!("preset-{index}")),
                    start: at_minute(start),
                    duration: minutes(u64::from(length)),
                    lock: if is_locked {
                        Lock::Locked
                    } else {
                        Lock::Unlocked
                    },
                };
                plan.set_time_box(&TaskId::new(&id), time_box).unwrap();
            }
            if generated.is_finished {
                plan.complete(&TaskId::new(&id), DAY).unwrap();
            }
        }
        plan
    }

    fn busy_spans(events: &[(u16, u16)]) -> Vec<Span> {
        events
            .iter()
            .map(|(start, length)| {
                Span::starting_at(at_minute(*start), minutes(u64::from(*length))).unwrap()
            })
            .collect()
    }

    fn untouchable_spans(plan: &DayPlan, busy: &[Span]) -> Vec<Span> {
        let mut spans = busy.to_vec();
        spans.extend(kept_boxes(plan));
        spans
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn projected_blocks_never_overlap_events_or_kept_boxes(
            tasks in prop::collection::vec(generated_task(), 0..8),
            events in generated_events(),
            from in -120_i16..=540,
            buffer in 0_u16..=30,
        ) {
            let plan = build_plan(&tasks);
            let busy = busy_spans(&events);
            let options = ScheduleOptions { buffer: minutes(u64::from(buffer)), ..ScheduleOptions::default() };
            let schedule = replan_from_now(&plan, &working_day(), &busy, at_minute(from), &options, &mut box_id_counter());
            let untouchable = untouchable_spans(&plan, &busy);
            for block in projected_spans(&schedule) {
                for kept in &untouchable {
                    prop_assert!(!block.overlaps(kept), "{block:?} overlaps {kept:?}");
                }
            }
        }

        #[test]
        fn projected_blocks_keep_list_order_and_do_not_overlap_each_other(
            tasks in prop::collection::vec(generated_task(), 0..8),
            events in generated_events(),
            from in -120_i16..=540,
            buffer in 0_u16..=30,
        ) {
            let plan = build_plan(&tasks);
            let busy = busy_spans(&events);
            let options = ScheduleOptions { buffer: minutes(u64::from(buffer)), ..ScheduleOptions::default() };
            let schedule = auto_schedule(&plan, &working_day(), &busy, at_minute(from), &options, &mut box_id_counter());
            let positions: Vec<usize> = schedule
                .projected
                .iter()
                .map(|projection| plan.position_of(&projection.task).unwrap())
                .collect();
            prop_assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "list order broken: {positions:?}");
            let blocks = projected_spans(&schedule);
            for pair in blocks.windows(2) {
                prop_assert!(pair[0].end() <= pair[1].start(), "{:?} runs into {:?}", pair[0], pair[1]);
                let gap = Duration::try_from(pair[1].start().duration_since(pair[0].end())).unwrap();
                prop_assert!(gap >= options.buffer, "gap {gap:?} is under the buffer {:?}", options.buffer);
            }
        }

        #[test]
        fn every_projected_block_lies_inside_the_working_day(
            tasks in prop::collection::vec(generated_task(), 0..8),
            events in generated_events(),
            from in -120_i16..=540,
        ) {
            let plan = build_plan(&tasks);
            let busy = busy_spans(&events);
            let schedule = replan_from_now(&plan, &working_day(), &busy, at_minute(from), &ScheduleOptions::default(), &mut box_id_counter());
            let day = working_day().span();
            for block in projected_spans(&schedule) {
                prop_assert!(day.contains(&block), "{block:?} leaves the day");
                prop_assert!(block.start() >= schedule.from);
            }
            let placed_or_not = schedule.projected.len() + schedule.unplaced.len();
            let candidates = plan.entries().iter().filter(|entry| {
                entry.task().is_unfinished() && !entry.time_box().is_some_and(TimeBox::is_locked)
            }).count();
            prop_assert_eq!(placed_or_not, candidates);
        }

        #[test]
        fn replan_is_idempotent(
            tasks in prop::collection::vec(generated_task(), 0..8),
            events in generated_events(),
            from in -120_i16..=540,
            buffer in 0_u16..=30,
        ) {
            let mut plan = build_plan(&tasks);
            let busy = busy_spans(&events);
            let options = ScheduleOptions { buffer: minutes(u64::from(buffer)), ..ScheduleOptions::default() };
            let first = replan_from_now(&plan, &working_day(), &busy, at_minute(from), &options, &mut box_id_counter());
            apply(&mut plan, &first).unwrap();
            let after_first = plan.clone();
            let second = replan_from_now(&plan, &working_day(), &busy, at_minute(from), &options, &mut box_id_counter());
            prop_assert_eq!(&second, &first);
            apply(&mut plan, &second).unwrap();
            prop_assert_eq!(plan, after_first);
        }
    }
}
