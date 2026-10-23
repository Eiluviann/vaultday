//! A Toggl Track API v9 mock on a loopback port, the workspace document and
//! the app state over temporary folders. Every test of the crate talks to
//! this server; none reaches the live service.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use app_state::{
    AppStateStore, DataKey, DeviceId, DeviceIdentity, InMemorySecretStore, PlainSyncFiles,
    SecretBytes, SecretItem, SecretKind, SyncConfig, WorkspaceSync, create_workspace_key,
    workspace_key,
};
use domain::workspace::WorkspaceId;
use serde_json::{Value, json};
use vaultday_feature_toggl::{HttpTogglApi, RetryPolicy};

pub const WORKSPACE: &str = "ws-toggl-test";
pub const TOKEN: &str = "synthetic-token-0001";
pub const TOGGL_WORKSPACE: u64 = 4242;

/// One request the mock received.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: String,
}

/// A fault the mock answers the next matching request with.
#[derive(Debug, Clone)]
pub enum Fault {
    /// Answers with this status and an error body; nothing happens.
    Status(u16),
    /// Answers 429 with `Retry-After` in seconds; nothing happens.
    RateLimited(u64),
    /// Carries the request out, then answers 500: the ambiguous case.
    ServerErrorAfterCommit,
    /// Answers 302 with this `Location`; nothing happens.
    Redirect(String),
}

#[derive(Default)]
struct MockState {
    entries: BTreeMap<u64, Value>,
    next_id: u64,
    faults: VecDeque<(String, Fault)>,
    requests: Vec<Received>,
}

/// The mock server.
pub struct MockToggl {
    pub base: String,
    state: Arc<Mutex<MockState>>,
}

impl MockToggl {
    pub fn start() -> MockToggl {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/api/v9", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(MockState {
            next_id: 9000,
            ..MockState::default()
        }));
        let shared = state.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let shared = shared.clone();
                thread::spawn(move || serve(stream, &shared));
            }
        });
        MockToggl { base, state }
    }

    /// Queues a fault for the next request whose `METHOD path-prefix` matches.
    pub fn fail_next(&self, route: &str, fault: Fault) {
        self.state
            .lock()
            .unwrap()
            .faults
            .push_back((route.to_owned(), fault));
    }

    pub fn requests(&self) -> Vec<Received> {
        self.state.lock().unwrap().requests.clone()
    }

    pub fn requests_to(&self, method: &str, prefix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.method == method && request.path.starts_with(prefix))
            .count()
    }

    pub fn entries(&self) -> Vec<Value> {
        self.state
            .lock()
            .unwrap()
            .entries
            .values()
            .cloned()
            .collect()
    }

    /// A client for this server, retrying without waiting and recording
    /// each wait in [`slept`].
    pub fn client(&self, secrets: &InMemorySecretStore) -> HttpTogglApi {
        HttpTogglApi::loopback(&self.base, &workspace_id(), Arc::new(secrets.clone()))
            .unwrap()
            .with_retry(RetryPolicy {
                sleep: record_sleep,
                ..RetryPolicy::default()
            })
    }
}

thread_local! {
    static SLEPT: RefCell<Vec<Duration>> = const { RefCell::new(Vec::new()) };
}

fn record_sleep(duration: Duration) {
    SLEPT.with(|slept| slept.borrow_mut().push(duration));
}

/// The waits the client asked for on this thread, emptied.
pub fn slept() -> Vec<Duration> {
    SLEPT.with(|slept| slept.borrow_mut().drain(..).collect())
}

fn serve(stream: TcpStream, state: &Mutex<MockState>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let path = parts.next().unwrap_or_default().to_owned();
        let mut headers = BTreeMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        let length: usize = headers
            .get("content-length")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let received = Received {
            method,
            path,
            authorization: headers.get("authorization").cloned(),
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        let (status, extra, body) = respond(state, received);
        let mut response = format!(
            "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in extra {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        response.push_str(&body);
        if out.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

fn respond(state: &Mutex<MockState>, received: Received) -> (u16, Vec<(String, String)>, String) {
    let mut state = state.lock().unwrap();
    state.requests.push(received.clone());
    let route = format!("{} {}", received.method, received.path);
    if received.authorization != Some(expected_authorization()) {
        return (401, Vec::new(), "\"unauthorized\"".to_owned());
    }
    let fault = state
        .faults
        .iter()
        .position(|(prefix, _)| route.starts_with(prefix.as_str()))
        .and_then(|index| state.faults.remove(index))
        .map(|(_, fault)| fault);
    match fault {
        Some(Fault::Status(status)) => return (status, Vec::new(), "\"fault\"".to_owned()),
        Some(Fault::RateLimited(seconds)) => {
            return (
                429,
                vec![("Retry-After".to_owned(), seconds.to_string())],
                "\"slow down\"".to_owned(),
            );
        }
        Some(Fault::ServerErrorAfterCommit) => {
            let _ = handle(&mut state, &received);
            return (500, Vec::new(), "\"oops\"".to_owned());
        }
        Some(Fault::Redirect(location)) => {
            return (302, vec![("Location".to_owned(), location)], String::new());
        }
        None => {}
    }
    handle(&mut state, &received)
}

fn handle(state: &mut MockState, received: &Received) -> (u16, Vec<(String, String)>, String) {
    let path = received
        .path
        .strip_prefix("/api/v9")
        .unwrap_or(&received.path);
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    let segments: Vec<&str> = route.trim_matches('/').split('/').collect();
    let ok = |value: Value| (200, Vec::new(), value.to_string());
    match (received.method.as_str(), segments.as_slice()) {
        ("GET", ["me"]) => ok(json!({
            "id": 77, "fullname": "Mock User", "default_workspace_id": TOGGL_WORKSPACE,
        })),
        ("GET", ["me", "workspaces"]) => ok(json!([
            {"id": TOGGL_WORKSPACE, "name": "Mock Studio"},
            {"id": 7, "name": "Side work"},
        ])),
        ("GET", ["workspaces", _, "projects"]) => ok(json!([
            {"id": 501, "workspace_id": TOGGL_WORKSPACE, "name": "Website", "client_id": 31, "active": true},
            {"id": 502, "workspace_id": TOGGL_WORKSPACE, "name": "Retainer", "client_id": null, "active": false},
        ])),
        ("GET", ["me", "time_entries"]) => {
            let parameter = |name: &str| {
                query.split('&').find_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    (key == name).then(|| value.to_owned())
                })
            };
            let (Some(from), Some(to)) = (parameter("start_date"), parameter("end_date")) else {
                return (400, Vec::new(), "\"dates\"".to_owned());
            };
            let from: jiff::Timestamp = from.parse().unwrap();
            let to: jiff::Timestamp = to.parse().unwrap();
            let listed: Vec<Value> = state
                .entries
                .values()
                .filter(|entry| {
                    let start: jiff::Timestamp = entry["start"].as_str().unwrap().parse().unwrap();
                    start >= from && start < to
                })
                .cloned()
                .collect();
            ok(Value::Array(listed))
        }
        ("POST", ["workspaces", workspace, "time_entries"]) => {
            let mut entry: Value = serde_json::from_str(&received.body).unwrap();
            let id = state.next_id;
            state.next_id += 1;
            entry["id"] = json!(id);
            entry["workspace_id"] = json!(workspace.parse::<u64>().unwrap());
            state.entries.insert(id, entry.clone());
            ok(entry)
        }
        ("PUT", ["workspaces", _, "time_entries", id]) => {
            let id: u64 = id.parse().unwrap();
            if !state.entries.contains_key(&id) {
                return (404, Vec::new(), "\"not found\"".to_owned());
            }
            let mut entry: Value = serde_json::from_str(&received.body).unwrap();
            entry["id"] = json!(id);
            state.entries.insert(id, entry.clone());
            ok(entry)
        }
        ("DELETE", ["workspaces", _, "time_entries", id]) => {
            let id: u64 = id.parse().unwrap();
            match state.entries.remove(&id) {
                Some(_) => (200, Vec::new(), String::new()),
                None => (404, Vec::new(), "\"not found\"".to_owned()),
            }
        }
        _ => (404, Vec::new(), "\"no route\"".to_owned()),
    }
}

pub fn expected_authorization() -> String {
    use base64::Engine;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{TOKEN}:api_token"))
    )
}

pub fn workspace_id() -> WorkspaceId {
    WorkspaceId::new(WORKSPACE)
}

/// A secret store holding the synthetic token for the test workspace.
pub fn secrets_with_token() -> InMemorySecretStore {
    let secrets = InMemorySecretStore::new();
    store_token(&secrets);
    secrets
}

pub fn store_token(secrets: &InMemorySecretStore) {
    use app_state::SecretStore;
    secrets
        .put(SecretItem::new(
            SecretKind::TogglApiToken,
            WORKSPACE,
            SecretBytes::new(TOKEN.as_bytes().to_vec()),
        ))
        .unwrap();
}

/// A device's encrypted workspace document over `root`.
pub fn create_workspace(root: &Path, secrets: &InMemorySecretStore, device: &str) -> WorkspaceSync {
    if workspace_key(secrets, &workspace_id()).unwrap().is_none() {
        create_workspace_key(secrets, &workspace_id()).unwrap();
    }
    WorkspaceSync::create(
        Arc::new(PlainSyncFiles),
        root,
        DeviceId::new(device).unwrap(),
        key(secrets),
        SyncConfig::default(),
    )
    .unwrap()
}

/// The same device's workspace document read back from disk, as after a
/// restart: whatever was not flushed is gone.
pub fn reopen_workspace(root: &Path, secrets: &InMemorySecretStore, device: &str) -> WorkspaceSync {
    WorkspaceSync::open(
        Arc::new(PlainSyncFiles),
        root,
        DeviceId::new(device).unwrap(),
        key(secrets),
        SyncConfig::default(),
    )
    .unwrap()
    .0
}

pub fn key(secrets: &InMemorySecretStore) -> DataKey {
    workspace_key(secrets, &workspace_id()).unwrap().unwrap()
}

/// The app-level document over `dir`.
pub fn open_app_state(dir: &Path, secrets: &InMemorySecretStore) -> AppStateStore {
    let identity = DeviceIdentity::load_or_create(secrets).unwrap();
    AppStateStore::open(Arc::new(PlainSyncFiles), dir, secrets, identity)
        .unwrap()
        .0
}

/// The feature, switched on for the test workspace.
pub fn feature() -> vaultday_feature_toggl::TogglFeature {
    let mut registry = domain::registry::Registry::new();
    vaultday_feature_toggl::register(&mut registry).unwrap();
    let mut enablement = domain::registry::Enablement::new();
    enablement.enable_for_app(vaultday_feature_toggl::FEATURE_ID);
    enablement.enable_for_workspace(WORKSPACE, vaultday_feature_toggl::FEATURE_ID);
    vaultday_feature_toggl::TogglFeature::for_workspace(&registry, &enablement, &workspace_id())
        .unwrap()
}

/// The mapping every queue test sends under.
pub fn mapping() -> vaultday_feature_toggl::TogglMapping {
    let mut mapping = vaultday_feature_toggl::TogglMapping {
        toggl_workspace_id: Some(TOGGL_WORKSPACE),
        default_project_id: Some(502),
        ..Default::default()
    };
    mapping.projects_by_story.insert("WEB-1".to_owned(), 501);
    mapping
}
