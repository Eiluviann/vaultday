//! The crate's error type. Expected outcomes of valid use that are not
//! failures (a change file that has not arrived yet, a snapshot refused on
//! load) are reported in outcome values instead, see
//! [`crate::sync_folder::PullOutcome`] and [`crate::signed::AppStateLoad`].

use std::io;
use std::path::PathBuf;

use automerge::AutomergeError;
use thiserror::Error;

use crate::secret_store::{SecretId, SecretStoreError};

/// Why app state could not be read, written or keyed.
#[derive(Debug, Error)]
pub enum AppStateError {
    /// A file or folder could not be read or written.
    #[error("{operation} {path}: {source}")]
    Io {
        /// What was attempted.
        operation: &'static str,
        /// The path concerned.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// Automerge refused an edit or a load of bytes this device produced.
    #[error("the document refused the operation: {0}")]
    Document(#[source] Box<AutomergeError>),
    /// A top-level object the schema change creates is missing: the document
    /// was not started from its schema change.
    #[error("the document has no {0:?} object")]
    MissingSchemaObject(String),
    /// A device id is empty, too long or not a plain folder name.
    #[error("{0:?} is not a device id: 1 to 64 letters, digits, `-` and `_`")]
    InvalidDeviceId(String),
    /// The secret store failed.
    #[error(transparent)]
    SecretStore(#[from] SecretStoreError),
    /// The operating system could not supply random bytes.
    #[error("no randomness available: {0}")]
    Randomness(getrandom::Error),
    /// A plaintext beyond what ChaCha20-Poly1305 can seal in one message.
    #[error("{0} bytes are too many to encrypt in one file")]
    TooLargeToEncrypt(usize),
    /// A key was to be minted where one already exists.
    #[error("{0} already exists; a key is minted once")]
    KeyExists(SecretId),
    /// A synchronisable key has not reached this device yet while other
    /// devices already use it; minting another would orphan their data.
    #[error("{0} has not reached this device yet")]
    KeyNotArrived(SecretId),
    /// A stored item does not hold key material of the expected shape.
    #[error("{0} does not hold a valid key")]
    InvalidKeyMaterial(SecretId),
    /// This device's part of the secret store holds more than one signing
    /// key, which only a broken store or a broken restore leaves behind.
    #[error("this device holds {0} signing keys; expected one")]
    SeveralDeviceKeys(usize),
    /// A workspace was to be created over a sync folder that already has files.
    #[error("{0} already holds app state; open it instead of creating it")]
    WorkspaceExists(PathBuf),
    /// Every id block of a kind is claimed.
    #[error("every {0} id block is claimed")]
    IdSpaceExhausted(&'static str),
    /// A volatile state folder was placed inside a workspace folder.
    #[error("{path} is inside the workspace folder {workspace}; volatile state lives outside it")]
    VolatileInsideWorkspace {
        /// The refused volatile state folder.
        path: PathBuf,
        /// The workspace folder it was inside.
        workspace: PathBuf,
    },
}

impl From<AutomergeError> for AppStateError {
    fn from(source: AutomergeError) -> Self {
        AppStateError::Document(Box::new(source))
    }
}

pub(crate) fn io_error(
    operation: &'static str,
    path: impl Into<PathBuf>,
    source: io::Error,
) -> AppStateError {
    AppStateError::Io {
        operation,
        path: path.into(),
        source,
    }
}
