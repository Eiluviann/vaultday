//! The periods a close-out covers, a working week (Monday to Friday) or a
//! calendar month, and their working days once leave and public holidays
//! are taken out.
//!
//! Weekends are never working days here: the close-out follows a Monday to
//! Friday contract week. Leave and holidays arrive as plain dates (the
//! `Leave.md` note is parsed in `vault-base`).

use std::collections::BTreeSet;

use jiff::civil::{Date, Weekday};

use crate::error::{DomainError, Result};

/// Whether a period is a week or a month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeriodKind {
    /// Monday to Friday of one week.
    Week,
    /// The first to the last day of one month.
    Month,
}

/// A stretch of days a close-out covers, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Period {
    kind: PeriodKind,
    start: Date,
    end: Date,
}

/// Days the user does not work besides weekends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaysOff {
    /// Days of leave.
    pub leave: BTreeSet<Date>,
    /// Public holidays.
    pub holidays: BTreeSet<Date>,
}

/// What a day of a period is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayKind {
    /// A working day.
    Working,
    /// A day of leave.
    Leave,
    /// A public holiday.
    Holiday,
    /// Saturday or Sunday.
    Weekend,
}

/// One day of a period and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeriodDay {
    /// The day.
    pub date: Date,
    /// What it is.
    pub kind: DayKind,
}

impl Period {
    /// Monday to Friday of the week that holds `date`; a Saturday or a Sunday
    /// belongs to the week that ends just before it.
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] at the ends of the calendar.
    pub fn week_containing(date: Date) -> Result<Period> {
        let back = i64::from(date.weekday().to_monday_zero_offset());
        let start = date
            .checked_sub(jiff::Span::new().days(back))
            .map_err(|_| DomainError::TimeOutOfRange)?;
        let end = start
            .checked_add(jiff::Span::new().days(4))
            .map_err(|_| DomainError::TimeOutOfRange)?;
        Ok(Period {
            kind: PeriodKind::Week,
            start,
            end,
        })
    }

    /// The calendar month that holds `date`.
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] at the ends of the calendar.
    pub fn month_containing(date: Date) -> Result<Period> {
        let start = date.first_of_month();
        let end = date.last_of_month();
        Ok(Period {
            kind: PeriodKind::Month,
            start,
            end,
        })
    }

    /// The week before this one; a month answers the week before its first day.
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] at the start of the calendar.
    pub fn previous_week(&self) -> Result<Period> {
        let before = self
            .start
            .checked_sub(jiff::Span::new().days(7))
            .map_err(|_| DomainError::TimeOutOfRange)?;
        Period::week_containing(before)
    }

    /// Week or month.
    pub const fn kind(&self) -> PeriodKind {
        self.kind
    }

    /// The first day.
    pub const fn start(&self) -> Date {
        self.start
    }

    /// The last day, included.
    pub const fn end(&self) -> Date {
        self.end
    }

    /// A stable name for the period: `week-2026-10-19` (its Monday) or `month-2026-10`.
    pub fn id(&self) -> String {
        match self.kind {
            PeriodKind::Week => format!("week-{}", self.start),
            PeriodKind::Month => {
                format!("month-{:04}-{:02}", self.start.year(), self.start.month())
            }
        }
    }

    /// Whether `date` falls inside the period.
    pub fn contains(&self, date: Date) -> bool {
        self.start <= date && date <= self.end
    }

    /// Every day of the period with what it is, in order.
    pub fn days(&self, off: &DaysOff) -> Vec<PeriodDay> {
        self.start
            .series(jiff::Span::new().days(1))
            .take_while(|date| *date <= self.end)
            .map(|date| PeriodDay {
                date,
                kind: day_kind(date, off),
            })
            .collect()
    }

    /// The working days of the period: Monday to Friday minus leave and holidays.
    pub fn working_days(&self, off: &DaysOff) -> Vec<Date> {
        self.days(off)
            .into_iter()
            .filter(|day| day.kind == DayKind::Working)
            .map(|day| day.date)
            .collect()
    }

    /// The last working day of the period, `None` when it has none.
    pub fn last_working_day(&self, off: &DaysOff) -> Option<Date> {
        self.working_days(off).last().copied()
    }
}

/// What `date` is: leave wins over a holiday, a holiday over a weekend.
pub fn day_kind(date: Date, off: &DaysOff) -> DayKind {
    if off.leave.contains(&date) {
        DayKind::Leave
    } else if off.holidays.contains(&date) {
        DayKind::Holiday
    } else if matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday) {
        DayKind::Weekend
    } else {
        DayKind::Working
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use proptest::prelude::*;

    fn off(leave: &[Date], holidays: &[Date]) -> DaysOff {
        DaysOff {
            leave: leave.iter().copied().collect(),
            holidays: holidays.iter().copied().collect(),
        }
    }

    #[test]
    fn a_week_runs_monday_to_friday_whatever_day_it_is_asked_from() {
        for day in 19..=25 {
            let week = Period::week_containing(date(2026, 10, day)).unwrap();
            assert_eq!(
                (week.start(), week.end()),
                (date(2026, 10, 19), date(2026, 10, 23)),
                "asked from the {day}th"
            );
            assert_eq!(week.id(), "week-2026-10-19");
        }
        let previous = Period::week_containing(date(2026, 10, 26))
            .unwrap()
            .previous_week()
            .unwrap();
        assert_eq!(previous.start(), date(2026, 10, 19));
    }

    #[test]
    fn working_days_leave_out_leave_holidays_and_weekends() {
        let week = Period::week_containing(date(2026, 10, 21)).unwrap();
        let days_off = off(&[date(2026, 10, 20)], &[date(2026, 10, 22)]);
        assert_eq!(
            week.working_days(&days_off),
            [date(2026, 10, 19), date(2026, 10, 21), date(2026, 10, 23)]
        );
        let kinds: Vec<DayKind> = week.days(&days_off).iter().map(|day| day.kind).collect();
        assert_eq!(
            kinds,
            [
                DayKind::Working,
                DayKind::Leave,
                DayKind::Working,
                DayKind::Holiday,
                DayKind::Working,
            ]
        );
        let all_off = off(
            &[
                date(2026, 10, 19),
                date(2026, 10, 20),
                date(2026, 10, 21),
                date(2026, 10, 22),
            ],
            &[date(2026, 10, 23)],
        );
        assert!(week.working_days(&all_off).is_empty());
        assert_eq!(week.last_working_day(&all_off), None);
    }

    #[test]
    fn a_month_ends_on_its_last_working_day() {
        let october = Period::month_containing(date(2026, 10, 23)).unwrap();
        assert_eq!(october.id(), "month-2026-10");
        assert_eq!(
            (october.start(), october.end()),
            (date(2026, 10, 1), date(2026, 10, 31))
        );
        // 31 October 2026 is a Saturday.
        assert_eq!(
            october.last_working_day(&DaysOff::default()),
            Some(date(2026, 10, 30))
        );
        assert_eq!(october.working_days(&DaysOff::default()).len(), 22);
        let leave_at_the_end = off(&[date(2026, 10, 30)], &[date(2026, 10, 29)]);
        assert_eq!(
            october.last_working_day(&leave_at_the_end),
            Some(date(2026, 10, 28))
        );
    }

    proptest! {
        #[test]
        fn every_week_has_five_weekdays_holding_the_date(offset in 0_i32..20_000) {
            let day = date(2000, 1, 1).checked_add(jiff::Span::new().days(offset)).unwrap();
            let week = Period::week_containing(day).unwrap();
            prop_assert_eq!(week.start().weekday(), Weekday::Monday);
            prop_assert_eq!(week.working_days(&DaysOff::default()).len(), 5);
            let is_weekend = matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday);
            prop_assert_eq!(week.contains(day), !is_weekend);
        }
    }
}
