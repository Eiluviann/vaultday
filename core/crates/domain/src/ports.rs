//! Ports: what the domain needs from the outside, as traits, with in-memory
//! implementations for tests and for callers that have no real clock yet.
//!
//! A real clock reads the system time zone, which is IO, so it lives above
//! this crate (the FFI and Node bindings supply one). The same goes for the
//! calendar: [`CalendarSource`] is implemented over EventKit by the Apple
//! layer; [`InMemoryCalendar`] serves tests and callers without a calendar.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use thiserror::Error;

use crate::day_plan::TimeBox;
use crate::estimate::{Estimate, SpentTime};
use crate::scheduler::{Event, Span};
use crate::session::SessionId;
use crate::task::{StoryNumber, TaskId, TimeBoxId};

/// The current instant and the current civil day.
pub trait Clock {
    /// The current instant.
    fn now(&self) -> Timestamp;
    /// Today's date in the user's time zone.
    fn today(&self) -> Date;
    /// The current wall-clock time in the user's time zone, for rituals
    /// that run at a time of day.
    fn local_now(&self) -> DateTime;
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
    /// The Monday alert: the close-out of the week starting `week_start`
    /// in `workspace` still misses what `missing` names.
    fn close_out_missing(&self, workspace: &str, week_start: Date, missing: &[String]);
}

/// Why the calendar could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CalendarError {
    /// The calendar store refused or failed; the message is the platform's.
    #[error("the calendar is unavailable: {reason}")]
    Unavailable {
        /// What the platform reported.
        reason: String,
    },
    /// No calendar with this name exists in the store.
    #[error("there is no calendar named {name}")]
    UnknownCalendar {
        /// The name that was looked up.
        name: String,
    },
    /// The named calendar holds no event for this time box.
    #[error("calendar {calendar} holds no event for time box {id}")]
    TimeBoxNotFound {
        /// The calendar that was searched.
        calendar: String,
        /// The time box whose event was looked for.
        id: TimeBoxId,
    },
}

/// The user's calendars: events to plan around, and the private calendar the
/// locked time boxes are written to.
///
/// Events come back in the user's local civil time, which is what the day plan
/// and the scheduler work in; the adapter converts from the platform's
/// instants. A time box written here is read back as an event of its calendar,
/// so a caller planning a day leaves the time-box calendar out of the busy
/// events (the plan already holds those boxes as locked).
pub trait CalendarSource {
    /// Every event that touches a day from `from` to `to`, both inclusive,
    /// across all calendars, in no particular order.
    ///
    /// The scheduler treats every event returned as busy time, so the adapter
    /// leaves out what blocks no time: all-day events (a holiday, a birthday)
    /// and events without length, which a [`Span`] cannot hold anyway.
    ///
    /// # Errors
    /// [`CalendarError::Unavailable`] when the store cannot be read.
    fn events(&self, from: Date, to: Date) -> Result<Vec<Event>, CalendarError>;

    /// Writes a locked time box as an event titled `title` into the calendar
    /// named `calendar`, replacing that calendar's event for the same time box
    /// if there is one.
    ///
    /// # Errors
    /// [`CalendarError::UnknownCalendar`] when no such calendar exists, or
    /// [`CalendarError::Unavailable`] when the store cannot be written.
    fn write_time_box(
        &mut self,
        calendar: &str,
        title: &str,
        time_box: &TimeBox,
    ) -> Result<(), CalendarError>;

    /// Deletes the event written for a time box from the calendar named `calendar`.
    ///
    /// # Errors
    /// [`CalendarError::UnknownCalendar`] when no such calendar exists,
    /// [`CalendarError::TimeBoxNotFound`] when the calendar holds no event for
    /// the box, or [`CalendarError::Unavailable`] when the store cannot be written.
    fn delete_time_box(&mut self, calendar: &str, id: &TimeBoxId) -> Result<(), CalendarError>;
}

/// A calendar store held in memory: named calendars, their events and the
/// time boxes written to them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryCalendar {
    calendars: Vec<String>,
    events: Vec<Event>,
    time_boxes: Vec<WrittenTimeBox>,
}

/// A time box as the in-memory store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WrittenTimeBox {
    calendar: String,
    title: String,
    time_box: TimeBox,
}

impl InMemoryCalendar {
    /// A store with the named calendars and no events.
    pub fn with_calendars(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            calendars: names.into_iter().map(Into::into).collect(),
            events: Vec::new(),
            time_boxes: Vec::new(),
        }
    }

    /// Adds an event, creating its calendar when the store does not know it yet.
    pub fn add_event(&mut self, event: Event) {
        if !self.has_calendar(&event.calendar) {
            self.calendars.push(event.calendar.clone());
        }
        self.events.push(event);
    }

    /// The names of the calendars in the store, in the order they were added.
    pub fn calendars(&self) -> &[String] {
        &self.calendars
    }

    /// The time boxes written so far, with the calendar each sits in.
    pub fn written_time_boxes(&self) -> Vec<(&str, &TimeBox)> {
        self.time_boxes
            .iter()
            .map(|written| (written.calendar.as_str(), &written.time_box))
            .collect()
    }

    fn has_calendar(&self, name: &str) -> bool {
        self.calendars.iter().any(|known| known == name)
    }

    fn ensure_calendar(&self, name: &str) -> Result<(), CalendarError> {
        if self.has_calendar(name) {
            Ok(())
        } else {
            Err(CalendarError::UnknownCalendar {
                name: name.to_owned(),
            })
        }
    }
}

impl CalendarSource for InMemoryCalendar {
    fn events(&self, from: Date, to: Date) -> Result<Vec<Event>, CalendarError> {
        let day_starts = from.to_datetime(Time::MIN);
        let touches_range =
            |event: &Event| event.span.start().date() <= to && event.span.end() > day_starts;
        let written = self.time_boxes.iter().filter_map(|written| {
            let end = written.time_box.end().ok()?;
            let span = Span::new(written.time_box.start, end).ok()?;
            Some(Event {
                id: written.time_box.id.as_str().to_owned(),
                calendar: written.calendar.clone(),
                title: written.title.clone(),
                span,
            })
        });
        Ok(self
            .events
            .iter()
            .cloned()
            .chain(written)
            .filter(touches_range)
            .collect())
    }

    fn write_time_box(
        &mut self,
        calendar: &str,
        title: &str,
        time_box: &TimeBox,
    ) -> Result<(), CalendarError> {
        self.ensure_calendar(calendar)?;
        self.time_boxes.retain(|written| {
            !(written.calendar == calendar && written.time_box.id == time_box.id)
        });
        self.time_boxes.push(WrittenTimeBox {
            calendar: calendar.to_owned(),
            title: title.to_owned(),
            time_box: time_box.clone(),
        });
        Ok(())
    }

    fn delete_time_box(&mut self, calendar: &str, id: &TimeBoxId) -> Result<(), CalendarError> {
        self.ensure_calendar(calendar)?;
        let before = self.time_boxes.len();
        self.time_boxes
            .retain(|written| !(written.calendar == calendar && written.time_box.id == *id));
        if self.time_boxes.len() == before {
            return Err(CalendarError::TimeBoxNotFound {
                calendar: calendar.to_owned(),
                id: id.clone(),
            });
        }
        Ok(())
    }
}

/// A clock that stands still until a test moves it.
#[derive(Debug, Clone)]
pub struct FixedClock {
    now: Cell<Timestamp>,
    today: Cell<Date>,
    offset: Offset,
}

impl FixedClock {
    /// A clock fixed at `now` on `today`, in UTC.
    pub const fn new(now: Timestamp, today: Date) -> Self {
        Self {
            now: Cell::new(now),
            today: Cell::new(today),
            offset: Offset::UTC,
        }
    }

    /// A clock fixed at the zoned instant, whose date is today, in the
    /// instant's offset.
    pub fn at(zoned: &Zoned) -> Self {
        Self {
            offset: zoned.offset(),
            ..Self::new(zoned.timestamp(), zoned.date())
        }
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

    fn local_now(&self) -> DateTime {
        self.now
            .get()
            .to_zoned(TimeZone::fixed(self.offset))
            .datetime()
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
    /// See [`Notifier::close_out_missing`].
    CloseOutMissing {
        /// The workspace.
        workspace: String,
        /// The Monday of the week.
        week_start: Date,
        /// What is still missing.
        missing: Vec<String>,
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

    fn close_out_missing(&self, workspace: &str, week_start: Date, missing: &[String]) {
        self.sent.borrow_mut().push(Notification::CloseOutMissing {
            workspace: workspace.to_owned(),
            week_start,
            missing: missing.to_vec(),
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
        assert_eq!(clock.local_now(), date(2026, 10, 22).at(9, 30, 0, 0));
    }

    #[test]
    fn in_memory_calendar_reads_events_and_written_boxes_by_day() {
        let mut calendar = InMemoryCalendar::with_calendars(["Work", "Vaultday"]);
        let span = |day: Date, start: (i8, i8), end: (i8, i8)| {
            Span::new(day.at(start.0, start.1, 0, 0), day.at(end.0, end.1, 0, 0)).unwrap()
        };
        calendar.add_event(Event::new(
            "ev-1",
            "Work",
            "Stand-up",
            span(date(2026, 10, 22), (9, 0), (9, 15)),
        ));
        calendar.add_event(Event::new(
            "ev-2",
            "Shared",
            "Planning",
            span(date(2026, 10, 23), (10, 0), (11, 0)),
        ));
        assert_eq!(calendar.calendars(), ["Work", "Vaultday", "Shared"]);

        let time_box = TimeBox::unlocked(
            "box-1",
            date(2026, 10, 22).at(14, 0, 0, 0),
            Duration::from_secs(3600),
        );
        calendar
            .write_time_box("Vaultday", "Write the checkout test", &time_box)
            .unwrap();
        // Writing the same box again replaces the event instead of adding one.
        calendar
            .write_time_box("Vaultday", "Write the checkout test", &time_box)
            .unwrap();
        assert_eq!(calendar.written_time_boxes().len(), 1);

        let day = calendar
            .events(date(2026, 10, 22), date(2026, 10, 22))
            .unwrap();
        let titles: Vec<&str> = day.iter().map(|event| event.title.as_str()).collect();
        assert_eq!(titles, ["Stand-up", "Write the checkout test"]);
        let written = day.iter().find(|event| event.id == "box-1").unwrap();
        assert_eq!(written.calendar, "Vaultday");
        assert_eq!(written.span, span(date(2026, 10, 22), (14, 0), (15, 0)));

        let both = calendar
            .events(date(2026, 10, 22), date(2026, 10, 23))
            .unwrap();
        assert_eq!(both.len(), 3);
        // An event ending at midnight belongs to the day before only.
        calendar.add_event(Event::new(
            "ev-3",
            "Work",
            "Late call",
            span(date(2026, 10, 23), (23, 0), (23, 59)),
        ));
        calendar.add_event(Event::new(
            "ev-4",
            "Work",
            "Night shift",
            Span::new(
                date(2026, 10, 23).at(23, 0, 0, 0),
                date(2026, 10, 24).at(0, 0, 0, 0),
            )
            .unwrap(),
        ));
        assert!(
            calendar
                .events(date(2026, 10, 24), date(2026, 10, 24))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            calendar
                .events(date(2026, 10, 23), date(2026, 10, 23))
                .unwrap()
                .len(),
            3
        );
        assert!(
            calendar
                .events(date(2026, 10, 24), date(2026, 10, 24))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn in_memory_calendar_refuses_unknown_calendars_and_missing_boxes() {
        let mut calendar = InMemoryCalendar::with_calendars(["Vaultday"]);
        let time_box = TimeBox::unlocked(
            "box-1",
            date(2026, 10, 22).at(14, 0, 0, 0),
            Duration::from_secs(1800),
        );
        assert_eq!(
            calendar.write_time_box("Nowhere", "x", &time_box),
            Err(CalendarError::UnknownCalendar {
                name: "Nowhere".to_owned()
            })
        );
        assert_eq!(
            calendar.delete_time_box("Vaultday", &TimeBoxId::new("box-1")),
            Err(CalendarError::TimeBoxNotFound {
                calendar: "Vaultday".to_owned(),
                id: TimeBoxId::new("box-1"),
            })
        );
        calendar.write_time_box("Vaultday", "x", &time_box).unwrap();
        calendar
            .delete_time_box("Vaultday", &TimeBoxId::new("box-1"))
            .unwrap();
        assert!(calendar.written_time_boxes().is_empty());
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
