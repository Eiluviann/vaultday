//! [`HttpTogglApi`]: the Toggl Track API v9 over HTTP.
//!
//! The token is read from the secret store for each request and dropped
//! with it: the struct never holds it, and neither its `Debug` form nor any
//! error carries it. Two constructors exist: [`HttpTogglApi::live`], gated
//! three ways, and [`HttpTogglApi::loopback`], which accepts only a server on
//! this machine (the tests' mock server).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use app_state::{AppState, SecretId, SecretKind, SecretStore};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::registry::Registry;
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::json;

use crate::api::{NewTimeEntry, TimeEntry, TogglApi, TogglMe, TogglProject, TogglWorkspace};
use crate::error::TogglError;
use crate::feature::TogglFeature;
use crate::mapping::allow_live;
use crate::transport::{HttpRequest, HttpResponse, Method, TogglTransport, UreqTransport};

/// The live service. Reached only through [`HttpTogglApi::live`].
pub const LIVE_BASE_URL: &str = "https://api.track.toggl.com/api/v9";
/// What Toggl records as the creating app.
const CREATED_WITH: &str = "Vaultday";
/// The longest error body kept in a [`TogglError::Rejected`].
const MAX_MESSAGE_CHARS: usize = 200;

/// How often and how patiently a transient failure is retried by the client.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Attempts in all, the first included; at least one.
    pub attempts: u32,
    /// The wait before the first retry; it doubles each time.
    pub base_delay: Duration,
    /// The longest wait, a server's `Retry-After` included.
    pub max_delay: Duration,
    /// How to wait; tests pass a function that returns at once.
    pub sleep: fn(Duration),
}

/// The Toggl client.
pub struct HttpTogglApi {
    base_url: String,
    workspace: WorkspaceId,
    secrets: Arc<dyn SecretStore>,
    transport: Box<dyn TogglTransport>,
    retry: RetryPolicy,
}

#[derive(Deserialize)]
struct WireMe {
    id: u64,
    fullname: Option<String>,
    default_workspace_id: Option<u64>,
}

#[derive(Deserialize)]
struct WireWorkspace {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct WireProject {
    id: u64,
    workspace_id: u64,
    name: String,
    client_id: Option<u64>,
    active: Option<bool>,
}

#[derive(Deserialize)]
struct WireEntry {
    id: u64,
    workspace_id: u64,
    project_id: Option<u64>,
    description: Option<String>,
    start: String,
    duration: i64,
    tags: Option<Vec<String>>,
    billable: Option<bool>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            attempts: 4,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            sleep: std::thread::sleep,
        }
    }
}

impl RetryPolicy {
    /// The wait before retry number `attempt` (1-based): the server's ask, or
    /// the doubling backoff, never above the cap.
    #[must_use]
    pub fn delay(&self, attempt: u32, asked: Option<Duration>) -> Duration {
        let backoff = self
            .base_delay
            .saturating_mul(2_u32.saturating_pow(attempt.saturating_sub(1)));
        asked.unwrap_or(backoff).min(self.max_delay)
    }
}

impl HttpTogglApi {
    /// The client for the live service, for `workspace`. Refused unless the
    /// feature is on for the workspace, a token is stored for it, and the
    /// app-level `allow_live` switch is on; `allow_live` is off until set.
    ///
    /// # Errors
    /// [`TogglError::LiveRefused`] naming the first unmet condition, or
    /// [`TogglError::SecretStore`] when the store cannot be read.
    pub fn live(
        registry: &Registry,
        app_state: &AppState,
        workspace: &WorkspaceId,
        secrets: Arc<dyn SecretStore>,
    ) -> Result<HttpTogglApi, TogglError> {
        let Some(feature) =
            TogglFeature::for_workspace(registry, &app_state.enablement(), workspace)
        else {
            return Err(TogglError::LiveRefused(
                "the feature is off for this workspace",
            ));
        };
        if !has_token(secrets.as_ref(), feature.workspace())? {
            return Err(TogglError::LiveRefused(
                "no token is stored for this workspace",
            ));
        }
        if !allow_live(app_state) {
            return Err(TogglError::LiveRefused("allow_live is off"));
        }
        Ok(HttpTogglApi::new(LIVE_BASE_URL, workspace, secrets))
    }

    /// A client for a server on this machine (`127.0.0.1`, `[::1]` or
    /// `localhost`), such as the tests' mock server.
    ///
    /// # Errors
    /// [`TogglError::NotLoopback`] for any other address.
    pub fn loopback(
        base_url: &str,
        workspace: &WorkspaceId,
        secrets: Arc<dyn SecretStore>,
    ) -> Result<HttpTogglApi, TogglError> {
        if !is_loopback(base_url) {
            return Err(TogglError::NotLoopback(base_url.to_owned()));
        }
        Ok(HttpTogglApi::new(
            base_url.trim_end_matches('/'),
            workspace,
            secrets,
        ))
    }

    /// The same client with another retry policy.
    #[must_use]
    pub fn with_retry(self, retry: RetryPolicy) -> HttpTogglApi {
        HttpTogglApi { retry, ..self }
    }

    /// Where requests go.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn new(base_url: &str, workspace: &WorkspaceId, secrets: Arc<dyn SecretStore>) -> Self {
        HttpTogglApi {
            base_url: base_url.to_owned(),
            workspace: workspace.clone(),
            secrets,
            transport: Box::new(UreqTransport::new()),
            retry: RetryPolicy::default(),
        }
    }

    /// The `Authorization` value, built from the token read just now.
    fn authorization(&self) -> Result<String, TogglError> {
        let item = self
            .secrets
            .get(&token_id(&self.workspace))
            .map_err(|error| TogglError::SecretStore(error.message))?
            .ok_or(TogglError::NoToken)?;
        let mut credentials = item.value.expose().to_vec();
        credentials.extend_from_slice(b":api_token");
        Ok(format!("Basic {}", STANDARD.encode(&credentials)))
    }

    /// Sends one request, retrying transient failures by the policy. Server
    /// and network failures are retried only when `is_idempotent`; a rate
    /// limit always is, since Toggl did nothing.
    fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
        is_idempotent: bool,
    ) -> Result<HttpResponse, TogglError> {
        let attempts = self.retry.attempts.max(1);
        let mut attempt = 1;
        loop {
            let request = HttpRequest {
                method,
                url: format!("{}{path}", self.base_url),
                headers: vec![
                    ("Authorization".to_owned(), self.authorization()?),
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                ],
                body: body.clone(),
            };
            let outcome = self.transport.send(&request).and_then(checked);
            match outcome {
                Err(error)
                    if error.is_transient()
                        && (is_idempotent || !error.is_ambiguous())
                        && attempt < attempts =>
                {
                    (self.retry.sleep)(self.retry.delay(attempt, error.retry_after()));
                    attempt += 1;
                }
                outcome => return outcome,
            }
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, TogglError> {
        let response = self.send(Method::Get, path, None, true)?;
        decode(&response.body)
    }
}

impl fmt::Debug for HttpTogglApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTogglApi")
            .field("base_url", &self.base_url)
            .field("workspace", &self.workspace)
            .field("token", &"<read from the secret store per request>")
            .finish_non_exhaustive()
    }
}

impl TogglApi for HttpTogglApi {
    fn me(&self) -> Result<TogglMe, TogglError> {
        let me: WireMe = self.get_json("/me")?;
        Ok(TogglMe {
            id: me.id,
            full_name: me.fullname,
            default_workspace_id: me.default_workspace_id,
        })
    }

    fn workspaces(&self) -> Result<Vec<TogglWorkspace>, TogglError> {
        let workspaces: Vec<WireWorkspace> = self.get_json("/me/workspaces")?;
        Ok(workspaces
            .into_iter()
            .map(|workspace| TogglWorkspace {
                id: workspace.id,
                name: workspace.name,
            })
            .collect())
    }

    fn projects(&self, workspace_id: u64) -> Result<Vec<TogglProject>, TogglError> {
        let projects: Option<Vec<WireProject>> =
            self.get_json(&format!("/workspaces/{workspace_id}/projects"))?;
        Ok(projects
            .unwrap_or_default()
            .into_iter()
            .map(|project| TogglProject {
                id: project.id,
                workspace_id: project.workspace_id,
                name: project.name,
                client_id: project.client_id,
                is_active: project.active.unwrap_or(true),
            })
            .collect())
    }

    fn create_time_entry(&self, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError> {
        let path = format!("/workspaces/{}/time_entries", entry.workspace_id);
        let response = self.send(Method::Post, &path, Some(entry_body(entry)?), false)?;
        entry_from_wire(decode(&response.body)?)
    }

    fn update_time_entry(&self, id: u64, entry: &NewTimeEntry) -> Result<TimeEntry, TogglError> {
        let path = format!("/workspaces/{}/time_entries/{id}", entry.workspace_id);
        let response = self.send(Method::Put, &path, Some(entry_body(entry)?), true)?;
        entry_from_wire(decode(&response.body)?)
    }

    fn delete_time_entry(&self, workspace_id: u64, id: u64) -> Result<(), TogglError> {
        let path = format!("/workspaces/{workspace_id}/time_entries/{id}");
        match self.send(Method::Delete, &path, None, true) {
            Ok(_) | Err(TogglError::Rejected { status: 404, .. }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn time_entries(&self, start: Timestamp, end: Timestamp) -> Result<Vec<TimeEntry>, TogglError> {
        let entries: Option<Vec<WireEntry>> = self.get_json(&format!(
            "/me/time_entries?start_date={start}&end_date={end}"
        ))?;
        entries
            .unwrap_or_default()
            .into_iter()
            .map(entry_from_wire)
            .collect()
    }
}

/// The secret store id of a workspace's Toggl token.
#[must_use]
pub fn token_id(workspace: &WorkspaceId) -> SecretId {
    SecretId::new(SecretKind::TogglApiToken, workspace.as_str())
}

/// Whether a token is stored for `workspace`. Reads the store; never returns the token.
///
/// # Errors
/// [`TogglError::SecretStore`] when the store cannot be read.
pub fn has_token(secrets: &dyn SecretStore, workspace: &WorkspaceId) -> Result<bool, TogglError> {
    secrets
        .get(&token_id(workspace))
        .map(|item| item.is_some_and(|item| !item.value.expose().is_empty()))
        .map_err(|error| TogglError::SecretStore(error.message))
}

fn checked(response: HttpResponse) -> Result<HttpResponse, TogglError> {
    match response.status {
        200..=299 => Ok(response),
        401 | 403 => Err(TogglError::Unauthorized),
        429 => Err(TogglError::RateLimited {
            retry_after: response.retry_after,
        }),
        500..=599 => Err(TogglError::Server {
            status: response.status,
        }),
        status => Err(TogglError::Rejected {
            status,
            message: response.body.chars().take(MAX_MESSAGE_CHARS).collect(),
        }),
    }
}

fn decode<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, TogglError> {
    serde_json::from_str(body).map_err(|error| TogglError::Decode(error.to_string()))
}

fn entry_body(entry: &NewTimeEntry) -> Result<String, TogglError> {
    let seconds = i64::try_from(entry.duration_secs)
        .map_err(|_| TogglError::InvalidInput("the entry is too long".to_owned()))?;
    let stop = entry
        .start
        .checked_add(jiff::SignedDuration::from_secs(seconds))
        .map_err(|error| TogglError::InvalidInput(error.to_string()))?;
    Ok(json!({
        "created_with": CREATED_WITH,
        "workspace_id": entry.workspace_id,
        "project_id": entry.project_id,
        "description": entry.description,
        "start": entry.start.to_string(),
        "stop": stop.to_string(),
        "duration": seconds,
        "tags": entry.tags,
        "billable": entry.is_billable,
    })
    .to_string())
}

fn entry_from_wire(wire: WireEntry) -> Result<TimeEntry, TogglError> {
    let start = wire
        .start
        .parse::<Timestamp>()
        .map_err(|error| TogglError::Decode(format!("start {:?}: {error}", wire.start)))?;
    Ok(TimeEntry {
        id: wire.id,
        workspace_id: wire.workspace_id,
        project_id: wire.project_id,
        description: wire.description.unwrap_or_default(),
        start,
        duration_secs: wire.duration,
        tags: wire.tags.unwrap_or_default(),
        is_billable: wire.billable.unwrap_or(false),
    })
}

fn is_loopback(base_url: &str) -> bool {
    let Some(rest) = base_url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_addresses_are_accepted_for_the_mock() {
        for url in [
            "http://127.0.0.1:8080",
            "http://localhost:1/api",
            "http://[::1]:9",
        ] {
            assert!(is_loopback(url), "{url}");
        }
        for url in [
            LIVE_BASE_URL,
            "http://api.track.toggl.com",
            "http://127.0.0.1.example.test",
            "https://127.0.0.1:8080",
            "http://localhost.example.test:1",
            "http://user@api.track.toggl.com",
        ] {
            assert!(!is_loopback(url), "{url}");
        }
    }

    #[test]
    fn delays_double_and_respect_the_server_and_the_cap() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay(1, None), Duration::from_secs(1));
        assert_eq!(policy.delay(3, None), Duration::from_secs(4));
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(900))),
            Duration::from_secs(60)
        );
    }
}
