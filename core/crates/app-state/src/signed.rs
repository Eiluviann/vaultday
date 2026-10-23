//! The signed app-level state. Each device keeps its copy of the
//! [`AppState`] document as one file, `<app dir>/<device-id>.bin`: the
//! document saved, sealed under the app data key (the envelope format in `SECURITY.md`),
//! then signed with the device's Ed25519 key over the sealed bytes. Loading
//! verifies every file against the device public keys published in the
//! secret store before anything is decrypted, and merges those that verify.
//! A file that fails is refused and reported in [`AppStateLoad::refused`];
//! its content never reaches the document.
//!
//! Merging is monotonic, so replaying an old signed file cannot take a
//! setting back: it only re-adds history the document already has.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use automerge::AutoCommit;
use automerge::transaction::CommitOptions;
use ed25519_dalek::{SIGNATURE_LENGTH, Signature};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::app_doc::{APP_SCHEMA_VERSION, AppState};
use crate::device::DeviceId;
use crate::envelope::{self, EnvelopeKind, Header, OpenError};
use crate::error::{AppStateError, io_error};
use crate::keys::{DataKey, DeviceIdentity, TrustedDevices, app_key, app_key_id, create_app_key};
use crate::secret_store::SecretStore;
use crate::sync_files::{ICLOUD_PLACEHOLDER_SUFFIX, SyncFiles, TEMP_SUFFIX};

/// Prefixed to the signed bytes so a signature over an app snapshot can never
/// be replayed as a signature over anything else.
const SIGNING_CONTEXT: &[u8] = b"vaultday app snapshot v1\0";
const SNAPSHOT_EXTENSION: &str = ".bin";

/// Why a snapshot file was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnapshotRefusal {
    /// The file could not be read.
    #[error("the file could not be read")]
    Unreadable,
    /// The bytes are not a signed app snapshot.
    #[error("not a signed app snapshot")]
    Malformed,
    /// No public key is published for the device the file names.
    #[error("no published key for device {0}: the snapshot is not trusted")]
    UnknownDevice(DeviceId),
    /// The signature does not verify: the file was altered, or not written by that device.
    #[error("the signature of device {0} does not verify: the snapshot was altered")]
    BadSignature(DeviceId),
    /// Signed correctly but sealed under another app key.
    #[error("signed by device {0} but sealed under another app key")]
    Undecryptable(DeviceId),
    /// Signed and decrypted but not an app document.
    #[error("signed by device {0} but not an app document")]
    Unparsable(DeviceId),
    /// Written by a newer build of the app.
    #[error("written by a newer version of the app")]
    NewerVersion,
}

/// A snapshot file that was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedSnapshot {
    /// The file.
    pub path: PathBuf,
    /// Why.
    pub reason: SnapshotRefusal,
}

/// What a load did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppStateLoad {
    /// Snapshots verified and merged.
    pub merged: usize,
    /// Snapshots refused; the app reports them to the user.
    pub refused: Vec<RefusedSnapshot>,
    /// Placeholders of snapshots the sync provider has not downloaded yet;
    /// a download was requested and the next reload merges them.
    pub pending: Vec<PathBuf>,
}

/// The app-level document of this device over its snapshot folder.
pub struct AppStateStore {
    files: Arc<dyn SyncFiles>,
    dir: PathBuf,
    key: DataKey,
    identity: DeviceIdentity,
    state: AppState,
    /// Whether this device's own snapshot was refused at the last load; it is
    /// set aside before the next save, never overwritten.
    is_own_snapshot_refused: bool,
}

impl AppStateStore {
    /// Opens the app-level state in `dir`: verifies every device's snapshot
    /// and merges those that verify. The first device, finding an empty
    /// folder and no app key, mints the key.
    ///
    /// # Errors
    /// [`AppStateError::KeyNotArrived`] when the folder holds snapshots but
    /// the app key has not synchronised to this device yet (minting one would
    /// orphan them); otherwise folder or secret store failures.
    pub fn open(
        files: Arc<dyn SyncFiles>,
        dir: &Path,
        secrets: &dyn SecretStore,
        identity: DeviceIdentity,
    ) -> Result<(AppStateStore, AppStateLoad), AppStateError> {
        let key = match app_key(secrets)? {
            Some(key) => key,
            // An evicted snapshot counts: another device already holds the key.
            None if list_snapshots(files.as_ref(), dir)?.is_empty() => create_app_key(secrets)?,
            None => {
                return Err(AppStateError::KeyNotArrived(app_key_id()));
            }
        };
        let state = AppState::new(identity.device_id().clone())?;
        let mut store = AppStateStore {
            files,
            dir: dir.to_path_buf(),
            key,
            identity,
            state,
            is_own_snapshot_refused: false,
        };
        let load = store.reload(secrets)?;
        Ok((store, load))
    }

    /// The document.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// This device's identity.
    #[must_use]
    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    /// The document heads: equal on two devices exactly when they converged.
    pub fn heads(&mut self) -> Vec<automerge::ChangeHash> {
        self.state.heads()
    }

    /// Runs `edit` and commits what it did as one change; when `edit` fails,
    /// nothing it did is kept. Call [`AppStateStore::save`] to write it.
    ///
    /// # Errors
    /// Whatever `edit` returns.
    pub fn edit<R>(
        &mut self,
        edit: impl FnOnce(&mut AppState) -> Result<R, AppStateError>,
    ) -> Result<R, AppStateError> {
        match edit(&mut self.state) {
            Ok(result) => {
                self.state.doc_mut().commit_with(CommitOptions::default());
                Ok(result)
            }
            Err(error) => {
                self.state.doc_mut().rollback();
                Err(error)
            }
        }
    }

    /// Writes this device's signed snapshot. Returns its path. When the
    /// last load refused this device's own snapshot, that file is first kept
    /// aside as `.<device-id>.bin.refused`, which loads never read.
    ///
    /// # Errors
    /// File errors.
    pub fn save(&mut self) -> Result<PathBuf, AppStateError> {
        let path = self.own_snapshot_path();
        if self.is_own_snapshot_refused {
            self.set_refused_aside(&path)?;
        }
        let plaintext = Zeroizing::new(self.state.doc_mut().save());
        let header = Header {
            kind: EnvelopeKind::AppSnapshot,
            schema: APP_SCHEMA_VERSION,
            device: self.identity.device_id().clone(),
            sequence: 0,
        };
        let mut bytes = envelope::seal(&self.key, &header, &plaintext)?;
        let signature = self.identity.sign(&signed_message(&bytes));
        bytes.extend_from_slice(&signature.to_bytes());
        self.files
            .write_atomic(&path, &bytes)
            .map_err(|source| io_error("write", &path, source))?;
        Ok(path)
    }

    /// Verifies and merges every snapshot in the folder again, with the
    /// device keys published now.
    ///
    /// # Errors
    /// The folder cannot be listed, or the secret store fails.
    pub fn reload(&mut self, secrets: &dyn SecretStore) -> Result<AppStateLoad, AppStateError> {
        let trusted = TrustedDevices::load(secrets)?;
        let listing = list_snapshots(self.files.as_ref(), &self.dir)?;
        let mut load = AppStateLoad::default();
        for placeholder in listing.placeholders {
            let _ = self.files.request_download(&placeholder);
            load.pending.push(placeholder);
        }
        for path in listing.snapshots {
            let verified = self
                .files
                .read(&path)
                .map_err(|_| SnapshotRefusal::Unreadable)
                .and_then(|bytes| verify_snapshot(&bytes, &trusted, &self.key));
            match verified.and_then(|snapshot| {
                // Not load-and-merge: a merge drops changes still waiting for
                // their dependencies.
                self.state
                    .doc_mut()
                    .load_incremental(&snapshot.plaintext)
                    .map_err(|_| SnapshotRefusal::Unparsable(snapshot.device))
            }) {
                Ok(_) => load.merged += 1,
                Err(reason) => load.refused.push(RefusedSnapshot { path, reason }),
            }
        }
        let own_path = self.own_snapshot_path();
        self.is_own_snapshot_refused = load.refused.iter().any(|refused| refused.path == own_path);
        Ok(load)
    }

    fn own_snapshot_path(&self) -> PathBuf {
        self.dir
            .join(format!("{}{SNAPSHOT_EXTENSION}", self.identity.device_id()))
    }

    fn set_refused_aside(&mut self, path: &Path) -> Result<(), AppStateError> {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(());
        };
        let aside = self.dir.join(format!(".{name}.refused"));
        let bytes = self
            .files
            .read(path)
            .map_err(|source| io_error("read", path, source))?;
        self.files
            .write_atomic(&aside, &bytes)
            .map_err(|source| io_error("write", &aside, source))?;
        self.is_own_snapshot_refused = false;
        Ok(())
    }
}

impl std::fmt::Debug for AppStateStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateStore")
            .field("dir", &self.dir)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// A snapshot whose signature verified and whose content decrypted and parsed.
struct VerifiedSnapshot {
    device: DeviceId,
    plaintext: Zeroizing<Vec<u8>>,
}

/// The snapshot files of the app folder and the placeholders of evicted ones.
#[derive(Default)]
struct SnapshotListing {
    snapshots: Vec<PathBuf>,
    placeholders: Vec<PathBuf>,
}

impl SnapshotListing {
    fn is_empty(&self) -> bool {
        self.snapshots.is_empty() && self.placeholders.is_empty()
    }
}

/// Checks the signature first, against the key published for the device the
/// header names, and decrypts only then.
fn verify_snapshot(
    bytes: &[u8],
    trusted: &TrustedDevices,
    key: &DataKey,
) -> Result<VerifiedSnapshot, SnapshotRefusal> {
    let (sealed, signature) = bytes
        .split_last_chunk::<SIGNATURE_LENGTH>()
        .ok_or(SnapshotRefusal::Malformed)?;
    let claimed = envelope::peek_header(sealed).map_err(|error| match error {
        OpenError::NewerFormat => SnapshotRefusal::NewerVersion,
        OpenError::Malformed | OpenError::Undecryptable => SnapshotRefusal::Malformed,
    })?;
    let device = claimed.device;
    let public_key = trusted
        .key_of(&device)
        .ok_or_else(|| SnapshotRefusal::UnknownDevice(device.clone()))?;
    public_key
        .verify_strict(&signed_message(sealed), &Signature::from_bytes(signature))
        .map_err(|_| SnapshotRefusal::BadSignature(device.clone()))?;
    let (header, plaintext) =
        envelope::open(key, sealed).map_err(|_| SnapshotRefusal::Undecryptable(device.clone()))?;
    if header.kind != EnvelopeKind::AppSnapshot {
        return Err(SnapshotRefusal::Malformed);
    }
    if header.schema > APP_SCHEMA_VERSION {
        return Err(SnapshotRefusal::NewerVersion);
    }
    // Parsed apart first, so a document that does not load is refused whole.
    AutoCommit::load(&plaintext).map_err(|_| SnapshotRefusal::Unparsable(device.clone()))?;
    Ok(VerifiedSnapshot { device, plaintext })
}

/// Every snapshot file in `dir` and every placeholder of one, each sorted.
fn list_snapshots(files: &dyn SyncFiles, dir: &Path) -> Result<SnapshotListing, AppStateError> {
    let mut listing = SnapshotListing::default();
    for entry in files
        .list(dir)
        .map_err(|source| io_error("list", dir, source))?
    {
        if entry.is_dir {
            continue;
        }
        if is_snapshot_name(&entry.path) {
            listing.snapshots.push(entry.path);
        } else if is_snapshot_placeholder_name(&entry.path) {
            listing.placeholders.push(entry.path);
        }
    }
    listing.snapshots.sort();
    listing.placeholders.sort();
    Ok(listing)
}

fn signed_message(sealed: &[u8]) -> Vec<u8> {
    [SIGNING_CONTEXT, sealed].concat()
}

/// `.<device-id>.bin.icloud`: a snapshot the sync provider has evicted.
fn is_snapshot_placeholder_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix('.'))
        .and_then(|name| name.strip_suffix(ICLOUD_PLACEHOLDER_SUFFIX))
        .is_some_and(|evicted| is_snapshot_name(Path::new(evicted)))
}

/// `<device-id>.bin` and its conflict copies; never in-flight temp files.
fn is_snapshot_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            !name.starts_with('.')
                && !name.ends_with(TEMP_SUFFIX)
                && name.ends_with(SNAPSHOT_EXTENSION)
        })
}
