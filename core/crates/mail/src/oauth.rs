//! Microsoft sign-in through the OAuth 2.0 device-code flow (RFC 8628): the
//! app shows a code, the user enters it at Microsoft's page on any device,
//! and the app polls until the grant arrives. The refresh token goes to the
//! secret store; access tokens live only in memory.
//!
//! The client id is configurable. Vaultday has no registered Microsoft
//! application yet, so the shipped value is [`GRAPH_CLIENT_ID_PLACEHOLDER`]
//! and [`DeviceCodeFlow::start`] refuses to run under it.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use app_state::{ConnectionId, SecretStore};
use serde::Deserialize;

use crate::credentials;
use crate::error::MailError;
use crate::http::{HttpRequest, HttpTransport};

/// The client id until the user registers Vaultday's Microsoft application.
pub const GRAPH_CLIENT_ID_PLACEHOLDER: &str = "00000000-0000-0000-0000-000000000000";
/// Microsoft's authority for work, school and personal accounts.
pub const MICROSOFT_AUTHORITY: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";
/// What Graph mail reading needs: read mail, and stay signed in.
pub const GRAPH_SCOPES: &str = "offline_access User.Read Mail.Read";
/// What IMAP with XOAUTH2 needs on Microsoft's servers.
pub const IMAP_SCOPES: &str = "offline_access https://outlook.office.com/IMAP.AccessAsUser.All";
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The device-code flow against one authority under one client id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeFlow {
    /// The authority's base, ending in `/oauth2/v2.0`.
    pub authority: String,
    /// The application's client id.
    pub client_id: String,
    /// The scopes asked for, space-separated.
    pub scopes: String,
}

/// What the user is shown to sign in. Its `Debug` leaves out the device
/// code, which with the client id is enough to collect the grant.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceCodePrompt {
    /// The code the app polls with; not a secret on its own, but not shown.
    pub device_code: String,
    /// The code the user types.
    pub user_code: String,
    /// Where the user types it.
    pub verification_uri: String,
    /// How long the code is valid.
    pub expires_in: Duration,
    /// How long to wait between polls.
    pub interval: Duration,
}

/// Tokens from a grant or a refresh. Never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    /// The access token.
    pub access_token: String,
    /// The refresh token, when the authority sent one.
    pub refresh_token: Option<String>,
}

/// What one poll found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePoll {
    /// The user has not finished; poll again after the interval.
    Pending,
    /// Polling too fast; add five seconds to the interval.
    SlowDown,
    /// Signed in.
    Granted(TokenSet),
    /// The user declined.
    Declined,
    /// The code expired before the user finished.
    Expired,
}

/// Access tokens from a stored refresh token, refreshed on demand and
/// rotated back into the secret store.
pub struct RefreshingTokens {
    flow: DeviceCodeFlow,
    http: Arc<dyn HttpTransport>,
    secrets: Arc<dyn SecretStore>,
    connection: ConnectionId,
    cached: Option<String>,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

impl DeviceCodeFlow {
    /// Microsoft's flow for Graph under `client_id`.
    #[must_use]
    pub fn microsoft(client_id: &str) -> DeviceCodeFlow {
        DeviceCodeFlow {
            authority: MICROSOFT_AUTHORITY.to_owned(),
            client_id: client_id.to_owned(),
            scopes: GRAPH_SCOPES.to_owned(),
        }
    }

    /// The same flow asking for other scopes.
    #[must_use]
    pub fn with_scopes(&self, scopes: &str) -> DeviceCodeFlow {
        DeviceCodeFlow {
            scopes: scopes.to_owned(),
            ..self.clone()
        }
    }

    /// Whether the client id is still the placeholder.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.client_id.trim().is_empty() && self.client_id != GRAPH_CLIENT_ID_PLACEHOLDER
    }

    /// Asks the authority for a code to show the user.
    ///
    /// # Errors
    /// [`MailError::NotConfigured`] under the placeholder client id; network
    /// or protocol failures otherwise.
    pub fn start(&self, http: &dyn HttpTransport) -> Result<DeviceCodePrompt, MailError> {
        self.require_configured()?;
        let response = http.send(&HttpRequest::post_form(
            format!("{}/devicecode", self.authority),
            &[
                ("client_id", self.client_id.as_str()),
                ("scope", self.scopes.as_str()),
            ],
        ))?;
        if response.status != 200 {
            return Err(status_error(
                "the sign-in code",
                response.status,
                &response.body,
            ));
        }
        let code: DeviceCodeResponse = serde_json::from_str(&response.body)
            .map_err(|error| MailError::Protocol(format!("the sign-in code: {error}")))?;
        Ok(DeviceCodePrompt {
            device_code: code.device_code,
            user_code: code.user_code,
            verification_uri: code.verification_uri,
            expires_in: Duration::from_secs(code.expires_in),
            interval: Duration::from_secs(code.interval.unwrap_or(5)),
        })
    }

    /// Asks once whether the user finished signing in.
    ///
    /// # Errors
    /// Network failures, or an answer the flow does not define.
    pub fn poll(
        &self,
        http: &dyn HttpTransport,
        device_code: &str,
    ) -> Result<DevicePoll, MailError> {
        self.require_configured()?;
        let response = http.send(&HttpRequest::post_form(
            format!("{}/token", self.authority),
            &[
                ("grant_type", DEVICE_CODE_GRANT),
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
            ],
        ))?;
        let token = parse_token(&response.body, response.status)?;
        match token.error.as_deref() {
            None => Ok(DevicePoll::Granted(granted(token)?)),
            Some("authorization_pending") => Ok(DevicePoll::Pending),
            Some("slow_down") => Ok(DevicePoll::SlowDown),
            Some("authorization_declined" | "access_denied") => Ok(DevicePoll::Declined),
            Some("expired_token" | "code_expired") => Ok(DevicePoll::Expired),
            Some(other) => Err(MailError::Protocol(format!(
                "sign-in failed: {other} {}",
                token.error_description.unwrap_or_default()
            ))),
        }
    }

    /// Trades a refresh token for new tokens.
    ///
    /// # Errors
    /// [`MailError::Auth`] when the authority no longer honours the refresh
    /// token (`invalid_grant`); network or protocol failures otherwise.
    pub fn refresh(
        &self,
        http: &dyn HttpTransport,
        refresh_token: &str,
    ) -> Result<TokenSet, MailError> {
        self.require_configured()?;
        let response = http.send(&HttpRequest::post_form(
            format!("{}/token", self.authority),
            &[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("refresh_token", refresh_token),
                ("scope", self.scopes.as_str()),
            ],
        ))?;
        if response.status >= 500 || response.status == 429 {
            return Err(MailError::Network {
                message: format!("the sign-in service answered {}", response.status),
                retry_after: response.retry_after,
            });
        }
        let token = parse_token(&response.body, response.status)?;
        match token.error.as_deref() {
            None => granted(token),
            Some("invalid_grant" | "interaction_required" | "invalid_client") => Err(
                MailError::Auth("the sign-in has expired; sign in to the account again".to_owned()),
            ),
            Some(other) => Err(MailError::Protocol(format!(
                "token refresh failed: {other}"
            ))),
        }
    }

    fn require_configured(&self) -> Result<(), MailError> {
        if self.is_configured() {
            return Ok(());
        }
        Err(MailError::NotConfigured(
            "Microsoft sign-in needs Vaultday's client id, which is not set in this build"
                .to_owned(),
        ))
    }
}

impl fmt::Debug for DeviceCodePrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceCodePrompt")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("has_refresh_token", &self.refresh_token.is_some())
            .finish_non_exhaustive()
    }
}

impl RefreshingTokens {
    /// Tokens for the account `connection`, whose refresh token is in `secrets`.
    pub fn new(
        flow: DeviceCodeFlow,
        http: Arc<dyn HttpTransport>,
        secrets: Arc<dyn SecretStore>,
        connection: ConnectionId,
    ) -> RefreshingTokens {
        RefreshingTokens {
            flow,
            http,
            secrets,
            connection,
            cached: None,
        }
    }

    /// An access token: the cached one, else a fresh one from the refresh
    /// token, whose rotated successor replaces it in the secret store.
    ///
    /// # Errors
    /// [`MailError::MissingCredential`] without a stored refresh token,
    /// [`MailError::Auth`] when it is no longer honoured, network failures.
    pub fn access_token(&mut self) -> Result<String, MailError> {
        if let Some(token) = &self.cached {
            return Ok(token.clone());
        }
        let stored = credentials::refresh_token(self.secrets.as_ref(), &self.connection)?;
        let refresh_token = String::from_utf8_lossy(stored.expose()).into_owned();
        let tokens = self.flow.refresh(self.http.as_ref(), &refresh_token)?;
        if let Some(rotated) = &tokens.refresh_token {
            credentials::store_refresh_token(self.secrets.as_ref(), &self.connection, rotated)?;
        }
        self.cached = Some(tokens.access_token.clone());
        Ok(tokens.access_token)
    }

    /// Forgets the cached access token after the server refused it.
    pub fn invalidate(&mut self) {
        self.cached = None;
    }
}

impl fmt::Debug for RefreshingTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshingTokens")
            .field("connection", &self.connection)
            .field("has_cached_token", &self.cached.is_some())
            .finish_non_exhaustive()
    }
}

fn parse_token(body: &str, status: u16) -> Result<TokenResponse, MailError> {
    serde_json::from_str(body).map_err(|error| {
        MailError::Protocol(format!("the sign-in service answered {status}: {error}"))
    })
}

fn granted(token: TokenResponse) -> Result<TokenSet, MailError> {
    let access_token = token
        .access_token
        .ok_or_else(|| MailError::Protocol("the grant holds no access token".to_owned()))?;
    Ok(TokenSet {
        access_token,
        refresh_token: token.refresh_token,
    })
}

fn status_error(what: &str, status: u16, body: &str) -> MailError {
    if status >= 500 || status == 429 {
        return MailError::network(format!("{what}: the service answered {status}"));
    }
    let detail: Option<TokenResponse> = serde_json::from_str(body).ok();
    let reason = detail
        .and_then(|detail| detail.error_description.or(detail.error))
        .unwrap_or_default();
    MailError::Protocol(format!("{what}: the service answered {status} {reason}"))
}
