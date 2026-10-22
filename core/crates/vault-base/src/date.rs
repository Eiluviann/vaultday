//! Calendar dates and clock times without a timezone, enough for the vault's
//! `YYYY-MM-DD` fields and `HH:MM` log prefixes. Arithmetic uses the proleptic
//! Gregorian calendar so ages in days are exact.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// A calendar date, `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

/// A wall-clock time of day, `HH:MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time {
    hour: u8,
    minute: u8,
}

const SECONDS_PER_DAY: u64 = 86_400;

impl Date {
    /// Builds a date, returning `None` when the day does not exist in that month.
    #[must_use]
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Parses a strict `YYYY-MM-DD` string; anything else is `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
            return None;
        }
        let year: i32 = parse_digits(text.get(0..4)?)?.try_into().ok()?;
        let month: u8 = parse_digits(text.get(5..7)?)?.try_into().ok()?;
        let day: u8 = parse_digits(text.get(8..10)?)?.try_into().ok()?;
        Date::new(year, month, day)
    }

    /// The UTC calendar date of a system time; the caller decides whether UTC
    /// is the right day for "today".
    #[must_use]
    pub fn from_system_time_utc(time: SystemTime) -> Date {
        let days_since_epoch = match time.duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_secs() / SECONDS_PER_DAY).unwrap_or(i64::MAX),
            Err(before_epoch) => {
                let seconds = before_epoch.duration().as_secs();
                -i64::try_from(seconds.div_ceil(SECONDS_PER_DAY)).unwrap_or(i64::MAX)
            }
        };
        Date::from_days_since_epoch(days_since_epoch)
    }

    /// The year.
    #[must_use]
    pub fn year(self) -> i32 {
        self.year
    }

    /// The month, 1 to 12.
    #[must_use]
    pub fn month(self) -> u8 {
        self.month
    }

    /// The day of the month, 1 to 31.
    #[must_use]
    pub fn day(self) -> u8 {
        self.day
    }

    /// Days since 1970-01-01, negative before it.
    #[must_use]
    pub fn days_since_epoch(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month = i64::from(self.month);
        let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5
            + i64::from(self.day)
            - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// The date `days` days after 1970-01-01. Days beyond the `i32` year
    /// range clamp to the first or last representable date.
    #[must_use]
    pub fn from_days_since_epoch(days: i64) -> Date {
        // Howard Hinnant's civil_from_days, in i128 so no input can overflow.
        let shifted = i128::from(days) + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + i128::from(month <= 2);
        match i32::try_from(year) {
            Ok(year) => Date {
                year,
                month: u8::try_from(month).unwrap_or(1),
                day: u8::try_from(day).unwrap_or(1),
            },
            Err(_) if year < 0 => Date {
                year: i32::MIN,
                month: 1,
                day: 1,
            },
            Err(_) => Date {
                year: i32::MAX,
                month: 12,
                day: 31,
            },
        }
    }

    /// Signed number of days from `self` to `other`; positive when `other` is later.
    #[must_use]
    pub fn days_until(self, other: Date) -> i64 {
        other.days_since_epoch() - self.days_since_epoch()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

impl Time {
    /// Builds a time, returning `None` outside `00:00` to `23:59`.
    #[must_use]
    pub fn new(hour: u8, minute: u8) -> Option<Time> {
        (hour < 24 && minute < 60).then_some(Time { hour, minute })
    }

    /// Parses a zero-padded `HH:MM`; anything else is `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Time> {
        if text.len() != 5 || text.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        let hour: u8 = parse_digits(text.get(0..2)?)?.try_into().ok()?;
        let minute: u8 = parse_digits(text.get(3..5)?)?.try_into().ok()?;
        Time::new(hour, minute)
    }

    /// The hour, 0 to 23.
    #[must_use]
    pub fn hour(self) -> u8 {
        self.hour
    }

    /// The minute, 0 to 59.
    #[must_use]
    pub fn minute(self) -> u8 {
        self.minute
    }

    /// Minutes since midnight.
    #[must_use]
    pub fn minutes_since_midnight(self) -> u32 {
        u32::from(self.hour) * 60 + u32::from(self.minute)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:02}:{:02}", self.hour, self.minute)
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        _ => 28,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Parses an all-ASCII-digit string; `None` on any other character.
fn parse_digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{Date, Time};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn parses_and_prints_strict_dates() {
        let date = Date::parse("2026-10-22").expect("valid date");
        assert_eq!((date.year(), date.month(), date.day()), (2026, 10, 22));
        assert_eq!(date.to_string(), "2026-10-22");
        for bad in [
            "2026-1-22",
            "2026-13-01",
            "2026-02-30",
            "26-10-22",
            "2026/10/22",
            "2026-10-22 ",
        ] {
            assert!(Date::parse(bad).is_none(), "{bad}");
        }
        assert!(Date::parse("2024-02-29").is_some());
        assert!(Date::parse("2023-02-29").is_none());
    }

    #[test]
    fn day_arithmetic_round_trips_through_the_epoch() {
        for text in [
            "1970-01-01",
            "1969-12-31",
            "2000-02-29",
            "2026-10-22",
            "2100-03-01",
        ] {
            let date = Date::parse(text).expect("valid date");
            assert_eq!(
                Date::from_days_since_epoch(date.days_since_epoch()),
                date,
                "{text}"
            );
        }
        assert_eq!(Date::from_days_since_epoch(i64::MAX).year(), i32::MAX);
        assert_eq!(Date::from_days_since_epoch(i64::MIN).year(), i32::MIN);
        let from = Date::parse("2026-10-01").expect("valid date");
        let to = Date::parse("2026-10-22").expect("valid date");
        assert_eq!(from.days_until(to), 21);
        assert_eq!(to.days_until(from), -21);
    }

    #[test]
    fn system_time_maps_to_the_utc_day() {
        let time = UNIX_EPOCH + Duration::from_secs(1_792_000_000);
        assert_eq!(Date::from_system_time_utc(time).to_string(), "2026-10-14");
        assert_eq!(
            Date::from_system_time_utc(UNIX_EPOCH).to_string(),
            "1970-01-01"
        );
    }

    #[test]
    fn parses_zero_padded_times_only() {
        assert_eq!(
            Time::parse("09:05").map(|time| time.minutes_since_midnight()),
            Some(545)
        );
        assert_eq!(
            Time::parse("23:59").map(|time| time.to_string()),
            Some("23:59".to_owned())
        );
        for bad in ["9:05", "24:00", "12:60", "12-00", "12:0"] {
            assert!(Time::parse(bad).is_none(), "{bad}");
        }
    }
}
