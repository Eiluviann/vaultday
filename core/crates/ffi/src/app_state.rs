//! App state over the bridge: the signed app-level document
//! ([`AppStateHandle`]) and one workspace's synced document
//! ([`WorkspaceStateHandle`]), both reached through the platform's
//! [`SecretStorePort`] and [`SyncFilesPort`].

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use app_state::{
    AppStateError, AppStateLoad, AppStateStore, Connection, ConnectionId, ConnectionKind, DeviceId,
    DeviceIdentity, FolderGrant, KeyboardScheme, NotArrived, PullOutcome, Source, SyncConfig,
    WorkspaceSync, claim_workspace_id, create_workspace_key, delete_workspace, holds_app_state,
    read_workspace_id, sync_folder, workspace_key,
};
use domain::session::{Session, SessionRecord, SessionState};
use domain::task::TaskId;
use domain::workspace::WorkspaceId;
use jiff::civil::Date;

use crate::CoreError;
use crate::files::{ForeignSyncFiles, SyncFilesPort, path_text};
use crate::secrets::{ForeignSecretStore, SecretStorePort};

/// What the user's connection reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionKindView {
    /// An Obsidian vault.
    Vault,
    /// A plain folder of markdown.
    Folder,
    /// A calendar.
    Calendar,
    /// A reminders list.
    ReminderList,
    /// A mail account.
    MailAccount,
}

/// One connection of the app-level document.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConnectionView {
    /// The connection's id, chosen by the app.
    pub id: String,
    /// What it reaches.
    pub kind: ConnectionKindView,
    /// What the user calls it.
    pub label: String,
}

/// A workspace reading through a connection: a folder, or a calendar it picked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SourceView {
    /// The workspace id.
    pub workspace: String,
    /// The connection id.
    pub connection: String,
    /// The path inside a vault, `/`-separated, empty for its root or for a calendar.
    pub subpath: String,
}

/// This device's grant to open a connection's folder.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderGrantView {
    /// The connection the grant opens.
    pub connection: String,
    /// The security-scoped bookmark.
    pub bookmark: Vec<u8>,
    /// The folder's path when the bookmark was made or last resolved.
    pub path: String,
}

/// The keyboard scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum KeyboardSchemeView {
    /// The platform's standard keys.
    Standard,
    /// Vim motions.
    Vim,
}

/// A snapshot that failed verification; the app reports it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RefusedSnapshotView {
    /// The file.
    pub path: String,
    /// Why, in words for the user.
    pub reason: String,
}

/// What loading the app-level document did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppStateLoadView {
    /// Snapshots verified and merged.
    pub merged: u32,
    /// Snapshots refused.
    pub refused: Vec<RefusedSnapshotView>,
    /// Placeholders whose download was requested.
    pub pending: Vec<String>,
}

/// A change file a pull skipped for now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PendingFileView {
    /// The file.
    pub path: String,
    /// Why: `placeholder`, `unreadable`, `undecryptable`, `unparsable`,
    /// `misplaced` or `newer-version`.
    pub reason: String,
}

/// What a pull of a workspace's change files did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PullView {
    /// Files applied.
    pub applied: u32,
    /// Files that have not arrived yet.
    pub pending: Vec<PendingFileView>,
}

/// One timer session of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SessionView {
    /// The session id, `ses-…`.
    pub id: String,
    /// The task it ran on.
    pub task: Option<String>,
    /// The story it belongs to.
    pub story: Option<String>,
    /// The meeting title when it came from a calendar event.
    pub meeting_title: Option<String>,
    /// RFC 3339 start.
    pub started_at: String,
    /// RFC 3339 stop; `None` while running or paused.
    pub stopped_at: Option<String>,
    /// Started, not stopped, and not counting.
    pub is_paused: bool,
    /// Time counted so far, pauses cut out, in whole seconds.
    pub elapsed_seconds: u64,
}

/// This device's app-level document over its snapshot folder.
#[derive(uniffi::Object)]
pub struct AppStateHandle {
    store: Mutex<AppStateStore>,
    secrets: ForeignSecretStore,
    last_load: Mutex<AppStateLoad>,
}

/// One workspace's synced app state on this device.
#[derive(uniffi::Object)]
pub struct WorkspaceStateHandle {
    pub(crate) sync: Mutex<WorkspaceSync>,
    last_pull: Mutex<PullOutcome>,
}

/// This device's id: the one its device-only signing key is stored under,
/// minted with the key on first launch. Publishes the device's public key.
///
/// # Errors
/// [`CoreError::SecretStore`] when the store fails.
#[uniffi::export]
pub fn load_device_id(secrets: Arc<dyn SecretStorePort>) -> Result<String, CoreError> {
    let identity = DeviceIdentity::load_or_create(&ForeignSecretStore::new(secrets))?;
    Ok(identity.device_id().as_str().to_owned())
}

/// Opens the app-level document in `dir` (inside the App Group container),
/// verifying and merging every device's signed snapshot there.
///
/// # Errors
/// [`CoreError::KeyNotArrived`] when snapshots exist but the app key has not
/// synchronised to this device yet; store or file failures otherwise.
#[uniffi::export]
pub fn open_app_state(
    files: Arc<dyn SyncFilesPort>,
    dir: String,
    secrets: Arc<dyn SecretStorePort>,
) -> Result<Arc<AppStateHandle>, CoreError> {
    let secrets = ForeignSecretStore::new(secrets);
    let identity = DeviceIdentity::load_or_create(&secrets)?;
    let files = Arc::new(ForeignSyncFiles::new(files));
    let (store, load) = AppStateStore::open(files, Path::new(&dir), &secrets, identity)?;
    Ok(Arc::new(AppStateHandle {
        store: Mutex::new(store),
        secrets,
        last_load: Mutex::new(load),
    }))
}

/// Starts the app state of a workspace no device has started: mints its
/// data key (synchronisable) and writes the first change file. The sync
/// folder is checked before the key is minted, so a Mac that already sees
/// another device's change files waits for that device's key instead of
/// minting a second one. Two devices starting the same workspace before
/// either's files or key synchronised still each mint one (see SECURITY.md).
///
/// # Errors
/// [`CoreError::Conflict`] when the workspace already has a key or change
/// files; store or file failures otherwise.
#[uniffi::export]
pub fn create_workspace_state(
    files: Arc<dyn SyncFilesPort>,
    workspace_root: String,
    workspace_id: String,
    device_id: String,
    secrets: Arc<dyn SecretStorePort>,
) -> Result<Arc<WorkspaceStateHandle>, CoreError> {
    let secrets = ForeignSecretStore::new(secrets);
    let device = DeviceId::new(device_id)?;
    let files = Arc::new(ForeignSyncFiles::new(files));
    let root = Path::new(&workspace_root);
    if holds_app_state(files.as_ref(), root)? {
        return Err(CoreError::Conflict(format!(
            "{} already holds app state from another device; open it once its key arrives",
            path_text(&sync_folder(root))
        )));
    }
    let key = create_workspace_key(&secrets, &WorkspaceId::new(workspace_id))?;
    let sync = WorkspaceSync::create(files, root, device, key, SyncConfig::default())?;
    Ok(Arc::new(WorkspaceStateHandle {
        sync: Mutex::new(sync),
        last_pull: Mutex::new(PullOutcome::default()),
    }))
}

/// Opens a workspace's app state with the data key the secret store holds,
/// applying every change file that has arrived.
///
/// # Errors
/// [`CoreError::KeyNotArrived`] when the key has not reached this device;
/// store or file failures otherwise.
#[uniffi::export]
pub fn open_workspace_state(
    files: Arc<dyn SyncFilesPort>,
    workspace_root: String,
    workspace_id: String,
    device_id: String,
    secrets: Arc<dyn SecretStorePort>,
) -> Result<Arc<WorkspaceStateHandle>, CoreError> {
    let secrets = ForeignSecretStore::new(secrets);
    let workspace = WorkspaceId::new(workspace_id);
    let Some(key) = workspace_key(&secrets, &workspace)? else {
        return Err(CoreError::KeyNotArrived(format!(
            "the key of workspace {workspace} has not reached this device yet"
        )));
    };
    let (sync, outcome) = WorkspaceSync::open(
        Arc::new(ForeignSyncFiles::new(files)),
        Path::new(&workspace_root),
        DeviceId::new(device_id)?,
        key,
        SyncConfig::default(),
    )?;
    Ok(Arc::new(WorkspaceStateHandle {
        sync: Mutex::new(sync),
        last_pull: Mutex::new(outcome),
    }))
}

/// The workspace's id as its sync folder names it (`.vaultday/sync/workspace.id`),
/// `None` before any device started its app state.
///
/// # Errors
/// [`CoreError::AppState`] for an invalid id file, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn workspace_state_id(
    files: Arc<dyn SyncFilesPort>,
    workspace_root: String,
) -> Result<Option<String>, CoreError> {
    let id = read_workspace_id(&ForeignSyncFiles::new(files), Path::new(&workspace_root))?;
    Ok(id.map(|id| id.as_str().to_owned()))
}

/// The workspace's id, claimed with a new random one when no device has:
/// the id the workspace's data key is stored under. It never follows the
/// manifest's name, so a rename or two workspaces of one name keep their keys apart.
///
/// # Errors
/// As [`workspace_state_id`], or the id file cannot be written.
#[uniffi::export]
pub fn claim_workspace_state_id(
    files: Arc<dyn SyncFilesPort>,
    workspace_root: String,
) -> Result<String, CoreError> {
    let id = claim_workspace_id(&ForeignSyncFiles::new(files), Path::new(&workspace_root))?;
    Ok(id.as_str().to_owned())
}

/// Destroys a removed workspace's data key on every device; its change files
/// become unreadable by anyone.
///
/// # Errors
/// [`CoreError::SecretStore`] when the store fails.
#[uniffi::export]
pub fn delete_workspace_keys(
    secrets: Arc<dyn SecretStorePort>,
    workspace_id: String,
) -> Result<(), CoreError> {
    delete_workspace(
        &ForeignSecretStore::new(secrets),
        &WorkspaceId::new(workspace_id),
    )?;
    Ok(())
}

#[uniffi::export]
impl AppStateHandle {
    /// This device's id.
    pub fn device_id(&self) -> String {
        lock(&self.store).identity().device_id().as_str().to_owned()
    }

    /// What the last open or reload did.
    pub fn last_load(&self) -> AppStateLoadView {
        load_view(&lock(&self.last_load))
    }

    /// Verifies and merges every snapshot again.
    ///
    /// # Errors
    /// Folder or store failures.
    pub fn reload(&self) -> Result<AppStateLoadView, CoreError> {
        let load = lock(&self.store).reload(&self.secrets)?;
        let view = load_view(&load);
        *lock(&self.last_load) = load;
        Ok(view)
    }

    /// Writes this device's signed snapshot; returns its path.
    ///
    /// # Errors
    /// File failures.
    pub fn save(&self) -> Result<String, CoreError> {
        Ok(path_text(&lock(&self.store).save()?))
    }

    /// Every connection.
    pub fn connections(&self) -> Vec<ConnectionView> {
        lock(&self.store)
            .state()
            .connections()
            .into_iter()
            .map(connection_view)
            .collect()
    }

    /// Adds or replaces a connection.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn put_connection(&self, connection: ConnectionView) -> Result<(), CoreError> {
        let connection = Connection {
            id: ConnectionId::new(connection.id),
            kind: connection_kind(connection.kind),
            label: connection.label,
        };
        lock(&self.store).edit(|state| state.put_connection(&connection))?;
        Ok(())
    }

    /// Removes a connection, its sources and this device's grant for it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_connection(&self, id: String) -> Result<(), CoreError> {
        let id = ConnectionId::new(id);
        lock(&self.store).edit(|state| {
            state.remove_folder_grant(&id)?;
            state.remove_connection(&id)
        })?;
        Ok(())
    }

    /// The sources of one workspace.
    pub fn sources(&self, workspace: String) -> Vec<SourceView> {
        lock(&self.store)
            .state()
            .sources(&WorkspaceId::new(workspace))
            .into_iter()
            .map(|source| SourceView {
                workspace: source.workspace.as_str().to_owned(),
                connection: source.vault.as_str().to_owned(),
                subpath: source.subpath,
            })
            .collect()
    }

    /// Adds a source.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn add_source(&self, source: SourceView) -> Result<(), CoreError> {
        let source = core_source(source);
        lock(&self.store).edit(|state| state.add_source(&source))?;
        Ok(())
    }

    /// Removes a source.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_source(&self, source: SourceView) -> Result<(), CoreError> {
        let source = core_source(source);
        lock(&self.store).edit(|state| state.remove_source(&source))?;
        Ok(())
    }

    /// This device's folder grants.
    pub fn folder_grants(&self) -> Vec<FolderGrantView> {
        lock(&self.store)
            .state()
            .folder_grants()
            .into_iter()
            .map(|grant| FolderGrantView {
                connection: grant.connection.as_str().to_owned(),
                bookmark: grant.bookmark,
                path: grant.path,
            })
            .collect()
    }

    /// Stores or refreshes this device's grant for a connection.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn put_folder_grant(&self, grant: FolderGrantView) -> Result<(), CoreError> {
        let grant = FolderGrant {
            connection: ConnectionId::new(grant.connection),
            bookmark: grant.bookmark,
            path: grant.path,
        };
        lock(&self.store).edit(|state| state.put_folder_grant(&grant))?;
        Ok(())
    }

    /// Forgets this device's grant for a connection.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_folder_grant(&self, connection: String) -> Result<(), CoreError> {
        let connection = ConnectionId::new(connection);
        lock(&self.store).edit(|state| state.remove_folder_grant(&connection))?;
        Ok(())
    }

    /// The keyboard scheme.
    pub fn keyboard_scheme(&self) -> KeyboardSchemeView {
        match lock(&self.store).state().keyboard_scheme() {
            KeyboardScheme::Standard => KeyboardSchemeView::Standard,
            KeyboardScheme::Vim => KeyboardSchemeView::Vim,
        }
    }

    /// Sets the keyboard scheme.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_keyboard_scheme(&self, scheme: KeyboardSchemeView) -> Result<(), CoreError> {
        let scheme = match scheme {
            KeyboardSchemeView::Standard => KeyboardScheme::Standard,
            KeyboardSchemeView::Vim => KeyboardScheme::Vim,
        };
        lock(&self.store).edit(|state| state.set_keyboard_scheme(scheme))?;
        Ok(())
    }

    /// Whether hovering selects on this device.
    pub fn hover_selects(&self) -> bool {
        lock(&self.store).state().hover_selects()
    }

    /// Sets hover selects for this device.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_hover_selects(&self, is_on: bool) -> Result<(), CoreError> {
        lock(&self.store).edit(|state| state.set_hover_selects(is_on))?;
        Ok(())
    }
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// This device's id.
    pub fn device_id(&self) -> String {
        lock(&self.sync).device_id().as_str().to_owned()
    }

    /// What the open, or the last pull, did.
    pub fn last_pull(&self) -> PullView {
        pull_view(&lock(&self.last_pull))
    }

    /// The task ids planned for `date` (`YYYY-MM-DD`), in order.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date.
    pub fn day_order(&self, date: String) -> Result<Vec<String>, CoreError> {
        let date = parse_date(&date)?;
        Ok(lock(&self.sync)
            .state()
            .day_order(date)
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect())
    }

    /// Replaces the order of `date`; written with the current burst.
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed date, or the document refused the edit.
    pub fn set_day_order(&self, date: String, task_ids: Vec<String>) -> Result<(), CoreError> {
        let date = parse_date(&date)?;
        let order: Vec<TaskId> = task_ids.into_iter().map(TaskId::new).collect();
        lock(&self.sync).edit(Instant::now(), |state| state.set_day_order(date, &order))?;
        Ok(())
    }

    /// Every session, by id.
    pub fn sessions(&self) -> Vec<SessionView> {
        lock(&self.sync)
            .state()
            .sessions()
            .into_iter()
            .map(session_view)
            .collect()
    }

    /// Writes the pending burst as one change file; returns its path, `None`
    /// when nothing was waiting.
    ///
    /// # Errors
    /// File failures.
    pub fn flush(&self) -> Result<Option<String>, CoreError> {
        Ok(lock(&self.sync).flush()?.as_deref().map(path_text))
    }

    /// Applies every change file that arrived since the last pull.
    ///
    /// # Errors
    /// The sync folder cannot be listed.
    pub fn pull(&self) -> Result<PullView, CoreError> {
        let outcome = lock(&self.sync).pull()?;
        let view = pull_view(&outcome);
        *lock(&self.last_pull) = outcome;
        Ok(view)
    }

    /// The document heads as hex: equal on two devices exactly when they converged.
    pub fn heads(&self) -> Vec<String> {
        lock(&self.sync)
            .heads()
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

impl std::fmt::Debug for FolderGrantView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FolderGrantView")
            .field("connection", &self.connection)
            .field("bookmark", &format_args!("{} bytes", self.bookmark.len()))
            .field("path", &self.path)
            .finish()
    }
}

impl From<AppStateError> for CoreError {
    fn from(error: AppStateError) -> Self {
        let message = error.to_string();
        match error {
            AppStateError::SecretStore(_) => CoreError::SecretStore(message),
            AppStateError::KeyNotArrived(_) | AppStateError::WorkspaceIdNotArrived(_) => {
                CoreError::KeyNotArrived(message)
            }
            AppStateError::KeyExists(_) | AppStateError::WorkspaceExists(_) => {
                CoreError::Conflict(message)
            }
            AppStateError::Io { .. } => CoreError::FileAccess(message),
            AppStateError::InvalidDeviceId(_)
            | AppStateError::InvalidSetting(_)
            | AppStateError::InvalidMailAccount(_) => CoreError::InvalidInput(message),
            AppStateError::UnknownSignal(_) => CoreError::NotFound(message),
            _ => CoreError::AppState(message),
        }
    }
}

// A panic while a handle is locked leaves the document as the last finished
// call left it, so a poisoned lock is still safe to use.
impl AppStateHandle {
    /// Reads from the document.
    pub(crate) fn read_state<T>(&self, read: impl FnOnce(&app_state::AppState) -> T) -> T {
        read(lock(&self.store).state())
    }

    /// The secret store the handle was opened with.
    pub(crate) fn secrets(&self) -> &ForeignSecretStore {
        &self.secrets
    }

    /// Edits the document.
    pub(crate) fn edit_state(
        &self,
        edit: impl FnOnce(&mut app_state::AppState) -> Result<(), AppStateError>,
    ) -> Result<(), CoreError> {
        lock(&self.store).edit(edit)?;
        Ok(())
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn parse_date(text: &str) -> Result<Date, CoreError> {
    text.parse()
        .map_err(|_| CoreError::InvalidDate(format!("{text} is not a date; expected YYYY-MM-DD")))
}

fn connection_view(connection: Connection) -> ConnectionView {
    ConnectionView {
        id: connection.id.as_str().to_owned(),
        kind: match connection.kind {
            ConnectionKind::Vault => ConnectionKindView::Vault,
            ConnectionKind::Folder => ConnectionKindView::Folder,
            ConnectionKind::Calendar => ConnectionKindView::Calendar,
            ConnectionKind::ReminderList => ConnectionKindView::ReminderList,
            ConnectionKind::MailAccount => ConnectionKindView::MailAccount,
        },
        label: connection.label,
    }
}

fn connection_kind(kind: ConnectionKindView) -> ConnectionKind {
    match kind {
        ConnectionKindView::Vault => ConnectionKind::Vault,
        ConnectionKindView::Folder => ConnectionKind::Folder,
        ConnectionKindView::Calendar => ConnectionKind::Calendar,
        ConnectionKindView::ReminderList => ConnectionKind::ReminderList,
        ConnectionKindView::MailAccount => ConnectionKind::MailAccount,
    }
}

fn core_source(source: SourceView) -> Source {
    Source {
        workspace: WorkspaceId::new(source.workspace),
        vault: ConnectionId::new(source.connection),
        subpath: source.subpath,
    }
}

fn load_view(load: &AppStateLoad) -> AppStateLoadView {
    AppStateLoadView {
        merged: u32::try_from(load.merged).unwrap_or(u32::MAX),
        refused: load
            .refused
            .iter()
            .map(|refused| RefusedSnapshotView {
                path: path_text(&refused.path),
                reason: refused.reason.to_string(),
            })
            .collect(),
        pending: load.pending.iter().map(|path| path_text(path)).collect(),
    }
}

fn pull_view(outcome: &PullOutcome) -> PullView {
    PullView {
        applied: u32::try_from(outcome.applied).unwrap_or(u32::MAX),
        pending: outcome
            .pending
            .iter()
            .map(|file| PendingFileView {
                path: path_text(&file.path),
                reason: not_arrived_label(file.reason).to_owned(),
            })
            .collect(),
    }
}

fn not_arrived_label(reason: NotArrived) -> &'static str {
    match reason {
        NotArrived::Placeholder => "placeholder",
        NotArrived::Unreadable => "unreadable",
        NotArrived::Undecryptable => "undecryptable",
        NotArrived::Unparsable => "unparsable",
        NotArrived::Misplaced => "misplaced",
        NotArrived::NewerVersion => "newer-version",
    }
}

pub(crate) fn session_view(record: SessionRecord) -> SessionView {
    let (is_paused, elapsed_seconds) = match Session::from_record(record.clone()) {
        Ok(session) => (
            session.state() == SessionState::Paused,
            session.elapsed(jiff::Timestamp::now()).as_secs(),
        ),
        Err(_) => (false, 0),
    };
    SessionView {
        id: record.id.as_str().to_owned(),
        task: record.target.task.map(|task| task.as_str().to_owned()),
        story: record.target.story.map(|story| story.as_str().to_owned()),
        meeting_title: match record.source {
            domain::session::SessionSource::Meeting { title } => Some(title),
            domain::session::SessionSource::Timer => None,
        },
        started_at: record.started_at.to_string(),
        stopped_at: record.stopped_at.map(|at| at.to_string()),
        is_paused,
        elapsed_seconds,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::files::plain_sync_files;
    use crate::secrets::{SecretId, SecretItem, SecretKind, SecretStoreFailure};

    /// Two devices: one shared partition for synchronisable items, one
    /// private partition each, as iCloud Keychain gives two Macs.
    #[derive(Clone, Default)]
    pub(crate) struct Keychain {
        synchronised: Arc<Mutex<BTreeMap<(SecretKind, String), SecretItem>>>,
        device_only: Arc<Mutex<BTreeMap<(SecretKind, String), SecretItem>>>,
    }

    impl Keychain {
        fn another_device(&self) -> Keychain {
            Keychain {
                synchronised: Arc::clone(&self.synchronised),
                device_only: Arc::default(),
            }
        }

        fn partition(
            &self,
            kind: SecretKind,
        ) -> MutexGuard<'_, BTreeMap<(SecretKind, String), SecretItem>> {
            if crate::secrets::secret_kind_is_synchronisable(kind) {
                lock(&self.synchronised)
            } else {
                lock(&self.device_only)
            }
        }
    }

    impl SecretStorePort for Keychain {
        fn get(&self, id: SecretId) -> Result<Option<SecretItem>, SecretStoreFailure> {
            Ok(self.partition(id.kind).get(&(id.kind, id.name)).cloned())
        }
        fn put(&self, item: SecretItem) -> Result<(), SecretStoreFailure> {
            self.partition(item.id.kind)
                .insert((item.id.kind, item.id.name.clone()), item);
            Ok(())
        }
        fn add(&self, item: SecretItem) -> Result<bool, SecretStoreFailure> {
            let mut partition = self.partition(item.id.kind);
            let key = (item.id.kind, item.id.name.clone());
            if partition.contains_key(&key) {
                return Ok(false);
            }
            partition.insert(key, item);
            Ok(true)
        }
        fn delete(&self, id: SecretId) -> Result<(), SecretStoreFailure> {
            self.partition(id.kind).remove(&(id.kind, id.name));
            Ok(())
        }
        fn list(&self, kind: SecretKind) -> Result<Vec<SecretId>, SecretStoreFailure> {
            Ok(self
                .partition(kind)
                .values()
                .filter(|item| item.id.kind == kind)
                .map(|item| item.id.clone())
                .collect())
        }
    }

    fn text(path: &Path) -> String {
        path_text(path)
    }

    #[test]
    fn two_devices_converge_through_the_bridge() {
        let temp = tempfile::tempdir().unwrap();
        let root = text(temp.path());
        let mac = Arc::new(Keychain::default());
        let other = Arc::new(mac.another_device());
        let mac_id = load_device_id(mac.clone()).unwrap();
        let other_id = load_device_id(other.clone()).unwrap();
        assert_ne!(mac_id, other_id);

        let first = create_workspace_state(
            plain_sync_files(),
            root.clone(),
            "mock-studio".to_owned(),
            mac_id,
            mac.clone(),
        )
        .unwrap();
        first
            .set_day_order("2026-10-22".to_owned(), vec!["task-1".to_owned()])
            .unwrap();
        assert!(first.flush().unwrap().is_some());

        let second = open_workspace_state(
            plain_sync_files(),
            root.clone(),
            "mock-studio".to_owned(),
            other_id,
            other.clone(),
        )
        .unwrap();
        assert_eq!(
            second.day_order("2026-10-22".to_owned()).unwrap(),
            ["task-1"]
        );
        second
            .set_day_order(
                "2026-10-22".to_owned(),
                vec!["task-2".to_owned(), "task-1".to_owned()],
            )
            .unwrap();
        second.flush().unwrap();
        assert!(first.pull().unwrap().applied >= 1);
        assert_eq!(first.heads(), second.heads());

        let fresh = Arc::new(Keychain::default());
        let fresh_id = load_device_id(fresh.clone()).unwrap();
        let refused = create_workspace_state(
            plain_sync_files(),
            root.clone(),
            "mock-studio".to_owned(),
            fresh_id,
            fresh.clone(),
        );
        assert!(matches!(refused, Err(CoreError::Conflict(_))));
        assert!(
            fresh.list(SecretKind::WorkspaceDataKey).unwrap().is_empty(),
            "no second key is minted over another device's files"
        );

        delete_workspace_keys(other.clone(), "mock-studio".to_owned()).unwrap();
        let refused = open_workspace_state(
            plain_sync_files(),
            root,
            "mock-studio".to_owned(),
            first.device_id(),
            mac,
        );
        assert!(matches!(refused, Err(CoreError::KeyNotArrived(_))));
    }

    #[test]
    fn the_rhythm_ai_and_workspace_choices_survive_a_reopen() {
        use crate::onboarding::FeatureScopeModeView;
        use crate::preferences::*;

        let temp = tempfile::tempdir().unwrap();
        let dir = text(&temp.path().join("app"));
        let keychain = Arc::new(Keychain::default());
        let app = open_app_state(plain_sync_files(), dir.clone(), keychain.clone()).unwrap();
        app.set_break_pattern(BreakPatternView::TwentyFiveFive)
            .unwrap();
        app.set_rollover_position(ListPositionView::Bottom).unwrap();
        app.set_new_task_position(ListPositionView::Top).unwrap();
        app.set_shortcut_caps(ShortcutCapsView::Instant).unwrap();
        app.set_workload_threshold_minutes(390).unwrap();
        app.set_ritual_minute("planning".to_owned(), Some(525))
            .unwrap();
        app.set_ritual_day("weekly-review".to_owned(), Some(WeekdayView::Friday))
            .unwrap();
        app.set_ai_scope(
            "level".to_owned(),
            AiScopeView {
                mode: FeatureScopeModeView::PerSphere,
                targets: vec!["work".to_owned()],
                tiers: vec![AiTierEntryView {
                    target: "work".to_owned(),
                    tier: AiTierView::Insight,
                }],
            },
        )
        .unwrap();
        app.put_provider_key(ProviderKeyView {
            id: "key-1".to_owned(),
            provider: ProviderKindView::Claude,
            name: "Work key".to_owned(),
            scope: ProviderKeyScopeView::Default,
            targets: Vec::new(),
            insight_model: None,
            glance_model: None,
            endpoint: None,
        })
        .unwrap();
        app.set_last_single_workspace(Some("ws-1".to_owned()))
            .unwrap();
        assert!(matches!(
            app.set_workload_threshold_minutes(0),
            Err(CoreError::InvalidInput(_))
        ));
        assert!(matches!(
            app.set_ritual_minute("planning".to_owned(), Some(1440)),
            Err(CoreError::InvalidInput(_))
        ));
        app.save().unwrap();

        let reopened = open_app_state(plain_sync_files(), dir, keychain).unwrap();
        assert_eq!(reopened.break_pattern(), BreakPatternView::TwentyFiveFive);
        assert_eq!(reopened.rollover_position(), ListPositionView::Bottom);
        assert_eq!(reopened.new_task_position(), ListPositionView::Top);
        assert_eq!(reopened.shortcut_caps(), ShortcutCapsView::Instant);
        assert_eq!(reopened.workload_threshold_minutes(), 390);
        assert_eq!(reopened.ritual_minute("planning".to_owned()), Some(525));
        assert_eq!(
            reopened.ritual_day("weekly-review".to_owned()),
            Some(WeekdayView::Friday)
        );
        assert_eq!(reopened.ai_scope("level".to_owned()).targets, ["work"]);
        assert_eq!(reopened.provider_keys().len(), 1);
        assert_eq!(reopened.last_single_workspace().as_deref(), Some("ws-1"));
    }

    #[test]
    fn the_app_document_keeps_connections_sources_and_grants() {
        let temp = tempfile::tempdir().unwrap();
        let dir = text(&temp.path().join("app"));
        let keychain = Arc::new(Keychain::default());
        let app = open_app_state(plain_sync_files(), dir.clone(), keychain.clone()).unwrap();
        app.put_connection(ConnectionView {
            id: "con-1".to_owned(),
            kind: ConnectionKindView::Folder,
            label: "Mock vault".to_owned(),
        })
        .unwrap();
        app.add_source(SourceView {
            workspace: "mock-studio".to_owned(),
            connection: "con-1".to_owned(),
            subpath: String::new(),
        })
        .unwrap();
        app.put_folder_grant(FolderGrantView {
            connection: "con-1".to_owned(),
            bookmark: vec![1, 2, 3],
            path: "/tmp/mock".to_owned(),
        })
        .unwrap();
        app.set_keyboard_scheme(KeyboardSchemeView::Standard)
            .unwrap();
        app.save().unwrap();

        let reopened = open_app_state(plain_sync_files(), dir, keychain).unwrap();
        assert_eq!(reopened.last_load().merged, 1);
        assert_eq!(reopened.connections().len(), 1);
        assert_eq!(reopened.sources("mock-studio".to_owned()).len(), 1);
        assert_eq!(reopened.folder_grants()[0].bookmark, [1, 2, 3]);
        assert_eq!(reopened.keyboard_scheme(), KeyboardSchemeView::Standard);

        reopened.remove_connection("con-1".to_owned()).unwrap();
        assert!(reopened.folder_grants().is_empty());
        assert!(reopened.sources("mock-studio".to_owned()).is_empty());
    }

    #[test]
    fn the_workspace_id_is_claimed_in_the_sync_folder_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = text(temp.path());
        assert_eq!(
            workspace_state_id(plain_sync_files(), root.clone()).unwrap(),
            None
        );
        let id = claim_workspace_state_id(plain_sync_files(), root.clone()).unwrap();
        assert!(id.starts_with("ws-"), "{id}");
        assert_eq!(
            claim_workspace_state_id(plain_sync_files(), root.clone()).unwrap(),
            id
        );
        assert_eq!(
            workspace_state_id(plain_sync_files(), root).unwrap(),
            Some(id)
        );
    }

    #[test]
    fn a_malformed_date_and_device_id_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let keychain = Arc::new(Keychain::default());
        let refused = create_workspace_state(
            plain_sync_files(),
            text(temp.path()),
            "w".to_owned(),
            "not/a device".to_owned(),
            keychain.clone(),
        );
        assert!(matches!(refused, Err(CoreError::InvalidInput(_))));
        let device = load_device_id(keychain.clone()).unwrap();
        let state = create_workspace_state(
            plain_sync_files(),
            text(temp.path()),
            "w2".to_owned(),
            device,
            keychain,
        )
        .unwrap();
        assert!(matches!(
            state.day_order("tomorrow".to_owned()),
            Err(CoreError::InvalidDate(_))
        ));
    }
}
