//! Reconciliation for the close-out of an hourly or fixed-price workspace:
//! the period's tracked sessions grouped per work item, billed by the same
//! arithmetic the queue uses ([`billed_parts`], over `domain::billing`), so the table the user
//! copies and the entries Toggl receives agree.
//!
//! - Hourly: each session rounds up to the increment on its own, as its
//!   Toggl entry does; a work item's billed time is the sum. The task's
//!   billable flag, else the workspace default, decides the column.
//! - Fixed price: the estimate caps what is billed per work item across all
//!   its sessions, earlier periods included; the rest is the unpaid overrun.

use std::collections::BTreeMap;
use std::time::Duration;

use jiff::Timestamp;

use crate::billing::{BillingPolicy, billed_parts};

/// One stopped session of the period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSession {
    /// The session's id.
    pub session: String,
    /// When it started; sessions are billed in this order.
    pub start: Timestamp,
    /// Its story, when known.
    pub story: Option<String>,
    /// Its task, when it ran on one.
    pub task: Option<String>,
    /// What the row is called: the task's text, else the story or meeting.
    pub title: String,
    /// Its real length.
    pub duration: Duration,
}

/// What a work item brings besides its sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkItemFacts {
    /// Hourly: the task's own billable flag.
    pub is_billable: Option<bool>,
    /// Fixed price: the estimate, the quote and the cap.
    pub estimate: Option<Duration>,
    /// Fixed price: time tracked on it before the period.
    pub tracked_before: Duration,
}

/// One row of the hourly table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyRow {
    /// The work item: its task, else its story, else the session.
    pub key: String,
    /// The story.
    pub story: Option<String>,
    /// The row's title.
    pub title: String,
    /// Real time tracked in the period.
    pub tracked: Duration,
    /// Time billed: each session rounded up to the increment.
    pub billed: Duration,
    /// Whether it is billable.
    pub is_billable: bool,
    /// Sessions in the row.
    pub sessions: u32,
}

/// One row of the fixed-price table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedRow {
    /// The work item: its task, else its story, else the session.
    pub key: String,
    /// The story.
    pub story: Option<String>,
    /// The row's title.
    pub title: String,
    /// The estimate; `None` quotes nothing, so all of it is overrun.
    pub estimate: Option<Duration>,
    /// Real time tracked in the period.
    pub tracked: Duration,
    /// What the period bills under the cap.
    pub billable: Duration,
    /// What the period tracked beyond the cap, unpaid.
    pub overrun: Duration,
}

/// The work item a session belongs to.
#[must_use]
pub fn work_item_key(session: &WorkSession) -> String {
    session
        .task
        .clone()
        .or_else(|| session.story.clone())
        .unwrap_or_else(|| session.session.clone())
}

/// The hourly table: one row per work item and billable flag, by key.
#[must_use]
pub fn reconcile_hourly(
    sessions: &[WorkSession],
    facts: &BTreeMap<String, WorkItemFacts>,
    rounding_minutes: u32,
    billable_default: bool,
) -> Vec<HourlyRow> {
    let mut rows: BTreeMap<(String, bool), HourlyRow> = BTreeMap::new();
    for session in in_start_order(sessions) {
        let key = work_item_key(session);
        let is_billable = facts
            .get(&key)
            .and_then(|facts| facts.is_billable)
            .unwrap_or(billable_default);
        let policy = BillingPolicy::Hourly {
            rounding_minutes,
            is_billable,
        };
        let billed: Duration = billed_parts(&policy, session.duration, Duration::ZERO)
            .iter()
            .map(|part| part.duration)
            .sum();
        let row = rows
            .entry((key.clone(), is_billable))
            .or_insert_with(|| HourlyRow {
                key,
                story: session.story.clone(),
                title: session.title.clone(),
                tracked: Duration::ZERO,
                billed: Duration::ZERO,
                is_billable,
                sessions: 0,
            });
        row.tracked += whole_seconds(session.duration);
        row.billed += billed;
        row.sessions += 1;
    }
    rows.into_values().collect()
}

/// The fixed-price table: one row per work item, by key.
#[must_use]
pub fn reconcile_fixed(
    sessions: &[WorkSession],
    facts: &BTreeMap<String, WorkItemFacts>,
) -> Vec<FixedRow> {
    let mut rows: BTreeMap<String, (FixedRow, Duration)> = BTreeMap::new();
    for session in in_start_order(sessions) {
        let key = work_item_key(session);
        let item = facts.get(&key).copied().unwrap_or_default();
        let (row, already_billed) = rows.entry(key.clone()).or_insert_with(|| {
            let billed_before = item
                .estimate
                .map_or(Duration::ZERO, |estimate| item.tracked_before.min(estimate));
            (
                FixedRow {
                    key,
                    story: session.story.clone(),
                    title: session.title.clone(),
                    estimate: item.estimate,
                    tracked: Duration::ZERO,
                    billable: Duration::ZERO,
                    overrun: Duration::ZERO,
                },
                billed_before,
            )
        });
        let policy = BillingPolicy::Fixed {
            estimate: item.estimate,
        };
        for part in billed_parts(&policy, session.duration, *already_billed) {
            if part.is_billable {
                row.billable += part.duration;
                *already_billed += part.duration;
            } else {
                row.overrun += part.duration;
            }
        }
        row.tracked += whole_seconds(session.duration);
    }
    rows.into_values().map(|(row, _)| row).collect()
}

fn in_start_order(sessions: &[WorkSession]) -> Vec<&WorkSession> {
    let mut ordered: Vec<&WorkSession> = sessions.iter().collect();
    ordered.sort_by(|a, b| (a.start, &a.session).cmp(&(b.start, &b.session)));
    ordered
}

/// Sessions bill whole seconds, as [`billed_parts`] does.
fn whole_seconds(duration: Duration) -> Duration {
    Duration::from_secs(duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, at: i64, task: Option<&str>, story: &str, minutes: u64) -> WorkSession {
        WorkSession {
            session: id.to_owned(),
            start: Timestamp::from_second(1_792_000_000 + at * 3600).unwrap(),
            story: Some(story.to_owned()),
            task: task.map(str::to_owned),
            title: task.unwrap_or(story).to_owned(),
            duration: Duration::from_secs(minutes * 60) + Duration::from_millis(400),
        }
    }

    #[test]
    fn hourly_rounds_each_session_and_honours_the_billable_flag() {
        // (sessions as (task, minutes)), rounding, default, expected rows as
        // (key, tracked, billed, billable, sessions)
        type Case = (
            Vec<(Option<&'static str>, u64)>,
            u32,
            bool,
            Vec<(&'static str, u64, u64, bool, u32)>,
        );
        let facts = BTreeMap::from([(
            "t-internal".to_owned(),
            WorkItemFacts {
                is_billable: Some(false),
                ..WorkItemFacts::default()
            },
        )]);
        let cases: Vec<Case> = vec![
            (
                vec![(Some("t-1"), 7)],
                15,
                true,
                vec![("t-1", 7, 15, true, 1)],
            ),
            (
                vec![(Some("t-1"), 7), (Some("t-1"), 16)],
                15,
                true,
                vec![("t-1", 23, 45, true, 2)],
            ),
            (
                vec![(Some("t-1"), 30)],
                15,
                true,
                vec![("t-1", 30, 30, true, 1)],
            ),
            (
                vec![(Some("t-1"), 1)],
                6,
                false,
                vec![("t-1", 1, 6, false, 1)],
            ),
            (
                vec![(Some("t-internal"), 10), (None, 20)],
                15,
                true,
                vec![("WEB-1", 20, 30, true, 1), ("t-internal", 10, 15, false, 1)],
            ),
            (
                vec![(Some("t-1"), 0)],
                15,
                true,
                vec![("t-1", 0, 0, true, 1)],
            ),
        ];
        for (sessions, rounding, default, expected) in cases {
            let sessions: Vec<WorkSession> = sessions
                .iter()
                .enumerate()
                .map(|(index, (task, minutes))| {
                    session(
                        &format!("s{index}"),
                        i64::try_from(index).unwrap(),
                        *task,
                        "WEB-1",
                        *minutes,
                    )
                })
                .collect();
            let rows: Vec<(String, u64, u64, bool, u32)> =
                reconcile_hourly(&sessions, &facts, rounding, default)
                    .iter()
                    .map(|row| {
                        (
                            row.key.clone(),
                            row.tracked.as_secs() / 60,
                            row.billed.as_secs() / 60,
                            row.is_billable,
                            row.sessions,
                        )
                    })
                    .collect();
            let expected: Vec<(String, u64, u64, bool, u32)> = expected
                .into_iter()
                .map(|(key, tracked, billed, billable, count)| {
                    (key.to_owned(), tracked, billed, billable, count)
                })
                .collect();
            assert_eq!(rows, expected);
        }
    }

    #[test]
    fn fixed_caps_at_the_estimate_and_keeps_the_overrun_apart() {
        // (estimate, tracked before, sessions in minutes) → (tracked, billable, overrun)
        type Case = (Option<u64>, u64, Vec<u64>, (u64, u64, u64));
        let cases: [Case; 6] = [
            (Some(60), 0, vec![40], (40, 40, 0)),
            (Some(60), 0, vec![40, 40], (80, 60, 20)),
            (Some(60), 50, vec![30], (30, 10, 20)),
            (Some(60), 90, vec![30], (30, 0, 30)),
            (None, 0, vec![30], (30, 0, 30)),
            (Some(60), 0, vec![60], (60, 60, 0)),
        ];
        for (estimate, before, minutes, expected) in cases {
            let facts = BTreeMap::from([(
                "t-1".to_owned(),
                WorkItemFacts {
                    estimate: estimate.map(|m| Duration::from_secs(m * 60)),
                    tracked_before: Duration::from_secs(before * 60),
                    is_billable: None,
                },
            )]);
            let sessions: Vec<WorkSession> = minutes
                .iter()
                .enumerate()
                .map(|(index, m)| {
                    session(
                        &format!("s{index}"),
                        i64::try_from(index).unwrap(),
                        Some("t-1"),
                        "WEB-1",
                        *m,
                    )
                })
                .collect();
            let rows = reconcile_fixed(&sessions, &facts);
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(
                (
                    row.tracked.as_secs() / 60,
                    row.billable.as_secs() / 60,
                    row.overrun.as_secs() / 60
                ),
                expected,
                "{estimate:?} {before} {minutes:?}"
            );
            assert_eq!(row.billable + row.overrun, row.tracked);
        }
    }

    #[test]
    fn rows_group_by_task_then_story_then_session() {
        let mut lone = session("s9", 9, None, "WEB-1", 5);
        lone.story = None;
        let sessions = [
            session("s1", 1, Some("t-2"), "WEB-1", 5),
            session("s2", 2, None, "WEB-1", 5),
            lone,
        ];
        let keys: Vec<String> = reconcile_fixed(&sessions, &BTreeMap::new())
            .into_iter()
            .map(|row| row.key)
            .collect();
        assert_eq!(keys, ["WEB-1", "s9", "t-2"]);
    }
}
