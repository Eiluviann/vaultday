//! Which tasks a day's list holds, and in what order.
//!
//! The list is the union of what the vault says and what the app remembers:
//! a top-level task belongs to a date when its `[scheduled:: ]` is that date,
//! when the date is today and the task is overdue (scheduled earlier and still
//! unfinished), or when the workspace's app state lists it in the date's
//! order. Tasks finished on the date stay on the list, after the unfinished
//! ones, so the day keeps its record. Cancelled tasks and subtasks never
//! appear on their own: a subtask shows inside its parent's card.
//!
//! The stored order wins. Tasks it does not name yet (added in Obsidian, or
//! scheduled since the last reorder) join at the top or the bottom, as the
//! user chose for new tasks, in the order the vault lists them.

use std::collections::HashSet;

use jiff::civil::Date;

use crate::task::{TaskId, TaskStatus, VaultTask};

/// Where tasks the stored order does not name yet join the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NewTaskPlacement {
    /// Above the ordered ones.
    Top,
    /// Below the ordered ones.
    Bottom,
}

/// The tasks of one date, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodayList {
    /// Open and in-progress tasks, in list order.
    pub unfinished: Vec<TaskId>,
    /// Tasks completed on the date, in list order.
    pub done: Vec<TaskId>,
}

impl TodayList {
    /// Every id, unfinished first: the order the app stores for the date.
    pub fn order(&self) -> Vec<TaskId> {
        self.unfinished.iter().chain(&self.done).cloned().collect()
    }
}

/// Builds the list of `date` from the vault's tasks and the stored order.
///
/// `tasks` is every task of the workspace in vault order, where a repeated id
/// keeps only its first line; `stored_order` is
/// what the app state holds for `date` and may name tasks that have since
/// left the vault or been cancelled, which are skipped. `today` decides
/// whether overdue tasks are pulled in.
pub fn today_list(
    tasks: &[VaultTask],
    stored_order: &[TaskId],
    date: Date,
    today: Date,
    placement: NewTaskPlacement,
) -> TodayList {
    let stored: HashSet<&TaskId> = stored_order.iter().collect();
    // Ids are meant to be unique; when a line is repeated the first one wins.
    let mut seen: HashSet<&TaskId> = HashSet::new();
    let members: Vec<&VaultTask> = tasks
        .iter()
        .filter(|task| belongs_to(task, stored.contains(&task.id), date, today))
        .filter(|task| seen.insert(&task.id))
        .collect();
    let unfinished: Vec<&VaultTask> = members
        .iter()
        .copied()
        .filter(|task| task.status.is_unfinished())
        .collect();
    let done: Vec<&VaultTask> = members
        .iter()
        .copied()
        .filter(|task| task.status == TaskStatus::Done)
        .collect();
    TodayList {
        unfinished: ordered(&unfinished, stored_order, placement),
        done: ordered(&done, stored_order, NewTaskPlacement::Bottom),
    }
}

/// Whether a top-level task is on the list of `date`.
fn belongs_to(task: &VaultTask, is_stored: bool, date: Date, today: Date) -> bool {
    if task.parent.is_some() {
        return false;
    }
    match task.status {
        TaskStatus::Cancelled => false,
        TaskStatus::Done => {
            task.completion == Some(date) && (is_stored || task.scheduled.is_some())
        }
        TaskStatus::Open | TaskStatus::InProgress => {
            let is_scheduled_here = task.scheduled == Some(date);
            let is_overdue = date == today && task.scheduled.is_some_and(|day| day < today);
            is_stored || is_scheduled_here || is_overdue
        }
    }
}

/// The stored ids first, in stored order, with the unnamed ones at `placement`.
fn ordered(
    members: &[&VaultTask],
    stored_order: &[TaskId],
    placement: NewTaskPlacement,
) -> Vec<TaskId> {
    let member_ids: HashSet<&TaskId> = members.iter().map(|task| &task.id).collect();
    let mut seen: HashSet<&TaskId> = HashSet::new();
    let kept: Vec<TaskId> = stored_order
        .iter()
        .filter(|id| member_ids.contains(id) && seen.insert(id))
        .cloned()
        .collect();
    let joining: Vec<TaskId> = members
        .iter()
        .map(|task| &task.id)
        .filter(|id| !seen.contains(id))
        .cloned()
        .collect();
    match placement {
        NewTaskPlacement::Top => joining.into_iter().chain(kept).collect(),
        NewTaskPlacement::Bottom => kept.into_iter().chain(joining).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn task(id: &str, status: TaskStatus, scheduled: Option<Date>) -> VaultTask {
        let mut task = VaultTask::open(id, format!("Task {id}"));
        task.status = status;
        task.scheduled = scheduled;
        task
    }

    fn ids(list: &[TaskId]) -> Vec<&str> {
        list.iter().map(TaskId::as_str).collect()
    }

    fn order(ids: &[&str]) -> Vec<TaskId> {
        ids.iter().copied().map(TaskId::from).collect()
    }

    const DAY: Date = date(2026, 10, 23);

    #[test]
    fn scheduled_overdue_and_stored_tasks_belong_to_today() {
        let tasks = [
            task("t1", TaskStatus::Open, Some(DAY)),
            task("t2", TaskStatus::Open, Some(date(2026, 10, 20))),
            task("t3", TaskStatus::Open, None),
            task("t4", TaskStatus::InProgress, None),
            task("t5", TaskStatus::Open, Some(date(2026, 10, 24))),
        ];
        let list = today_list(&tasks, &order(&["t4"]), DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(ids(&list.unfinished), ["t4", "t1", "t2"]);
        assert!(list.done.is_empty());
    }

    #[test]
    fn overdue_tasks_join_only_the_list_of_today() {
        let tasks = [task("t2", TaskStatus::Open, Some(date(2026, 10, 20)))];
        let tomorrow = date(2026, 10, 24);
        let list = today_list(&tasks, &[], tomorrow, DAY, NewTaskPlacement::Bottom);
        assert!(list.unfinished.is_empty());
    }

    #[test]
    fn the_stored_order_wins_and_new_tasks_join_where_the_user_chose() {
        let tasks = [
            task("t1", TaskStatus::Open, Some(DAY)),
            task("t2", TaskStatus::Open, Some(DAY)),
            task("t3", TaskStatus::Open, Some(DAY)),
        ];
        let stored = order(&["t3", "t1"]);
        let bottom = today_list(&tasks, &stored, DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(ids(&bottom.unfinished), ["t3", "t1", "t2"]);
        let top = today_list(&tasks, &stored, DAY, DAY, NewTaskPlacement::Top);
        assert_eq!(ids(&top.unfinished), ["t2", "t3", "t1"]);
    }

    #[test]
    fn stale_and_repeated_ids_in_the_stored_order_are_skipped() {
        let tasks = [task("t1", TaskStatus::Open, Some(DAY))];
        let stored = order(&["gone", "t1", "t1"]);
        let list = today_list(&tasks, &stored, DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(ids(&list.unfinished), ["t1"]);
        assert_eq!(ids(&list.order()), ["t1"]);
    }

    #[test]
    fn tasks_done_on_the_day_follow_and_cancelled_ones_and_subtasks_never_show() {
        let mut done = task("t1", TaskStatus::Done, Some(DAY));
        done.completion = Some(DAY);
        let mut done_earlier = task("t2", TaskStatus::Done, Some(DAY));
        done_earlier.completion = Some(date(2026, 10, 22));
        let cancelled = task("t3", TaskStatus::Cancelled, Some(DAY));
        let mut subtask = task("t4", TaskStatus::Open, Some(DAY));
        subtask.parent = Some(TaskId::from("t5"));
        let open = task("t5", TaskStatus::Open, Some(DAY));
        let mut done_unplanned = task("t6", TaskStatus::Done, None);
        done_unplanned.completion = Some(DAY);
        let tasks = [done, done_earlier, cancelled, subtask, open, done_unplanned];
        let list = today_list(&tasks, &order(&["t3"]), DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(ids(&list.unfinished), ["t5"]);
        assert_eq!(ids(&list.done), ["t1"]);
        assert_eq!(ids(&list.order()), ["t5", "t1"]);
    }

    #[test]
    fn a_task_done_on_the_day_stays_when_the_order_names_it() {
        let mut done = task("t1", TaskStatus::Done, None);
        done.completion = Some(DAY);
        let list = today_list(&[done], &order(&["t1"]), DAY, DAY, NewTaskPlacement::Top);
        assert_eq!(ids(&list.done), ["t1"]);
    }

    #[test]
    fn a_repeated_id_gives_one_entry_the_first_line() {
        let mut copy = task("t1", TaskStatus::Done, Some(DAY));
        copy.completion = Some(DAY);
        let tasks = [task("t1", TaskStatus::Open, Some(DAY)), copy];
        let list = today_list(&tasks, &order(&["t1"]), DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(ids(&list.unfinished), ["t1"]);
        assert!(list.done.is_empty());
    }

    #[test]
    fn an_empty_vault_gives_an_empty_list() {
        let list = today_list(&[], &order(&["t1"]), DAY, DAY, NewTaskPlacement::Bottom);
        assert_eq!(list, TodayList::default());
    }
}
