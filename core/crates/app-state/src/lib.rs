//! App state skeleton: one Automerge document with a `sessions` list, written
//! as one change file per commit under `<sync root>/<device id>/NNNN.bin`.
//!
//! Every device starts from the same schema change (a fixed actor, a zero
//! timestamp), so the `sessions` list is one object on every device and
//! appends made apart merge into one list. Each device numbers its files
//! without gaps, so a missing number or an unreadable file means the sync
//! service has not delivered it whole yet: that device's later files wait and
//! the file is retried on the next pull. Change files are plaintext here; PR 4
//! wraps them in the encryption layer and adds the snapshot.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use automerge::transaction::{CommitOptions, Transactable};
use automerge::{
    ActorId, AutoCommit, AutomergeError, Change, ChangeHash, ObjId, ObjType, ROOT, ReadDoc,
};
use thiserror::Error;

/// The key of the sessions list in the document root.
const SESSIONS_KEY: &str = "sessions";
/// Actor of the schema change every device shares; never used for user changes.
const SCHEMA_ACTOR: &[u8] = b"vaultday-schema";
/// Minimum width of the change file number, `0001.bin`; `10000.bin` follows `9999.bin`.
const FILE_NUMBER_WIDTH: usize = 4;
const CHANGE_FILE_EXTENSION: &str = "bin";

/// Which device wrote a change; also the Automerge actor of that device.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

/// One timer session as stored in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    /// The session id.
    pub id: String,
    /// What the session was on: a task id, a story number or a label.
    pub target: String,
    /// When it started, RFC 3339.
    pub started_at: String,
}

/// What one [`AppState::pull`] did.
#[derive(Debug, Default)]
pub struct PullOutcome {
    /// How many change files were applied.
    pub applied: usize,
    /// One error per device whose next change file could not be read or
    /// applied; its later files wait for the next pull.
    pub stalled: Vec<AppStateError>,
}

/// Why the state could not be read or written.
#[derive(Debug, Error)]
pub enum AppStateError {
    /// A change file or folder could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The path concerned.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// A change file holds bytes Automerge refuses.
    #[error("{path}: not an Automerge change: {source}")]
    Corrupt {
        /// The file concerned.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: Box<AutomergeError>,
    },
    /// The document is missing the sessions list, which the schema change creates.
    #[error("the document has no sessions list")]
    MissingSessions,
    /// A device id is empty or would not be a plain folder name.
    #[error("{0:?} is not a device id: letters, digits, `-` and `_` only")]
    InvalidDeviceId(String),
}

/// The app state of one device over a shared sync folder.
#[derive(Debug)]
pub struct AppState {
    doc: AutoCommit,
    sessions: ObjId,
    sync_root: PathBuf,
    device_id: DeviceId,
    next_file_number: u32,
    applied: BTreeMap<DeviceId, u32>,
}

impl DeviceId {
    /// Wraps a device id after checking it is a plain folder name.
    ///
    /// # Errors
    /// [`AppStateError::InvalidDeviceId`] for an empty id or one with other
    /// characters than letters, digits, `-` and `_`.
    pub fn new(id: impl Into<String>) -> Result<DeviceId, AppStateError> {
        let id = id.into();
        let is_plain = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !is_plain {
            return Err(AppStateError::InvalidDeviceId(id));
        }
        Ok(DeviceId(id))
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AppState {
    /// Opens the state for `device_id` over `sync_root`, creating the device
    /// folder and loading every change file already there from every device.
    /// A change file that cannot be applied yet does not stop the open; the
    /// next [`AppState::pull`] retries and reports it.
    ///
    /// # Errors
    /// The sync root or a folder in it cannot be created or listed.
    pub fn open(sync_root: &Path, device_id: DeviceId) -> Result<AppState, AppStateError> {
        let device_dir = sync_root.join(device_id.as_str());
        fs::create_dir_all(&device_dir).map_err(|source| AppStateError::Io {
            path: device_dir.clone(),
            source,
        })?;
        let mut doc = schema_document()?.with_actor(ActorId::from(device_id.as_str().as_bytes()));
        let sessions = sessions_list(&doc)?;
        let mut state = AppState {
            doc: AutoCommit::new(),
            sessions,
            sync_root: sync_root.to_path_buf(),
            device_id,
            next_file_number: 1,
            applied: BTreeMap::new(),
        };
        std::mem::swap(&mut state.doc, &mut doc);
        state.pull()?;
        state.next_file_number = state
            .applied
            .get(&state.device_id)
            .map_or(1, |highest| highest + 1);
        Ok(state)
    }

    /// The device this state writes as.
    #[must_use]
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Appends a session and writes the change as the next file of this
    /// device. Returns the file's path.
    ///
    /// # Errors
    /// File errors while writing the change file.
    pub fn append_session(&mut self, entry: &SessionEntry) -> Result<PathBuf, AppStateError> {
        let index = self.doc.length(&self.sessions);
        let session = self
            .doc
            .insert_object(&self.sessions, index, ObjType::Map)
            .map_err(|source| corrupt(self.device_dir(), source))?;
        for (key, value) in [
            ("id", &entry.id),
            ("target", &entry.target),
            ("started_at", &entry.started_at),
        ] {
            self.doc
                .put(&session, key, value.as_str())
                .map_err(|source| corrupt(self.device_dir(), source))?;
        }
        self.doc
            .commit_with(CommitOptions::default().with_message(format!("session {}", entry.id)));
        let bytes = self
            .doc
            .get_last_local_change()
            .map(|change| change.raw_bytes().to_vec())
            .unwrap_or_default();
        let path = self
            .device_dir()
            .join(change_file_name(self.next_file_number));
        write_atomic(&path, &bytes)?;
        self.applied
            .insert(self.device_id.clone(), self.next_file_number);
        self.next_file_number += 1;
        Ok(path)
    }

    /// Loads every change file not yet applied, from every device folder, in
    /// each device's order. A device stops at its first missing number or at
    /// a file that cannot be read or applied, which [`PullOutcome::stalled`]
    /// reports; the other devices still load.
    ///
    /// # Errors
    /// The sync root or a device folder cannot be listed.
    pub fn pull(&mut self) -> Result<PullOutcome, AppStateError> {
        let mut outcome = PullOutcome::default();
        for (device, dir) in device_folders(&self.sync_root)? {
            let mut highest = self.applied.get(&device).copied().unwrap_or(0);
            for (number, path) in change_files_above(&dir, highest)? {
                if number != highest + 1 {
                    break;
                }
                if let Err(error) = self.apply_change_file(&path) {
                    outcome.stalled.push(error);
                    break;
                }
                highest = number;
                self.applied.insert(device.clone(), number);
                outcome.applied += 1;
            }
        }
        Ok(outcome)
    }

    /// Every session in the document, in list order.
    pub fn sessions(&self) -> Vec<SessionEntry> {
        self.doc
            .list_range(&self.sessions, ..)
            .map(|item| SessionEntry {
                id: self.string_field(&item.id(), "id"),
                target: self.string_field(&item.id(), "target"),
                started_at: self.string_field(&item.id(), "started_at"),
            })
            .collect()
    }

    /// The document heads: equal on two devices exactly when they converged.
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    fn apply_change_file(&mut self, path: &Path) -> Result<(), AppStateError> {
        let bytes = fs::read(path).map_err(|source| AppStateError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        // `Change::from_bytes` validates the file; `load_incremental` would
        // silently skip bytes that are not a change chunk.
        let change =
            Change::from_bytes(bytes).map_err(|source| corrupt(path.to_path_buf(), source))?;
        self.doc
            .apply_changes([change])
            .map_err(|source| corrupt(path.to_path_buf(), source))
    }

    fn string_field(&self, obj: &ObjId, key: &str) -> String {
        match self.doc.get(obj, key) {
            Ok(Some((value, _))) => value.as_str().unwrap_or_default().to_owned(),
            _ => String::new(),
        }
    }

    fn device_dir(&self) -> PathBuf {
        self.sync_root.join(self.device_id.as_str())
    }
}

/// The one change every device starts from: it creates the sessions list
/// under a fixed actor at time zero, so its hash is the same everywhere.
fn schema_document() -> Result<AutoCommit, AppStateError> {
    let mut doc = AutoCommit::new().with_actor(ActorId::from(SCHEMA_ACTOR));
    doc.put_object(ROOT, SESSIONS_KEY, ObjType::List)
        .map_err(|source| corrupt(PathBuf::from("<schema>"), source))?;
    doc.commit_with(CommitOptions::default().with_time(0));
    Ok(doc)
}

fn sessions_list(doc: &AutoCommit) -> Result<ObjId, AppStateError> {
    match doc.get(ROOT, SESSIONS_KEY) {
        Ok(Some((_, id))) => Ok(id),
        _ => Err(AppStateError::MissingSessions),
    }
}

fn device_folders(sync_root: &Path) -> Result<Vec<(DeviceId, PathBuf)>, AppStateError> {
    let entries = fs::read_dir(sync_root).map_err(|source| AppStateError::Io {
        path: sync_root.to_path_buf(),
        source,
    })?;
    let mut folders = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| AppStateError::Io {
            path: sync_root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(device) = entry
            .file_name()
            .to_str()
            .and_then(|name| DeviceId::new(name).ok())
        else {
            continue;
        };
        folders.push((device, path));
    }
    folders.sort();
    Ok(folders)
}

fn change_files_above(dir: &Path, highest: u32) -> Result<Vec<(u32, PathBuf)>, AppStateError> {
    let entries = fs::read_dir(dir).map_err(|source| AppStateError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut files: Vec<(u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let number = change_file_number(&path)?;
            (number > highest).then_some((number, path))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn change_file_number(path: &Path) -> Option<u32> {
    if path.extension()?.to_str()? != CHANGE_FILE_EXTENSION {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if !stem.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number: u32 = stem.parse().ok()?;
    // Exactly the name `change_file_name` writes: `007.bin` and `00007.bin` are not `0007.bin`.
    (change_file_name(number) == path.file_name()?.to_str()?).then_some(number)
}

fn corrupt(path: PathBuf, source: impl Into<AutomergeError>) -> AppStateError {
    AppStateError::Corrupt {
        path,
        source: Box::new(source.into()),
    }
}

fn change_file_name(number: u32) -> String {
    format!("{number:0FILE_NUMBER_WIDTH$}.{CHANGE_FILE_EXTENSION}")
}

/// Writes through a sibling temp file, flushed to disk, and renames, so a
/// reader never sees a half-written change, even after a power loss.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppStateError> {
    let io_error = |source| AppStateError::Io {
        path: path.to_path_buf(),
        source,
    };
    let temp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temp_path, path).map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_ids_are_plain_folder_names() {
        assert!(DeviceId::new("mac-a_1").is_ok());
        assert!(matches!(
            DeviceId::new(""),
            Err(AppStateError::InvalidDeviceId(_))
        ));
        assert!(matches!(
            DeviceId::new("../x"),
            Err(AppStateError::InvalidDeviceId(_))
        ));
    }

    #[test]
    fn change_file_names_are_four_digits() {
        assert_eq!(change_file_name(7), "0007.bin");
        assert_eq!(change_file_number(Path::new("a/0007.bin")), Some(7));
        assert_eq!(change_file_number(Path::new("a/0007.tmp")), None);
        assert_eq!(change_file_number(Path::new("a/7.bin")), None);
        assert_eq!(change_file_number(Path::new("a/+007.bin")), None);
    }

    #[test]
    fn numbering_continues_past_four_digits() {
        assert_eq!(change_file_name(10_000), "10000.bin");
        assert_eq!(change_file_number(Path::new("a/10000.bin")), Some(10_000));
    }

    #[test]
    fn the_schema_change_is_identical_on_every_device() {
        let mut a = schema_document().unwrap();
        let mut b = schema_document().unwrap();
        assert_eq!(a.get_heads(), b.get_heads());
        assert_eq!(sessions_list(&a).unwrap(), sessions_list(&b).unwrap());
    }
}
