//! When a close-out is due: the period the header offers, the deadline from
//! `log_deadline`, and the Monday 07:30 alert.
//!
//! A week becomes due on its Friday at 14:00 and stays the one offered until
//! the next Friday at 14:00; a month becomes due on its last working day at
//! 14:00 and stays until the next month's does. The alert for a week fires
//! from the Monday after it at 07:30 until that week's successor becomes due.

use jiff::civil::{Date, DateTime, Time, Weekday};
use vault_base::{LogDeadline, SettingsWeekday};

use domain::week::{DaysOff, Period, PeriodKind};

use crate::error::CloseOutError;

/// The time of day a period becomes due on its last working day.
pub const OPENS_AT: Time = Time::constant(14, 0, 0, 0);
/// The time of the Monday alert.
pub const ALERT_AT: Time = Time::constant(7, 30, 0, 0);

/// The week the close-out offers at `now`: this week from its Friday 14:00,
/// the week before until then.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] at the ends of the calendar.
pub fn due_week(now: DateTime) -> Result<Period, CloseOutError> {
    let this_week = Period::week_containing(now.date())?;
    if now >= this_week.end().to_datetime(OPENS_AT) {
        Ok(this_week)
    } else {
        Ok(this_week.previous_week()?)
    }
}

/// The month the close-out offers at `now`: this month from its last working
/// day at 14:00, the month before until then.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] at the ends of the calendar.
pub fn due_month(now: DateTime, off: &DaysOff) -> Result<Period, CloseOutError> {
    let this_month = Period::month_containing(now.date())?;
    if now >= opens_at(&this_month, off) {
        return Ok(this_month);
    }
    let before = this_month
        .start()
        .yesterday()
        .map_err(|error| CloseOutError::InvalidInput(error.to_string()))?;
    Ok(Period::month_containing(before)?)
}

/// When `period` becomes due: its last working day at 14:00, or its last day
/// when it has no working day.
pub fn opens_at(period: &Period, off: &DaysOff) -> DateTime {
    period
        .last_working_day(off)
        .unwrap_or(period.end())
        .to_datetime(OPENS_AT)
}

/// The deadline of `period`: the first `log_deadline` weekday on or after
/// its last working day, at the deadline's time.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] for a malformed time or the calendar's end.
pub fn deadline(
    period: &Period,
    off: &DaysOff,
    log_deadline: &LogDeadline,
) -> Result<DateTime, CloseOutError> {
    let (hour, minute) = log_deadline.hour_minute().ok_or_else(|| {
        CloseOutError::InvalidInput(format!("{} is not HH:MM", log_deadline.time))
    })?;
    let last = period.last_working_day(off).unwrap_or(period.end());
    let weekday = weekday(log_deadline.weekday);
    let day = if last.weekday() == weekday {
        last
    } else {
        last.nth_weekday(1, weekday)
            .map_err(|error| CloseOutError::InvalidInput(error.to_string()))?
    };
    let time = Time::new(
        i8::try_from(hour).unwrap_or(0),
        i8::try_from(minute).unwrap_or(0),
        0,
        0,
    )
    .map_err(|error| CloseOutError::InvalidInput(error.to_string()))?;
    Ok(day.to_datetime(time))
}

/// Whether the Monday alert of week `period` may fire at `now`: from the
/// Monday after it at 07:30 until its successor is due.
///
/// # Errors
/// [`CloseOutError::InvalidInput`] at the ends of the calendar.
pub fn is_alert_window(period: &Period, now: DateTime) -> Result<bool, CloseOutError> {
    if period.kind() != PeriodKind::Week {
        return Ok(false);
    }
    let monday = monday_after(period.end())?;
    Ok(now >= monday.to_datetime(ALERT_AT) && due_week(now)? == *period)
}

/// The key the alert of `period` is recorded under in the workspace document.
pub fn alert_key(period: &Period) -> String {
    format!("close-out-alert:{}", period.id())
}

/// The Monday after `day`.
fn monday_after(day: Date) -> Result<Date, CloseOutError> {
    day.nth_weekday(1, Weekday::Monday)
        .map_err(|error| CloseOutError::InvalidInput(error.to_string()))
}

const fn weekday(day: SettingsWeekday) -> Weekday {
    match day {
        SettingsWeekday::Monday => Weekday::Monday,
        SettingsWeekday::Tuesday => Weekday::Tuesday,
        SettingsWeekday::Wednesday => Weekday::Wednesday,
        SettingsWeekday::Thursday => Weekday::Thursday,
        SettingsWeekday::Friday => Weekday::Friday,
        SettingsWeekday::Saturday => Weekday::Saturday,
        SettingsWeekday::Sunday => Weekday::Sunday,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn at(day: Date, hour: i8, minute: i8) -> DateTime {
        day.at(hour, minute, 0, 0)
    }

    fn deadline_at(weekday: SettingsWeekday, time: &str) -> LogDeadline {
        LogDeadline {
            weekday,
            time: time.to_owned(),
        }
    }

    #[test]
    fn the_week_turns_on_friday_at_two() {
        let friday = date(2026, 10, 23);
        assert_eq!(
            due_week(at(friday, 13, 59)).unwrap().start(),
            date(2026, 10, 12)
        );
        assert_eq!(
            due_week(at(friday, 14, 0)).unwrap().start(),
            date(2026, 10, 19)
        );
        assert_eq!(
            due_week(at(date(2026, 10, 26), 7, 30)).unwrap().start(),
            date(2026, 10, 19)
        );
    }

    #[test]
    fn the_deadline_is_the_next_named_weekday() {
        let week = Period::week_containing(date(2026, 10, 21)).unwrap();
        let off = DaysOff::default();
        assert_eq!(
            deadline(&week, &off, &deadline_at(SettingsWeekday::Monday, "08:00")).unwrap(),
            at(date(2026, 10, 26), 8, 0)
        );
        assert_eq!(
            deadline(&week, &off, &deadline_at(SettingsWeekday::Friday, "17:30")).unwrap(),
            at(date(2026, 10, 23), 17, 30)
        );
        let friday_off = DaysOff {
            holidays: [date(2026, 10, 23)].into_iter().collect(),
            ..DaysOff::default()
        };
        assert_eq!(
            deadline(
                &week,
                &friday_off,
                &deadline_at(SettingsWeekday::Friday, "17:30")
            )
            .unwrap(),
            at(date(2026, 10, 23), 17, 30)
        );
        assert!(deadline(&week, &off, &deadline_at(SettingsWeekday::Monday, "8")).is_err());
    }

    #[test]
    fn the_alert_window_opens_monday_at_half_past_seven() {
        let week = Period::week_containing(date(2026, 10, 21)).unwrap();
        let monday = date(2026, 10, 26);
        assert!(!is_alert_window(&week, at(date(2026, 10, 23), 15, 0)).unwrap());
        assert!(!is_alert_window(&week, at(monday, 7, 29)).unwrap());
        assert!(is_alert_window(&week, at(monday, 7, 30)).unwrap());
        assert!(is_alert_window(&week, at(date(2026, 10, 30), 13, 0)).unwrap());
        assert!(!is_alert_window(&week, at(date(2026, 10, 30), 14, 0)).unwrap());
        assert_eq!(alert_key(&week), "close-out-alert:week-2026-10-19");
    }

    #[test]
    fn the_month_turns_on_its_last_working_day() {
        let off = DaysOff::default();
        assert_eq!(
            due_month(at(date(2026, 10, 30), 13, 0), &off).unwrap().id(),
            "month-2026-09"
        );
        assert_eq!(
            due_month(at(date(2026, 10, 30), 14, 0), &off).unwrap().id(),
            "month-2026-10"
        );
        assert_eq!(
            due_month(at(date(2026, 11, 3), 9, 0), &off).unwrap().id(),
            "month-2026-10"
        );
    }
}
