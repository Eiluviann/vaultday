//! Ports: what the domain needs from the outside, as traits, with in-memory
//! implementations for tests and for callers that have no real clock yet.
//!
//! A real clock reads the system time zone, which is IO, so it lives above
//! this crate (the FFI and Node bindings supply one).

use std::cell::{Cell, RefCell};
use std::time::Duration;

use jiff::civil::Date;
use jiff::{Timestamp, Zoned};

use crate::estimate::{Estimate, SpentTime};
use crate::session::SessionId;
use crate::task::{StoryNumber, TaskId};

/// The current instant and the current civil day.
pub trait Clock {
    /// The current instant.
    fn now(&self) -> Timestamp;
    /// Today's date in the user's time zone.
    fn today(&self) -> Date;
}

/// What the app tells the user about, without saying how.
pub trait Notifier {
    /// Reported time on a task reached the alert threshold of its estimate.
    fn threshold_crossed(
        &self,
        task: &TaskId,
        story: Option<&StoryNumber>,
        reported: SpentTime,
        estimate: Estimate,
    );
    /// A session has been running longer than the long-timer limit.
    fn long_timer(&self, session: &SessionId, elapsed: SpentTime);
}

/// A clock that stands still until a test moves it.
#[derive(Debug, Clone)]
pub struct FixedClock {
    now: Cell<Timestamp>,
    today: Cell<Date>,
}

impl FixedClock {
    /// A clock fixed at `now` on `today`.
    pub const fn new(now: Timestamp, today: Date) -> Self {
        Self {
            now: Cell::new(now),
            today: Cell::new(today),
        }
    }

    /// A clock fixed at the zoned instant, whose date is today.
    pub fn at(zoned: &Zoned) -> Self {
        Self::new(zoned.timestamp(), zoned.date())
    }

    /// Moves the clock forward by `duration`, saturating at the end of time.
    /// The date does not move: set it with `set_today` when a test crosses midnight.
    pub fn advance(&self, duration: Duration) {
        let moved = self
            .now
            .get()
            .saturating_add(duration)
            .unwrap_or(Timestamp::MAX);
        self.now.set(moved);
    }

    /// Sets the current instant.
    pub fn set_now(&self, now: Timestamp) {
        self.now.set(now);
    }

    /// Sets today's date.
    pub fn set_today(&self, today: Date) {
        self.today.set(today);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now.get()
    }

    fn today(&self) -> Date {
        self.today.get()
    }
}

/// A notification a [`RecordingNotifier`] received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    /// See [`Notifier::threshold_crossed`].
    ThresholdCrossed {
        /// The task that crossed the threshold.
        task: TaskId,
        /// Its story, when it has one.
        story: Option<StoryNumber>,
        /// Reported time at the crossing.
        reported: SpentTime,
        /// The estimate that was crossed.
        estimate: Estimate,
    },
    /// See [`Notifier::long_timer`].
    LongTimer {
        /// The long-running session.
        session: SessionId,
        /// Its elapsed time at the check.
        elapsed: SpentTime,
    },
}

/// A notifier that keeps every notification in memory.
#[derive(Debug, Default)]
pub struct RecordingNotifier {
    sent: RefCell<Vec<Notification>>,
}

impl RecordingNotifier {
    /// An empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every notification received so far, oldest first.
    pub fn sent(&self) -> Vec<Notification> {
        self.sent.borrow().clone()
    }

    /// Forgets what was received.
    pub fn clear(&self) {
        self.sent.borrow_mut().clear();
    }
}

impl Notifier for RecordingNotifier {
    fn threshold_crossed(
        &self,
        task: &TaskId,
        story: Option<&StoryNumber>,
        reported: SpentTime,
        estimate: Estimate,
    ) {
        self.sent.borrow_mut().push(Notification::ThresholdCrossed {
            task: task.clone(),
            story: story.cloned(),
            reported,
            estimate,
        });
    }

    fn long_timer(&self, session: &SessionId, elapsed: SpentTime) {
        self.sent.borrow_mut().push(Notification::LongTimer {
            session: session.clone(),
            elapsed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn fixed_clock_moves_only_when_told() {
        let start = Timestamp::from_second(1_800_000_000).unwrap();
        let clock = FixedClock::new(start, date(2027, 1, 15));
        assert_eq!(clock.now(), start);
        assert_eq!(clock.today(), date(2027, 1, 15));
        clock.advance(Duration::from_secs(90));
        assert_eq!(clock.now().duration_since(start).as_secs(), 90);
        assert_eq!(clock.today(), date(2027, 1, 15));
        clock.set_today(date(2027, 1, 16));
        assert_eq!(clock.today(), date(2027, 1, 16));
        clock.set_now(Timestamp::MAX);
        clock.advance(Duration::from_secs(1));
        assert_eq!(clock.now(), Timestamp::MAX);
    }

    #[test]
    fn fixed_clock_from_a_zoned_instant() {
        let zoned: Zoned = "2026-10-22T09:30:00+02:00[+02:00]".parse().unwrap();
        let clock = FixedClock::at(&zoned);
        assert_eq!(clock.today(), date(2026, 10, 22));
        assert_eq!(clock.now(), zoned.timestamp());
    }

    #[test]
    fn recording_notifier_keeps_order() {
        let notifier = RecordingNotifier::new();
        notifier.long_timer(&SessionId::new("ses-1"), SpentTime::from_minutes(240));
        notifier.threshold_crossed(
            &TaskId::new("task-1"),
            Some(&StoryNumber::new("WEB-142")),
            SpentTime::from_minutes(48),
            Estimate::from_minutes(60),
        );
        assert_eq!(
            notifier.sent(),
            vec![
                Notification::LongTimer {
                    session: SessionId::new("ses-1"),
                    elapsed: SpentTime::from_minutes(240),
                },
                Notification::ThresholdCrossed {
                    task: TaskId::new("task-1"),
                    story: Some(StoryNumber::new("WEB-142")),
                    reported: SpentTime::from_minutes(48),
                    estimate: Estimate::from_minutes(60),
                },
            ]
        );
        notifier.clear();
        assert!(notifier.sent().is_empty());
    }
}
