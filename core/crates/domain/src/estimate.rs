//! The three time types and the estimate maths per task and per story.
//!
//! [`SpentTime`], [`LoggedTime`] and [`Estimate`] wrap the same duration but
//! are distinct types: each adds only to itself, so a budget is never summed
//! into real time by accident. Derived amounts (remaining, overtime) are plain
//! [`Duration`]s because they are neither of the three.
//!
//! ```compile_fail
//! use domain::estimate::{Estimate, SpentTime};
//! let _ = SpentTime::from_minutes(1) + Estimate::from_minutes(1);
//! ```
//!
//! ```compile_fail
//! use domain::estimate::{LoggedTime, SpentTime};
//! let _ = SpentTime::from_minutes(1) + LoggedTime::from_minutes(1);
//! ```
//!
//! ```compile_fail
//! use domain::estimate::{Estimate, LoggedTime};
//! let _ = LoggedTime::from_minutes(1) + Estimate::from_minutes(1);
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign};
use std::time::Duration;

use jiff::civil::Date;

use crate::task::{StoryNumber, TaskId};

/// The share of the estimate at which the alert fires, in percent.
pub const THRESHOLD_PERCENT: u64 = 80;

macro_rules! time_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Duration);

        impl $name {
            /// No time at all.
            pub const ZERO: Self = Self(Duration::ZERO);

            /// Wraps a duration.
            pub const fn new(duration: Duration) -> Self {
                Self(duration)
            }

            /// Builds the amount from whole minutes, saturating on overflow.
            pub const fn from_minutes(minutes: u64) -> Self {
                Self(Duration::from_secs(minutes.saturating_mul(60)))
            }

            /// The underlying duration.
            pub const fn as_duration(self) -> Duration {
                self.0
            }

            /// Whole seconds.
            pub const fn as_secs(self) -> u64 {
                self.0.as_secs()
            }

            /// Whether the amount is zero.
            pub const fn is_zero(self) -> bool {
                self.0.is_zero()
            }
        }

        impl Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self {
                Self(self.0.saturating_add(rhs.0))
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl Sum for $name {
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                iter.fold(Self::ZERO, Add::add)
            }
        }

        impl From<Duration> for $name {
            fn from(duration: Duration) -> Self {
                Self(duration)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&format_hours_minutes(self.0))
            }
        }
    };
}

time_type! {
    /// Real time spent, measured by sessions. Private to the user.
    SpentTime
}

time_type! {
    /// Time allocated to a story for a day, as written to the day log.
    LoggedTime
}

time_type! {
    /// The budget for a task or a story.
    Estimate
}

/// Reported time, estimate and the derived amounts for one task or story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortReport {
    /// Time spent so far.
    pub reported: SpentTime,
    /// The budget, when one was given.
    pub estimate: Option<Estimate>,
    /// Budget left: estimate minus reported, never below zero. Zero without an estimate.
    pub remaining: Duration,
    /// Reported time beyond the budget, never below zero. Zero without an estimate.
    pub overtime: Duration,
}

impl EffortReport {
    /// Reported time as a share of the estimate, in whole percent, rounded down.
    ///
    /// `None` without an estimate or with a zero estimate: there is no budget
    /// to measure against.
    pub fn percent_used(&self) -> Option<u64> {
        let estimate = self.estimate.filter(|estimate| !estimate.is_zero())?;
        let percent =
            self.reported.as_duration().as_nanos() * 100 / estimate.as_duration().as_nanos();
        Some(u64::try_from(percent).unwrap_or(u64::MAX))
    }

    /// Whether reported time exceeds the estimate.
    pub fn is_over(&self) -> bool {
        !self.overtime.is_zero()
    }
}

/// One task's effort, the input of the per-story aggregation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEffort {
    /// The task.
    pub task: TaskId,
    /// The story the task belongs to, when it has one.
    pub story: Option<StoryNumber>,
    /// Time spent on the task.
    pub reported: SpentTime,
    /// The task's estimate.
    pub estimate: Option<Estimate>,
}

/// Time logged against a story on a day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryLogEntry {
    /// The story.
    pub story: StoryNumber,
    /// The day the time was logged for.
    pub date: Date,
    /// The amount logged.
    pub logged: LoggedTime,
}

/// The once-firing 80 % alert for one task.
///
/// `observe` returns `true` exactly once per estimate: the first time reported
/// time reaches [`THRESHOLD_PERCENT`] of it. It fires again only after the
/// estimate changes. Without an estimate, or with a zero one, it never fires.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThresholdAlert {
    fired_for: Option<Estimate>,
}

impl ThresholdAlert {
    /// A tracker that has not fired yet.
    pub const fn new() -> Self {
        Self { fired_for: None }
    }

    /// Reconstitutes a tracker that already fired for `estimate`.
    pub const fn fired_for(estimate: Estimate) -> Self {
        Self {
            fired_for: Some(estimate),
        }
    }

    /// The estimate the alert last fired for, if any.
    pub const fn fired_estimate(&self) -> Option<Estimate> {
        self.fired_for
    }

    /// Feeds the current numbers and says whether the alert fires now.
    pub fn observe(&mut self, reported: SpentTime, estimate: Option<Estimate>) -> bool {
        let Some(estimate) = estimate.filter(|estimate| !estimate.is_zero()) else {
            self.fired_for = None;
            return false;
        };
        if self.fired_for.is_some_and(|fired| fired != estimate) {
            self.fired_for = None;
        }
        if self.fired_for.is_some() || !reaches_threshold(reported, estimate) {
            return false;
        }
        self.fired_for = Some(estimate);
        true
    }
}

/// Reported time, estimate, remaining and overtime for one task.
pub fn report(reported: SpentTime, estimate: Option<Estimate>) -> EffortReport {
    let budget = estimate.map_or(Duration::ZERO, Estimate::as_duration);
    let spent = reported.as_duration();
    EffortReport {
        reported,
        estimate,
        remaining: budget.saturating_sub(spent),
        overtime: match estimate {
            Some(_) => spent.saturating_sub(budget),
            None => Duration::ZERO,
        },
    }
}

/// Aggregates task efforts per story: reported time is summed, the estimate
/// is the sum of the estimates that exist (`None` when no task has one).
/// Tasks without a story are left out.
pub fn reports_by_story<'a>(
    efforts: impl IntoIterator<Item = &'a TaskEffort>,
) -> BTreeMap<StoryNumber, EffortReport> {
    let mut totals: BTreeMap<StoryNumber, (SpentTime, Option<Estimate>)> = BTreeMap::new();
    for effort in efforts {
        let Some(story) = &effort.story else {
            continue;
        };
        let entry = totals.entry(story.clone()).or_default();
        entry.0 += effort.reported;
        if let Some(estimate) = effort.estimate {
            entry.1 = Some(entry.1.unwrap_or_default() + estimate);
        }
    }
    totals
        .into_iter()
        .map(|(story, (reported, estimate))| (story, report(reported, estimate)))
        .collect()
}

/// Sums logged time per story over the given entries.
pub fn logged_by_story<'a>(
    entries: impl IntoIterator<Item = &'a StoryLogEntry>,
) -> BTreeMap<StoryNumber, LoggedTime> {
    entries
        .into_iter()
        .fold(BTreeMap::new(), |mut totals, entry| {
            *totals.entry(entry.story.clone()).or_default() += entry.logged;
            totals
        })
}

/// Where a story's budget on the Hours tab comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// The story note's own `estimate`, the budget set upstream.
    Story,
    /// The sum of its tasks' estimates, for a story without one.
    Tasks,
    /// Neither the story nor any task gives one.
    None,
}

/// One story's row of the Hours tab: tracked time against its budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryHours {
    /// The story.
    pub story: StoryNumber,
    /// Tracked time, estimate, remaining and overtime.
    pub report: EffortReport,
    /// Where the estimate came from.
    pub budget: BudgetSource,
}

/// The Hours row of one story: the story's own estimate is its budget, and
/// only a story without one falls back to the sum of its tasks' estimates.
pub fn story_hours(
    story: StoryNumber,
    story_estimate: Option<Estimate>,
    task_estimates: impl IntoIterator<Item = Option<Estimate>>,
    tracked: SpentTime,
) -> StoryHours {
    let (estimate, budget) = story_budget(story_estimate, task_estimates);
    StoryHours {
        story,
        report: report(tracked, estimate),
        budget,
    }
}

/// A story's budget and where it comes from: its own estimate, else the sum
/// of its tasks' estimates, else none.
pub fn story_budget(
    story_estimate: Option<Estimate>,
    task_estimates: impl IntoIterator<Item = Option<Estimate>>,
) -> (Option<Estimate>, BudgetSource) {
    let from_tasks = task_estimates
        .into_iter()
        .flatten()
        .fold(None, |total: Option<Estimate>, estimate| {
            Some(total.unwrap_or_default() + estimate)
        });
    match (story_estimate, from_tasks) {
        (Some(estimate), _) => (Some(estimate), BudgetSource::Story),
        (None, Some(estimate)) => (Some(estimate), BudgetSource::Tasks),
        (None, None) => (None, BudgetSource::None),
    }
}

/// Logged time against a budget. Tracked time never enters it: what is left
/// of the estimate is what has not been logged yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoggedReport {
    /// Time logged so far.
    pub logged: LoggedTime,
    /// The budget, when one was given.
    pub estimate: Option<Estimate>,
    /// Estimate minus logged, never below zero. Zero without an estimate.
    pub remaining: Duration,
    /// Logged time beyond the budget, never below zero. Zero without an estimate.
    pub overtime: Duration,
}

/// Logged time, estimate, remaining and overtime for one story.
pub fn logged_report(logged: LoggedTime, estimate: Option<Estimate>) -> LoggedReport {
    let budget = estimate.map_or(Duration::ZERO, Estimate::as_duration);
    let done = logged.as_duration();
    LoggedReport {
        logged,
        estimate,
        remaining: budget.saturating_sub(done),
        overtime: match estimate {
            Some(_) => done.saturating_sub(budget),
            None => Duration::ZERO,
        },
    }
}

/// A story's `estimate` as written: days (`3d`, `1.5d`), each the length
/// of the workspace's working day `day`, or any task form ([`parse_estimate`]).
/// `None` for anything else.
///
/// [`parse_estimate`]: crate::quick_add::parse_estimate
pub fn parse_story_budget(text: &str, day: Duration) -> Option<Estimate> {
    let text = text.trim().to_ascii_lowercase();
    let Some(days) = text.strip_suffix('d') else {
        return crate::quick_add::parse_estimate(&text);
    };
    let (whole, tenths) = match days.split_once('.') {
        Some((whole, fraction)) if fraction.len() == 1 => (whole, fraction.parse::<u64>().ok()?),
        Some(_) => return None,
        None => (days, 0),
    };
    let whole: u64 = whole.parse().ok()?;
    let tenths_of_days = whole.checked_mul(10)?.checked_add(tenths)?;
    let seconds = day.as_secs().checked_mul(tenths_of_days)? / 10;
    Some(Estimate::new(Duration::from_secs(seconds - seconds % 60)))
}

/// The daily target from a workspace's `hours_per_day`, in whole minutes
/// (6.5 h is 390 minutes); `None` for a value that is not a positive number
/// of at most 24 hours.
pub fn daily_target(hours_per_day: f64) -> Option<Duration> {
    if !hours_per_day.is_finite() || hours_per_day <= 0.0 || hours_per_day > 24.0 {
        return None;
    }
    // In range by the guard: at most 1440 minutes, so the cast is exact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let minutes = (hours_per_day * 60.0).round() as u64;
    Some(Duration::from_secs(minutes * 60))
}

/// Formats a duration as hours and minutes: `1h28m`, `45m`, `2h`, `0m`.
/// Seconds are dropped, never rounded up.
pub fn format_hours_minutes(duration: Duration) -> String {
    let minutes = duration.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m}m"),
    }
}

fn reaches_threshold(reported: SpentTime, estimate: Estimate) -> bool {
    reported.as_duration().as_nanos() * 100
        >= estimate.as_duration().as_nanos() * u128::from(THRESHOLD_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn minutes(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn story(number: &str) -> StoryNumber {
        StoryNumber::new(number)
    }

    #[test]
    fn report_derives_remaining_and_overtime() {
        let cases = [
            ("under budget", 30, Some(60), 30, 0, Some(50)),
            ("on budget", 60, Some(60), 0, 0, Some(100)),
            ("over budget", 90, Some(60), 0, 30, Some(150)),
            ("nothing spent", 0, Some(45), 45, 0, Some(0)),
            ("zero estimate", 20, Some(0), 0, 20, None),
            ("no estimate", 20, None, 0, 0, None),
            ("nothing at all", 0, None, 0, 0, None),
        ];
        for (name, spent, estimate, remaining, overtime, percent) in cases {
            let got = report(
                SpentTime::from_minutes(spent),
                estimate.map(Estimate::from_minutes),
            );
            assert_eq!(got.remaining, minutes(remaining), "{name}: remaining");
            assert_eq!(got.overtime, minutes(overtime), "{name}: overtime");
            assert_eq!(got.percent_used(), percent, "{name}: percent");
            assert_eq!(got.is_over(), overtime > 0, "{name}: is_over");
        }
    }

    #[test]
    fn alert_fires_once_when_crossing_the_threshold() {
        let estimate = Some(Estimate::from_minutes(100));
        let mut alert = ThresholdAlert::new();
        let steps = [
            ("below", 79, false),
            ("crossing", 80, true),
            ("still above", 81, false),
            ("far above", 200, false),
            ("back below", 10, false),
            ("crossing again", 95, false),
        ];
        for (name, spent, fires) in steps {
            assert_eq!(
                alert.observe(SpentTime::from_minutes(spent), estimate),
                fires,
                "{name}"
            );
        }
        assert_eq!(alert.fired_estimate(), estimate);
    }

    #[test]
    fn alert_fires_again_only_after_the_estimate_changes() {
        let mut alert = ThresholdAlert::new();
        let spent = SpentTime::from_minutes(85);
        assert!(alert.observe(spent, Some(Estimate::from_minutes(100))));
        assert!(!alert.observe(spent, Some(Estimate::from_minutes(100))));
        // A bigger estimate drops the share under 80 %: no alert, and the tracker rearms.
        assert!(!alert.observe(spent, Some(Estimate::from_minutes(200))));
        assert_eq!(alert.fired_estimate(), None);
        // Back to the old estimate: it fires once more.
        assert!(alert.observe(spent, Some(Estimate::from_minutes(100))));
        // A smaller estimate that is still crossed fires right away.
        assert!(alert.observe(spent, Some(Estimate::from_minutes(90))));
        assert!(!alert.observe(spent, Some(Estimate::from_minutes(90))));
    }

    #[test]
    fn alert_never_fires_without_a_usable_estimate() {
        let mut alert = ThresholdAlert::new();
        let spent = SpentTime::from_minutes(500);
        assert!(!alert.observe(spent, None));
        assert!(!alert.observe(spent, Some(Estimate::ZERO)));
        assert_eq!(alert.fired_estimate(), None);
        // Losing the estimate after firing rearms the alert.
        assert!(alert.observe(spent, Some(Estimate::from_minutes(100))));
        assert!(!alert.observe(spent, None));
        assert!(alert.observe(spent, Some(Estimate::from_minutes(100))));
    }

    #[test]
    fn sub_second_estimates_are_exact() {
        let estimate = Some(Estimate::new(Duration::from_millis(500)));
        let report = report(SpentTime::new(Duration::from_millis(250)), estimate);
        assert_eq!(report.percent_used(), Some(50));
        let mut alert = ThresholdAlert::new();
        assert!(!alert.observe(SpentTime::ZERO, estimate));
        assert!(!alert.observe(SpentTime::new(Duration::from_millis(399)), estimate));
        assert!(alert.observe(SpentTime::new(Duration::from_millis(400)), estimate));
    }

    #[test]
    fn reconstituted_alert_does_not_refire_for_the_same_estimate() {
        let estimate = Estimate::from_minutes(60);
        let mut alert = ThresholdAlert::fired_for(estimate);
        assert!(!alert.observe(SpentTime::from_minutes(59), Some(estimate)));
        assert!(alert.observe(
            SpentTime::from_minutes(59),
            Some(Estimate::from_minutes(30))
        ));
    }

    #[test]
    fn story_reports_sum_tasks_and_skip_tasks_without_a_story() {
        let efforts = [
            TaskEffort {
                task: TaskId::new("task-1"),
                story: Some(story("WEB-142")),
                reported: SpentTime::from_minutes(30),
                estimate: Some(Estimate::from_minutes(60)),
            },
            TaskEffort {
                task: TaskId::new("task-2"),
                story: Some(story("WEB-142")),
                reported: SpentTime::from_minutes(50),
                estimate: None,
            },
            TaskEffort {
                task: TaskId::new("task-3"),
                story: Some(story("WEB-7")),
                reported: SpentTime::from_minutes(5),
                estimate: None,
            },
            TaskEffort {
                task: TaskId::new("task-4"),
                story: None,
                reported: SpentTime::from_minutes(999),
                estimate: Some(Estimate::from_minutes(999)),
            },
        ];
        let reports = reports_by_story(&efforts);
        assert_eq!(reports.len(), 2);
        let web_142 = reports[&story("WEB-142")];
        assert_eq!(web_142.reported, SpentTime::from_minutes(80));
        assert_eq!(web_142.estimate, Some(Estimate::from_minutes(60)));
        assert_eq!(web_142.overtime, minutes(20));
        let web_7 = reports[&story("WEB-7")];
        assert_eq!(web_7.estimate, None);
        assert_eq!(web_7.remaining, Duration::ZERO);
        assert!(reports_by_story(&[]).is_empty());
    }

    #[test]
    fn the_logged_report_keeps_tracked_time_out() {
        let report = logged_report(
            LoggedTime::from_minutes(120),
            Some(Estimate::from_minutes(360)),
        );
        assert_eq!(report.remaining, Duration::from_secs(240 * 60));
        assert_eq!(report.overtime, Duration::ZERO);
        let over = logged_report(
            LoggedTime::from_minutes(90),
            Some(Estimate::from_minutes(60)),
        );
        assert_eq!(over.remaining, Duration::ZERO);
        assert_eq!(over.overtime, Duration::from_secs(30 * 60));
        let unbudgeted = logged_report(LoggedTime::from_minutes(90), None);
        assert_eq!(unbudgeted.overtime, Duration::ZERO);
        assert_eq!(
            story_budget(
                None,
                [
                    Some(Estimate::from_minutes(30)),
                    None,
                    Some(Estimate::from_minutes(15))
                ]
            ),
            (Some(Estimate::from_minutes(45)), BudgetSource::Tasks)
        );
    }

    #[test]
    fn logged_time_sums_per_story() {
        let entries = [
            StoryLogEntry {
                story: story("WEB-142"),
                date: date(2026, 10, 21),
                logged: LoggedTime::from_minutes(120),
            },
            StoryLogEntry {
                story: story("WEB-142"),
                date: date(2026, 10, 22),
                logged: LoggedTime::from_minutes(60),
            },
            StoryLogEntry {
                story: story("WEB-7"),
                date: date(2026, 10, 22),
                logged: LoggedTime::from_minutes(15),
            },
        ];
        let totals = logged_by_story(&entries);
        assert_eq!(totals[&story("WEB-142")], LoggedTime::from_minutes(180));
        assert_eq!(totals[&story("WEB-7")], LoggedTime::from_minutes(15));
    }

    #[test]
    fn same_type_amounts_add_and_sum() {
        let total: SpentTime = [SpentTime::from_minutes(10), SpentTime::from_minutes(20)]
            .into_iter()
            .sum();
        assert_eq!(total, SpentTime::from_minutes(30));
        assert_eq!(
            Estimate::from_minutes(1) + Estimate::from_minutes(2),
            Estimate::from_minutes(3)
        );
        assert_eq!(
            SpentTime::new(Duration::MAX) + SpentTime::from_minutes(1),
            SpentTime::new(Duration::MAX)
        );
    }

    #[test]
    fn formats_hours_and_minutes() {
        let cases = [
            (0, "0m"),
            (45, "45m"),
            (60, "1h"),
            (88, "1h28m"),
            (600, "10h"),
        ];
        for (minutes, text) in cases {
            assert_eq!(
                format_hours_minutes(Duration::from_secs(minutes * 60)),
                text
            );
            assert_eq!(SpentTime::from_minutes(minutes).to_string(), text);
        }
        assert_eq!(format_hours_minutes(Duration::from_secs(119)), "1m");
    }

    #[test]
    fn a_story_budget_is_its_own_estimate_before_its_tasks() {
        let hours = story_hours(
            StoryNumber::new("WEB-142"),
            Some(Estimate::from_minutes(360)),
            [Some(Estimate::from_minutes(90)), None],
            SpentTime::from_minutes(125),
        );
        assert_eq!(hours.budget, BudgetSource::Story);
        assert_eq!(hours.report.remaining, Duration::from_secs(235 * 60));

        let from_tasks = story_hours(
            StoryNumber::new("ACME-9"),
            None,
            [
                Some(Estimate::from_minutes(30)),
                Some(Estimate::from_minutes(45)),
                None,
            ],
            SpentTime::from_minutes(90),
        );
        assert_eq!(from_tasks.budget, BudgetSource::Tasks);
        assert_eq!(from_tasks.report.estimate, Some(Estimate::from_minutes(75)));
        assert_eq!(from_tasks.report.remaining, Duration::ZERO);
        assert_eq!(from_tasks.report.overtime, Duration::from_secs(15 * 60));

        let none = story_hours(
            StoryNumber::new("ACME-12"),
            None,
            [None],
            SpentTime::from_minutes(5),
        );
        assert_eq!(none.budget, BudgetSource::None);
        assert_eq!(none.report.remaining, Duration::ZERO);
        assert!(!none.report.is_over(), "no budget, no overtime");
    }

    #[test]
    fn the_daily_target_comes_from_hours_per_day() {
        assert_eq!(daily_target(6.5), Some(Duration::from_secs(390 * 60)));
        assert_eq!(daily_target(8.0), Some(Duration::from_secs(480 * 60)));
        for unusable in [0.0, -1.0, 24.5, f64::NAN, f64::INFINITY] {
            assert_eq!(daily_target(unusable), None, "{unusable}");
        }
    }

    #[test]
    fn story_budgets_in_days_use_the_working_day() {
        let day = Duration::from_secs(8 * 3600);
        assert_eq!(
            parse_story_budget("3d", day),
            Some(Estimate::from_minutes(3 * 480))
        );
        assert_eq!(
            parse_story_budget("1.5d", day),
            Some(Estimate::from_minutes(720))
        );
        assert_eq!(
            parse_story_budget("6h", day),
            Some(Estimate::from_minutes(360))
        );
        assert_eq!(
            parse_story_budget("1d", Duration::from_secs(390 * 60)),
            Some(Estimate::from_minutes(390))
        );
        for unusable in ["", "d", "1.25d", "soon", "-1d"] {
            assert_eq!(parse_story_budget(unusable, day), None, "{unusable}");
        }
    }
}
