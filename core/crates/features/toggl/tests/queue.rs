//! The offline queue against the mock server: idempotent by session id,
//! bounded retries, and never a duplicate entry after a retry.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, Instant};

use app_state::WorkspaceSync;
use domain::session::{SessionId, SessionRecord, SessionSource, SessionTarget};
use jiff::Timestamp;
use support::{
    Fault, MockToggl, create_workspace, feature, mapping, reopen_workspace, secrets_with_token,
};
use vaultday_feature_toggl::queue::MAX_ATTEMPTS;
use vaultday_feature_toggl::{
    BillingPolicy, EnqueueOutcome, EntryDetails, NewTimeEntry, QueueState, TimeEntry, TogglApi,
    TogglError, TogglMapping, TogglMe, TogglProject, TogglWorkspace,
};

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn session(id: &str, start: &str, stop: Option<&str>) -> SessionRecord {
    SessionRecord {
        id: SessionId::new(id),
        target: SessionTarget::task_in_story("task-1", "WEB-1"),
        source: SessionSource::Timer,
        started_at: at(start),
        stopped_at: stop.map(at),
        pauses: Vec::new(),
        has_long_timer_fired: false,
    }
}

fn details() -> EntryDetails {
    EntryDetails {
        description: "Write the brief".to_owned(),
        story: Some("WEB-1".to_owned()),
        task: Some("task-1".to_owned()),
        tags: vec!["vaultday".to_owned()],
    }
}

const HOURLY: BillingPolicy = BillingPolicy::Hourly {
    rounding_minutes: 15,
    is_billable: true,
};

fn enqueue(
    sync: &mut WorkspaceSync,
    record: &SessionRecord,
    policy: &BillingPolicy,
) -> EnqueueOutcome {
    let feature = feature();
    sync.edit(Instant::now(), |state| {
        feature
            .enqueue_session(state, record, policy, &details())
            .map_err(|error| app_state::AppStateError::InvalidSetting(error.to_string()))
    })
    .unwrap()
}

#[test]
fn a_session_is_queued_once_and_sent_once() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let record = session(
        "ses-1",
        "2026-10-19T09:00:00Z",
        Some("2026-10-19T09:20:00Z"),
    );
    assert_eq!(
        enqueue(&mut sync, &record, &HOURLY),
        EnqueueOutcome::Queued(1)
    );
    assert_eq!(
        enqueue(&mut sync, &record, &HOURLY),
        EnqueueOutcome::AlreadyQueued
    );
    let items = feature().queue(sync.state());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].duration_secs, 30 * 60, "rounded up to 15 minutes");
    assert_eq!(items[0].state, QueueState::Queued);

    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let report = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T09:21:00Z"))
        .unwrap();
    assert_eq!(report.sent, 1);
    let entries = mock.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["project_id"], 501, "the story's project");
    assert_eq!(entries[0]["description"], "Write the brief");
    assert_eq!(entries[0]["tags"], serde_json::json!(["vaultday"]));

    // A second drain and a second stop send nothing more.
    let again = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T10:00:00Z"))
        .unwrap();
    assert_eq!(again.sent, 0);
    assert_eq!(
        enqueue(&mut sync, &record, &HOURLY),
        EnqueueOutcome::AlreadyQueued
    );
    assert_eq!(mock.entries().len(), 1);
    let sent = &feature().queue(sync.state())[0];
    assert_eq!(sent.state, QueueState::Sent);
    assert!(sent.toggl_entry_id.is_some());
}

#[test]
fn a_5xx_then_200_sends_exactly_one_entry() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let record = session(
        "ses-2",
        "2026-10-19T11:00:00Z",
        Some("2026-10-19T12:00:00Z"),
    );
    enqueue(&mut sync, &record, &HOURLY);
    let mock = MockToggl::start();
    let api = mock.client(&secrets);

    mock.fail_next("POST", Fault::Status(503));
    let first = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T12:00:00Z"))
        .unwrap();
    assert_eq!((first.sent, first.retrying), (0, 1));
    let waiting = &feature().queue(sync.state())[0];
    assert_eq!((waiting.attempts, waiting.may_exist), (1, true));
    let next_attempt = waiting.next_attempt_at.unwrap();
    assert_eq!(next_attempt, at("2026-10-19T12:01:00Z"));

    // Not due yet: nothing is sent.
    let early = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T12:00:30Z"))
        .unwrap();
    assert_eq!(early.waiting, 1);
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 1);

    let second = feature()
        .drain_queue(&mut sync, &api, &mapping(), next_attempt)
        .unwrap();
    assert_eq!(second.sent, 1);
    assert_eq!(
        mock.requests_to("GET", "/api/v9/me/time_entries"),
        1,
        "looked first"
    );
    assert_eq!(mock.entries().len(), 1);
}

#[test]
fn an_entry_created_despite_a_5xx_is_adopted_not_posted_again() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    enqueue(
        &mut sync,
        &session(
            "ses-3",
            "2026-10-19T13:00:00Z",
            Some("2026-10-19T13:45:00Z"),
        ),
        &HOURLY,
    );
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    mock.fail_next("POST", Fault::ServerErrorAfterCommit);
    feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T14:00:00Z"))
        .unwrap();
    assert_eq!(
        mock.entries().len(),
        1,
        "Toggl holds it although it said 500"
    );
    let report = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T15:00:00Z"))
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 1);
    assert_eq!(mock.entries().len(), 1);
    let item = &feature().queue(sync.state())[0];
    assert_eq!(
        item.toggl_entry_id,
        mock.entries()[0]["id"].as_u64(),
        "the queue adopted Toggl's entry"
    );
}

/// Delegates to the mock client, then dies once Toggl has answered a post:
/// the app crashed before the answer reached the workspace document.
struct CrashAfterPost<'a>(&'a dyn TogglApi);

impl TogglApi for CrashAfterPost<'_> {
    fn me(&self) -> Result<TogglMe, TogglError> {
        self.0.me()
    }
    fn workspaces(&self) -> Result<Vec<TogglWorkspace>, TogglError> {
        self.0.workspaces()
    }
    fn projects(&self, workspace_id: u64) -> Result<Vec<TogglProject>, TogglError> {
        self.0.projects(workspace_id)
    }
    fn create_time_entry(&self, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError> {
        let created = self.0.create_time_entry(entry);
        panic!(
            "crashed after the post: {:?}",
            created.map(|entry| entry.id)
        );
    }
    fn update_time_entry(&self, id: u64, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError> {
        self.0.update_time_entry(id, entry)
    }
    fn delete_time_entry(&self, workspace_id: u64, id: u64) -> Result<(), TogglError> {
        self.0.delete_time_entry(workspace_id, id)
    }
    fn time_entries(&self, start: Timestamp, end: Timestamp) -> Result<Vec<TimeEntry>, TogglError> {
        self.0.time_entries(start, end)
    }
}

#[test]
fn a_crash_between_the_post_and_its_record_never_posts_again() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    enqueue(
        &mut sync,
        &session(
            "ses-crash",
            "2026-10-19T16:00:00Z",
            Some("2026-10-19T16:30:00Z"),
        ),
        &HOURLY,
    );
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        feature().drain_queue(
            &mut sync,
            &CrashAfterPost(&api),
            &mapping(),
            at("2026-10-19T16:31:00Z"),
        )
    }));
    assert!(crashed.is_err());
    assert_eq!(mock.entries().len(), 1, "Toggl holds the entry");
    // The process died: nothing unflushed survives.
    std::mem::forget(sync);

    let mut sync = reopen_workspace(dir.path(), &secrets, "mac-a");
    let item = &feature().queue(sync.state())[0];
    assert_eq!(item.state, QueueState::Queued);
    assert!(item.may_exist, "marked on disk before the post");
    let report = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T16:40:00Z"))
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 1);
    assert_eq!(mock.entries().len(), 1);
    assert_eq!(
        feature().queue(sync.state())[0].toggl_entry_id,
        mock.entries()[0]["id"].as_u64()
    );
}

#[test]
fn the_lookup_survives_an_edited_description_and_a_new_project_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    enqueue(
        &mut sync,
        &session(
            "ses-edit",
            "2026-10-19T17:00:00Z",
            Some("2026-10-19T17:15:00Z"),
        ),
        &HOURLY,
    );
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    mock.fail_next("POST", Fault::ServerErrorAfterCommit);
    feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T17:16:00Z"))
        .unwrap();
    let created = mock.entries()[0].clone();
    let id = created["id"].as_u64().unwrap();

    // Before the retry, the user renames the entry in Toggl and the story
    // is mapped to another project.
    let item = &feature().queue(sync.state())[0];
    let mut renamed = item.to_new_entry(&mapping()).unwrap();
    renamed.description = "Brief, renamed in Toggl".to_owned();
    api.update_time_entry(id, &renamed).unwrap();
    let mut remapped = mapping();
    remapped.projects_by_story.insert("WEB-1".to_owned(), 777);

    let report = feature()
        .drain_queue(&mut sync, &api, &remapped, at("2026-10-19T18:00:00Z"))
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 1);
    assert_eq!(mock.entries().len(), 1);
    assert_eq!(feature().queue(sync.state())[0].toggl_entry_id, Some(id));
}

#[test]
fn retries_are_bounded_and_a_failed_entry_waits_for_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    enqueue(
        &mut sync,
        &session(
            "ses-4",
            "2026-10-19T09:00:00Z",
            Some("2026-10-19T10:00:00Z"),
        ),
        &HOURLY,
    );
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let mut now = at("2026-10-20T00:00:00Z");
    for _ in 0..MAX_ATTEMPTS {
        mock.fail_next("POST", Fault::Status(500));
        feature()
            .drain_queue(&mut sync, &api, &mapping(), now)
            .unwrap();
        now = now
            .checked_add(jiff::SignedDuration::from_hours(2))
            .unwrap();
    }
    let item = feature().queue(sync.state()).remove(0);
    assert_eq!(item.state, QueueState::Failed);
    assert_eq!(item.attempts, MAX_ATTEMPTS);
    let posts = mock.requests_to("POST", "/api/v9/workspaces");
    feature()
        .drain_queue(&mut sync, &api, &mapping(), now)
        .unwrap();
    assert_eq!(
        mock.requests_to("POST", "/api/v9/workspaces"),
        posts,
        "failed stays put"
    );

    let retried = sync
        .edit(Instant::now(), |state| {
            feature()
                .retry_failed(state, "ses-4")
                .map_err(|error| app_state::AppStateError::InvalidSetting(error.to_string()))
        })
        .unwrap();
    assert!(retried);
    let report = feature()
        .drain_queue(&mut sync, &api, &mapping(), now)
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(mock.entries().len(), 1);
}

#[test]
fn a_refused_token_stops_the_drain_and_costs_no_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = app_state::InMemorySecretStore::new();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    for id in ["ses-5", "ses-6"] {
        enqueue(
            &mut sync,
            &session(id, "2026-10-19T09:00:00Z", Some("2026-10-19T09:30:00Z")),
            &HOURLY,
        );
    }
    use app_state::SecretStore;
    secrets
        .put(app_state::SecretItem::new(
            app_state::SecretKind::TogglApiToken,
            support::WORKSPACE,
            app_state::SecretBytes::new(b"revoked".to_vec()),
        ))
        .unwrap();
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let report = feature()
        .drain_queue(&mut sync, &api, &mapping(), at("2026-10-19T10:00:00Z"))
        .unwrap();
    assert!(report.is_unauthorized);
    assert_eq!((report.sent, report.waiting), (0, 2));
    assert_eq!(mock.requests().len(), 1, "stopped after the first refusal");
    assert!(
        feature()
            .queue(sync.state())
            .iter()
            .all(|item| item.attempts == 0)
    );
}

#[test]
fn nothing_is_sent_without_a_mapped_workspace_or_from_another_device() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    enqueue(
        &mut sync,
        &session(
            "ses-7",
            "2026-10-19T09:00:00Z",
            Some("2026-10-19T09:30:00Z"),
        ),
        &HOURLY,
    );
    let mock = MockToggl::start();
    let api = mock.client(&secrets);
    let report = feature()
        .drain_queue(
            &mut sync,
            &api,
            &TogglMapping::default(),
            at("2026-10-19T10:00:00Z"),
        )
        .unwrap();
    assert!(report.is_unmapped);
    assert!(mock.requests().is_empty());

    sync.flush().unwrap();
    let (mut other, _) = WorkspaceSync::open(
        std::sync::Arc::new(app_state::PlainSyncFiles),
        dir.path(),
        app_state::DeviceId::new("mac-b").unwrap(),
        support::key(&secrets),
        app_state::SyncConfig::default(),
    )
    .unwrap();
    assert_eq!(feature().queue(other.state()).len(), 1, "the record synced");
    let report = feature()
        .drain_queue(&mut other, &api, &mapping(), at("2026-10-19T10:00:00Z"))
        .unwrap();
    assert_eq!(report.sent, 0);
    assert!(mock.requests().is_empty(), "only the queueing device sends");
}

#[test]
fn fixed_price_splits_the_overrun_and_fte_and_running_sessions_send_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_with_token();
    let mut sync = create_workspace(dir.path(), &secrets, "mac-a");
    let fixed = BillingPolicy::Fixed {
        estimate: Some(Duration::from_secs(60 * 60)),
    };
    assert_eq!(
        enqueue(
            &mut sync,
            &session(
                "ses-8",
                "2026-10-19T09:00:00Z",
                Some("2026-10-19T09:40:00Z")
            ),
            &fixed
        ),
        EnqueueOutcome::Queued(1)
    );
    assert_eq!(
        enqueue(
            &mut sync,
            &session(
                "ses-9",
                "2026-10-19T10:00:00Z",
                Some("2026-10-19T10:40:00Z")
            ),
            &fixed
        ),
        EnqueueOutcome::Queued(2)
    );
    let items = feature().queue(sync.state());
    let shape: Vec<(&str, u64, bool)> = items
        .iter()
        .map(|item| (item.key.as_str(), item.duration_secs, item.is_billable))
        .collect();
    assert_eq!(
        shape,
        [
            ("ses-8", 2400, true),
            ("ses-9", 1200, true),
            ("ses-9.overrun", 1200, false),
        ]
    );
    assert_eq!(items[2].start, at("2026-10-19T10:20:00Z"));
    assert_eq!(items[1].details.tags, ["vaultday"]);
    assert_eq!(
        items[2].details.tags,
        ["vaultday", "overrun"],
        "the unpaid overrun is tagged as one"
    );

    assert_eq!(
        enqueue(
            &mut sync,
            &session(
                "ses-10",
                "2026-10-19T11:00:00Z",
                Some("2026-10-19T12:00:00Z")
            ),
            &BillingPolicy::Fte
        ),
        EnqueueOutcome::NotSent
    );
    assert_eq!(
        enqueue(
            &mut sync,
            &session("ses-11", "2026-10-19T13:00:00Z", None),
            &HOURLY
        ),
        EnqueueOutcome::StillRunning
    );
    assert_eq!(feature().queue(sync.state()).len(), 3);
}
