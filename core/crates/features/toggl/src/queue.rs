//! The offline queue: a stopped session of an hourly or fixed-price
//! workspace becomes one or two entries kept in the workspace document,
//! keyed by session id, and the app's scheduler drains them.
//!
//! The queue never duplicates an entry: enqueueing a session twice adds
//! nothing, only the device that queued an entry sends it, a sent entry is
//! never sent again, and every entry is written to disk as possibly sent
//! before its post, so any later attempt (after a 5xx, a lost answer or a
//! crash before the answer was recorded) looks it up before posting again. Retries are bounded: after
//! [`MAX_ATTEMPTS`] an entry is marked failed and waits for the user.

use std::time::{Duration, Instant};

use app_state::{AppStateError, WorkspaceState, WorkspaceSync};
use domain::session::{Session, SessionRecord};
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::api::{NewTimeEntry, TogglApi};
use crate::billing::{BillingPolicy, billed_parts};
use crate::entry::{Delivery, EntryDetails, deliver, offset};
use crate::error::TogglError;
use crate::feature::{TogglFeature, feature_id};
use crate::mapping::TogglMapping;

/// The workspace-document collection of session entries.
pub(crate) const QUEUE: &str = "queue";
/// Attempts before an entry is marked failed.
pub const MAX_ATTEMPTS: u32 = 6;
/// The wait after the first failed attempt; it doubles up to [`MAX_RETRY_DELAY`].
pub const BASE_RETRY_DELAY: Duration = Duration::from_secs(60);
/// The longest wait between attempts.
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

/// Where an entry stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    /// Waiting to be sent, or to be sent again.
    Queued,
    /// Toggl holds it.
    Sent,
    /// Given up after [`MAX_ATTEMPTS`] or refused by Toggl; waits for the user.
    Failed,
}

/// One entry in the queue (or one flat FTE day).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    /// The record key: the session id, `<session>.overrun`, or the day.
    pub key: String,
    /// The session it came from; `None` for a flat day.
    pub session: Option<String>,
    /// The device that queued it, the only one that sends it.
    pub device: String,
    /// Description, story, task and tags.
    pub details: EntryDetails,
    /// When the time starts.
    pub start: Timestamp,
    /// How long, in whole seconds.
    pub duration_secs: u64,
    /// Whether Toggl marks it billable.
    pub is_billable: bool,
    /// Where it stands.
    pub state: QueueState,
    /// Whether an earlier attempt may have created it in Toggl.
    pub may_exist: bool,
    /// Failed attempts so far.
    pub attempts: u32,
    /// Not before this instant is it sent again.
    pub next_attempt_at: Option<Timestamp>,
    /// Its Toggl id, once sent.
    pub toggl_entry_id: Option<u64>,
    /// Why the last attempt failed.
    pub last_error: Option<String>,
}

/// What enqueueing a session did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// This many entries were queued (two for a fixed-price overrun).
    Queued(usize),
    /// The session was queued before; nothing changed.
    AlreadyQueued,
    /// The session sends nothing: an FTE workspace or no billable time.
    NotSent,
    /// The session has not stopped.
    StillRunning,
}

/// What one drain did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrainReport {
    /// Entries Toggl now holds.
    pub sent: u32,
    /// Entries that failed this time and will be tried again.
    pub retrying: u32,
    /// Entries that failed for good this time.
    pub failed: u32,
    /// Entries left for later: not due yet, or after a refused token.
    pub waiting: u32,
    /// Toggl refused the token; nothing more was sent.
    pub is_unauthorized: bool,
    /// No Toggl workspace is mapped; nothing was sent.
    pub is_unmapped: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredItem {
    session: Option<String>,
    device: String,
    description: String,
    story: Option<String>,
    task: Option<String>,
    tags: Vec<String>,
    start: String,
    duration_secs: u64,
    billable: bool,
    state: String,
    may_exist: bool,
    attempts: u32,
    next_attempt_at: Option<String>,
    toggl_entry_id: Option<u64>,
    last_error: Option<String>,
}

impl QueueState {
    const fn label(self) -> &'static str {
        match self {
            QueueState::Queued => "queued",
            QueueState::Sent => "sent",
            QueueState::Failed => "failed",
        }
    }

    fn parse(label: &str) -> Option<QueueState> {
        [QueueState::Queued, QueueState::Sent, QueueState::Failed]
            .into_iter()
            .find(|state| state.label() == label)
    }
}

impl QueueItem {
    /// The entry as Toggl gets it under `mapping`; `None` until a Toggl
    /// workspace is mapped.
    #[must_use]
    pub fn to_new_entry(&self, mapping: &TogglMapping) -> Option<NewTimeEntry> {
        Some(NewTimeEntry {
            workspace_id: mapping.toggl_workspace_id?,
            project_id: mapping.project_for(self.details.story.as_deref()),
            description: self.details.description.clone(),
            start: self.start,
            duration_secs: self.duration_secs,
            tags: self.details.tags.clone(),
            is_billable: self.is_billable,
        })
    }

    fn is_due(&self, device: &str, now: Timestamp) -> bool {
        self.state == QueueState::Queued
            && self.device == device
            && self.next_attempt_at.is_none_or(|at| at <= now)
    }

    fn encode(&self) -> Result<String, TogglError> {
        let stored = StoredItem {
            session: self.session.clone(),
            device: self.device.clone(),
            description: self.details.description.clone(),
            story: self.details.story.clone(),
            task: self.details.task.clone(),
            tags: self.details.tags.clone(),
            start: self.start.to_string(),
            duration_secs: self.duration_secs,
            billable: self.is_billable,
            state: self.state.label().to_owned(),
            may_exist: self.may_exist,
            attempts: self.attempts,
            next_attempt_at: self.next_attempt_at.map(|at| at.to_string()),
            toggl_entry_id: self.toggl_entry_id,
            last_error: self.last_error.clone(),
        };
        serde_json::to_string(&stored).map_err(|error| TogglError::InvalidInput(error.to_string()))
    }

    /// A stored record; `None` for one a newer build wrote in another shape.
    fn decode(key: &str, text: &str) -> Option<QueueItem> {
        let stored: StoredItem = serde_json::from_str(text).ok()?;
        let next_attempt_at = match stored.next_attempt_at {
            Some(at) => Some(at.parse().ok()?),
            None => None,
        };
        Some(QueueItem {
            key: key.to_owned(),
            session: stored.session,
            device: stored.device,
            details: EntryDetails {
                description: stored.description,
                story: stored.story,
                task: stored.task,
                tags: stored.tags,
            },
            start: stored.start.parse().ok()?,
            duration_secs: stored.duration_secs,
            is_billable: stored.billable,
            state: QueueState::parse(&stored.state)?,
            may_exist: stored.may_exist,
            attempts: stored.attempts,
            next_attempt_at,
            toggl_entry_id: stored.toggl_entry_id,
            last_error: stored.last_error,
        })
    }

    /// The item after one attempt ended with `delivery`; `None` for a refused token.
    fn after(&self, delivery: Delivery, now: Timestamp) -> Option<QueueItem> {
        let mut next = self.clone();
        match delivery {
            Delivery::Sent(id) => {
                next.state = QueueState::Sent;
                next.toggl_entry_id = Some(id);
                next.next_attempt_at = None;
                next.last_error = None;
            }
            Delivery::Retry { error, may_exist } => {
                next.attempts = self.attempts.saturating_add(1);
                next.may_exist = may_exist;
                next.last_error = Some(error.to_string());
                if next.attempts >= MAX_ATTEMPTS {
                    next.state = QueueState::Failed;
                    next.next_attempt_at = None;
                } else {
                    let wait = error
                        .retry_after()
                        .unwrap_or_else(|| retry_delay(next.attempts));
                    next.next_attempt_at = Some(later(now, wait));
                }
            }
            Delivery::Failed(error) => {
                next.attempts = self.attempts.saturating_add(1);
                next.state = QueueState::Failed;
                next.next_attempt_at = None;
                next.last_error = Some(error.to_string());
            }
            Delivery::Unauthorized => return None,
        }
        Some(next)
    }
}

impl TogglFeature {
    /// Queues the entries a stopped session becomes under `policy`. A session
    /// already queued is left alone, so every stop surface may call this.
    ///
    /// # Errors
    /// [`TogglError::InvalidInput`] for a session whose stored fields break
    /// their rules, or the document refused the edit.
    pub fn enqueue_session(
        &self,
        state: &mut WorkspaceState,
        session: &SessionRecord,
        policy: &BillingPolicy,
        details: &EntryDetails,
    ) -> Result<EnqueueOutcome, TogglError> {
        let Some(stopped_at) = session.stopped_at else {
            return Ok(EnqueueOutcome::StillRunning);
        };
        let session_key = session.id.as_str();
        let items = read_items(state, QUEUE);
        if items
            .iter()
            .any(|item| item.session.as_deref() == Some(session_key))
        {
            return Ok(EnqueueOutcome::AlreadyQueued);
        }
        if !policy.sends_sessions() {
            return Ok(EnqueueOutcome::NotSent);
        }
        let actual = Session::from_record(session.clone())
            .map_err(|error| TogglError::InvalidInput(error.to_string()))?
            .elapsed(stopped_at)
            .as_duration();
        let already_billed = billed_for_task(&items, details.task.as_deref());
        let parts = billed_parts(policy, actual, already_billed);
        if parts.is_empty() {
            return Ok(EnqueueOutcome::NotSent);
        }
        let device = state.device_id().as_str().to_owned();
        for part in &parts {
            let (key, part_details) = match part.suffix {
                // The suffix also tags the part, so an unpaid overrun reads
                // as one in Toggl and not as a second session.
                Some(suffix) => (
                    format!("{session_key}.{suffix}"),
                    EntryDetails {
                        tags: tagged(&details.tags, suffix),
                        ..details.clone()
                    },
                ),
                None => (session_key.to_owned(), details.clone()),
            };
            let item = QueueItem {
                key,
                session: Some(session_key.to_owned()),
                device: device.clone(),
                details: part_details,
                start: offset(session.started_at, part.offset.as_secs())?,
                duration_secs: part.duration.as_secs(),
                is_billable: part.is_billable,
                state: QueueState::Queued,
                may_exist: false,
                attempts: 0,
                next_attempt_at: None,
                toggl_entry_id: None,
                last_error: None,
            };
            store_item(state, QUEUE, &item)?;
        }
        Ok(EnqueueOutcome::Queued(parts.len()))
    }

    /// Every entry in the queue, by start.
    #[must_use]
    pub fn queue(&self, state: &WorkspaceState) -> Vec<QueueItem> {
        let mut items = read_items(state, QUEUE);
        items.sort_by(|a, b| (a.start, &a.key).cmp(&(b.start, &b.key)));
        items
    }

    /// Sends this device's due entries through `api`, one attempt each, and
    /// records every outcome as it happens.
    ///
    /// # Errors
    /// The document refused an edit; failures of single entries are
    /// recorded on them and counted in the report instead.
    pub fn drain_queue(
        &self,
        sync: &mut WorkspaceSync,
        api: &dyn TogglApi,
        mapping: &TogglMapping,
        now: Timestamp,
    ) -> Result<DrainReport, TogglError> {
        drain_collection(sync, QUEUE, api, mapping, now)
    }

    /// Puts a failed entry back in the queue with its attempts reset, at the
    /// user's request. Returns whether there was such an entry.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn retry_failed(&self, state: &mut WorkspaceState, key: &str) -> Result<bool, TogglError> {
        retry_failed_in(state, QUEUE, key)
    }
}

/// Every item of `collection` that decodes.
pub(crate) fn read_items(state: &WorkspaceState, collection: &str) -> Vec<QueueItem> {
    state
        .feature_records(&feature_id(), collection)
        .into_iter()
        .filter_map(|(key, text)| QueueItem::decode(&key, &text))
        .collect()
}

pub(crate) fn store_item(
    state: &mut WorkspaceState,
    collection: &str,
    item: &QueueItem,
) -> Result<(), TogglError> {
    state.put_feature_record(&feature_id(), collection, &item.key, &item.encode()?)?;
    Ok(())
}

pub(crate) fn retry_failed_in(
    state: &mut WorkspaceState,
    collection: &str,
    key: &str,
) -> Result<bool, TogglError> {
    let Some(item) = read_items(state, collection)
        .into_iter()
        .find(|item| item.key == key && item.state == QueueState::Failed)
    else {
        return Ok(false);
    };
    let retried = QueueItem {
        state: QueueState::Queued,
        attempts: 0,
        next_attempt_at: None,
        ..item
    };
    store_item(state, collection, &retried)?;
    Ok(true)
}

/// One attempt for each due item of this device in `collection`.
pub(crate) fn drain_collection(
    sync: &mut WorkspaceSync,
    collection: &str,
    api: &dyn TogglApi,
    mapping: &TogglMapping,
    now: Timestamp,
) -> Result<DrainReport, TogglError> {
    let device = sync.device_id().as_str().to_owned();
    let items: Vec<QueueItem> = read_items(sync.state(), collection)
        .into_iter()
        .filter(|item| item.state == QueueState::Queued && item.device == device)
        .collect();
    let mut report = DrainReport::default();
    if mapping.toggl_workspace_id.is_none() {
        report.is_unmapped = !items.is_empty();
        report.waiting = count(items.len());
        return Ok(report);
    }
    for item in &items {
        if report.is_unauthorized || !item.is_due(&device, now) {
            report.waiting += 1;
            continue;
        }
        let Some(entry) = item.to_new_entry(mapping) else {
            report.waiting += 1;
            continue;
        };
        mark_in_flight(sync, collection, item)?;
        let Some(next) = item.after(deliver(api, &entry, item.may_exist), now) else {
            report.is_unauthorized = true;
            report.waiting += 1;
            continue;
        };
        match next.state {
            QueueState::Sent => report.sent += 1,
            QueueState::Queued => report.retrying += 1,
            QueueState::Failed => report.failed += 1,
        }
        sync.edit(Instant::now(), |state| {
            store_item(state, collection, &next).map_err(app_state_error)
        })?;
    }
    Ok(report)
}

/// Records on disk, before the post, that the item may exist in Toggl from
/// now on, so a crash between the post and the record of its answer is
/// followed by a lookup, never a second post.
fn mark_in_flight(
    sync: &mut WorkspaceSync,
    collection: &str,
    item: &QueueItem,
) -> Result<(), TogglError> {
    if !item.may_exist {
        let marked = QueueItem {
            may_exist: true,
            ..item.clone()
        };
        sync.edit(Instant::now(), |state| {
            store_item(state, collection, &marked).map_err(app_state_error)
        })?;
    }
    if sync.has_unflushed_edits() {
        sync.flush()?;
    }
    Ok(())
}

/// What earlier entries of `task` billed under a fixed-price cap.
fn billed_for_task(items: &[QueueItem], task: Option<&str>) -> Duration {
    let Some(task) = task else {
        return Duration::ZERO;
    };
    Duration::from_secs(
        items
            .iter()
            .filter(|item| item.is_billable && item.details.task.as_deref() == Some(task))
            .map(|item| item.duration_secs)
            .sum(),
    )
}

/// `tags` with `tag` added once.
fn tagged(tags: &[String], tag: &str) -> Vec<String> {
    let mut tags = tags.to_vec();
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_owned());
    }
    tags
}

fn retry_delay(attempts: u32) -> Duration {
    BASE_RETRY_DELAY
        .saturating_mul(2_u32.saturating_pow(attempts.saturating_sub(1)))
        .min(MAX_RETRY_DELAY)
}

fn later(now: Timestamp, wait: Duration) -> Timestamp {
    let wait = SignedDuration::try_from(wait).unwrap_or(SignedDuration::MAX);
    now.checked_add(wait).unwrap_or(Timestamp::MAX)
}

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

/// Carries a feature error through a document edit, which speaks `AppStateError`.
fn app_state_error(error: TogglError) -> AppStateError {
    match error {
        TogglError::AppState(error) => error,
        other => AppStateError::InvalidSetting(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delays_double_up_to_the_cap() {
        assert_eq!(retry_delay(1), Duration::from_secs(60));
        assert_eq!(retry_delay(2), Duration::from_secs(120));
        assert_eq!(retry_delay(10), MAX_RETRY_DELAY);
    }

    #[test]
    fn a_record_round_trips_and_a_foreign_one_is_skipped() {
        let item = QueueItem {
            key: "ses-1".to_owned(),
            session: Some("ses-1".to_owned()),
            device: "mac-a".to_owned(),
            details: EntryDetails {
                description: "Write the brief".to_owned(),
                story: Some("WEB-1".to_owned()),
                task: Some("task-1".to_owned()),
                tags: vec!["vaultday".to_owned()],
            },
            start: "2026-10-19T09:00:00Z".parse().unwrap(),
            duration_secs: 900,
            is_billable: true,
            state: QueueState::Queued,
            may_exist: true,
            attempts: 2,
            next_attempt_at: Some("2026-10-19T10:00:00Z".parse().unwrap()),
            toggl_entry_id: None,
            last_error: Some("Toggl failed with status 503".to_owned()),
        };
        assert_eq!(
            QueueItem::decode("ses-1", &item.encode().unwrap()),
            Some(item)
        );
        assert_eq!(QueueItem::decode("x", r#"{"v":9}"#), None);
    }
}
