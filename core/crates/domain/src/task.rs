//! The task: the fields that live on the vault's task line plus the fields the
//! app keeps about the task's place in a day.
//!
//! The vault side ([`VaultTask`]) is plain input: the parser in `vault-base`
//! produces it and the writer maps it back; this crate never reads a file.
//! The app side ([`PlanState`]) lives in app state and is owned by the
//! [`crate::day_plan::DayPlan`] that holds the task.

use jiff::civil::Date;

use crate::estimate::Estimate;

/// How many shutdown rollovers in a row flag a task for the backlog.
pub const BACKLOG_AFTER_ROLLOVERS: u32 = 4;

id_type! {
    /// The task's `[id:: ]` field, such as `task-118`.
    TaskId
}

id_type! {
    /// The number of the story a task belongs to, such as `WEB-142`.
    StoryNumber
}

id_type! {
    /// Identifies a time box in the plan, and the calendar event written for it once locked.
    TimeBoxId
}

/// The checkbox state of the task line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    /// `[ ]`
    Open,
    /// `[x]`
    Done,
    /// `[-]`
    Cancelled,
    /// `[/]`
    InProgress,
}

impl TaskStatus {
    /// The character between the brackets.
    pub const fn marker(self) -> char {
        match self {
            Self::Open => ' ',
            Self::Done => 'x',
            Self::Cancelled => '-',
            Self::InProgress => '/',
        }
    }

    /// Parses the character between the brackets; `None` for any other character.
    pub const fn from_marker(marker: char) -> Option<Self> {
        match marker {
            ' ' => Some(Self::Open),
            'x' => Some(Self::Done),
            '-' => Some(Self::Cancelled),
            '/' => Some(Self::InProgress),
            _ => None,
        }
    }

    /// Open or in progress: the task still needs work.
    pub const fn is_unfinished(self) -> bool {
        matches!(self, Self::Open | Self::InProgress)
    }
}

/// The section of the note the task line sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// `## Next Steps`
    NextSteps,
    /// `## Pending Input`
    PendingInput,
}

/// The fields written on the task line in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultTask {
    /// `[id:: ]`
    pub id: TaskId,
    /// The line text without its fields.
    pub text: String,
    /// The checkbox state.
    pub status: TaskStatus,
    /// The section the line sits in.
    pub section: Section,
    /// The story whose note holds the line, when there is one.
    pub story: Option<StoryNumber>,
    /// `[created:: ]`
    pub created: Option<Date>,
    /// `[scheduled:: ]`
    pub scheduled: Option<Date>,
    /// `[due:: ]`
    pub due: Option<Date>,
    /// `[estimate:: ]`
    pub estimate: Option<Estimate>,
    /// `[assigned_to:: ]`
    pub assigned_to: Option<String>,
    /// `[completion:: ]`
    pub completion: Option<Date>,
    /// The parent task of a subtask.
    pub parent: Option<TaskId>,
}

impl VaultTask {
    /// An open Next Steps task with only an id and text; every other field unset.
    pub fn open(id: impl Into<TaskId>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            status: TaskStatus::Open,
            section: Section::NextSteps,
            story: None,
            created: None,
            scheduled: None,
            due: None,
            estimate: None,
            assigned_to: None,
            completion: None,
            parent: None,
        }
    }
}

/// What the app keeps about a task's place in a day. Lives in app state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanState {
    /// The day the task is planned for.
    pub planned_day: Option<Date>,
    /// The task's position in that day's list, from zero.
    pub list_position: Option<usize>,
    /// The time box the task holds in that day's plan.
    pub time_box: Option<TimeBoxId>,
    /// Shutdown rollovers in a row: counted up at every shutdown the task rolls
    /// over, reset when the task is finished (completed or cancelled, in the app
    /// or in the vault) or taken out of a plan. A deliberate `>>` keeps it.
    pub rollover_count: u32,
}

/// A task: its vault line plus the app's plan state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    vault: VaultTask,
    plan: PlanState,
}

impl Task {
    /// A task that is not planned yet.
    pub fn new(vault: VaultTask) -> Self {
        Self {
            vault,
            plan: PlanState::default(),
        }
    }

    /// Reconstitutes a task from its vault line and stored plan state.
    pub fn with_plan(vault: VaultTask, plan: PlanState) -> Self {
        Self { vault, plan }
    }

    /// The task's id.
    pub fn id(&self) -> &TaskId {
        &self.vault.id
    }

    /// The vault-side fields.
    pub fn vault(&self) -> &VaultTask {
        &self.vault
    }

    /// The app-side plan state.
    pub fn plan(&self) -> &PlanState {
        &self.plan
    }

    /// The task's estimate, if it has one.
    pub fn estimate(&self) -> Option<Estimate> {
        self.vault.estimate
    }

    /// Open or in progress.
    pub fn is_unfinished(&self) -> bool {
        self.vault.status.is_unfinished()
    }

    /// Whether the task is a subtask of another task.
    pub fn is_subtask(&self) -> bool {
        self.vault.parent.is_some()
    }

    /// Rolled over at least [`BACKLOG_AFTER_ROLLOVERS`] days in a row: offer the backlog.
    pub fn is_backlog_candidate(&self) -> bool {
        self.plan.rollover_count >= BACKLOG_AFTER_ROLLOVERS
    }

    /// Replaces the vault-side fields after the line changed in the vault. A
    /// line that is now done or cancelled resets the rollover counter, the same
    /// as completing the task in the app.
    ///
    /// # Panics
    /// When `vault` carries another task's id: a line whose id changed is a
    /// different task, not an edit of this one.
    pub fn update_vault(&mut self, vault: VaultTask) {
        assert_eq!(
            vault.id, self.vault.id,
            "update_vault: the line belongs to another task"
        );
        if !vault.status.is_unfinished() {
            self.plan.rollover_count = 0;
        }
        self.vault = vault;
    }

    /// Replaces the estimate (`None` clears it).
    pub fn set_estimate(&mut self, estimate: Option<Estimate>) {
        self.vault.estimate = estimate;
    }

    /// Marks the task done on `date` and resets its rollover counter.
    pub fn complete(&mut self, date: Date) {
        self.vault.status = TaskStatus::Done;
        self.vault.completion = Some(date);
        self.plan.rollover_count = 0;
    }

    /// Reopens a done or cancelled task; the completion date is cleared.
    pub fn reopen(&mut self) {
        self.vault.status = TaskStatus::Open;
        self.vault.completion = None;
    }

    /// Marks the task as being worked on.
    pub fn start_progress(&mut self) {
        self.vault.status = TaskStatus::InProgress;
    }

    /// Cancels the task and resets its rollover counter.
    pub fn cancel(&mut self) {
        self.vault.status = TaskStatus::Cancelled;
        self.plan.rollover_count = 0;
    }

    pub(crate) fn plan_mut(&mut self) -> &mut PlanState {
        &mut self.plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn status_markers_round_trip() {
        let cases = [
            (' ', Some(TaskStatus::Open), true),
            ('x', Some(TaskStatus::Done), false),
            ('-', Some(TaskStatus::Cancelled), false),
            ('/', Some(TaskStatus::InProgress), true),
            ('X', None, false),
            ('1', None, false),
        ];
        for (marker, status, unfinished) in cases {
            assert_eq!(TaskStatus::from_marker(marker), status, "{marker:?}");
            if let Some(status) = status {
                assert_eq!(status.marker(), marker);
                assert_eq!(status.is_unfinished(), unfinished);
            }
        }
    }

    #[test]
    fn completing_resets_the_rollover_counter() {
        let mut task = Task::with_plan(
            VaultTask::open("task-1", "Confirm the field mapping"),
            PlanState {
                rollover_count: 3,
                ..PlanState::default()
            },
        );
        assert!(!task.is_backlog_candidate());
        task.plan_mut().rollover_count = BACKLOG_AFTER_ROLLOVERS;
        assert!(task.is_backlog_candidate());
        task.complete(date(2026, 10, 22));
        assert_eq!(task.vault().status, TaskStatus::Done);
        assert_eq!(task.vault().completion, Some(date(2026, 10, 22)));
        assert_eq!(task.plan().rollover_count, 0);
        assert!(!task.is_unfinished());
        task.reopen();
        assert_eq!(task.vault().status, TaskStatus::Open);
        assert_eq!(task.vault().completion, None);
    }

    #[test]
    fn finishing_the_line_in_the_vault_resets_the_counter_too() {
        let cases = [
            ("ticked in the vault", TaskStatus::Done, 0),
            ("cancelled in the vault", TaskStatus::Cancelled, 0),
            ("still open", TaskStatus::Open, 3),
            ("in progress", TaskStatus::InProgress, 3),
        ];
        for (name, status, count) in cases {
            let mut task = Task::with_plan(
                VaultTask::open("task-1", "Confirm the field mapping"),
                PlanState {
                    rollover_count: 3,
                    ..PlanState::default()
                },
            );
            task.update_vault(VaultTask {
                status,
                ..VaultTask::open("task-1", "Confirm the field mapping (edited)")
            });
            assert_eq!(task.plan().rollover_count, count, "{name}");
            assert_eq!(
                task.vault().text,
                "Confirm the field mapping (edited)",
                "{name}"
            );
        }
        let mut task = Task::with_plan(
            VaultTask::open("task-2", "Old approach"),
            PlanState {
                rollover_count: 2,
                ..PlanState::default()
            },
        );
        task.cancel();
        assert_eq!(task.plan().rollover_count, 0);
    }

    #[test]
    #[should_panic(expected = "the line belongs to another task")]
    fn a_line_with_another_id_is_not_an_edit_of_this_task() {
        let mut task = Task::new(VaultTask::open("task-1", "Confirm the field mapping"));
        task.update_vault(VaultTask::open("task-2", "Another line"));
    }

    #[test]
    fn subtasks_carry_their_parent() {
        let parent = Task::new(VaultTask::open("task-1", "Parent"));
        let child = Task::new(VaultTask {
            parent: Some(parent.id().clone()),
            ..VaultTask::open("task-2", "Child")
        });
        assert!(!parent.is_subtask());
        assert!(child.is_subtask());
        assert_eq!(
            child.vault().parent.as_ref().map(TaskId::as_str),
            Some("task-1")
        );
    }
}
