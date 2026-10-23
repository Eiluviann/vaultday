//! The feature's error type. A refused live client, a missing token and a
//! rejected token are outcomes the app shows; the HTTP failures say whether a
//! retry may help ([`TogglError::is_transient`]).

use std::time::Duration;

use app_state::AppStateError;
use thiserror::Error;

/// Why a Toggl operation did not happen.
#[derive(Debug, Error)]
pub enum TogglError {
    /// The live client was refused: the feature is off, no token is stored,
    /// or `allow_live` is off. The reason names which.
    #[error("Toggl is not connected: {0}")]
    LiveRefused(&'static str),
    /// A loopback client was asked for a host other than this machine.
    #[error("{0} is not a loopback address; only the live client leaves this machine")]
    NotLoopback(String),
    /// No token is stored for the workspace.
    #[error("no Toggl token is stored for this workspace")]
    NoToken,
    /// Toggl refused the token (401 or 403).
    #[error("Toggl refused the token")]
    Unauthorized,
    /// Toggl asked to slow down (429).
    #[error("Toggl asked to slow down")]
    RateLimited {
        /// The server's `Retry-After`, when sent.
        retry_after: Option<Duration>,
    },
    /// Toggl failed on its side (5xx).
    #[error("Toggl failed with status {status}")]
    Server {
        /// The status code.
        status: u16,
    },
    /// Toggl refused the request (another 4xx).
    #[error("Toggl refused the request with status {status}: {message}")]
    Rejected {
        /// The status code.
        status: u16,
        /// The body Toggl sent, cut short.
        message: String,
    },
    /// The service could not be reached.
    #[error("Toggl could not be reached: {0}")]
    Network(String),
    /// A response did not have the expected shape.
    #[error("Toggl sent a response that could not be read: {0}")]
    Decode(String),
    /// The feature's entry in `settings.json` does not match its schema.
    #[error("the toggl-billing settings are not valid: {0}")]
    InvalidSettings(String),
    /// An argument breaks the operation's contract.
    #[error("{0}")]
    InvalidInput(String),
    /// The secret store failed.
    #[error("secret store: {0}")]
    SecretStore(String),
    /// The app state refused an edit.
    #[error(transparent)]
    AppState(#[from] AppStateError),
}

impl TogglError {
    /// Whether a later attempt may succeed: rate limits, server failures and
    /// network failures.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            TogglError::RateLimited { .. } | TogglError::Server { .. } | TogglError::Network(_)
        )
    }

    /// Whether the request may have been carried out although no answer
    /// came back: a server failure or a broken connection. A rate limit and a
    /// refusal are answered before anything is done.
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, TogglError::Server { .. } | TogglError::Network(_))
    }

    /// The delay the server asked for, if any.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            TogglError::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }
}
