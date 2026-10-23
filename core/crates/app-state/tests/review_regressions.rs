//! Regressions found in review: a run that cannot read some files yet must
//! not lose them through compaction, must keep queued changes through a
//! snapshot, must not mint an id twice, and must not mint a second app key.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use app_state::{
    AppStateError, AppStateStore, CompactionOutcome, DataKey, DeviceId, DeviceIdentity,
    InMemorySecretStore, PlainSyncFiles, SyncConfig, SyncEntry, SyncFiles, WorkspaceSync,
    create_workspace_key,
};
use domain::task::TaskId;
use domain::workspace::WorkspaceId;
use jiff::civil::{Date, date};

const DAY: Date = date(2026, 10, 22);

/// Plain files, except that reads of the listed paths fail as if not
/// downloaded yet; remembers which placeholders a download was asked for.
#[derive(Debug, Default)]
struct NotYetDownloaded {
    failing: Mutex<Vec<PathBuf>>,
    download_requests: Mutex<Vec<PathBuf>>,
}

impl NotYetDownloaded {
    fn failing(path: &Path) -> Arc<NotYetDownloaded> {
        let files = NotYetDownloaded::default();
        files.failing.lock().unwrap().push(path.to_path_buf());
        Arc::new(files)
    }

    fn arrive(&self) {
        self.failing.lock().unwrap().clear();
    }
}

impl SyncFiles for NotYetDownloaded {
    fn list(&self, dir: &Path) -> io::Result<Vec<SyncEntry>> {
        PlainSyncFiles.list(dir)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if self
            .failing
            .lock()
            .unwrap()
            .iter()
            .any(|failing| failing == path)
        {
            return Err(io::Error::other("not downloaded yet"));
        }
        PlainSyncFiles.read(path)
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        PlainSyncFiles.write_atomic(path, bytes)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        PlainSyncFiles.remove(path)
    }
    fn request_download(&self, placeholder: &Path) -> io::Result<()> {
        self.download_requests
            .lock()
            .unwrap()
            .push(placeholder.to_path_buf());
        Ok(())
    }
}

struct Setup {
    secrets: InMemorySecretStore,
    device: DeviceId,
    key: DataKey,
}

fn setup() -> Setup {
    let secrets = InMemorySecretStore::new();
    let device = DeviceIdentity::load_or_create(&secrets)
        .unwrap()
        .device_id()
        .clone();
    let key = create_workspace_key(&secrets, &WorkspaceId::new("synthetic")).unwrap();
    Setup {
        secrets,
        device,
        key,
    }
}

fn open(files: Arc<dyn SyncFiles>, root: &Path, device: &DeviceId, key: &DataKey) -> WorkspaceSync {
    WorkspaceSync::open(
        files,
        root,
        device.clone(),
        key.clone(),
        SyncConfig::default(),
    )
    .unwrap()
    .0
}

fn plain() -> Arc<dyn SyncFiles> {
    Arc::new(PlainSyncFiles)
}

fn plan(sync: &mut WorkspaceSync, order: &[&str]) -> PathBuf {
    let order: Vec<TaskId> = order.iter().map(|id| TaskId::new(*id)).collect();
    sync.edit(Instant::now(), |state| state.set_day_order(DAY, &order))
        .unwrap();
    sync.flush().unwrap().unwrap()
}

#[test]
fn compaction_never_deletes_an_own_file_it_could_not_read() {
    let temp = tempfile::tempdir().unwrap();
    let Setup { device, key, .. } = setup();
    let mut first_run = WorkspaceSync::create(
        plain(),
        temp.path(),
        device.clone(),
        key.clone(),
        SyncConfig::default(),
    )
    .unwrap();
    let unread = plan(&mut first_run, &["task-1"]);
    drop(first_run);

    let files = NotYetDownloaded::failing(&unread);
    let mut second_run = open(files.clone(), temp.path(), &device, &key);
    second_run
        .edit(Instant::now(), |state| {
            state.set_ritual_answer(DAY, "shutdown", "wins", "x")
        })
        .unwrap();
    assert!(matches!(
        second_run.compact().unwrap(),
        CompactionOutcome::WaitingForOwnFiles(_)
    ));
    files.arrive();
    second_run.pull().unwrap();
    assert!(unread.exists(), "a file no snapshot holds is kept");
    drop(second_run);

    let third_run = open(plain(), temp.path(), &device, &key);
    assert_eq!(third_run.state().day_order(DAY), [TaskId::new("task-1")]);
}

#[test]
fn a_snapshot_keeps_changes_whose_dependencies_have_not_arrived() {
    let temp = tempfile::tempdir().unwrap();
    let mac = setup();
    let phone_secrets = mac.secrets.for_another_device();
    let phone = DeviceIdentity::load_or_create(&phone_secrets)
        .unwrap()
        .device_id()
        .clone();
    let mut mac_sync = WorkspaceSync::create(
        plain(),
        temp.path(),
        mac.device.clone(),
        mac.key.clone(),
        SyncConfig::default(),
    )
    .unwrap();
    let mac_file = plan(&mut mac_sync, &["task-1"]);
    let mut phone_sync = open(plain(), temp.path(), &phone, &mac.key);
    plan(&mut phone_sync, &["task-1", "task-2"]);
    drop(phone_sync);

    // The phone's change depends on the Mac's file, which this run cannot read.
    let files = NotYetDownloaded::failing(&mac_file);
    let mut phone_again = open(files, temp.path(), &phone, &mac.key);
    let _ = phone_again.compact().unwrap();
    drop(phone_again);

    let phone_later = open(plain(), temp.path(), &phone, &mac.key);
    assert!(
        phone_later
            .state()
            .day_order(DAY)
            .contains(&TaskId::new("task-2")),
        "{:?}",
        phone_later.state().day_order(DAY)
    );
}

#[test]
fn a_run_that_cannot_read_its_own_last_file_mints_no_id_twice() {
    let temp = tempfile::tempdir().unwrap();
    let Setup { device, key, .. } = setup();
    let mut first_run = WorkspaceSync::create(
        plain(),
        temp.path(),
        device.clone(),
        key.clone(),
        SyncConfig::default(),
    )
    .unwrap();
    let mut minted = Vec::new();
    first_run
        .edit(Instant::now(), |state| {
            for _ in 0..5 {
                minted.push(state.mint_session_id()?.to_string());
            }
            Ok(())
        })
        .unwrap();
    let unread = first_run.flush().unwrap().unwrap();
    drop(first_run);

    let files = NotYetDownloaded::failing(&unread);
    let mut second_run = open(files.clone(), temp.path(), &device, &key);
    let second = second_run
        .edit(Instant::now(), |state| state.mint_session_id())
        .unwrap();
    assert!(
        !minted.contains(&second.to_string()),
        "{second} was minted before"
    );

    // Once the file arrives the run may go back to the old block, after its last id.
    files.arrive();
    second_run.pull().unwrap();
    second_run.flush().unwrap();
    drop(second_run);
    let mut third_run = open(plain(), temp.path(), &device, &key);
    let third = third_run
        .edit(Instant::now(), |state| state.mint_session_id())
        .unwrap();
    assert!(
        !minted.contains(&third.to_string()) && third != second,
        "{third}"
    );
}

#[test]
fn a_second_device_waits_for_the_app_key_instead_of_minting_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let mac = InMemorySecretStore::new();
    let mac_identity = DeviceIdentity::load_or_create(&mac).unwrap();
    let (mut mac_store, _) = AppStateStore::open(plain(), temp.path(), &mac, mac_identity).unwrap();
    mac_store.save().unwrap();

    // The phone's keychain has not received the Mac's app key yet.
    let phone = InMemorySecretStore::new();
    let phone_identity = DeviceIdentity::load_or_create(&phone).unwrap();
    assert!(matches!(
        AppStateStore::open(plain(), temp.path(), &phone, phone_identity),
        Err(AppStateError::KeyNotArrived(_))
    ));
}

/// Saves the Mac's app snapshot in `dir`, then evicts it to an iCloud
/// placeholder. Returns the Mac's store and the placeholder path.
fn mac_snapshot_evicted(dir: &Path) -> (InMemorySecretStore, PathBuf) {
    let mac = InMemorySecretStore::new();
    let mac_identity = DeviceIdentity::load_or_create(&mac).unwrap();
    let (mut mac_store, _) = AppStateStore::open(plain(), dir, &mac, mac_identity).unwrap();
    let snapshot = mac_store.save().unwrap();
    let name = snapshot.file_name().unwrap().to_str().unwrap();
    let placeholder = dir.join(format!(".{name}.icloud"));
    std::fs::rename(&snapshot, &placeholder).unwrap();
    (mac, placeholder)
}

#[test]
fn an_evicted_app_snapshot_still_stops_a_second_device_minting_the_app_key() {
    let temp = tempfile::tempdir().unwrap();
    mac_snapshot_evicted(temp.path());

    // A new device: its iCloud Drive shows the Mac's snapshot as a
    // placeholder and its keychain has not received the app key yet.
    let phone = InMemorySecretStore::new();
    let phone_identity = DeviceIdentity::load_or_create(&phone).unwrap();
    assert!(matches!(
        AppStateStore::open(plain(), temp.path(), &phone, phone_identity),
        Err(AppStateError::KeyNotArrived(_))
    ));
}

#[test]
fn an_evicted_app_snapshot_is_downloaded_and_reported_pending() {
    let temp = tempfile::tempdir().unwrap();
    let (mac, placeholder) = mac_snapshot_evicted(temp.path());

    let phone = mac.for_another_device();
    let phone_identity = DeviceIdentity::load_or_create(&phone).unwrap();
    let files = Arc::new(NotYetDownloaded::default());
    let (_, load) =
        AppStateStore::open(files.clone(), temp.path(), &phone, phone_identity).unwrap();
    assert_eq!(load.merged, 0);
    assert!(load.refused.is_empty(), "{:?}", load.refused);
    assert_eq!(load.pending, std::slice::from_ref(&placeholder));
    assert_eq!(
        *files.download_requests.lock().unwrap(),
        std::slice::from_ref(&placeholder)
    );
}
