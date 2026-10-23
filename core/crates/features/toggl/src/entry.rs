//! The entry model and how one entry reaches Toggl without ever being
//! created twice.
//!
//! An entry carries a description from the task and tags from the
//! workspace; its Toggl workspace and project come from the mapping when it
//! is sent, so a mapping typed in later still applies to queued entries.
//! Toggl derives the client from the project. When an attempt may have
//! created the entry although no answer came back, the next attempt looks
//! for it among the user's entries before it posts again.

use jiff::{SignedDuration, Timestamp};

use crate::api::{NewTimeEntry, TogglApi};
use crate::error::TogglError;

/// How far around an entry's start the lookup for a possibly created entry reaches.
const LOOKUP_MARGIN: SignedDuration = SignedDuration::from_secs(60);

/// What an entry says besides its time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryDetails {
    /// The task's text; empty for a flat FTE day.
    pub description: String,
    /// The story the time belongs to, for its project mapping.
    pub story: Option<String>,
    /// The task the time was spent on.
    pub task: Option<String>,
    /// The workspace's tags.
    pub tags: Vec<String>,
}

/// How one attempt to send an entry ended.
#[derive(Debug)]
pub(crate) enum Delivery {
    /// Toggl holds the entry under this id.
    Sent(u64),
    /// Try again later; `may_exist` when the entry may have been created.
    Retry { error: TogglError, may_exist: bool },
    /// Toggl refused the token: stop sending until it is replaced.
    Unauthorized,
    /// Toggl refused the entry itself; another attempt would fail the same way.
    Failed(TogglError),
}

/// Sends `entry` once. When `may_exist`, looks for it first and adopts the
/// one Toggl already holds.
pub(crate) fn deliver(api: &dyn TogglApi, entry: &NewTimeEntry, may_exist: bool) -> Delivery {
    if may_exist {
        match find_existing(api, entry) {
            Ok(Some(id)) => return Delivery::Sent(id),
            Ok(None) => {}
            Err(error) => return failure(error, true),
        }
    }
    match api.create_time_entry(entry) {
        Ok(created) => Delivery::Sent(created.id),
        Err(error) => {
            let may_exist =
                may_exist || error.is_ambiguous() || matches!(error, TogglError::Decode(_));
            failure(error, may_exist)
        }
    }
}

fn find_existing(api: &dyn TogglApi, entry: &NewTimeEntry) -> Result<Option<u64>, TogglError> {
    let from = entry
        .start
        .checked_sub(LOOKUP_MARGIN)
        .map_err(|error| TogglError::InvalidInput(error.to_string()))?;
    let to = entry
        .start
        .checked_add(LOOKUP_MARGIN)
        .map_err(|error| TogglError::InvalidInput(error.to_string()))?;
    Ok(api
        .time_entries(from, to)?
        .into_iter()
        .find(|existing| existing.matches(entry))
        .map(|existing| existing.id))
}

fn failure(error: TogglError, may_exist: bool) -> Delivery {
    match error {
        TogglError::Unauthorized | TogglError::NoToken => Delivery::Unauthorized,
        error if error.is_transient() || matches!(error, TogglError::Decode(_)) => {
            Delivery::Retry { error, may_exist }
        }
        error => Delivery::Failed(error),
    }
}

/// `start` moved by `seconds`, for an entry part after the first.
pub(crate) fn offset(start: Timestamp, seconds: u64) -> Result<Timestamp, TogglError> {
    let seconds = i64::try_from(seconds)
        .map_err(|_| TogglError::InvalidInput("the offset is too long".to_owned()))?;
    start
        .checked_add(SignedDuration::from_secs(seconds))
        .map_err(|error| TogglError::InvalidInput(error.to_string()))
}
