//! The FTE flat week: one entry of `flat_hours` per working day, with no
//! task detail, created through the user's Toggl account by a Friday job.
//! Leave and public holidays are left out (the dates come in as lists; the
//! `Leave.md` parser is a later PR). Each day is a record keyed by its date,
//! so a second run creates nothing.

use std::time::Instant;

use app_state::{WorkspaceState, WorkspaceSync};
use jiff::Timestamp;
use jiff::civil::{Date, Weekday};

use crate::api::TogglApi;
use crate::entry::EntryDetails;
use crate::error::TogglError;
use crate::feature::TogglFeature;
use crate::mapping::TogglMapping;
use crate::queue::{DrainReport, QueueItem, QueueState, drain_collection, read_items, store_item};

/// The workspace-document collection of flat days.
pub(crate) const FLAT: &str = "flat";
/// The longest range one run covers, in days.
pub const MAX_FLAT_RANGE_DAYS: usize = 62;

/// What one flat job covers.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatJobInput {
    /// The first day.
    pub from: Date,
    /// The last day, included.
    pub to: Date,
    /// The weekdays that are working days.
    pub working_days: Vec<Weekday>,
    /// Days of leave.
    pub leave: Vec<Date>,
    /// Public holidays.
    pub holidays: Vec<Date>,
    /// Hours of each day's entry, above 0 and at most 24.
    pub flat_hours: f64,
    /// The workspace's tags.
    pub tags: Vec<String>,
}

/// What the job does, or did, with one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatDayStatus {
    /// A run would create its entry (the dry run's answer).
    WillCreate,
    /// Its entry waits to be sent.
    Queued,
    /// Toggl holds its entry.
    Sent,
    /// Its entry failed and waits for the user.
    Failed,
    /// A day of leave: no entry.
    Leave,
    /// A public holiday: no entry.
    Holiday,
    /// Not a working day: no entry.
    NotWorkingDay,
}

/// One day of the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatDay {
    /// The day.
    pub date: Date,
    /// What happens to it.
    pub status: FlatDayStatus,
    /// The entry's length in seconds; 0 for a day without one.
    pub duration_secs: u64,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRunReport {
    /// Days newly queued by this run.
    pub queued: u32,
    /// The sending of every queued day.
    pub drain: DrainReport,
    /// The range as it stands after the run.
    pub days: Vec<FlatDay>,
}

impl FlatJobInput {
    /// A Monday-to-Friday job over `from..=to` without leave or holidays.
    #[must_use]
    pub fn weekdays(from: Date, to: Date, flat_hours: f64) -> FlatJobInput {
        FlatJobInput {
            from,
            to,
            working_days: vec![
                Weekday::Monday,
                Weekday::Tuesday,
                Weekday::Wednesday,
                Weekday::Thursday,
                Weekday::Friday,
            ],
            leave: Vec::new(),
            holidays: Vec::new(),
            flat_hours,
            tags: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), TogglError> {
        if self.from > self.to {
            return Err(TogglError::InvalidInput(
                "the flat job's range ends before it starts".to_owned(),
            ));
        }
        if !self.flat_hours.is_finite() || self.flat_hours <= 0.0 || self.flat_hours > 24.0 {
            return Err(TogglError::InvalidInput(format!(
                "flat hours must be above 0 and at most 24, got {}",
                self.flat_hours
            )));
        }
        Ok(())
    }

    fn duration_secs(&self) -> u64 {
        // In range by `validate`: at most 86 400 seconds.
        let seconds = (self.flat_hours * 3600.0).round();
        if seconds >= 1.0 { seconds as u64 } else { 1 }
    }

    fn days(&self) -> Result<Vec<Date>, TogglError> {
        self.validate()?;
        let mut days = Vec::new();
        let mut day = self.from;
        while day <= self.to {
            if days.len() == MAX_FLAT_RANGE_DAYS {
                return Err(TogglError::InvalidInput(format!(
                    "the flat job covers at most {MAX_FLAT_RANGE_DAYS} days"
                )));
            }
            days.push(day);
            day = day
                .tomorrow()
                .map_err(|error| TogglError::InvalidInput(error.to_string()))?;
        }
        Ok(days)
    }

    fn exclusion(&self, day: Date) -> Option<FlatDayStatus> {
        if self.leave.contains(&day) {
            Some(FlatDayStatus::Leave)
        } else if self.holidays.contains(&day) {
            Some(FlatDayStatus::Holiday)
        } else if !self.working_days.contains(&day.weekday()) {
            Some(FlatDayStatus::NotWorkingDay)
        } else {
            None
        }
    }
}

/// The range a Friday job covers: Monday to that Friday. `None` on other days.
#[must_use]
pub fn flat_job_range(today: Date) -> Option<(Date, Date)> {
    if today.weekday() != Weekday::Friday {
        return None;
    }
    let monday = today.nth_weekday(-1, Weekday::Monday).ok()?;
    Some((monday, today))
}

impl TogglFeature {
    /// The dry run: what a run over `input` would do with each day, changing nothing.
    ///
    /// # Errors
    /// [`TogglError::InvalidInput`] for a reversed or too long range or bad flat hours.
    pub fn preview_flat_job(
        &self,
        state: &WorkspaceState,
        input: &FlatJobInput,
    ) -> Result<Vec<FlatDay>, TogglError> {
        let stored = read_items(state, FLAT);
        let duration = input.duration_secs();
        input
            .days()?
            .into_iter()
            .map(|day| {
                if let Some(excluded) = input.exclusion(day) {
                    return Ok(FlatDay {
                        date: day,
                        status: excluded,
                        duration_secs: 0,
                    });
                }
                let key = day.to_string();
                let status = match stored.iter().find(|item| item.key == key) {
                    Some(item) => match item.state {
                        QueueState::Queued => FlatDayStatus::Queued,
                        QueueState::Sent => FlatDayStatus::Sent,
                        QueueState::Failed => FlatDayStatus::Failed,
                    },
                    None => FlatDayStatus::WillCreate,
                };
                let duration_secs = stored
                    .iter()
                    .find(|item| item.key == key)
                    .map_or(duration, |item| item.duration_secs);
                Ok(FlatDay {
                    date: day,
                    status,
                    duration_secs,
                })
            })
            .collect()
    }

    /// Queues an entry for every working day of `input` that has none yet,
    /// then sends what is due. `start_of_day` gives the instant a day's entry
    /// starts at (the caller knows the time zone). Days already queued, sent
    /// or failed are left alone, so a second run creates nothing.
    ///
    /// # Errors
    /// [`TogglError::InvalidInput`] for a bad range, an error of
    /// `start_of_day`, or the document refused an edit.
    pub fn run_flat_job(
        &self,
        sync: &mut WorkspaceSync,
        api: &dyn TogglApi,
        mapping: &TogglMapping,
        input: &FlatJobInput,
        start_of_day: &dyn Fn(Date) -> Result<Timestamp, TogglError>,
        now: Timestamp,
    ) -> Result<FlatRunReport, TogglError> {
        let device = sync.device_id().as_str().to_owned();
        let new_items: Vec<QueueItem> = self
            .preview_flat_job(sync.state(), input)?
            .into_iter()
            .filter(|day| day.status == FlatDayStatus::WillCreate)
            .map(|day| {
                Ok(QueueItem {
                    key: day.date.to_string(),
                    session: None,
                    device: device.clone(),
                    details: EntryDetails {
                        tags: input.tags.clone(),
                        ..EntryDetails::default()
                    },
                    start: start_of_day(day.date)?,
                    duration_secs: day.duration_secs,
                    // The flat day carries no billing detail; rates stay in the app.
                    is_billable: false,
                    state: QueueState::Queued,
                    may_exist: false,
                    attempts: 0,
                    next_attempt_at: None,
                    toggl_entry_id: None,
                    last_error: None,
                })
            })
            .collect::<Result<_, TogglError>>()?;
        let queued = u32::try_from(new_items.len()).unwrap_or(u32::MAX);
        if !new_items.is_empty() {
            sync.edit(Instant::now(), |state| {
                for item in &new_items {
                    store_item(state, FLAT, item).map_err(|error| match error {
                        TogglError::AppState(error) => error,
                        other => app_state::AppStateError::InvalidSetting(other.to_string()),
                    })?;
                }
                Ok(())
            })?;
        }
        let drain = drain_collection(sync, FLAT, api, mapping, now)?;
        let days = self.preview_flat_job(sync.state(), input)?;
        Ok(FlatRunReport {
            queued,
            drain,
            days,
        })
    }

    /// Every flat day recorded, by date.
    #[must_use]
    pub fn flat_days(&self, state: &WorkspaceState) -> Vec<QueueItem> {
        let mut items = read_items(state, FLAT);
        items.sort_by(|a, b| a.key.cmp(&b.key));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn the_friday_job_covers_monday_to_friday() {
        assert_eq!(
            flat_job_range(date(2026, 10, 23)),
            Some((date(2026, 10, 19), date(2026, 10, 23)))
        );
        assert_eq!(flat_job_range(date(2026, 10, 22)), None);
    }

    #[test]
    fn days_are_classified_and_ranges_checked() {
        let mut input = FlatJobInput::weekdays(date(2026, 10, 19), date(2026, 10, 25), 8.0);
        input.leave.push(date(2026, 10, 20));
        input.holidays.push(date(2026, 10, 21));
        let statuses: Vec<Option<FlatDayStatus>> = input
            .days()
            .unwrap()
            .into_iter()
            .map(|day| input.exclusion(day))
            .collect();
        assert_eq!(
            statuses,
            [
                None,
                Some(FlatDayStatus::Leave),
                Some(FlatDayStatus::Holiday),
                None,
                None,
                Some(FlatDayStatus::NotWorkingDay),
                Some(FlatDayStatus::NotWorkingDay),
            ]
        );
        assert_eq!(input.duration_secs(), 8 * 3600);
        let reversed = FlatJobInput::weekdays(date(2026, 10, 23), date(2026, 10, 19), 8.0);
        assert!(reversed.days().is_err());
        let too_long = FlatJobInput::weekdays(date(2026, 1, 1), date(2026, 6, 1), 8.0);
        assert!(too_long.days().is_err());
        let no_hours = FlatJobInput::weekdays(date(2026, 10, 19), date(2026, 10, 19), 0.0);
        assert!(no_hours.days().is_err());
    }
}
