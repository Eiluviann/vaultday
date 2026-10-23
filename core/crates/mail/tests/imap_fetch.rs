//! The IMAP source against an in-process fake server, and what its fetches
//! do to the encrypted signal store.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use app_state::{
    ConnectionId, InMemorySecretStore, MailAccount, MailAuth, MailProvider, SecretStore,
    SignalAction, SignalState, sync_folder,
};
use jiff::Timestamp;
use mail::credentials::store_password;
use mail::error::MailError;
use mail::fetch::cursors;
use mail::oauth::GRAPH_CLIENT_ID_PLACEHOLDER;
use mail::{
    FetchLimits, FolderRole, MailEnvironment, MailSource, fetch_account, ingest, open_source,
};
use support::{
    Canned, FakeImap, ImapState, MockHttp, create_workspace, instant_retries, mailbox,
    reopen_workspace, synthetic_message, workspace_id,
};

fn now() -> Timestamp {
    "2026-10-23T09:00:00Z".parse().unwrap()
}

fn account(auth: MailAuth) -> MailAccount {
    MailAccount {
        connection: ConnectionId::new("con-mail-1"),
        label: "someone@example.test".to_owned(),
        workspace: workspace_id(),
        address: "someone@example.test".to_owned(),
        provider: MailProvider::Imap,
        host: "imap.example.test".to_owned(),
        port: 993,
        auth,
    }
}

fn environment(server: &FakeImap) -> MailEnvironment {
    MailEnvironment {
        imap: server.connector(),
        ..MailEnvironment::production(GRAPH_CLIENT_ID_PLACEHOLDER)
    }
}

/// A source signed in with the stored app password.
fn source(server: &FakeImap, secrets: &InMemorySecretStore) -> Box<dyn MailSource> {
    let store: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    open_source(&account(MailAuth::AppPassword), store, &environment(server)).unwrap()
}

fn secrets_with_password(password: &str) -> InMemorySecretStore {
    let secrets = InMemorySecretStore::new();
    store_password(&secrets, &ConnectionId::new("con-mail-1"), password).unwrap();
    secrets
}

fn folders(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// Fetches `names` from the cursors the document holds and records the result.
fn fetch_into(
    sync: &mut app_state::WorkspaceSync,
    source: &mut dyn MailSource,
    names: &[&str],
) -> mail::IngestSummary {
    let folders = folders(names);
    let account = ConnectionId::new("con-mail-1");
    let held = cursors(sync.state(), &account, &folders);
    let fetched = fetch_account(
        source,
        &account,
        &folders,
        &held,
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    )
    .unwrap();
    sync.edit(Instant::now(), |state| ingest(state, &fetched))
        .unwrap()
}

#[test]
fn the_first_fetch_records_the_inbox_as_signals() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);

    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);

    assert_eq!(summary.added.len(), 2);
    assert_eq!(summary.duplicates, 0);
    let records = sync.state().signals();
    let reply = records
        .iter()
        .find(|record| record.signal.message_id == "<synthetic-2@mail.example.test>")
        .unwrap();
    assert_eq!(reply.state, SignalState::New);
    assert_eq!(reply.signal.subject, "Re: Café plan");
    assert_eq!(
        reply.signal.sender_name.as_deref(),
        Some("Synthetic Sender")
    );
    assert_eq!(reply.signal.sender_address, "sender@example.test");
    assert_eq!(
        reply.signal.thread_id, "<synthetic-1@mail.example.test>",
        "a reply joins the thread its References start"
    );
    assert_eq!(
        reply.signal.excerpt,
        "Café is booked for the synthetic review."
    );
    assert_eq!(
        reply.signal.link,
        "message:%3Csynthetic-2@mail.example.test%3E"
    );
    assert_eq!(reply.signal.folder, "INBOX");
    assert_eq!(reply.signal.provider_ref, "7:2");
    assert_eq!(
        reply.signal.received_at,
        "2026-10-22T07:00:00Z".parse::<Timestamp>().unwrap()
    );
    let first = records
        .iter()
        .find(|record| record.signal.message_id == "<synthetic-1@mail.example.test>")
        .unwrap();
    assert_eq!(first.signal.excerpt, "Hello, This is synthetic message 1.");
    assert!(
        sync.state()
            .signal_raw_text(&first.signal.id)
            .unwrap()
            .contains("> quoted"),
        "the raw text keeps what the excerpt leaves out"
    );
    assert!(
        server
            .log()
            .iter()
            .all(|command| !command.starts_with("SELECT")),
        "folders are only examined, so nothing is marked read"
    );
    assert_eq!(
        sync.state()
            .mail_cursor(&ConnectionId::new("con-mail-1"), "INBOX")
            .as_deref(),
        Some("7:2")
    );
}

#[test]
fn an_incremental_fetch_starts_from_the_cursor() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);
    fetch_into(&mut sync, source.as_mut(), &["INBOX"]);

    let nothing_new = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    assert_eq!(nothing_new.added.len(), 0);
    assert_eq!(
        nothing_new.duplicates, 0,
        "the highest UID that `3:*` always returns is not fetched again"
    );

    server.add_message("INBOX", synthetic_message(3, "Synthetic follow-up"));
    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    assert_eq!(summary.added.len(), 1);
    assert_eq!(sync.state().signals().len(), 3);
    assert!(server.log().contains(&"UID SEARCH UID 3:*".to_owned()));
    let fetched_sets: Vec<String> = server
        .log()
        .iter()
        .filter(|command| command.contains("ENVELOPE"))
        .cloned()
        .collect();
    assert_eq!(fetched_sets.len(), 2, "{fetched_sets:?}");
    assert!(fetched_sets[1].starts_with("UID FETCH 3 "));
}

#[test]
fn two_fetches_of_one_mailbox_produce_no_duplicates() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);
    fetch_into(&mut sync, source.as_mut(), &["INBOX"]);

    // A fetch that has lost its cursor (another device, or a renumbered
    // folder) brings every message again; the store keeps one of each.
    let account = ConnectionId::new("con-mail-1");
    let again = fetch_account(
        source.as_mut(),
        &account,
        &folders(&["INBOX"]),
        &BTreeMap::new(),
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    )
    .unwrap();
    assert_eq!(again.folders[0].messages.len(), 2);
    let summary = sync
        .edit(Instant::now(), |state| ingest(state, &again))
        .unwrap();
    assert_eq!(summary.added.len(), 0);
    assert_eq!(summary.duplicates, 2);
    assert_eq!(sync.state().signals().len(), 2);
}

#[test]
fn the_per_folder_limit_defers_messages_and_never_skips_them() {
    let server = FakeImap::start(mailbox());
    for uid in 3..=5 {
        server.add_message("INBOX", synthetic_message(uid, "Synthetic backlog"));
    }
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let store: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    let mut environment = environment(&server);
    environment.limits.max_messages = 2;
    let mut source = open_source(&account(MailAuth::AppPassword), store, &environment).unwrap();

    let added: Vec<usize> = (0..4)
        .map(|_| {
            fetch_into(&mut sync, source.as_mut(), &["INBOX"])
                .added
                .len()
        })
        .collect();

    assert_eq!(
        added,
        [2, 2, 1, 0],
        "two per fetch, oldest first, none lost"
    );
    assert_eq!(sync.state().signals().len(), 5);
    assert_eq!(
        sync.state()
            .mail_cursor(&ConnectionId::new("con-mail-1"), "INBOX")
            .as_deref(),
        Some("7:5")
    );
}

#[test]
fn one_message_in_two_folders_is_one_signal() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);

    // `Archive` holds a copy of the inbox's first message (same Message-ID).
    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX", "Archive"]);

    assert_eq!((summary.added.len(), summary.duplicates), (2, 1));
    assert_eq!(sync.state().signals().len(), 2);
}

#[test]
fn a_renumbered_folder_is_fetched_afresh() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);
    fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    server.state.lock().unwrap().folders[0].uid_validity = 8;

    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    assert_eq!((summary.added.len(), summary.duplicates), (0, 2));
    assert_eq!(
        sync.state()
            .mail_cursor(&ConnectionId::new("con-mail-1"), "INBOX")
            .as_deref(),
        Some("8:2")
    );
}

#[test]
fn only_the_selected_folders_are_fetched() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);

    let listed = source.list_folders().unwrap();
    let roles: Vec<(&str, FolderRole)> = listed
        .iter()
        .map(|folder| (folder.id.as_str(), folder.role))
        .collect();
    assert_eq!(
        roles,
        [
            ("INBOX", FolderRole::Inbox),
            ("Sent", FolderRole::Sent),
            ("Archive", FolderRole::Other)
        ]
    );

    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX", "Sent"]);
    assert_eq!(summary.added.len(), 3);
    let subjects: Vec<String> = sync
        .state()
        .signals()
        .into_iter()
        .map(|record| record.signal.subject)
        .collect();
    assert!(subjects.contains(&"Waiting on the synthetic answer".to_owned()));
    assert!(!subjects.contains(&"Archived synthetic note".to_owned()));
    assert!(
        !server
            .log()
            .iter()
            .any(|command| command.contains("Archive") && command.starts_with("EXAMINE"))
    );
    let missing = fetch_account(
        source.as_mut(),
        &ConnectionId::new("con-mail-1"),
        &folders(&["Gone"]),
        &BTreeMap::new(),
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    );
    assert!(matches!(missing, Err(MailError::FolderNotFound(_))));
}

#[test]
fn a_refused_password_is_an_auth_failure_and_is_not_retried() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("not-the-password");
    let mut source = source(&server, &secrets);
    let outcome = fetch_account(
        source.as_mut(),
        &ConnectionId::new("con-mail-1"),
        &folders(&["INBOX"]),
        &BTreeMap::new(),
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    );
    assert!(matches!(outcome, Err(MailError::Auth(_))), "{outcome:?}");
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
}

#[test]
fn a_missing_password_is_reported_before_any_connection() {
    let server = FakeImap::start(mailbox());
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let outcome = open_source(
        &account(MailAuth::AppPassword),
        store,
        &environment(&server),
    );
    assert!(matches!(outcome, Err(MailError::MissingCredential(_))));
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}

#[test]
fn a_dropped_connection_is_retried() {
    let server = FakeImap::start(ImapState {
        drops_left: 1,
        ..mailbox()
    });
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);

    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    assert_eq!(summary.added.len(), 2);
    assert_eq!(server.connections.load(Ordering::SeqCst), 2);

    server.state.lock().unwrap().drops_left = 5;
    let outcome = fetch_account(
        source.as_mut(),
        &ConnectionId::new("con-mail-1"),
        &folders(&["INBOX"]),
        &BTreeMap::new(),
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    );
    assert!(
        matches!(&outcome, Err(error) if error.is_transient()),
        "three attempts, then the transient error: {outcome:?}"
    );
}

#[test]
fn xoauth2_signs_in_with_a_refreshed_token() {
    let server = FakeImap::start(ImapState {
        xoauth2_token: Some("synthetic-access-token".to_owned()),
        ..mailbox()
    });
    let oauth = MockHttp::start(|request| {
        assert!(request.body.contains("grant_type=refresh_token"));
        assert!(request.body.contains("IMAP.AccessAsUser.All"));
        Canned::json(
            200,
            r#"{"access_token":"synthetic-access-token","refresh_token":"synthetic-refresh-2","token_type":"Bearer"}"#,
        )
    });
    let secrets = InMemorySecretStore::new();
    let connection = ConnectionId::new("con-mail-1");
    mail::credentials::store_refresh_token(&secrets, &connection, "synthetic-refresh-1").unwrap();
    let mut environment = environment(&server);
    environment.oauth.authority = oauth.base.clone();
    environment.oauth.client_id = "11111111-2222-3333-4444-555555555555".to_owned();
    let store: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    let mut source = open_source(&account(MailAuth::XOAuth2), store, &environment).unwrap();

    let folders = source.list_folders().unwrap();
    assert_eq!(folders.len(), 3);
    let rotated = secrets
        .get(&app_state::SecretId::new(
            app_state::SecretKind::MailRefreshToken,
            "con-mail-1",
        ))
        .unwrap()
        .unwrap();
    assert_eq!(rotated.value.expose(), b"synthetic-refresh-2");
}

#[test]
fn dismissed_signals_stay_dismissed_across_refetch_and_relaunch() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);
    let summary = fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    let dismissed = summary.added[0].clone();
    sync.edit(Instant::now(), |state| {
        state.apply_signal_action(&dismissed, &SignalAction::Dismiss, now())
    })
    .unwrap();
    sync.flush().unwrap();
    drop(sync);

    // Relaunch: the same device opens its document from the change files.
    let mut relaunched = reopen_workspace(root.path(), &secrets, "mac-a");
    assert_eq!(
        relaunched.state().signal_state(&dismissed),
        SignalState::Dismissed
    );
    // A refetch from scratch brings the message again; it stays dismissed.
    let account = ConnectionId::new("con-mail-1");
    let again = fetch_account(
        source.as_mut(),
        &account,
        &folders(&["INBOX"]),
        &BTreeMap::new(),
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    )
    .unwrap();
    relaunched
        .edit(Instant::now(), |state| ingest(state, &again))
        .unwrap();
    assert_eq!(
        relaunched.state().signal_state(&dismissed),
        SignalState::Dismissed
    );
    relaunched.flush().unwrap();

    // Another device of the same user sees the dismissal through the change files.
    let other = reopen_workspace(root.path(), &secrets, "mac-b");
    assert_eq!(
        other.state().signal_state(&dismissed),
        SignalState::Dismissed
    );
    assert_eq!(other.state().signals().len(), 2);
}

#[test]
fn change_files_holding_signals_are_unreadable_without_the_key() {
    let server = FakeImap::start(mailbox());
    let secrets = secrets_with_password("synthetic-app-password");
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);
    fetch_into(&mut sync, source.as_mut(), &["INBOX"]);
    sync.flush().unwrap();
    assert_eq!(sync.state().signals().len(), 2);

    let mut written = Vec::new();
    collect_files(&sync_folder(root.path()), &mut written);
    assert!(!written.is_empty());
    for marker in [
        "Synthetic plan",
        "synthetic message 1",
        "sender@example.test",
        "synthetic-1@mail.example.test",
    ] {
        for bytes in &written {
            assert!(
                !contains(bytes, marker.as_bytes()),
                "{marker:?} is readable in a change file"
            );
        }
    }

    // A device holding another key applies none of the files.
    let stranger_secrets = InMemorySecretStore::new();
    app_state::create_workspace_key(&stranger_secrets, &workspace_id()).unwrap();
    let (stranger, outcome) = app_state::WorkspaceSync::open(
        Arc::new(app_state::PlainSyncFiles),
        root.path(),
        app_state::DeviceId::new("mac-z").unwrap(),
        support::key(&stranger_secrets),
        app_state::SyncConfig::default(),
    )
    .unwrap();
    assert!(stranger.state().signals().is_empty());
    assert!(
        !outcome.pending.is_empty(),
        "every file is reported undecryptable"
    );
}

fn collect_files(dir: &Path, into: &mut Vec<Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, into);
        } else {
            into.push(fs::read(path).unwrap());
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
