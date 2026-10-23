//! The side panel's day grid: the blocks of one date, events and the time
//! boxes of tasks, placed side by side where they overlap, with the tracked
//! share of each task block and the one text scheme every block follows.
//!
//! Where unlocked boxes land is the scheduler's call ([`crate::scheduler`]);
//! this module only lays out what it is given, plus the rule a pin obeys.

use jiff::civil::{Date, DateTime, Time};

use crate::day_plan::{Lock, TimeBox};
use crate::error::{DomainError, Result};
use crate::estimate::SpentTime;
use crate::scheduler::{Event, Span};
use crate::task::TaskId;

/// A task's time box with what its block shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBlockInput {
    /// The task.
    pub task: TaskId,
    /// The task's full text, as the list shows it.
    pub title: String,
    /// The box.
    pub time_box: TimeBox,
    /// Time tracked on the task that day.
    pub tracked: SpentTime,
    /// Whether the task carries an estimate.
    pub has_estimate: bool,
    /// Done or cancelled.
    pub is_done: bool,
}

/// What a block stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// An event read from one of the workspace's calendars; never moved or written.
    Event {
        /// The calendar's name.
        calendar: String,
    },
    /// A task's time box.
    Task {
        /// The task.
        task: TaskId,
        /// Locked (pinned, in the private calendar) or unlocked (a projection).
        lock: Lock,
        /// Time tracked on the task that day.
        tracked: SpentTime,
        /// Whether the task carries an estimate.
        has_estimate: bool,
        /// Done or cancelled.
        is_done: bool,
    },
}

/// One block of the day grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The event's id or the box's id.
    pub id: String,
    /// The item's full name.
    pub title: String,
    /// When the item runs, as stored.
    pub time_range: Span,
    /// The part of it on the grid's date, which places the block.
    pub span: Span,
    /// Event or task.
    pub kind: BlockKind,
    /// The column of the block among those it overlaps, from 0.
    pub lane: usize,
    /// How many columns its overlapping group needs; 1 when it overlaps nothing.
    pub lanes: usize,
}

impl Block {
    /// The tracked part of a task block's length, in thousandths, at most
    /// 1000: the fill drawn from the block's top. Zero for events.
    pub fn tracked_permille(&self) -> u32 {
        let BlockKind::Task { tracked, .. } = &self.kind else {
            return 0;
        };
        let length = self.time_range.duration().as_secs();
        if length == 0 {
            return 0;
        }
        let share = tracked.as_duration().as_secs().saturating_mul(1000) / length;
        u32::try_from(share.min(1000)).unwrap_or(1000)
    }

    /// The block's second line: the time range, then its state tokens joined
    /// by middle dots. "09:30 – 11:00 · locked · tracked 0:47",
    /// "16:30 – 17:30 · unlocked · no estimate", "11:00 – 12:00 · Work".
    pub fn detail(&self) -> String {
        let mut tokens = vec![format!(
            "{} – {}",
            clock_text(self.time_range.start()),
            clock_text(self.time_range.end())
        )];
        match &self.kind {
            BlockKind::Event { calendar } => {
                if !calendar.is_empty() {
                    tokens.push(calendar.clone());
                }
            }
            BlockKind::Task {
                lock,
                tracked,
                has_estimate,
                is_done,
                ..
            } => {
                tokens.push(
                    match lock {
                        Lock::Locked => "locked",
                        Lock::Unlocked => "unlocked",
                    }
                    .to_owned(),
                );
                if *is_done {
                    tokens.push("done".to_owned());
                }
                if !tracked.is_zero() {
                    tokens.push(format!("tracked {}", tracked_text(*tracked)));
                }
                if !has_estimate {
                    tokens.push("no estimate".to_owned());
                }
            }
        }
        tokens.join(" · ")
    }
}

/// The blocks of `date`: every event and box that touches the date, by
/// start (events before boxes at the same instant, longer first), each
/// clipped to the date and given a lane among the blocks it overlaps.
pub fn day_blocks(date: Date, events: &[Event], tasks: &[TaskBlockInput]) -> Vec<Block> {
    let Some(bounds) = day_span(date) else {
        return Vec::new();
    };
    let event_blocks = events.iter().filter_map(|event| {
        Some(Block {
            id: event.id.clone(),
            title: event.title.clone(),
            time_range: event.span,
            span: event.span.clipped_to(&bounds)?,
            kind: BlockKind::Event {
                calendar: event.calendar.clone(),
            },
            lane: 0,
            lanes: 1,
        })
    });
    let task_blocks = tasks.iter().filter_map(|input| {
        let time_range = box_span(&input.time_box)?;
        Some(Block {
            id: input.time_box.id.as_str().to_owned(),
            title: input.title.clone(),
            time_range,
            span: time_range.clipped_to(&bounds)?,
            kind: BlockKind::Task {
                task: input.task.clone(),
                lock: input.time_box.lock,
                tracked: input.tracked,
                has_estimate: input.has_estimate,
                is_done: input.is_done,
            },
            lane: 0,
            lanes: 1,
        })
    });
    let mut blocks: Vec<Block> = event_blocks.chain(task_blocks).collect();
    blocks.sort_by(|a, b| {
        (
            a.span.start(),
            is_task(a),
            std::cmp::Reverse(a.span.end()),
            &a.id,
        )
            .cmp(&(
                b.span.start(),
                is_task(b),
                std::cmp::Reverse(b.span.end()),
                &b.id,
            ))
    });
    assign_lanes(&mut blocks);
    blocks
}

/// Checks that `task` may be locked over `span`: no other task's locked box
/// shares any of that time. Events may overlap a pin; the user decides that.
///
/// # Errors
/// [`DomainError::LockedTimeTaken`] naming the task that holds the time.
pub fn ensure_lockable<'a>(
    task: &TaskId,
    span: &Span,
    boxes: impl IntoIterator<Item = (&'a TaskId, &'a TimeBox)>,
) -> Result<()> {
    let holder = boxes
        .into_iter()
        .filter(|(holder, time_box)| *holder != task && time_box.is_locked())
        .find(|(_, time_box)| box_span(time_box).is_some_and(|held| held.overlaps(span)));
    match holder {
        Some((holder, _)) => Err(DomainError::LockedTimeTaken {
            holder: holder.clone(),
        }),
        None => Ok(()),
    }
}

/// Gives each block its lane: a group of blocks that overlap one another,
/// directly or through a chain, shares one lane count, and each block takes
/// the first lane free at its start.
fn assign_lanes(blocks: &mut [Block]) {
    let mut group_start = 0;
    let mut group_end: Option<DateTime> = None;
    let mut lane_ends: Vec<DateTime> = Vec::new();
    for index in 0..blocks.len() {
        let span = blocks[index].span;
        if group_end.is_some_and(|end| span.start() >= end) {
            close_group(&mut blocks[group_start..index], lane_ends.len());
            group_start = index;
            lane_ends.clear();
        }
        let lane = match lane_ends.iter().position(|end| *end <= span.start()) {
            Some(free) => {
                lane_ends[free] = span.end();
                free
            }
            None => {
                lane_ends.push(span.end());
                lane_ends.len() - 1
            }
        };
        blocks[index].lane = lane;
        group_end = Some(group_end.map_or(span.end(), |end| end.max(span.end())));
    }
    let count = lane_ends.len();
    close_group(&mut blocks[group_start..], count);
}

fn close_group(group: &mut [Block], lanes: usize) {
    for block in group {
        block.lanes = lanes.max(1);
    }
}

fn is_task(block: &Block) -> bool {
    matches!(block.kind, BlockKind::Task { .. })
}

fn day_span(date: Date) -> Option<Span> {
    let start = date.to_datetime(Time::midnight());
    let end = date.tomorrow().ok()?.to_datetime(Time::midnight());
    Span::new(start, end).ok()
}

fn box_span(time_box: &TimeBox) -> Option<Span> {
    Span::starting_at(time_box.start, time_box.duration).ok()
}

fn clock_text(at: DateTime) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

/// Tracked time as the timer shows it: `0:47`, `2:05`.
fn tracked_text(tracked: SpentTime) -> String {
    let minutes = tracked.as_duration().as_secs() / 60;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::civil::date;

    use super::*;
    use crate::task::TimeBoxId;

    const DAY: Date = date(2026, 10, 15);

    fn at(hour: i8, minute: i8) -> DateTime {
        DAY.at(hour, minute, 0, 0)
    }

    fn span(from: (i8, i8), to: (i8, i8)) -> Span {
        Span::new(at(from.0, from.1), at(to.0, to.1)).unwrap()
    }

    fn event(id: &str, from: (i8, i8), to: (i8, i8)) -> Event {
        Event::new(id, "Work", format!("Event {id}"), span(from, to))
    }

    fn time_box(id: &str, from: (i8, i8), minutes: u64, lock: Lock) -> TimeBox {
        TimeBox {
            id: TimeBoxId::new(id),
            start: at(from.0, from.1),
            duration: Duration::from_secs(minutes * 60),
            lock,
        }
    }

    fn task(id: &str, time_box: TimeBox, tracked_minutes: u64) -> TaskBlockInput {
        TaskBlockInput {
            task: TaskId::new(id),
            title: format!("Task {id}"),
            time_box,
            tracked: SpentTime::from_minutes(tracked_minutes),
            has_estimate: true,
            is_done: false,
        }
    }

    #[test]
    fn blocks_follow_the_text_scheme_of_the_board() {
        let tasks = [
            task("task-1", time_box("box-1", (9, 30), 90, Lock::Locked), 47),
            TaskBlockInput {
                has_estimate: false,
                ..task("task-2", time_box("box-2", (16, 30), 60, Lock::Unlocked), 0)
            },
        ];
        let blocks = day_blocks(DAY, &[event("e1", (11, 0), (12, 0))], &tasks);
        let details: Vec<String> = blocks.iter().map(Block::detail).collect();
        assert_eq!(
            details,
            [
                "09:30 – 11:00 · locked · tracked 0:47",
                "11:00 – 12:00 · Work",
                "16:30 – 17:30 · unlocked · no estimate",
            ]
        );
        assert_eq!(blocks[0].title, "Task task-1", "the title is the full name");
        assert_eq!(blocks[0].tracked_permille(), 522, "47 of 90 minutes");
        assert_eq!(blocks[1].tracked_permille(), 0, "events carry no fill");
        assert!(
            blocks.iter().all(|block| block.lanes == 1),
            "11:00 touches, never overlaps"
        );
    }

    #[test]
    fn tracked_fill_stops_at_the_block_and_done_is_a_token() {
        let over = TaskBlockInput {
            is_done: true,
            ..task("task-1", time_box("box-1", (13, 0), 30, Lock::Locked), 75)
        };
        let blocks = day_blocks(DAY, &[], &[over]);
        assert_eq!(blocks[0].tracked_permille(), 1000);
        assert_eq!(
            blocks[0].detail(),
            "13:00 – 13:30 · locked · done · tracked 1:15"
        );
    }

    #[test]
    fn overlapping_blocks_share_lanes_and_chains_share_a_count() {
        let events = [
            event("a", (9, 0), (10, 0)),
            event("b", (9, 30), (11, 0)),
            event("c", (10, 0), (10, 30)),
            event("d", (12, 0), (13, 0)),
        ];
        let blocks = day_blocks(DAY, &events, &[]);
        let lanes: Vec<(&str, usize, usize)> = blocks
            .iter()
            .map(|block| (block.id.as_str(), block.lane, block.lanes))
            .collect();
        assert_eq!(
            lanes,
            [("a", 0, 2), ("b", 1, 2), ("c", 0, 2), ("d", 0, 1)],
            "c reuses a's lane once a ends; d stands alone"
        );
    }

    #[test]
    fn blocks_are_clipped_to_the_date_and_keep_their_times() {
        let overnight = Event::new(
            "late",
            "Work",
            "Release night",
            Span::new(at(23, 0), DAY.tomorrow().unwrap().at(1, 0, 0, 0)).unwrap(),
        );
        let elsewhere = event("other", (9, 0), (10, 0));
        let elsewhere = Event {
            span: Span::new(
                DAY.yesterday().unwrap().at(9, 0, 0, 0),
                DAY.yesterday().unwrap().at(10, 0, 0, 0),
            )
            .unwrap(),
            ..elsewhere
        };
        let blocks = day_blocks(DAY, &[overnight, elsewhere], &[]);
        assert_eq!(blocks.len(), 1, "yesterday's event is not on the grid");
        assert_eq!(blocks[0].span.end(), DAY.tomorrow().unwrap().at(0, 0, 0, 0));
        assert_eq!(blocks[0].detail(), "23:00 – 01:00 · Work");
    }

    #[test]
    fn a_pin_never_shares_time_with_another_pin() {
        let locked = time_box("box-1", (9, 30), 90, Lock::Locked);
        let floating = time_box("box-2", (13, 0), 60, Lock::Unlocked);
        let held = [
            (TaskId::new("task-1"), locked),
            (TaskId::new("task-2"), floating),
        ];
        let pairs = || held.iter().map(|(task, time_box)| (task, time_box));
        let mover = TaskId::new("task-3");
        assert_eq!(
            ensure_lockable(&mover, &span((10, 0), (10, 30)), pairs()),
            Err(DomainError::LockedTimeTaken {
                holder: TaskId::new("task-1")
            })
        );
        assert_eq!(
            ensure_lockable(&mover, &span((13, 0), (14, 0)), pairs()),
            Ok(()),
            "unlocked boxes float away"
        );
        assert_eq!(
            ensure_lockable(&mover, &span((11, 0), (12, 0)), pairs()),
            Ok(()),
            "touching is not sharing"
        );
        assert_eq!(
            ensure_lockable(&TaskId::new("task-1"), &span((10, 0), (11, 0)), pairs()),
            Ok(()),
            "a pin may move over its own old slot"
        );
    }
}
