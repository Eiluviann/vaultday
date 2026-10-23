//! The `CalendarSource` port as a foreign trait: the Mac app implements
//! [`CalendarPort`] over EventKit, and [`ForeignCalendar`] adapts it to
//! [`domain::ports::CalendarSource`] for the scheduler and the day plan.
//!
//! Times cross the bridge as civil date-times, `YYYY-MM-DDTHH:MM:SS` in the
//! user's local time; the platform converts from its instants.

use std::sync::Arc;

use domain::day_plan::TimeBox;
use domain::ports::{CalendarError, CalendarSource};
use domain::scheduler::{Event, Span};
use domain::task::TimeBoxId;
use jiff::civil::{Date, DateTime};

/// One timed event of a picked calendar.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CalendarEvent {
    /// The event's id in the store.
    pub id: String,
    /// The calendar's name.
    pub calendar: String,
    /// The title.
    pub title: String,
    /// Local start, `YYYY-MM-DDTHH:MM:SS`.
    pub start: String,
    /// Local end, after `start`.
    pub end: String,
}

/// A locked time box to write as an event.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CalendarTimeBox {
    /// The box id, stored on the event so it can be found again.
    pub id: String,
    /// Local start, `YYYY-MM-DDTHH:MM:SS`.
    pub start: String,
    /// Length in whole minutes.
    pub duration_minutes: u32,
}

/// Why the calendar store could not do what was asked.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CalendarFailure {
    /// The store refused or failed, access not granted included.
    #[error("{reason}")]
    Unavailable {
        /// What the platform reported, in words for the user.
        reason: String,
    },
    /// No writable calendar with this name exists.
    #[error("there is no calendar named {name}")]
    UnknownCalendar {
        /// The name that was looked up.
        name: String,
    },
    /// The calendar holds no event for this time box.
    #[error("calendar {calendar} holds no event for time box {id}")]
    TimeBoxNotFound {
        /// The calendar searched.
        calendar: String,
        /// The time box id.
        id: String,
    },
}

/// The user's calendars as the platform reads and writes them; the contract
/// is [`domain::ports::CalendarSource`]'s. Reads cover only the calendars
/// the workspace picked and leave out all-day events.
#[uniffi::export(with_foreign)]
pub trait CalendarPort: Send + Sync {
    /// Every timed event touching a day from `from` to `to` (`YYYY-MM-DD`, inclusive).
    ///
    /// # Errors
    /// [`CalendarFailure::Unavailable`] when the store cannot be read.
    fn events(&self, from: String, to: String) -> Result<Vec<CalendarEvent>, CalendarFailure>;

    /// Writes or replaces the event of a locked time box in `calendar`.
    ///
    /// # Errors
    /// [`CalendarFailure::UnknownCalendar`] or [`CalendarFailure::Unavailable`].
    fn write_time_box(
        &self,
        calendar: String,
        title: String,
        time_box: CalendarTimeBox,
    ) -> Result<(), CalendarFailure>;

    /// Deletes the event of a time box from `calendar`.
    ///
    /// # Errors
    /// [`CalendarFailure::UnknownCalendar`], [`CalendarFailure::TimeBoxNotFound`]
    /// or [`CalendarFailure::Unavailable`].
    fn delete_time_box(&self, calendar: String, id: String) -> Result<(), CalendarFailure>;
}

/// [`CalendarSource`] over a platform [`CalendarPort`]; the side panel (PR 9a)
/// hands it to the scheduler.
pub struct ForeignCalendar(Arc<dyn CalendarPort>);

impl ForeignCalendar {
    /// Adapts `port`.
    #[must_use]
    pub fn new(port: Arc<dyn CalendarPort>) -> ForeignCalendar {
        ForeignCalendar(port)
    }
}

impl std::fmt::Debug for ForeignCalendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ForeignCalendar(..)")
    }
}

impl CalendarSource for ForeignCalendar {
    fn events(&self, from: Date, to: Date) -> Result<Vec<Event>, CalendarError> {
        let events = self
            .0
            .events(from.to_string(), to.to_string())
            .map_err(calendar_error)?;
        // An event whose times do not parse or that has no length blocks no
        // time the scheduler can represent; it is left out like an all-day one.
        Ok(events.into_iter().filter_map(core_event).collect())
    }

    fn write_time_box(
        &mut self,
        calendar: &str,
        title: &str,
        time_box: &TimeBox,
    ) -> Result<(), CalendarError> {
        let minutes = u32::try_from(time_box.duration.as_secs() / 60).unwrap_or(u32::MAX);
        self.0
            .write_time_box(
                calendar.to_owned(),
                title.to_owned(),
                CalendarTimeBox {
                    id: time_box.id.as_str().to_owned(),
                    start: civil_text(time_box.start),
                    duration_minutes: minutes,
                },
            )
            .map_err(calendar_error)
    }

    fn delete_time_box(&mut self, calendar: &str, id: &TimeBoxId) -> Result<(), CalendarError> {
        self.0
            .delete_time_box(calendar.to_owned(), id.as_str().to_owned())
            .map_err(calendar_error)
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CalendarFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        CalendarFailure::Unavailable {
            reason: error.reason,
        }
    }
}

fn core_event(event: CalendarEvent) -> Option<Event> {
    let start: DateTime = event.start.parse().ok()?;
    let end: DateTime = event.end.parse().ok()?;
    let span = Span::new(start, end).ok()?;
    Some(Event::new(event.id, event.calendar, event.title, span))
}

fn civil_text(at: DateTime) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S").to_string()
}

fn calendar_error(failure: CalendarFailure) -> CalendarError {
    match failure {
        CalendarFailure::Unavailable { reason } => CalendarError::Unavailable { reason },
        CalendarFailure::UnknownCalendar { name } => CalendarError::UnknownCalendar { name },
        CalendarFailure::TimeBoxNotFound { calendar, id } => CalendarError::TimeBoxNotFound {
            calendar,
            id: TimeBoxId::new(id),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use domain::day_plan::Lock;

    use super::*;

    #[derive(Default)]
    struct FakePort {
        written: Mutex<Vec<(String, String, CalendarTimeBox)>>,
    }

    impl CalendarPort for FakePort {
        fn events(&self, from: String, to: String) -> Result<Vec<CalendarEvent>, CalendarFailure> {
            assert_eq!((from.as_str(), to.as_str()), ("2026-10-22", "2026-10-22"));
            let event = |id: &str, start: &str, end: &str| CalendarEvent {
                id: id.to_owned(),
                calendar: "Work".to_owned(),
                title: "Standup".to_owned(),
                start: start.to_owned(),
                end: end.to_owned(),
            };
            Ok(vec![
                event("e1", "2026-10-22T09:00:00", "2026-10-22T09:15:00"),
                event("zero", "2026-10-22T10:00:00", "2026-10-22T10:00:00"),
                event("garbled", "9 o'clock", "2026-10-22T10:00:00"),
            ])
        }

        fn write_time_box(
            &self,
            calendar: String,
            title: String,
            time_box: CalendarTimeBox,
        ) -> Result<(), CalendarFailure> {
            if calendar != "Vaultday" {
                return Err(CalendarFailure::UnknownCalendar { name: calendar });
            }
            self.written
                .lock()
                .unwrap()
                .push((calendar, title, time_box));
            Ok(())
        }

        fn delete_time_box(&self, calendar: String, id: String) -> Result<(), CalendarFailure> {
            Err(CalendarFailure::TimeBoxNotFound { calendar, id })
        }
    }

    fn minutes(count: u64) -> Duration {
        Duration::from_secs(count * 60)
    }

    fn date() -> Date {
        "2026-10-22".parse().unwrap()
    }

    #[test]
    fn events_without_a_length_or_a_parsable_time_are_left_out() {
        let calendar = ForeignCalendar::new(Arc::new(FakePort::default()));
        let events = calendar.events(date(), date()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "e1");
        assert_eq!(events[0].span.duration(), minutes(15));
    }

    #[test]
    fn a_time_box_is_written_in_civil_time_and_errors_map_to_the_port() {
        let port = Arc::new(FakePort::default());
        let mut calendar = ForeignCalendar::new(port.clone());
        let time_box = TimeBox {
            id: TimeBoxId::new("box-1"),
            start: "2026-10-22T13:30".parse().unwrap(),
            duration: minutes(45),
            lock: Lock::Locked,
        };
        calendar
            .write_time_box("Vaultday", "WEB-142 Checkout", &time_box)
            .unwrap();
        let written = port.written.lock().unwrap();
        assert_eq!(written[0].2.start, "2026-10-22T13:30:00");
        assert_eq!(written[0].2.duration_minutes, 45);
        drop(written);

        let unknown = calendar.write_time_box("Exchange", "x", &time_box);
        assert!(matches!(
            unknown,
            Err(CalendarError::UnknownCalendar { .. })
        ));
        let missing = calendar.delete_time_box("Vaultday", &TimeBoxId::new("box-9"));
        assert!(
            matches!(missing, Err(CalendarError::TimeBoxNotFound { id, .. }) if id.as_str() == "box-9")
        );
    }
}
