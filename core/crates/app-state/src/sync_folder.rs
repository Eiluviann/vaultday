//! The sync folder of a workspace: `<workspace>/.vaultday/sync/<device-id>/`,
//! one folder per device, written only by that device and read by all.
//!
//! - **Batches.** Edits commit to the document at once and reach disk as one
//!   change file per burst, `NNNN.bin`, after [`SyncConfig::flush_window`] of
//!   quiet or [`SyncConfig::max_batch_delay`] at most.
//! - **Encryption.** Every file is sealed with the workspace data key (see
//!   the envelope format in `SECURITY.md`); the header names the device and the sequence.
//! - **Tolerant reads.** A file that cannot be read, opened or parsed yet
//!   (an iCloud placeholder, a half-synced or truncated file, one sealed
//!   under another key) is [`NotArrived`]: skipped, reported in
//!   [`PullOutcome::pending`], retried on the next pull, never an error.
//!   Conflict copies (`0007 2.bin`) are read as ordinary files. Applying is
//!   idempotent and order-free: Automerge holds a change whose dependencies
//!   have not arrived until they do.
//! - **Compaction and the tombstone rule.** A device compacts only its own
//!   folder, and only once it has read every file there: it writes
//!   `snapshot-NNNN.bin` holding the whole document (changes still waiting
//!   for their dependencies included), then deletes its own files numbered
//!   up to `NNNN`. The snapshot stands for those numbers: the device never
//!   reuses them, and deletes a file that reappears under one of them only
//!   when every change in it is already in the document; anything else is
//!   applied and kept until the next compaction holds it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use automerge::transaction::CommitOptions;
use automerge::{Change, ChangeHash};
use domain::workspace::WorkspaceId;
use zeroize::Zeroizing;

use crate::device::DeviceId;
use crate::envelope::{self, EnvelopeKind, Header, OpenError};
use crate::error::{AppStateError, io_error};
use crate::keys::DataKey;
use crate::sync_files::{ICLOUD_PLACEHOLDER_SUFFIX, SyncFiles, TEMP_SUFFIX};
use crate::workspace_state::{WORKSPACE_SCHEMA_VERSION, WorkspaceState};

/// Minimum width of a file number: `0001.bin`; `10000.bin` follows `9999.bin`.
const FILE_NUMBER_WIDTH: usize = 4;
const CHANGE_FILE_EXTENSION: &str = ".bin";
const SNAPSHOT_PREFIX: &str = "snapshot-";

/// When a burst of edits is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncConfig {
    /// Quiet time after the last edit before the burst is written.
    pub flush_window: Duration,
    /// Longest an edit waits while edits keep coming.
    pub max_batch_delay: Duration,
}

/// Why a file was skipped this time; none of these is corruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotArrived {
    /// The sync provider has the file in the cloud only; a download was requested.
    Placeholder,
    /// The file could not be read.
    Unreadable,
    /// The file does not authenticate under the data key: truncated,
    /// half-written by the sync provider, or sealed under another key.
    Undecryptable,
    /// The file authenticates but its content does not apply.
    Unparsable,
    /// The authenticated header names another device, kind or number than
    /// the file's place in the folder.
    Misplaced,
    /// Written by a newer build of the app.
    NewerVersion,
}

/// A file skipped by a pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFile {
    /// The file.
    pub path: PathBuf,
    /// Why it was skipped.
    pub reason: NotArrived,
}

/// What one pull did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullOutcome {
    /// Files applied.
    pub applied: usize,
    /// Files that have not arrived yet; the next pull retries them.
    pub pending: Vec<PendingFile>,
}

/// What a call to [`WorkspaceSync::compact`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionOutcome {
    /// A snapshot was written and the files it holds deleted.
    Compacted(Compaction),
    /// Nothing new since the last snapshot, or nothing written yet.
    NothingNew,
    /// Some of this device's own files have not arrived; compacting now
    /// could delete changes no snapshot holds. Retried after they arrive.
    WaitingForOwnFiles(Vec<PathBuf>),
}

/// A compaction that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compaction {
    /// The snapshot written.
    pub snapshot: PathBuf,
    /// The last batch number the snapshot covers.
    pub covers_through: u64,
    /// Files of this device deleted after the snapshot.
    pub removed: usize,
}

/// A workspace's document over its sync folder, for one device.
pub struct WorkspaceSync {
    files: Arc<dyn SyncFiles>,
    sync_root: PathBuf,
    key: DataKey,
    config: SyncConfig,
    state: WorkspaceState,
    next_sequence: u64,
    flushed_heads: Vec<ChangeHash>,
    burst: Option<Burst>,
    applied: BTreeSet<PathBuf>,
    /// This device's files whose every change a snapshot on disk holds.
    deletable: BTreeSet<PathBuf>,
    /// The last number this device's newest applied snapshot covers.
    own_coverage: u64,
}

#[derive(Debug, Clone, Copy)]
struct Burst {
    first: Instant,
    last: Instant,
}

/// What a file name in a device folder stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileRole {
    Batch(u64),
    Snapshot(u64),
    Placeholder(Option<u64>),
    Ignored,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            flush_window: Duration::from_secs(2),
            max_batch_delay: Duration::from_secs(10),
        }
    }
}

/// The sync folder of the workspace rooted at `workspace_root`.
#[must_use]
pub fn sync_folder(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".vaultday").join("sync")
}

/// The file at the sync folder's root naming the workspace's id: written once
/// by the device that starts the workspace's app state, read by every other.
pub const WORKSPACE_ID_FILE: &str = "workspace.id";

/// The workspace's id as its sync folder names it, `None` before any device
/// claimed one. The id is what the workspace's data key is stored under, so
/// it must not follow the manifest's name: two workspaces with one name, or a
/// renamed one, would share or lose their key.
///
/// # Errors
/// [`AppStateError::InvalidWorkspaceId`] for a file that holds no valid id;
/// file errors other than "not found".
pub fn read_workspace_id(
    files: &dyn SyncFiles,
    workspace_root: &Path,
) -> Result<Option<WorkspaceId>, AppStateError> {
    let path = sync_folder(workspace_root).join(WORKSPACE_ID_FILE);
    let bytes = match files.read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("read", &path, error)),
    };
    let text = String::from_utf8_lossy(&bytes).trim().to_owned();
    let is_valid = !text.is_empty()
        && text.len() <= 64
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !is_valid {
        return Err(AppStateError::InvalidWorkspaceId(text));
    }
    Ok(Some(WorkspaceId::new(text)))
}

/// The workspace's id: the one its sync folder names, or a new random one
/// (`ws-` and 24 hex digits) written there when no device has started the
/// workspace's app state.
///
/// # Errors
/// [`AppStateError::WorkspaceIdNotArrived`] when the folder holds another
/// device's app state or an evicted `workspace.id` (its download is
/// requested); as [`read_workspace_id`]; or the id file cannot be written.
pub fn claim_workspace_id(
    files: &dyn SyncFiles,
    workspace_root: &Path,
) -> Result<WorkspaceId, AppStateError> {
    if let Some(id) = read_workspace_id(files, workspace_root)? {
        return Ok(id);
    }
    let sync_root = sync_folder(workspace_root);
    let placeholder_name = format!(".{WORKSPACE_ID_FILE}{ICLOUD_PLACEHOLDER_SUFFIX}");
    let placeholder = sync_root.join(&placeholder_name);
    let is_id_evicted = list(files, &sync_root)?
        .iter()
        .any(|entry| entry.path.file_name() == Some(placeholder_name.as_ref()));
    if is_id_evicted {
        files
            .request_download(&placeholder)
            .map_err(|source| io_error("download", &placeholder, source))?;
    }
    if is_id_evicted || holds_app_state(files, workspace_root)? {
        return Err(AppStateError::WorkspaceIdNotArrived(sync_root));
    }
    let bytes: [u8; 12] = crate::random::bytes()?;
    let id = format!(
        "ws-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let path = sync_root.join(WORKSPACE_ID_FILE);
    files
        .write_atomic(&path, format!("{id}\n").as_bytes())
        .map_err(|source| io_error("write", &path, source))?;
    Ok(WorkspaceId::new(id))
}

/// Whether any device has written app state into the workspace's sync
/// folder, evicted placeholders included. A caller checks this before
/// minting a workspace key: a folder that holds files belongs to a key
/// another device minted, which may not have synchronised here yet.
///
/// # Errors
/// The sync folder or a device folder cannot be listed.
pub fn holds_app_state(
    files: &dyn SyncFiles,
    workspace_root: &Path,
) -> Result<bool, AppStateError> {
    for folder in list(files, &sync_folder(workspace_root))?
        .into_iter()
        .filter(|entry| entry.is_dir)
    {
        if !list(files, &folder.path)?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

impl WorkspaceSync {
    /// Starts the app state of a new workspace: claims id block 0 for this
    /// device and writes the first change file.
    ///
    /// # Errors
    /// [`AppStateError::WorkspaceExists`] when the sync folder already holds
    /// files; otherwise file or document errors.
    pub fn create(
        files: Arc<dyn SyncFiles>,
        workspace_root: &Path,
        device: DeviceId,
        key: DataKey,
        config: SyncConfig,
    ) -> Result<WorkspaceSync, AppStateError> {
        let sync_root = sync_folder(workspace_root);
        if holds_app_state(files.as_ref(), workspace_root)? {
            return Err(AppStateError::WorkspaceExists(sync_root));
        }
        let mut sync = WorkspaceSync::new(files, sync_root, device, key, config)?;
        sync.state.claim_creator_blocks()?;
        sync.state.doc_mut().commit_with(CommitOptions::default());
        sync.flush()?;
        Ok(sync)
    }

    /// Opens a workspace's app state: loads every file that has arrived from
    /// every device and continues this device's numbering.
    ///
    /// # Errors
    /// The sync folder or a device folder cannot be listed, or a file of
    /// this device superseded by its snapshot cannot be deleted.
    pub fn open(
        files: Arc<dyn SyncFiles>,
        workspace_root: &Path,
        device: DeviceId,
        key: DataKey,
        config: SyncConfig,
    ) -> Result<(WorkspaceSync, PullOutcome), AppStateError> {
        let sync_root = sync_folder(workspace_root);
        let mut sync = WorkspaceSync::new(files, sync_root, device, key, config)?;
        let outcome = sync.pull()?;
        sync.next_sequence = sync.highest_own_number()? + 1;
        sync.flushed_heads = sync.state.heads();
        Ok((sync, outcome))
    }

    /// The document.
    #[must_use]
    pub fn state(&self) -> &WorkspaceState {
        &self.state
    }

    /// The device this sync writes as.
    #[must_use]
    pub fn device_id(&self) -> &DeviceId {
        self.state.device_id()
    }

    /// The document heads: equal on two devices exactly when they converged.
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.state.heads()
    }

    /// Runs `edit` on the document and commits what it did as one change,
    /// which joins the current burst. When `edit` fails, nothing it did is kept.
    ///
    /// # Errors
    /// Whatever `edit` returns.
    pub fn edit<R>(
        &mut self,
        now: Instant,
        edit: impl FnOnce(&mut WorkspaceState) -> Result<R, AppStateError>,
    ) -> Result<R, AppStateError> {
        match edit(&mut self.state) {
            Ok(result) => {
                if self
                    .state
                    .doc_mut()
                    .commit_with(CommitOptions::default())
                    .is_some()
                {
                    let first = self.burst.map_or(now, |burst| burst.first);
                    self.burst = Some(Burst { first, last: now });
                }
                Ok(result)
            }
            Err(error) => {
                self.state.doc_mut().rollback();
                Err(error)
            }
        }
    }

    /// Whether edits are waiting for the next change file.
    #[must_use]
    pub fn has_unflushed_edits(&self) -> bool {
        self.burst.is_some()
    }

    /// Writes the burst when it has been quiet for the flush window or has
    /// waited the longest delay. Returns the file written.
    ///
    /// # Errors
    /// File errors; the edits stay queued for the next attempt.
    pub fn flush_if_due(&mut self, now: Instant) -> Result<Option<PathBuf>, AppStateError> {
        let Some(burst) = self.burst else {
            return Ok(None);
        };
        let is_quiet = now.saturating_duration_since(burst.last) >= self.config.flush_window;
        let has_waited = now.saturating_duration_since(burst.first) >= self.config.max_batch_delay;
        if is_quiet || has_waited {
            return self.flush();
        }
        Ok(None)
    }

    /// Writes every edit not yet on disk as one change file now (on
    /// backgrounding or quitting). Returns the file written, `None` when
    /// there was nothing to write.
    ///
    /// # Errors
    /// File errors; the edits stay queued for the next attempt.
    pub fn flush(&mut self) -> Result<Option<PathBuf>, AppStateError> {
        let doc = self.state.doc_mut();
        let actor = doc.get_actor().clone();
        let changes: Vec<Change> = doc
            .get_changes(&self.flushed_heads)
            .into_iter()
            .filter(|change| change.actor_id() == &actor)
            .collect();
        let heads = doc.get_heads();
        if changes.is_empty() {
            self.burst = None;
            self.flushed_heads = heads;
            return Ok(None);
        }
        let plaintext = encode_batch(&changes);
        let path = self.device_dir().join(number_name("", self.next_sequence));
        let header = self.header(EnvelopeKind::ChangeBatch, self.next_sequence);
        let sealed = envelope::seal(&self.key, &header, &plaintext)?;
        self.files
            .write_atomic(&path, &sealed)
            .map_err(|source| io_error("write", &path, source))?;
        self.burst = None;
        self.applied.insert(path.clone());
        self.next_sequence += 1;
        self.flushed_heads = heads;
        Ok(Some(path))
    }

    /// Applies every file that arrived since the last pull, from every device.
    ///
    /// # Errors
    /// The sync folder or a device folder cannot be listed.
    pub fn pull(&mut self) -> Result<PullOutcome, AppStateError> {
        let mut outcome = PullOutcome::default();
        for (device, folder) in self.device_folders()? {
            self.pull_device(&device, &folder, &mut outcome)?;
        }
        let own_dir = self.device_dir();
        let has_whole_own_history = !outcome
            .pending
            .iter()
            .any(|file| file.path.parent() == Some(own_dir.as_path()));
        self.state.set_has_whole_own_history(has_whole_own_history);
        self.remove_deletable_own_files()?;
        Ok(outcome)
    }

    /// Compacts this device's folder: flushes, writes a snapshot of the whole
    /// document and deletes this device's files the snapshot holds.
    ///
    /// # Errors
    /// File errors. A snapshot that was written stays valid when deleting
    /// the files it holds fails; the next pull deletes them.
    pub fn compact(&mut self) -> Result<CompactionOutcome, AppStateError> {
        self.flush()?;
        let unread = self.unread_own_files()?;
        if !unread.is_empty() {
            return Ok(CompactionOutcome::WaitingForOwnFiles(unread));
        }
        let covers_through = self.next_sequence - 1;
        if covers_through <= self.own_coverage {
            return Ok(CompactionOutcome::NothingNew);
        }
        // `save` keeps changes still waiting for their dependencies.
        let plaintext = Zeroizing::new(self.state.doc_mut().save());
        let path = self
            .device_dir()
            .join(number_name(SNAPSHOT_PREFIX, covers_through));
        let header = self.header(EnvelopeKind::WorkspaceSnapshot, covers_through);
        let sealed = envelope::seal(&self.key, &header, &plaintext)?;
        self.files
            .write_atomic(&path, &sealed)
            .map_err(|source| io_error("write", &path, source))?;
        // Every own file is applied (checked above), so the snapshot holds them all.
        for entry in list(self.files.as_ref(), &self.device_dir())? {
            let is_held = match role(&entry.path) {
                FileRole::Batch(number) => number <= covers_through,
                FileRole::Snapshot(number) => number < covers_through,
                FileRole::Placeholder(_) | FileRole::Ignored => false,
            };
            if is_held && self.applied.contains(&entry.path) {
                self.deletable.insert(entry.path);
            }
        }
        self.applied.insert(path.clone());
        self.own_coverage = covers_through;
        let removed = self.remove_deletable_own_files()?;
        Ok(CompactionOutcome::Compacted(Compaction {
            snapshot: path,
            covers_through,
            removed,
        }))
    }

    fn new(
        files: Arc<dyn SyncFiles>,
        sync_root: PathBuf,
        device: DeviceId,
        key: DataKey,
        config: SyncConfig,
    ) -> Result<WorkspaceSync, AppStateError> {
        let mut state = WorkspaceState::new(device)?;
        let flushed_heads = state.heads();
        Ok(WorkspaceSync {
            files,
            sync_root,
            key,
            config,
            state,
            next_sequence: 1,
            flushed_heads,
            burst: None,
            applied: BTreeSet::new(),
            deletable: BTreeSet::new(),
            own_coverage: 0,
        })
    }

    fn pull_device(
        &mut self,
        device: &DeviceId,
        folder: &Path,
        outcome: &mut PullOutcome,
    ) -> Result<(), AppStateError> {
        let mut snapshots = Vec::new();
        let mut batches = Vec::new();
        for entry in list(self.files.as_ref(), folder)? {
            match role(&entry.path) {
                FileRole::Snapshot(number) => snapshots.push((number, entry.path)),
                FileRole::Batch(number) => batches.push((number, entry.path)),
                FileRole::Placeholder(_) => {
                    let _ = self.files.request_download(&entry.path);
                    outcome.pending.push(PendingFile {
                        path: entry.path,
                        reason: NotArrived::Placeholder,
                    });
                }
                FileRole::Ignored => {}
            }
        }
        // Newest snapshot first: once it applies, older files are covered.
        snapshots.sort_by(|a, b| b.cmp(a));
        batches.sort();
        let files = snapshots
            .into_iter()
            .map(|(number, path)| (EnvelopeKind::WorkspaceSnapshot, number, path))
            .chain(
                batches
                    .into_iter()
                    .map(|(number, path)| (EnvelopeKind::ChangeBatch, number, path)),
            );
        for (kind, number, path) in files {
            if self.applied.contains(&path) {
                continue;
            }
            let is_own = device == self.state.device_id();
            match self.apply_file(device, kind, number, &path) {
                Ok(was_held) => {
                    if is_own && kind == EnvelopeKind::WorkspaceSnapshot {
                        self.own_coverage = self.own_coverage.max(number);
                    }
                    // An own batch under the snapshot's numbers whose every
                    // change the snapshot already held: the tombstone rule.
                    if is_own && was_held && number <= self.own_coverage {
                        self.deletable.insert(path.clone());
                    }
                    self.applied.insert(path);
                    outcome.applied += 1;
                }
                Err(reason) => outcome.pending.push(PendingFile { path, reason }),
            }
        }
        Ok(())
    }

    /// Applies one file. Returns whether the document already held every
    /// change of a batch before it was applied (always false for a snapshot).
    fn apply_file(
        &mut self,
        device: &DeviceId,
        kind: EnvelopeKind,
        number: u64,
        path: &Path,
    ) -> Result<bool, NotArrived> {
        let bytes = self.files.read(path).map_err(|_| NotArrived::Unreadable)?;
        let (header, plaintext) =
            envelope::open(&self.key, &bytes).map_err(|error| match error {
                OpenError::NewerFormat => NotArrived::NewerVersion,
                OpenError::Malformed | OpenError::Undecryptable => NotArrived::Undecryptable,
            })?;
        if header.kind != kind || &header.device != device || header.sequence != number {
            return Err(NotArrived::Misplaced);
        }
        if header.schema > WORKSPACE_SCHEMA_VERSION {
            return Err(NotArrived::NewerVersion);
        }
        let doc = self.state.doc_mut();
        match kind {
            EnvelopeKind::WorkspaceSnapshot => {
                // Not load-and-merge: a merge drops the snapshot's changes
                // that still wait for their dependencies.
                doc.load_incremental(&plaintext)
                    .map_err(|_| NotArrived::Unparsable)?;
                Ok(false)
            }
            _ => {
                let changes = decode_batch(&plaintext).ok_or(NotArrived::Unparsable)?;
                let was_held = changes
                    .iter()
                    .all(|change| doc.get_change_by_hash(&change.hash()).is_some());
                doc.apply_changes(changes)
                    .map_err(|_| NotArrived::Unparsable)?;
                Ok(was_held)
            }
        }
    }

    /// Deletes this device's files a snapshot on disk holds. Returns how many.
    fn remove_deletable_own_files(&mut self) -> Result<usize, AppStateError> {
        let mut removed = 0;
        for path in std::mem::take(&mut self.deletable) {
            self.files
                .remove(&path)
                .map_err(|source| io_error("remove", &path, source))?;
            self.applied.remove(&path);
            removed += 1;
        }
        Ok(removed)
    }

    /// This device's files not applied yet, placeholders included.
    fn unread_own_files(&self) -> Result<Vec<PathBuf>, AppStateError> {
        Ok(list(self.files.as_ref(), &self.device_dir())?
            .into_iter()
            .filter(|entry| match role(&entry.path) {
                FileRole::Batch(_) | FileRole::Snapshot(_) | FileRole::Placeholder(_) => {
                    !self.applied.contains(&entry.path)
                }
                FileRole::Ignored => false,
            })
            .map(|entry| entry.path)
            .collect())
    }

    /// The highest number this device used: any batch, placeholder or
    /// conflict copy in its folder, or what its snapshot covers.
    fn highest_own_number(&self) -> Result<u64, AppStateError> {
        let named = list(self.files.as_ref(), &self.device_dir())?
            .iter()
            .filter_map(|entry| match role(&entry.path) {
                FileRole::Batch(number) | FileRole::Snapshot(number) => Some(number),
                FileRole::Placeholder(number) => number,
                FileRole::Ignored => None,
            })
            .max()
            .unwrap_or(0);
        Ok(named.max(self.own_coverage))
    }

    fn device_folders(&self) -> Result<Vec<(DeviceId, PathBuf)>, AppStateError> {
        let mut folders: Vec<(DeviceId, PathBuf)> = list(self.files.as_ref(), &self.sync_root)?
            .into_iter()
            .filter(|entry| entry.is_dir)
            .filter_map(|entry| {
                let name = entry.path.file_name()?.to_str()?;
                Some((DeviceId::new(name).ok()?, entry.path.clone()))
            })
            .collect();
        folders.sort();
        Ok(folders)
    }

    fn header(&self, kind: EnvelopeKind, sequence: u64) -> Header {
        Header {
            kind,
            schema: WORKSPACE_SCHEMA_VERSION,
            device: self.device_id().clone(),
            sequence,
        }
    }

    fn device_dir(&self) -> PathBuf {
        self.sync_root.join(self.device_id().as_str())
    }
}

fn list(
    files: &dyn SyncFiles,
    dir: &Path,
) -> Result<Vec<crate::sync_files::SyncEntry>, AppStateError> {
    files
        .list(dir)
        .map_err(|source| io_error("list", dir, source))
}

/// Changes framed as a count, then each change's length and bytes, all u32 big-endian.
fn encode_batch(changes: &[Change]) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::new());
    bytes.extend_from_slice(&frame_len(changes.len()));
    for change in changes {
        let raw = change.raw_bytes();
        bytes.extend_from_slice(&frame_len(raw.len()));
        bytes.extend_from_slice(raw);
    }
    bytes
}

fn decode_batch(mut bytes: &[u8]) -> Option<Vec<Change>> {
    let count = take_len(&mut bytes)?;
    let mut changes = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let len = take_len(&mut bytes)?;
        let (raw, rest) = bytes.split_at_checked(len)?;
        changes.push(Change::from_bytes(raw.to_vec()).ok()?);
        bytes = rest;
    }
    bytes.is_empty().then_some(changes)
}

fn frame_len(len: usize) -> [u8; 4] {
    // A burst of changes never nears 4 GiB; the cap keeps the framing total.
    u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes()
}

fn take_len(bytes: &mut &[u8]) -> Option<usize> {
    let (len, rest) = bytes.split_first_chunk::<4>()?;
    *bytes = rest;
    usize::try_from(u32::from_be_bytes(*len)).ok()
}

/// `0007.bin`, or `snapshot-0007.bin` with the snapshot prefix.
fn number_name(prefix: &str, number: u64) -> String {
    format!("{prefix}{number:0FILE_NUMBER_WIDTH$}{CHANGE_FILE_EXTENSION}")
}

/// Reads a file name: `0007.bin` and its conflict copies (`0007 2.bin`,
/// `0007 (conflicted copy).bin`, `0007.sync-conflict-….bin`), the same with
/// the snapshot prefix, and iCloud placeholders (`.0007.bin.icloud`).
fn role(path: &Path) -> FileRole {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return FileRole::Ignored;
    };
    if name.ends_with(TEMP_SUFFIX) {
        return FileRole::Ignored;
    }
    if let Some(evicted) = name
        .strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(ICLOUD_PLACEHOLDER_SUFFIX))
    {
        let number = match role(Path::new(evicted)) {
            FileRole::Batch(number) | FileRole::Snapshot(number) => Some(number),
            _ => None,
        };
        return FileRole::Placeholder(number);
    }
    let Some(stem) = name.strip_suffix(CHANGE_FILE_EXTENSION) else {
        return FileRole::Ignored;
    };
    let (is_snapshot, numbered) = match stem.strip_prefix(SNAPSHOT_PREFIX) {
        Some(rest) => (true, rest),
        None => (false, stem),
    };
    let digits_end = numbered
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(numbered.len());
    let (digits, rest) = numbered.split_at(digits_end);
    let is_copy_suffix = rest.is_empty() || rest.starts_with(' ') || rest.starts_with('.');
    let Ok(number) = digits.parse::<u64>() else {
        return FileRole::Ignored;
    };
    // Exactly the digits `number_name` writes: `007` and `00007` are not `0007`.
    if !is_copy_suffix || number == 0 || format!("{number:0FILE_NUMBER_WIDTH$}") != digits {
        return FileRole::Ignored;
    }
    if is_snapshot {
        FileRole::Snapshot(number)
    } else {
        FileRole::Batch(number)
    }
}

impl std::fmt::Debug for WorkspaceSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceSync")
            .field("sync_root", &self.sync_root)
            .field("device", self.device_id())
            .field("next_sequence", &self.next_sequence)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::AutoCommit;

    use crate::sync_files::PlainSyncFiles;

    fn role_of(name: &str) -> FileRole {
        role(Path::new("sync/dev").join(name).as_path())
    }

    #[test]
    fn a_workspace_id_is_claimed_once_and_read_back() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            read_workspace_id(&PlainSyncFiles, temp.path()).unwrap(),
            None
        );
        let id = claim_workspace_id(&PlainSyncFiles, temp.path()).unwrap();
        assert!(
            id.as_str().starts_with("ws-") && id.as_str().len() == 27,
            "{id}"
        );
        assert_eq!(
            claim_workspace_id(&PlainSyncFiles, temp.path()).unwrap(),
            id
        );
        assert_eq!(
            read_workspace_id(&PlainSyncFiles, temp.path()).unwrap(),
            Some(id)
        );
        assert!(
            !holds_app_state(&PlainSyncFiles, temp.path()).unwrap(),
            "the id file is not app state"
        );

        let other = tempfile::tempdir().unwrap();
        PlainSyncFiles
            .write_atomic(
                &sync_folder(other.path()).join(WORKSPACE_ID_FILE),
                b"../Escape",
            )
            .unwrap();
        assert!(matches!(
            read_workspace_id(&PlainSyncFiles, other.path()),
            Err(AppStateError::InvalidWorkspaceId(_))
        ));
    }

    #[test]
    fn no_id_is_claimed_over_another_devices_app_state() {
        // The sync provider brought another device's change files before its
        // `workspace.id`: a claim here would replace that device's id, and
        // both would wait for a key minted under the other.
        let temp = tempfile::tempdir().unwrap();
        PlainSyncFiles
            .write_atomic(
                &sync_folder(temp.path()).join("dev-b").join("0001.bin"),
                b"sealed",
            )
            .unwrap();
        assert!(matches!(
            claim_workspace_id(&PlainSyncFiles, temp.path()),
            Err(AppStateError::WorkspaceIdNotArrived(_))
        ));
        assert_eq!(
            read_workspace_id(&PlainSyncFiles, temp.path()).unwrap(),
            None
        );

        // An evicted id file is on its way too.
        let evicted = tempfile::tempdir().unwrap();
        PlainSyncFiles
            .write_atomic(
                &sync_folder(evicted.path()).join(".workspace.id.icloud"),
                b"",
            )
            .unwrap();
        assert!(matches!(
            claim_workspace_id(&PlainSyncFiles, evicted.path()),
            Err(AppStateError::WorkspaceIdNotArrived(_))
        ));
        assert_eq!(
            read_workspace_id(&PlainSyncFiles, evicted.path()).unwrap(),
            None
        );
    }

    #[test]
    fn file_names_are_read_strictly() {
        assert_eq!(number_name("", 7), "0007.bin");
        assert_eq!(number_name(SNAPSHOT_PREFIX, 12), "snapshot-0012.bin");
        assert_eq!(role_of("0007.bin"), FileRole::Batch(7));
        assert_eq!(role_of("10000.bin"), FileRole::Batch(10_000));
        assert_eq!(role_of("snapshot-0012.bin"), FileRole::Snapshot(12));
        for ignored in [
            "007.bin",
            "00007.bin",
            "+007.bin",
            "0000.bin",
            "0007.tmp",
            "x.bin",
            "0007x.bin",
        ] {
            assert_eq!(role_of(ignored), FileRole::Ignored, "{ignored}");
        }
        assert_eq!(role_of(".0007.bin.4242-0.tmp"), FileRole::Ignored);
    }

    #[test]
    fn conflict_copies_and_placeholders_are_recognised() {
        assert_eq!(role_of("0007 2.bin"), FileRole::Batch(7));
        assert_eq!(
            role_of("0007 (conflicted copy 2026-10-23).bin"),
            FileRole::Batch(7)
        );
        assert_eq!(
            role_of("0007.sync-conflict-20261023-ABC.bin"),
            FileRole::Batch(7)
        );
        assert_eq!(role_of("snapshot-0012 2.bin"), FileRole::Snapshot(12));
        assert_eq!(role_of(".0007.bin.icloud"), FileRole::Placeholder(Some(7)));
        assert_eq!(role_of(".notes.icloud"), FileRole::Placeholder(None));
    }

    #[test]
    fn a_batch_frame_round_trips_and_refuses_trailing_bytes() {
        let mut doc = AutoCommit::new();
        automerge::transaction::Transactable::put(&mut doc, automerge::ROOT, "k", "v").unwrap();
        doc.commit();
        let changes = doc.get_changes(&[]);
        let encoded = encode_batch(&changes);
        assert_eq!(decode_batch(&encoded).unwrap().len(), 1);
        let mut longer = encoded.to_vec();
        longer.push(0);
        assert!(decode_batch(&longer).is_none());
        assert!(decode_batch(&encoded[..encoded.len() - 1]).is_none());
    }
}
