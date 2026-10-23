//! The HTTP client against the mock server: the endpoints, the token as
//! basic auth read per call, 401, 429 with `Retry-After`, and 5xx with
//! bounded exponential retry.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use app_state::{InMemorySecretStore, SecretStore};
use support::{Fault, MockToggl, TOGGL_WORKSPACE, TOKEN, secrets_with_token, slept, workspace_id};
use vaultday_feature_toggl::{
    HttpTogglApi, LIVE_BASE_URL, NewTimeEntry, TogglApi, TogglError, token_id,
};

fn entry(description: &str) -> NewTimeEntry {
    NewTimeEntry {
        workspace_id: TOGGL_WORKSPACE,
        project_id: Some(501),
        description: description.to_owned(),
        start: "2026-10-19T09:00:00Z".parse().unwrap(),
        duration_secs: 1800,
        tags: vec!["vaultday".to_owned()],
        is_billable: true,
    }
}

#[test]
fn reads_me_workspaces_and_projects_with_the_token_as_basic_auth() {
    let mock = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    let me = api.me().unwrap();
    assert_eq!(me.id, 77);
    assert_eq!(me.default_workspace_id, Some(TOGGL_WORKSPACE));
    let workspaces = api.workspaces().unwrap();
    assert_eq!(workspaces.len(), 2);
    assert_eq!(workspaces[0].name, "Mock Studio");
    let projects = api.projects(TOGGL_WORKSPACE).unwrap();
    assert_eq!(projects[0].client_id, Some(31));
    assert!(projects[0].is_active && !projects[1].is_active);
    for request in mock.requests() {
        assert_eq!(
            request.authorization.as_deref(),
            Some(support::expected_authorization().as_str())
        );
    }
}

#[test]
fn creates_updates_and_deletes_an_entry() {
    let mock = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    let created = api.create_time_entry(&entry("Write the brief")).unwrap();
    assert_eq!(created.workspace_id, TOGGL_WORKSPACE);
    assert_eq!(created.duration_secs, 1800);
    assert!(created.matches(&entry("Write the brief")));
    let posted: serde_json::Value = serde_json::from_str(&mock.requests()[0].body).unwrap();
    assert_eq!(posted["created_with"], "Vaultday");
    assert_eq!(posted["stop"], "2026-10-19T09:30:00Z");
    assert_eq!(posted["tags"], serde_json::json!(["vaultday"]));
    assert!(posted.get("rate").is_none() && posted.get("currency").is_none());

    let updated = api
        .update_time_entry(created.id, &entry("Write the brief, revised"))
        .unwrap();
    assert_eq!(updated.id, created.id);
    assert_eq!(mock.entries()[0]["description"], "Write the brief, revised");

    let listed = api
        .time_entries(
            "2026-10-19T00:00:00Z".parse().unwrap(),
            "2026-10-20T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    assert_eq!(listed.len(), 1);

    api.delete_time_entry(TOGGL_WORKSPACE, created.id).unwrap();
    assert!(mock.entries().is_empty());
    // An entry already gone counts as deleted.
    api.delete_time_entry(TOGGL_WORKSPACE, created.id).unwrap();
}

#[test]
fn a_refused_token_is_unauthorized_and_not_retried() {
    let mock = MockToggl::start();
    let secrets = InMemorySecretStore::new();
    secrets
        .put(app_state::SecretItem::new(
            app_state::SecretKind::TogglApiToken,
            support::WORKSPACE,
            app_state::SecretBytes::new(b"wrong".to_vec()),
        ))
        .unwrap();
    let api = mock.client(&secrets);
    assert!(matches!(api.me(), Err(TogglError::Unauthorized)));
    assert_eq!(mock.requests().len(), 1);
    assert!(slept().is_empty());
}

#[test]
fn without_a_token_nothing_is_sent() {
    let mock = MockToggl::start();
    let api = mock.client(&InMemorySecretStore::new());
    assert!(matches!(api.me(), Err(TogglError::NoToken)));
    assert!(mock.requests().is_empty());
}

#[test]
fn the_token_is_read_per_call_and_never_printed() {
    let mock = MockToggl::start();
    let secrets = secrets_with_token();
    let api = mock.client(&secrets);
    let printed = format!("{api:?}");
    assert!(!printed.contains(TOKEN));
    assert!(printed.contains("HttpTogglApi"));
    api.me().unwrap();
    secrets.delete(&token_id(&workspace_id())).unwrap();
    assert!(matches!(api.me(), Err(TogglError::NoToken)));
    let error = TogglError::Unauthorized.to_string();
    assert!(!error.contains(TOKEN));
}

#[test]
fn a_rate_limit_waits_as_asked_and_retries() {
    let mock = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    mock.fail_next("GET /api/v9/me", Fault::RateLimited(7));
    assert_eq!(api.me().unwrap().id, 77);
    assert_eq!(slept(), [Duration::from_secs(7)]);
    // A create is retried after a rate limit too: Toggl did nothing.
    mock.fail_next("POST", Fault::RateLimited(3));
    api.create_time_entry(&entry("after a limit")).unwrap();
    assert_eq!(slept(), [Duration::from_secs(3)]);
    assert_eq!(mock.entries().len(), 1);
}

#[test]
fn server_failures_retry_with_doubling_waits_up_to_the_bound() {
    let mock = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    mock.fail_next("GET /api/v9/me", Fault::Status(503));
    mock.fail_next("GET /api/v9/me", Fault::Status(502));
    assert_eq!(api.me().unwrap().id, 77);
    assert_eq!(slept(), [Duration::from_secs(1), Duration::from_secs(2)]);

    for _ in 0..5 {
        mock.fail_next("GET /api/v9/me/workspaces", Fault::Status(500));
    }
    let before = mock.requests().len();
    assert!(matches!(
        api.workspaces(),
        Err(TogglError::Server { status: 500 })
    ));
    assert_eq!(mock.requests().len() - before, 4, "four attempts in all");
    assert_eq!(
        slept(),
        [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4)
        ]
    );
}

#[test]
fn a_create_is_not_retried_by_the_client_after_a_server_failure() {
    let mock = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    mock.fail_next("POST", Fault::ServerErrorAfterCommit);
    let error = api.create_time_entry(&entry("maybe created")).unwrap_err();
    assert!(error.is_ambiguous());
    assert_eq!(mock.requests_to("POST", "/api/v9/workspaces"), 1);
    assert_eq!(
        mock.entries().len(),
        1,
        "the queue looks before posting again"
    );
}

#[test]
fn a_loopback_client_refuses_any_other_host() {
    let secrets: Arc<dyn SecretStore> = Arc::new(secrets_with_token());
    for url in [LIVE_BASE_URL, "http://api.track.toggl.com/api/v9"] {
        assert!(matches!(
            HttpTogglApi::loopback(url, &workspace_id(), secrets.clone()),
            Err(TogglError::NotLoopback(_))
        ));
    }
}

#[test]
fn a_redirect_is_never_followed_to_another_host() {
    let mock = MockToggl::start();
    // A second loopback server stands in for any other host: it must see
    // nothing, and no request leaves this machine.
    let elsewhere = MockToggl::start();
    let api = mock.client(&secrets_with_token());
    mock.fail_next("GET", Fault::Redirect(format!("{}/me", elsewhere.base)));
    assert!(matches!(
        api.me(),
        Err(TogglError::Rejected { status: 302, .. })
    ));
    assert!(elsewhere.requests().is_empty());
}
