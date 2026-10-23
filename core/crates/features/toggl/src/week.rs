//! The Toggl week the Hours tabs show: per day, what is sent, queued and
//! failed, from the queue and the flat days in the workspace document. It
//! reads no network.

use app_state::WorkspaceState;
use jiff::Timestamp;
use jiff::civil::Date;

use crate::feature::TogglFeature;
use crate::flat::FLAT;
use crate::queue::{QUEUE, QueueItem, QueueState, read_items};

/// How a day stands, the most urgent state first: a failed entry outranks a
/// queued one, which outranks a sent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeekDayStatus {
    /// An entry failed and waits for the user.
    Failed,
    /// An entry waits to be sent.
    Queued,
    /// Every entry is in Toggl.
    Sent,
    /// Nothing for Toggl that day.
    Empty,
}

/// One day of the Toggl week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglWeekDay {
    /// The day.
    pub date: Date,
    /// Seconds Toggl holds.
    pub sent_secs: u64,
    /// Seconds waiting to be sent.
    pub queued_secs: u64,
    /// Seconds whose entries failed.
    pub failed_secs: u64,
    /// Whether the day's time is a flat FTE entry.
    pub is_flat: bool,
    /// How the day stands.
    pub status: WeekDayStatus,
}

impl TogglFeature {
    /// The seven days from `week_start`. With `story`, session entries of
    /// other stories are left out; flat days carry no story and always show.
    /// `day_of` gives the local day of an instant.
    #[must_use]
    pub fn week(
        &self,
        state: &WorkspaceState,
        week_start: Date,
        story: Option<&str>,
        day_of: &dyn Fn(Timestamp) -> Option<Date>,
    ) -> Vec<TogglWeekDay> {
        let sessions: Vec<(Date, QueueItem)> = read_items(state, QUEUE)
            .into_iter()
            .filter(|item| story.is_none_or(|story| item.details.story.as_deref() == Some(story)))
            .filter_map(|item| Some((day_of(item.start)?, item)))
            .collect();
        let flat: Vec<(Date, QueueItem)> = read_items(state, FLAT)
            .into_iter()
            .filter_map(|item| Some((item.key.parse::<Date>().ok()?, item)))
            .collect();
        let mut days = Vec::with_capacity(7);
        let mut day = week_start;
        for _ in 0..7 {
            let flat_items: Vec<&QueueItem> = flat
                .iter()
                .filter(|(date, _)| *date == day)
                .map(|(_, item)| item)
                .collect();
            let items: Vec<&QueueItem> = sessions
                .iter()
                .filter(|(date, _)| *date == day)
                .map(|(_, item)| item)
                .chain(flat_items.iter().copied())
                .collect();
            days.push(week_day(day, &items, !flat_items.is_empty()));
            let Ok(next) = day.tomorrow() else { break };
            day = next;
        }
        days
    }
}

fn week_day(date: Date, items: &[&QueueItem], is_flat: bool) -> TogglWeekDay {
    let seconds = |state: QueueState| -> u64 {
        items
            .iter()
            .filter(|item| item.state == state)
            .map(|item| item.duration_secs)
            .sum()
    };
    let status = items
        .iter()
        .map(|item| match item.state {
            QueueState::Failed => WeekDayStatus::Failed,
            QueueState::Queued => WeekDayStatus::Queued,
            QueueState::Sent => WeekDayStatus::Sent,
        })
        .min()
        .unwrap_or(WeekDayStatus::Empty);
    TogglWeekDay {
        date,
        sent_secs: seconds(QueueState::Sent),
        queued_secs: seconds(QueueState::Queued),
        failed_secs: seconds(QueueState::Failed),
        is_flat,
        status,
    }
}
