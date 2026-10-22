//! The day plan: the ordered tasks of one date, their time boxes, the moves
//! between days and the shutdown rollover.
//!
//! The plan owns its tasks' [`crate::task::PlanState`]: adding, moving and
//! rolling over keep `planned_day`, `list_position` and `time_box` in step.
//! Scheduling (where the unlocked boxes land) is the scheduler's job in a
//! later crate; here a time box is a value the plan stores and locks.

use std::time::Duration;

use jiff::civil::{Date, DateTime};

use crate::error::{DomainError, Result};
use crate::estimate::Estimate;
use crate::task::{Task, TaskId, TaskStatus, TimeBoxId, VaultTask};

/// Whether a time box keeps its slot through re-plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lock {
    /// Written to the private calendar; the scheduler never moves it.
    Locked,
    /// A projection that floats as the day changes.
    Unlocked,
}

/// The slot a task holds in the day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeBox {
    /// The box's id, shared with the calendar event written for it once locked.
    pub id: TimeBoxId,
    /// When the box starts, in the user's local time.
    pub start: DateTime,
    /// How long the box lasts.
    pub duration: Duration,
    /// Locked or unlocked.
    pub lock: Lock,
}

impl TimeBox {
    /// An unlocked box.
    pub fn unlocked(id: impl Into<TimeBoxId>, start: DateTime, duration: Duration) -> Self {
        Self {
            id: id.into(),
            start,
            duration,
            lock: Lock::Unlocked,
        }
    }

    /// Whether the box is locked.
    pub const fn is_locked(&self) -> bool {
        matches!(self.lock, Lock::Locked)
    }

    /// When the box ends.
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] when the end is unrepresentable.
    pub fn end(&self) -> Result<DateTime> {
        self.start
            .checked_add(self.duration)
            .map_err(|_| DomainError::TimeOutOfRange)
    }
}

/// A task in the plan with its time box, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEntry {
    task: Task,
    time_box: Option<TimeBox>,
}

impl PlanEntry {
    /// The task.
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// The task's time box, if any.
    pub fn time_box(&self) -> Option<&TimeBox> {
        self.time_box.as_ref()
    }
}

/// A task taken out of a plan, with the time box it held so the caller can
/// delete the calendar event of a locked one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    /// The task, no longer referencing the box.
    pub task: Task,
    /// The box the task held, if any.
    pub time_box: Option<TimeBox>,
}

/// Why a task rolled over at shutdown, in the user's words.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RolloverReason(String);

impl RolloverReason {
    /// Wraps the reason text.
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// The reason text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The outcome of rolling one task over: the task, ready for the next day's plan,
/// and the record for the day log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolledOver {
    /// The task, with `planned_day` set to `to` and its counter incremented.
    pub task: Task,
    /// The box the task held on `from`, if any; a locked one has a calendar event to delete.
    pub time_box: Option<TimeBox>,
    /// The day the task left.
    pub from: Date,
    /// The day the task goes to.
    pub to: Date,
    /// Why it rolled over.
    pub reason: RolloverReason,
    /// Rollovers in a row, including this one.
    pub rollover_count: u32,
    /// The counter reached the backlog threshold: offer the backlog.
    pub is_backlog_candidate: bool,
}

/// The day's totals against the free time the calendar leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayTotals {
    /// Estimates of every task in the plan that is not cancelled.
    pub planned: Estimate,
    /// Estimates of the done tasks.
    pub done: Estimate,
    /// The free time given.
    pub free_time: Duration,
    /// How far `planned` exceeds `free_time`; zero when it fits.
    pub over_free_time: Duration,
    /// Unfinished tasks without an estimate; they are not in `planned`.
    pub unestimated: usize,
}

/// The ordered tasks of one date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayPlan {
    date: Date,
    entries: Vec<PlanEntry>,
}

impl DayPlan {
    /// An empty plan for `date`.
    pub const fn new(date: Date) -> Self {
        Self {
            date,
            entries: Vec::new(),
        }
    }

    /// The plan's date.
    pub const fn date(&self) -> Date {
        self.date
    }

    /// The number of tasks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the plan has no tasks.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries in list order.
    pub fn entries(&self) -> &[PlanEntry] {
        &self.entries
    }

    /// The tasks in list order.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.entries.iter().map(PlanEntry::task)
    }

    /// The entry for a task.
    pub fn entry(&self, id: &TaskId) -> Option<&PlanEntry> {
        self.entries.iter().find(|entry| entry.task.id() == id)
    }

    /// A task by id.
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.entry(id).map(PlanEntry::task)
    }

    /// The task's position, from zero.
    pub fn position_of(&self, id: &TaskId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.task.id() == id)
    }

    /// Appends a task at the bottom of the list.
    ///
    /// # Errors
    /// [`DomainError::TaskAlreadyInPlan`] when the task is already planned here.
    pub fn add(&mut self, task: Task) -> Result<()> {
        let position = self.entries.len();
        self.insert(task, position)
    }

    /// Puts a task at the top of the list, where a rolled-over task lands.
    ///
    /// # Errors
    /// [`DomainError::TaskAlreadyInPlan`] when the task is already planned here.
    pub fn add_at_top(&mut self, task: Task) -> Result<()> {
        self.insert(task, 0)
    }

    /// Inserts a task at `position`.
    ///
    /// # Errors
    /// [`DomainError::TaskAlreadyInPlan`] when the task is already planned here,
    /// [`DomainError::PositionOutOfRange`] when `position` is past the end.
    pub fn insert(&mut self, mut task: Task, position: usize) -> Result<()> {
        if self.position_of(task.id()).is_some() {
            return Err(DomainError::TaskAlreadyInPlan {
                id: task.id().clone(),
                date: self.date,
            });
        }
        self.ensure_position(position, self.entries.len())?;
        task.plan_mut().planned_day = Some(self.date);
        task.plan_mut().time_box = None;
        self.entries.insert(
            position,
            PlanEntry {
                task,
                time_box: None,
            },
        );
        self.renumber();
        Ok(())
    }

    /// Takes a task out of planning: its planned day is cleared, its time box
    /// handed back and its rollover streak reset.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here.
    pub fn remove(&mut self, id: &TaskId) -> Result<Removed> {
        let index = self.index_of(id)?;
        let mut removed = self.detach(index, None);
        removed.task.plan_mut().rollover_count = 0;
        Ok(removed)
    }

    /// Replaces a planned task's vault fields after its line changed in the
    /// vault; position and time box stay. A line that is now finished resets
    /// the rollover counter, as [`Task::update_vault`] does.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here.
    ///
    /// # Panics
    /// When `vault.id` is not `id`: the plan's task ids stay unique, so a line
    /// whose id changed is removed and added, never updated in place.
    pub fn update_task(&mut self, id: &TaskId, vault: VaultTask) -> Result<()> {
        let index = self.index_of(id)?;
        self.entries[index].task.update_vault(vault);
        Ok(())
    }

    /// Moves a task to `position` in the list.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::PositionOutOfRange`] when `position` is past the last task.
    pub fn reorder(&mut self, id: &TaskId, position: usize) -> Result<()> {
        let index = self.index_of(id)?;
        self.ensure_position(position, self.entries.len().saturating_sub(1))?;
        let entry = self.entries.remove(index);
        self.entries.insert(position, entry);
        self.renumber();
        Ok(())
    }

    /// `>>`: takes the task out, planned for the next day. Its time box is
    /// handed back and its rollover counter untouched; add it to tomorrow's
    /// plan to place it.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::TimeOutOfRange`] when there is no next day.
    pub fn move_to_tomorrow(&mut self, id: &TaskId) -> Result<Removed> {
        let index = self.index_of(id)?;
        let tomorrow = self.tomorrow()?;
        Ok(self.detach(index, Some(tomorrow)))
    }

    /// `<<`: brings a task from another day into this plan, at the bottom.
    ///
    /// # Errors
    /// [`DomainError::TaskAlreadyInPlan`] when the task is already planned here.
    pub fn pull_in(&mut self, task: Task) -> Result<()> {
        self.add(task)
    }

    /// Marks a task done on `date`; it stays in the plan.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here.
    pub fn complete(&mut self, id: &TaskId, date: Date) -> Result<()> {
        let index = self.index_of(id)?;
        self.entries[index].task.complete(date);
        Ok(())
    }

    /// Rolls one unfinished task over to the next day with a reason, counting
    /// the rollover. The task leaves this plan; add it to tomorrow's plan to place it.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::TaskFinished`] when it is done or cancelled,
    /// [`DomainError::TimeOutOfRange`] when there is no next day.
    pub fn roll_over(&mut self, id: &TaskId, reason: RolloverReason) -> Result<RolledOver> {
        let index = self.index_of(id)?;
        if !self.entries[index].task.is_unfinished() {
            return Err(DomainError::TaskFinished { id: id.clone() });
        }
        let tomorrow = self.tomorrow()?;
        let Removed { mut task, time_box } = self.detach(index, Some(tomorrow));
        task.plan_mut().rollover_count = task.plan().rollover_count.saturating_add(1);
        Ok(RolledOver {
            from: self.date,
            to: tomorrow,
            reason,
            rollover_count: task.plan().rollover_count,
            is_backlog_candidate: task.is_backlog_candidate(),
            task,
            time_box,
        })
    }

    /// Shutdown: rolls every unfinished task over with the same reason, in list
    /// order. Done and cancelled tasks stay.
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] when there is no next day.
    pub fn roll_over_unfinished(&mut self, reason: &RolloverReason) -> Result<Vec<RolledOver>> {
        let unfinished: Vec<TaskId> = self
            .tasks()
            .filter(|task| task.is_unfinished())
            .map(|task| task.id().clone())
            .collect();
        unfinished
            .iter()
            .map(|id| self.roll_over(id, reason.clone()))
            .collect()
    }

    /// Gives a task a time box, replacing any it had, locked or not: the caller
    /// that replaces a locked box owns its calendar event and moves it.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::TimeBoxOutsideDay`] when the box starts on another day,
    /// [`DomainError::DuplicateTimeBoxId`] when another task holds a box with that id.
    pub fn set_time_box(&mut self, id: &TaskId, time_box: TimeBox) -> Result<()> {
        let index = self.index_of(id)?;
        if time_box.start.date() != self.date {
            return Err(DomainError::TimeBoxOutsideDay {
                id: time_box.id,
                start: time_box.start.date(),
                date: self.date,
            });
        }
        if let Some(holder) = self
            .entries
            .iter()
            .filter(|entry| entry.task.id() != id)
            .find(|entry| {
                entry
                    .time_box
                    .as_ref()
                    .is_some_and(|held| held.id == time_box.id)
            })
        {
            return Err(DomainError::DuplicateTimeBoxId {
                id: time_box.id,
                holder: holder.task.id().clone(),
            });
        }
        let entry = &mut self.entries[index];
        entry.task.plan_mut().time_box = Some(time_box.id.clone());
        entry.time_box = Some(time_box);
        Ok(())
    }

    /// Locks the task's time box.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::NoTimeBox`] when it has no time box.
    pub fn lock(&mut self, id: &TaskId) -> Result<()> {
        self.set_lock(id, Lock::Locked)
    }

    /// Unlocks the task's time box.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::NoTimeBox`] when it has no time box.
    pub fn unlock(&mut self, id: &TaskId) -> Result<()> {
        self.set_lock(id, Lock::Unlocked)
    }

    /// Removes the task's time box, returning it.
    ///
    /// # Errors
    /// [`DomainError::TaskNotInPlan`] when the task is not here,
    /// [`DomainError::NoTimeBox`] when it has no time box.
    pub fn clear_time_box(&mut self, id: &TaskId) -> Result<TimeBox> {
        let index = self.index_of(id)?;
        let entry = &mut self.entries[index];
        let time_box = entry
            .time_box
            .take()
            .ok_or_else(|| DomainError::NoTimeBox { id: id.clone() })?;
        entry.task.plan_mut().time_box = None;
        Ok(time_box)
    }

    /// The day's totals against `free_time`.
    pub fn totals(&self, free_time: Duration) -> DayTotals {
        let counted: Vec<&Task> = self
            .tasks()
            .filter(|task| task.vault().status != TaskStatus::Cancelled)
            .collect();
        let planned: Estimate = counted.iter().filter_map(|task| task.estimate()).sum();
        let done: Estimate = counted
            .iter()
            .filter(|task| task.vault().status == TaskStatus::Done)
            .filter_map(|task| task.estimate())
            .sum();
        let unestimated = counted
            .iter()
            .filter(|task| task.is_unfinished() && task.estimate().is_none())
            .count();
        DayTotals {
            planned,
            done,
            free_time,
            over_free_time: planned.as_duration().saturating_sub(free_time),
            unestimated,
        }
    }

    fn set_lock(&mut self, id: &TaskId, lock: Lock) -> Result<()> {
        let index = self.index_of(id)?;
        let time_box = self.entries[index]
            .time_box
            .as_mut()
            .ok_or_else(|| DomainError::NoTimeBox { id: id.clone() })?;
        time_box.lock = lock;
        Ok(())
    }

    fn detach(&mut self, index: usize, planned_day: Option<Date>) -> Removed {
        let entry = self.entries.remove(index);
        self.renumber();
        let mut task = entry.task;
        task.plan_mut().planned_day = planned_day;
        task.plan_mut().list_position = None;
        task.plan_mut().time_box = None;
        Removed {
            task,
            time_box: entry.time_box,
        }
    }

    fn renumber(&mut self) {
        for (position, entry) in self.entries.iter_mut().enumerate() {
            entry.task.plan_mut().list_position = Some(position);
        }
    }

    fn index_of(&self, id: &TaskId) -> Result<usize> {
        self.position_of(id)
            .ok_or_else(|| DomainError::TaskNotInPlan {
                id: id.clone(),
                date: self.date,
            })
    }

    fn ensure_position(&self, position: usize, last_allowed: usize) -> Result<()> {
        if position > last_allowed {
            return Err(DomainError::PositionOutOfRange {
                position,
                len: self.entries.len(),
            });
        }
        Ok(())
    }

    fn tomorrow(&self) -> Result<Date> {
        self.date
            .tomorrow()
            .map_err(|_| DomainError::TimeOutOfRange)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    const TODAY: Date = date(2026, 10, 22);

    fn task(id: &str, estimate_minutes: Option<u64>) -> Task {
        Task::new(VaultTask {
            estimate: estimate_minutes.map(Estimate::from_minutes),
            ..VaultTask::open(id, format!("Task {id}"))
        })
    }

    fn plan_with(ids: &[&str]) -> DayPlan {
        let mut plan = DayPlan::new(TODAY);
        for id in ids {
            plan.add(task(id, Some(30))).unwrap();
        }
        plan
    }

    fn order(plan: &DayPlan) -> Vec<&str> {
        plan.tasks().map(|task| task.id().as_str()).collect()
    }

    fn positions(plan: &DayPlan) -> Vec<Option<usize>> {
        plan.tasks().map(|task| task.plan().list_position).collect()
    }

    fn id(id: &str) -> TaskId {
        TaskId::new(id)
    }

    fn minutes(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    #[test]
    fn an_empty_plan_has_nothing_to_do() {
        let mut plan = DayPlan::new(TODAY);
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
        let totals = plan.totals(minutes(390));
        assert_eq!(totals.planned, Estimate::ZERO);
        assert_eq!(totals.over_free_time, Duration::ZERO);
        assert_eq!(totals.unestimated, 0);
        assert_eq!(
            plan.roll_over_unfinished(&RolloverReason::new("end of day")),
            Ok(vec![])
        );
        assert_eq!(
            plan.remove(&id("task-1")),
            Err(DomainError::TaskNotInPlan {
                id: id("task-1"),
                date: TODAY
            })
        );
        assert_eq!(
            plan.reorder(&id("task-1"), 0),
            Err(DomainError::TaskNotInPlan {
                id: id("task-1"),
                date: TODAY
            })
        );
        assert_eq!(
            plan.insert(task("task-1", None), 1),
            Err(DomainError::PositionOutOfRange {
                position: 1,
                len: 0
            })
        );
    }

    #[test]
    fn adding_sets_the_planned_day_and_positions() {
        let mut plan = plan_with(&["task-1", "task-2"]);
        plan.add_at_top(task("task-3", None)).unwrap();
        plan.insert(task("task-4", None), 2).unwrap();
        assert_eq!(order(&plan), ["task-3", "task-1", "task-4", "task-2"]);
        assert_eq!(positions(&plan), [Some(0), Some(1), Some(2), Some(3)]);
        assert!(
            plan.tasks()
                .all(|task| task.plan().planned_day == Some(TODAY))
        );
        assert_eq!(
            plan.add(task("task-1", None)),
            Err(DomainError::TaskAlreadyInPlan {
                id: id("task-1"),
                date: TODAY
            })
        );
        assert_eq!(plan.position_of(&id("task-4")), Some(2));
        assert_eq!(plan.position_of(&id("task-9")), None);
    }

    #[test]
    fn reorder_moves_within_the_list() {
        let cases = [
            (
                "to the top",
                "task-3",
                0,
                vec!["task-3", "task-1", "task-2"],
            ),
            (
                "to the bottom",
                "task-1",
                2,
                vec!["task-2", "task-3", "task-1"],
            ),
            (
                "to the middle",
                "task-1",
                1,
                vec!["task-2", "task-1", "task-3"],
            ),
            (
                "to where it is",
                "task-2",
                1,
                vec!["task-1", "task-2", "task-3"],
            ),
        ];
        for (name, moved, position, expected) in cases {
            let mut plan = plan_with(&["task-1", "task-2", "task-3"]);
            plan.reorder(&id(moved), position).unwrap();
            assert_eq!(order(&plan), expected, "{name}");
            assert_eq!(
                positions(&plan),
                [Some(0), Some(1), Some(2)],
                "{name}: renumbered"
            );
        }
        let mut plan = plan_with(&["task-1", "task-2", "task-3"]);
        assert_eq!(
            plan.reorder(&id("task-1"), 3),
            Err(DomainError::PositionOutOfRange {
                position: 3,
                len: 3
            })
        );
    }

    #[test]
    fn moving_to_tomorrow_and_pulling_in() {
        let mut today = plan_with(&["task-1", "task-2"]);
        today
            .set_time_box(
                &id("task-1"),
                TimeBox::unlocked("box-1", TODAY.at(9, 0, 0, 0), minutes(30)),
            )
            .unwrap();
        let Removed {
            task: moved,
            time_box,
        } = today.move_to_tomorrow(&id("task-1")).unwrap();
        assert_eq!(moved.plan().planned_day, Some(date(2026, 10, 23)));
        assert_eq!(moved.plan().time_box, None);
        assert_eq!(moved.plan().list_position, None);
        assert_eq!(
            time_box.map(|time_box| time_box.id),
            Some(TimeBoxId::new("box-1")),
            "the box is handed back"
        );
        assert_eq!(
            moved.plan().rollover_count,
            0,
            "a deliberate move is not a rollover"
        );
        assert_eq!(order(&today), ["task-2"]);
        assert_eq!(positions(&today), [Some(0)]);

        let mut tomorrow = DayPlan::new(date(2026, 10, 23));
        tomorrow.add(moved).unwrap();
        let back = tomorrow.remove(&id("task-1")).unwrap();
        assert_eq!(back.task.plan().planned_day, None);
        assert_eq!(back.time_box, None);
        today.pull_in(back.task).unwrap();
        assert_eq!(order(&today), ["task-2", "task-1"]);
        assert_eq!(
            today.task(&id("task-1")).unwrap().plan().planned_day,
            Some(TODAY)
        );
    }

    #[test]
    fn rollover_counts_days_in_a_row_and_flags_the_fourth() {
        let mut task = task("task-1", Some(45));
        let mut flagged_on = None;
        for day in 1..=5u32 {
            let date = TODAY
                .checked_add(jiff::Span::new().days(i64::from(day) - 1))
                .unwrap();
            let mut plan = DayPlan::new(date);
            plan.add_at_top(task).unwrap();
            let rolled = plan
                .roll_over(&id("task-1"), RolloverReason::new("no time"))
                .unwrap();
            assert_eq!(rolled.from, date);
            assert_eq!(rolled.to, date.tomorrow().unwrap());
            assert_eq!(rolled.rollover_count, day);
            assert_eq!(rolled.reason.as_str(), "no time");
            assert_eq!(rolled.is_backlog_candidate, day >= 4, "day {day}");
            assert_eq!(rolled.time_box, None);
            if rolled.is_backlog_candidate && flagged_on.is_none() {
                flagged_on = Some(day);
            }
            assert!(plan.is_empty());
            task = rolled.task;
            assert_eq!(task.plan().planned_day, Some(date.tomorrow().unwrap()));
        }
        assert_eq!(flagged_on, Some(4));
    }

    #[test]
    fn taking_a_task_out_of_planning_breaks_the_streak() {
        let mut plan = plan_with(&["task-1"]);
        let rolled = plan
            .roll_over(&id("task-1"), RolloverReason::new("no time"))
            .unwrap();
        let mut next = DayPlan::new(rolled.to);
        next.add(rolled.task).unwrap();
        let rolled = next
            .roll_over(&id("task-1"), RolloverReason::new("no time"))
            .unwrap();
        assert_eq!(rolled.rollover_count, 2);
        let mut later = DayPlan::new(rolled.to);
        later.add(rolled.task).unwrap();
        let removed = later.remove(&id("task-1")).unwrap();
        assert_eq!(
            removed.task.plan().rollover_count,
            0,
            "removed: the streak is over"
        );
        let mut much_later = DayPlan::new(date(2026, 11, 2));
        much_later.add(removed.task).unwrap();
        let rolled = much_later
            .roll_over(&id("task-1"), RolloverReason::new("no time"))
            .unwrap();
        assert_eq!(rolled.rollover_count, 1);
        assert!(!rolled.is_backlog_candidate);
    }

    #[test]
    fn rolling_over_hands_back_a_locked_box() {
        let mut plan = plan_with(&["task-1"]);
        plan.set_time_box(
            &id("task-1"),
            TimeBox::unlocked("box-1", TODAY.at(9, 0, 0, 0), minutes(30)),
        )
        .unwrap();
        plan.lock(&id("task-1")).unwrap();
        let rolled = plan
            .roll_over(&id("task-1"), RolloverReason::new("meetings"))
            .unwrap();
        let time_box = rolled.time_box.unwrap();
        assert_eq!(time_box.id, TimeBoxId::new("box-1"));
        assert!(time_box.is_locked());
        assert_eq!(rolled.task.plan().time_box, None);
    }

    #[test]
    fn a_vault_edit_updates_the_task_in_place() {
        let mut plan = plan_with(&["task-1", "task-2"]);
        plan.set_time_box(
            &id("task-2"),
            TimeBox::unlocked("box-2", TODAY.at(14, 0, 0, 0), minutes(60)),
        )
        .unwrap();
        plan.update_task(
            &id("task-2"),
            VaultTask {
                estimate: Some(Estimate::from_minutes(90)),
                ..VaultTask::open("task-2", "Task 2, reworded")
            },
        )
        .unwrap();
        let task = plan.task(&id("task-2")).unwrap();
        assert_eq!(task.vault().text, "Task 2, reworded");
        assert_eq!(task.estimate(), Some(Estimate::from_minutes(90)));
        assert_eq!(task.plan().list_position, Some(1));
        assert_eq!(task.plan().planned_day, Some(TODAY));
        assert_eq!(task.plan().time_box, Some(TimeBoxId::new("box-2")));
        assert!(plan.entry(&id("task-2")).unwrap().time_box().is_some());
        assert_eq!(
            plan.update_task(&id("task-9"), VaultTask::open("task-9", "Missing")),
            Err(DomainError::TaskNotInPlan {
                id: id("task-9"),
                date: TODAY
            })
        );
    }

    #[test]
    #[should_panic(expected = "the line belongs to another task")]
    fn a_vault_edit_cannot_rename_a_task_onto_another_id() {
        let mut plan = plan_with(&["task-1", "task-2"]);
        let _ = plan.update_task(
            &id("task-1"),
            VaultTask::open("task-2", "Renamed onto task-2"),
        );
    }

    #[test]
    fn time_boxes_must_sit_on_the_plan_day_with_unique_ids() {
        let mut plan = plan_with(&["task-1", "task-2"]);
        let elsewhere = TimeBox::unlocked("box-1", date(2026, 10, 23).at(9, 0, 0, 0), minutes(30));
        assert_eq!(
            plan.set_time_box(&id("task-1"), elsewhere),
            Err(DomainError::TimeBoxOutsideDay {
                id: TimeBoxId::new("box-1"),
                start: date(2026, 10, 23),
                date: TODAY,
            })
        );
        plan.set_time_box(
            &id("task-1"),
            TimeBox::unlocked("box-1", TODAY.at(9, 0, 0, 0), minutes(30)),
        )
        .unwrap();
        assert_eq!(
            plan.set_time_box(
                &id("task-2"),
                TimeBox::unlocked("box-1", TODAY.at(10, 0, 0, 0), minutes(30))
            ),
            Err(DomainError::DuplicateTimeBoxId {
                id: TimeBoxId::new("box-1"),
                holder: id("task-1"),
            })
        );
        // The same task may replace its own box, locked or not, under the same id.
        plan.lock(&id("task-1")).unwrap();
        plan.set_time_box(
            &id("task-1"),
            TimeBox::unlocked("box-1", TODAY.at(11, 0, 0, 0), minutes(45)),
        )
        .unwrap();
        let replaced = plan.entry(&id("task-1")).unwrap().time_box().unwrap();
        assert_eq!(replaced.start, TODAY.at(11, 0, 0, 0));
        assert!(!replaced.is_locked());
    }

    #[test]
    fn completing_resets_the_counter_and_finished_tasks_stay_at_shutdown() {
        let mut plan = plan_with(&["task-1", "task-2", "task-3"]);
        let mut rolled_before = plan
            .roll_over(&id("task-1"), RolloverReason::new("blocked"))
            .unwrap()
            .task;
        rolled_before.plan_mut().rollover_count = 3;
        plan.add_at_top(rolled_before).unwrap();
        plan.complete(&id("task-1"), TODAY).unwrap();
        assert_eq!(plan.task(&id("task-1")).unwrap().plan().rollover_count, 0);
        assert_eq!(
            plan.task(&id("task-1")).unwrap().vault().status,
            TaskStatus::Done
        );
        assert_eq!(
            plan.roll_over(&id("task-1"), RolloverReason::new("x")),
            Err(DomainError::TaskFinished { id: id("task-1") })
        );

        let rolled = plan
            .roll_over_unfinished(&RolloverReason::new("end of day"))
            .unwrap();
        let rolled_ids: Vec<&str> = rolled.iter().map(|r| r.task.id().as_str()).collect();
        assert_eq!(rolled_ids, ["task-2", "task-3"]);
        assert!(
            rolled
                .iter()
                .all(|r| r.rollover_count == 1 && !r.is_backlog_candidate)
        );
        assert_eq!(order(&plan), ["task-1"], "the done task stays");
        assert_eq!(
            plan.roll_over(&id("task-2"), RolloverReason::new("x")),
            Err(DomainError::TaskNotInPlan {
                id: id("task-2"),
                date: TODAY
            })
        );
    }

    #[test]
    fn time_boxes_lock_unlock_and_clear() {
        let mut plan = plan_with(&["task-1", "task-2"]);
        let no_box = DomainError::NoTimeBox { id: id("task-1") };
        assert_eq!(plan.lock(&id("task-1")), Err(no_box.clone()));
        assert_eq!(plan.unlock(&id("task-1")), Err(no_box.clone()));
        assert_eq!(plan.clear_time_box(&id("task-1")), Err(no_box));

        let time_box = TimeBox::unlocked("box-1", TODAY.at(9, 30, 0, 0), minutes(90));
        assert_eq!(time_box.end().unwrap(), TODAY.at(11, 0, 0, 0));
        plan.set_time_box(&id("task-1"), time_box.clone()).unwrap();
        assert_eq!(
            plan.task(&id("task-1")).unwrap().plan().time_box,
            Some(TimeBoxId::new("box-1"))
        );
        assert!(
            !plan
                .entry(&id("task-1"))
                .unwrap()
                .time_box()
                .unwrap()
                .is_locked()
        );
        plan.lock(&id("task-1")).unwrap();
        assert!(
            plan.entry(&id("task-1"))
                .unwrap()
                .time_box()
                .unwrap()
                .is_locked()
        );
        plan.unlock(&id("task-1")).unwrap();
        assert!(
            !plan
                .entry(&id("task-1"))
                .unwrap()
                .time_box()
                .unwrap()
                .is_locked()
        );
        assert_eq!(plan.clear_time_box(&id("task-1")), Ok(time_box));
        assert_eq!(plan.task(&id("task-1")).unwrap().plan().time_box, None);
        assert_eq!(
            plan.set_time_box(
                &id("task-9"),
                TimeBox::unlocked("box-2", TODAY.at(9, 0, 0, 0), minutes(1))
            ),
            Err(DomainError::TaskNotInPlan {
                id: id("task-9"),
                date: TODAY
            })
        );
        assert_eq!(
            TimeBox::unlocked("box-3", DateTime::MAX, minutes(1)).end(),
            Err(DomainError::TimeOutOfRange)
        );
    }

    #[test]
    fn totals_compare_planned_time_with_free_time() {
        let mut plan = DayPlan::new(TODAY);
        plan.add(task("task-1", Some(60))).unwrap();
        plan.add(task("task-2", Some(120))).unwrap();
        plan.add(task("task-3", None)).unwrap();
        plan.add(task("task-4", Some(30))).unwrap();
        plan.complete(&id("task-4"), TODAY).unwrap();
        let mut cancelled = task("task-5", Some(999));
        cancelled.cancel();
        plan.add(cancelled).unwrap();

        let cases = [
            ("fits", 240, 0),
            ("exactly", 210, 0),
            ("over by an hour", 150, 60),
            ("no free time at all", 0, 210),
        ];
        for (name, free, over) in cases {
            let totals = plan.totals(minutes(free));
            assert_eq!(
                totals.planned,
                Estimate::from_minutes(210),
                "{name}: cancelled tasks do not count"
            );
            assert_eq!(totals.done, Estimate::from_minutes(30), "{name}");
            assert_eq!(totals.free_time, minutes(free), "{name}");
            assert_eq!(totals.over_free_time, minutes(over), "{name}");
            assert_eq!(totals.unestimated, 1, "{name}");
        }
    }
}
