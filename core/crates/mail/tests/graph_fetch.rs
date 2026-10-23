//! The Microsoft Graph source and the device-code flow against an
//! in-process mock of Graph and the Microsoft authority. Nothing is run live.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_state::{
    ConnectionId, InMemorySecretStore, MailAccount, MailAuth, MailProvider, SecretId, SecretKind,
    SecretStore,
};
use jiff::Timestamp;
use mail::credentials::{has_credential, store_refresh_token};
use mail::error::MailError;
use mail::http::UreqTransport;
use mail::oauth::{DeviceCodeFlow, DevicePoll, GRAPH_CLIENT_ID_PLACEHOLDER};
use mail::{
    FetchLimits, FolderRole, MailEnvironment, MailSource, fetch_account, ingest, open_source,
};
use support::{Canned, MockHttp, Received, create_workspace, instant_retries, workspace_id};

const CLIENT_ID: &str = "11111111-2222-3333-4444-555555555555";

/// What the mock authority and Graph do.
#[derive(Default)]
struct Script {
    polls: AtomicUsize,
    deltas: AtomicUsize,
    throttle_first_delta: bool,
    refuse_refresh: bool,
    refuse_every_token: bool,
    base: Mutex<String>,
}

fn now() -> Timestamp {
    "2026-10-23T09:00:00Z".parse().unwrap()
}

fn message(id: &str, received: &str) -> String {
    format!(
        r#"{{"id":"{id}","internetMessageId":"<{id}@graph.example.test>","conversationId":"conv-{id}","subject":"Synthetic {id}","from":{{"emailAddress":{{"name":"Synthetic Colleague","address":"colleague@example.test"}}}},"receivedDateTime":"{received}","bodyPreview":"Preview of synthetic {id}","webLink":"https://outlook.office365.com/owa/?ItemID={id}&exvsurl=1"}}"#
    )
}

fn respond(script: &Script, request: &Received) -> Canned {
    let base = script.base.lock().unwrap().clone();
    let path = request.path.as_str();
    if path == "/oauth/devicecode" {
        return Canned::json(
            200,
            r#"{"device_code":"synthetic-device-code","user_code":"SYNTH-123","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5,"message":"synthetic"}"#,
        );
    }
    if path == "/oauth/token" {
        if request.body.contains("device_code") {
            let poll = script.polls.fetch_add(1, Ordering::SeqCst);
            return if poll == 0 {
                Canned::json(400, r#"{"error":"authorization_pending"}"#)
            } else {
                Canned::json(
                    200,
                    r#"{"access_token":"synthetic-access","refresh_token":"synthetic-refresh-1","expires_in":3600}"#,
                )
            };
        }
        if script.refuse_refresh {
            return Canned::json(
                400,
                r#"{"error":"invalid_grant","error_description":"expired"}"#,
            );
        }
        return Canned::json(
            200,
            r#"{"access_token":"synthetic-access","refresh_token":"synthetic-refresh-2","expires_in":3600}"#,
        );
    }
    let is_authorised =
        request.headers.get("authorization").map(String::as_str) == Some("Bearer synthetic-access");
    if !is_authorised || script.refuse_every_token {
        return Canned::json(401, r#"{"error":{"code":"InvalidAuthenticationToken"}}"#);
    }
    if path.starts_with("/me/mailFolders/inbox?") {
        return Canned::json(200, r#"{"displayName":"Inbox"}"#);
    }
    if path.starts_with("/me/mailFolders/sentitems?") {
        return Canned::json(200, r#"{"displayName":"Sent Items"}"#);
    }
    if path.starts_with("/me/mailFolders/inbox/messages/delta") {
        let call = script.deltas.fetch_add(1, Ordering::SeqCst);
        if script.throttle_first_delta && call == 0 {
            let mut canned = Canned::json(429, r#"{"error":{"code":"TooManyRequests"}}"#);
            canned
                .headers
                .push(("Retry-After".to_owned(), "1".to_owned()));
            return canned;
        }
        if path.contains("deltatoken=d1") {
            return Canned::json(
                200,
                &format!(
                    r#"{{"value":[{},{{"id":"m1","@removed":{{"reason":"changed"}}}}],"@odata.deltaLink":"{base}/me/mailFolders/inbox/messages/delta?$deltatoken=d2"}}"#,
                    message("m3", "2026-10-23T08:30:00Z")
                ),
            );
        }
        if path.contains("skiptoken=p2") {
            return Canned::json(
                200,
                &format!(
                    r#"{{"value":[{}],"@odata.deltaLink":"{base}/me/mailFolders/inbox/messages/delta?$deltatoken=d1"}}"#,
                    message("m2", "2026-10-22T10:00:00Z")
                ),
            );
        }
        return Canned::json(
            200,
            &format!(
                r#"{{"value":[{}],"@odata.nextLink":"{base}/me/mailFolders/inbox/messages/delta?$skiptoken=p2"}}"#,
                message("m1", "2026-10-22T09:00:00Z")
            ),
        );
    }
    if let Some(rest) = path.strip_prefix("/me/messages/") {
        let id = rest.split('?').next().unwrap_or_default();
        assert_eq!(
            request.headers.get("prefer").map(String::as_str),
            Some("outlook.body-content-type=\"text\"")
        );
        return Canned::json(
            200,
            &format!(
                r#"{{"body":{{"contentType":"text","content":"The whole synthetic body of {id}."}}}}"#
            ),
        );
    }
    Canned::json(404, r#"{"error":{"code":"ResourceNotFound"}}"#)
}

fn mock(script: Script) -> (MockHttp, Arc<Script>) {
    let script = Arc::new(script);
    let handler = script.clone();
    let server = MockHttp::start(move |request| respond(&handler, request));
    *script.base.lock().unwrap() = server.base.clone();
    (server, script)
}

fn environment(server: &MockHttp) -> MailEnvironment {
    let mut environment = MailEnvironment::production(CLIENT_ID);
    environment.graph_base = server.base.clone();
    environment.oauth.authority = format!("{}/oauth", server.base);
    environment
}

fn account() -> MailAccount {
    MailAccount {
        connection: ConnectionId::new("con-m365"),
        label: "someone@contoso.example.test".to_owned(),
        workspace: workspace_id(),
        address: "someone@contoso.example.test".to_owned(),
        provider: MailProvider::MicrosoftGraph,
        host: String::new(),
        port: 0,
        auth: MailAuth::DeviceCode,
    }
}

fn signed_in() -> InMemorySecretStore {
    let secrets = InMemorySecretStore::new();
    store_refresh_token(
        &secrets,
        &ConnectionId::new("con-m365"),
        "synthetic-refresh-1",
    )
    .unwrap();
    secrets
}

fn source(server: &MockHttp, secrets: &InMemorySecretStore) -> Box<dyn MailSource> {
    let store: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    open_source(&account(), store, &environment(server)).unwrap()
}

fn fetch(
    source: &mut dyn MailSource,
    cursors: &BTreeMap<String, String>,
) -> Result<mail::AccountFetch, MailError> {
    fetch_account(
        source,
        &ConnectionId::new("con-m365"),
        &["inbox".to_owned()],
        cursors,
        &FetchLimits::default(),
        &instant_retries(),
        now(),
    )
}

#[test]
fn the_device_code_flow_signs_in_against_the_mock_authority() {
    let (server, _) = mock(Script::default());
    let flow = environment(&server).oauth;
    let http = UreqTransport::new();
    let prompt = flow.start(&http).unwrap();
    assert_eq!(prompt.user_code, "SYNTH-123");
    assert_eq!(prompt.verification_uri, "https://microsoft.com/devicelogin");
    assert_eq!(prompt.interval, Duration::from_secs(5));
    assert!(
        !format!("{prompt:?}").contains("synthetic-device-code"),
        "the device code never reaches Debug output"
    );
    assert_eq!(
        flow.poll(&http, &prompt.device_code).unwrap(),
        DevicePoll::Pending
    );
    let DevicePoll::Granted(tokens) = flow.poll(&http, &prompt.device_code).unwrap() else {
        panic!("the second poll is granted");
    };
    assert_eq!(tokens.refresh_token.as_deref(), Some("synthetic-refresh-1"));
    assert!(
        !format!("{tokens:?}").contains("synthetic"),
        "tokens never print"
    );
    let requests = server.requests();
    assert!(requests[0].body.contains(&format!("client_id={CLIENT_ID}")));
    assert!(requests[0].body.contains("Mail.Read"));
    assert!(
        requests[1]
            .body
            .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
    );
}

#[test]
fn the_placeholder_client_id_never_reaches_the_network() {
    let (server, _) = mock(Script::default());
    let flow = DeviceCodeFlow {
        authority: format!("{}/oauth", server.base),
        ..DeviceCodeFlow::microsoft(GRAPH_CLIENT_ID_PLACEHOLDER)
    };
    assert!(!flow.is_configured());
    assert!(matches!(
        flow.start(&UreqTransport::new()),
        Err(MailError::NotConfigured(_))
    ));
    assert!(server.requests().is_empty());
}

#[test]
fn first_and_incremental_fetches_follow_the_delta_links() {
    let (server, _) = mock(Script::default());
    let secrets = signed_in();
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let mut source = source(&server, &secrets);

    let folders = source.list_folders().unwrap();
    let roles: Vec<(&str, FolderRole)> = folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder.role))
        .collect();
    assert_eq!(
        roles,
        [
            ("inbox", FolderRole::Inbox),
            ("sentitems", FolderRole::Sent)
        ]
    );

    let first = fetch(source.as_mut(), &BTreeMap::new()).unwrap();
    let page = &first.folders[0];
    assert_eq!(page.messages.len(), 2);
    assert!(page.cursor.ends_with("deltatoken=d1"));
    let m1 = &page.messages[0];
    assert_eq!(m1.message_id, "<m1@graph.example.test>");
    assert_eq!(m1.thread_id, "conv-m1");
    assert_eq!(m1.sender_name.as_deref(), Some("Synthetic Colleague"));
    assert_eq!(m1.excerpt, "Preview of synthetic m1");
    assert_eq!(m1.raw_text, "The whole synthetic body of m1.");
    assert_eq!(
        m1.link,
        "https://outlook.office365.com/owa/?ItemID=m1&exvsurl=1"
    );
    let first_delta = server
        .requests()
        .into_iter()
        .find(|request| request.path.contains("/messages/delta"))
        .unwrap();
    assert!(
        first_delta
            .path
            .contains("$filter=receivedDateTime+ge+2026-10-09T09:00:00Z"),
        "the first fetch reaches back fourteen days: {}",
        first_delta.path
    );
    let summary = sync
        .edit(Instant::now(), |state| ingest(state, &first))
        .unwrap();
    assert_eq!(summary.added.len(), 2);

    let account = ConnectionId::new("con-m365");
    let cursors = mail::fetch::cursors(sync.state(), &account, &["inbox".to_owned()]);
    let next = fetch(source.as_mut(), &cursors).unwrap();
    assert_eq!(
        next.folders[0].messages.len(),
        1,
        "the removed entry is skipped"
    );
    assert!(next.folders[0].cursor.ends_with("deltatoken=d2"));
    let summary = sync
        .edit(Instant::now(), |state| ingest(state, &next))
        .unwrap();
    assert_eq!(summary.added.len(), 1);
    assert_eq!(sync.state().signals().len(), 3);

    let again = fetch(source.as_mut(), &BTreeMap::new()).unwrap();
    let summary = sync
        .edit(Instant::now(), |state| ingest(state, &again))
        .unwrap();
    assert_eq!((summary.added.len(), summary.duplicates), (0, 2));
    assert_eq!(sync.state().signals().len(), 3);
}

#[test]
fn the_per_folder_limit_resumes_from_the_next_link_and_never_skips() {
    let (server, _) = mock(Script::default());
    let secrets = signed_in();
    let root = tempfile::tempdir().unwrap();
    let mut sync = create_workspace(root.path(), &secrets);
    let store: Arc<dyn SecretStore> = Arc::new(secrets.clone());
    let mut environment = environment(&server);
    environment.limits.max_messages = 1;
    let mut source = open_source(&account(), store, &environment).unwrap();
    let account = ConnectionId::new("con-m365");
    let folders = ["inbox".to_owned()];

    let mut cursors_seen = Vec::new();
    for _ in 0..3 {
        let held = mail::fetch::cursors(sync.state(), &account, &folders);
        let fetched = fetch(source.as_mut(), &held).unwrap();
        assert_eq!(fetched.folders[0].messages.len(), 1);
        cursors_seen.push(fetched.folders[0].cursor.clone());
        sync.edit(Instant::now(), |state| ingest(state, &fetched))
            .unwrap();
    }

    assert!(
        cursors_seen[0].ends_with("skiptoken=p2"),
        "{cursors_seen:?}"
    );
    assert!(
        cursors_seen[1].ends_with("deltatoken=d1"),
        "{cursors_seen:?}"
    );
    assert!(
        cursors_seen[2].ends_with("deltatoken=d2"),
        "{cursors_seen:?}"
    );
    assert_eq!(
        sync.state().signals().len(),
        3,
        "m1, m2 and m3, none skipped"
    );
    assert!(
        server
            .requests()
            .iter()
            .filter(|request| request.path.contains("/messages/delta"))
            .all(|request| request.headers.get("prefer").map(String::as_str)
                == Some("odata.maxpagesize=1")),
        "a page never holds more than the fetch may bring"
    );
}

#[test]
fn a_cursor_outside_graph_is_never_followed() {
    let (server, _) = mock(Script::default());
    let secrets = signed_in();
    let mut source = source(&server, &secrets);
    let planted = BTreeMap::from([(
        "inbox".to_owned(),
        "https://collector.example.test/steal?token=".to_owned(),
    )]);
    let fetched = fetch(source.as_mut(), &planted).unwrap();
    assert_eq!(
        fetched.folders[0].messages.len(),
        2,
        "fetched afresh instead"
    );
}

#[test]
fn throttling_is_retried_after_the_servers_delay() {
    let (server, script) = mock(Script {
        throttle_first_delta: true,
        ..Script::default()
    });
    let secrets = signed_in();
    let mut source = source(&server, &secrets);
    let fetched = fetch(source.as_mut(), &BTreeMap::new()).unwrap();
    assert_eq!(fetched.folders[0].messages.len(), 2);
    assert_eq!(
        script.deltas.load(Ordering::SeqCst),
        3,
        "429, then two pages"
    );
}

#[test]
fn an_expired_sign_in_is_an_auth_failure() {
    let (server, _) = mock(Script {
        refuse_refresh: true,
        ..Script::default()
    });
    let secrets = signed_in();
    let mut source = source(&server, &secrets);
    assert!(matches!(source.list_folders(), Err(MailError::Auth(_))));

    let (server, _) = mock(Script {
        refuse_every_token: true,
        ..Script::default()
    });
    let mut source = self::source(&server, &secrets);
    let outcome = fetch(source.as_mut(), &BTreeMap::new());
    assert!(matches!(outcome, Err(MailError::Auth(_))), "{outcome:?}");
    let refreshes = server
        .requests()
        .iter()
        .filter(|request| request.path == "/oauth/token")
        .count();
    assert_eq!(
        refreshes, 2,
        "a refused token is refreshed once, then reported"
    );
}

#[test]
fn the_refresh_token_rotates_in_the_secret_store() {
    let (server, _) = mock(Script::default());
    let secrets = signed_in();
    assert!(has_credential(&secrets, &account()).unwrap());
    let mut source = source(&server, &secrets);
    source.list_folders().unwrap();
    let stored = secrets
        .get(&SecretId::new(SecretKind::MailRefreshToken, "con-m365"))
        .unwrap()
        .unwrap();
    assert_eq!(stored.value.expose(), b"synthetic-refresh-2");
}
