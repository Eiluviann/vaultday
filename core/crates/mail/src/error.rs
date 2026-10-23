//! The crate's error type. Every case is an expected outcome of talking to a
//! mail server (a wrong password, a dropped connection, a server that says
//! something unexpected); [`MailError::is_transient`] says which ones a retry
//! may fix.

use std::time::Duration;

use app_state::SecretStoreError;
use thiserror::Error;

/// Why a mail call failed.
#[derive(Debug, Error)]
pub enum MailError {
    /// The server refused the credentials, or a token can no longer be refreshed.
    #[error("{0}")]
    Auth(String),
    /// The network failed or the server is busy; a retry may succeed.
    #[error("{message}")]
    Network {
        /// What happened.
        message: String,
        /// How long the server asked to wait, when it said.
        retry_after: Option<Duration>,
    },
    /// The server answered something this client does not understand.
    #[error("{0}")]
    Protocol(String),
    /// The folder does not exist on the server.
    #[error("no folder {0:?} on the server")]
    FolderNotFound(String),
    /// The secret store holds no password or token for the account on this device.
    #[error("{0}")]
    MissingCredential(String),
    /// Sign-in cannot start: the Microsoft client id is still the placeholder.
    #[error("{0}")]
    NotConfigured(String),
    /// The secret store failed.
    #[error(transparent)]
    SecretStore(#[from] SecretStoreError),
}

impl MailError {
    /// A network failure without a server-given delay.
    pub(crate) fn network(message: impl Into<String>) -> MailError {
        MailError::Network {
            message: message.into(),
            retry_after: None,
        }
    }

    /// Whether a retry may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, MailError::Network { .. })
    }

    /// The delay the server asked for, when it did.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            MailError::Network { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}
