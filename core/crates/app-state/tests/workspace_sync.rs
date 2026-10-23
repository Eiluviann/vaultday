//! Two in-process devices sharing one in-memory secret store converge through
//! encrypted, batched change files in a temporary sync folder.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_state::{
    CompactionOutcome, DataKey, DeviceId, DeviceIdentity, InMemorySecretStore, NotArrived,
    PlainSyncFiles, SecretStore, SyncConfig, SyncEntry, SyncFiles, WorkspaceSync,
    create_workspace_key, sync_folder, workspace_key,
};
use domain::task::TaskId;
use domain::workspace::WorkspaceId;
use jiff::civil::{Date, date};

const DAY: Date = date(2026, 10, 22);
const MARKER: &str = "synthetic-marker-answer";

/// Plain files that remember which placeholders a download was asked for.
#[derive(Debug, Default)]
struct RecordingFiles {
    download_requests: Mutex<Vec<PathBuf>>,
}

impl SyncFiles for RecordingFiles {
    fn list(&self, dir: &Path) -> io::Result<Vec<SyncEntry>> {
        PlainSyncFiles.list(dir)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
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

struct Device {
    secrets: InMemorySecretStore,
    identity: DeviceIdentity,
}

impl Device {
    fn first() -> Device {
        Device::with_store(InMemorySecretStore::new())
    }

    fn another(&self) -> Device {
        Device::with_store(self.secrets.for_another_device())
    }

    fn with_store(secrets: InMemorySecretStore) -> Device {
        let identity = DeviceIdentity::load_or_create(&secrets).unwrap();
        Device { secrets, identity }
    }

    fn id(&self) -> DeviceId {
        self.identity.device_id().clone()
    }

    fn key(&self) -> DataKey {
        workspace_key(&self.secrets, &workspace())
            .unwrap()
            .expect("the key has synchronised")
    }

    fn create(&self, root: &Path) -> WorkspaceSync {
        create_workspace_key(&self.secrets, &workspace()).unwrap();
        WorkspaceSync::create(plain(), root, self.id(), self.key(), config()).unwrap()
    }

    fn open(&self, root: &Path) -> WorkspaceSync {
        self.open_with(plain(), root).0
    }

    fn open_with(
        &self,
        files: Arc<dyn SyncFiles>,
        root: &Path,
    ) -> (WorkspaceSync, app_state::PullOutcome) {
        WorkspaceSync::open(files, root, self.id(), self.key(), config()).unwrap()
    }
}

fn workspace() -> WorkspaceId {
    WorkspaceId::new("synthetic")
}

fn plain() -> Arc<dyn SyncFiles> {
    Arc::new(PlainSyncFiles)
}

fn config() -> SyncConfig {
    SyncConfig {
        flush_window: Duration::from_secs(2),
        max_batch_delay: Duration::from_secs(10),
    }
}

fn tasks(ids: &[&str]) -> Vec<TaskId> {
    ids.iter().map(|id| TaskId::new(*id)).collect()
}

/// One edit, flushed at once: a file of its own.
fn plan(sync: &mut WorkspaceSync, order: &[&str]) -> PathBuf {
    sync.edit(Instant::now(), |state| {
        state.set_day_order(DAY, &tasks(order))
    })
    .unwrap();
    sync.flush().unwrap().expect("a file was written")
}

fn answer(sync: &mut WorkspaceSync, question: &str, text: &str) -> PathBuf {
    sync.edit(Instant::now(), |state| {
        state.set_ritual_answer(DAY, "shutdown", question, text)
    })
    .unwrap();
    sync.flush().unwrap().expect("a file was written")
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Mints 600 ids of each kind and fails on any id seen before.
fn mint_round(sync: &mut WorkspaceSync, minted: &mut BTreeSet<String>) {
    sync.edit(Instant::now(), |state| {
        for _ in 0..600 {
            assert!(minted.insert(state.mint_task_id(&|_| false)?.to_string()));
            assert!(minted.insert(state.mint_session_id()?.to_string()));
            assert!(minted.insert(state.mint_time_box_id()?.to_string()));
        }
        Ok(())
    })
    .unwrap();
    sync.flush().unwrap();
}

#[test]
fn two_devices_converge_through_a_conflict_copy_a_truncated_file_and_a_placeholder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    let phone = mac.another();

    let mut mac_sync = mac.create(&root);
    let mac_dir = sync_folder(&root).join(mac.id().as_str());
    assert_eq!(files_in(&mac_dir), ["0001.bin"]);

    // A restored copy of the Mac that kept its device id saw only 0001 and
    // writes its own 0002; the sync provider keeps both as a conflict copy.
    let restored_root = temp.path().join("restored");
    fs::create_dir_all(sync_folder(&restored_root).join(mac.id().as_str())).unwrap();
    fs::copy(
        mac_dir.join("0001.bin"),
        sync_folder(&restored_root)
            .join(mac.id().as_str())
            .join("0001.bin"),
    )
    .unwrap();
    let mut restored = mac.open(&restored_root);
    let restored_file = answer(&mut restored, "restored", "from the restored copy");
    assert!(restored_file.ends_with("0002.bin"));

    plan(&mut mac_sync, &["task-1", "task-2"]);
    fs::copy(&restored_file, mac_dir.join("0002 2.bin")).unwrap();

    let truncated_path = answer(&mut mac_sync, "wins", "Shipped the parser");
    let whole = fs::read(&truncated_path).unwrap();
    fs::write(&truncated_path, &whole[..whole.len() / 2]).unwrap();

    let evicted_path = answer(&mut mac_sync, "next", "Reviews");
    let placeholder = mac_dir.join(".0004.bin.icloud");
    let evicted = fs::read(&evicted_path).unwrap();
    fs::rename(&evicted_path, &placeholder).unwrap();

    let mut phone_sync_with_recording = {
        let recording = Arc::new(RecordingFiles::default());
        let (sync, outcome) = phone.open_with(recording.clone(), &root);
        let mut reasons: Vec<NotArrived> = outcome.pending.iter().map(|file| file.reason).collect();
        reasons.sort_by_key(|reason| format!("{reason:?}"));
        assert_eq!(
            reasons,
            [NotArrived::Placeholder, NotArrived::Undecryptable]
        );
        assert_eq!(outcome.applied, 3, "0001, 0002 and its conflict copy");
        assert_eq!(
            *recording.download_requests.lock().unwrap(),
            std::slice::from_ref(&placeholder)
        );
        sync
    };
    let phone_sync = &mut phone_sync_with_recording;
    assert_eq!(
        phone_sync.state().day_order(DAY),
        tasks(&["task-1", "task-2"])
    );
    let answers = phone_sync.state().ritual_answers(DAY, "shutdown");
    assert_eq!(
        answers["restored"], "from the restored copy",
        "the conflict copy is applied"
    );
    assert!(
        !answers.contains_key("wins"),
        "the truncated file has not arrived"
    );

    // The phone replans the day it has seen, while the Mac's files are still arriving.
    phone_sync
        .edit(Instant::now(), |state| {
            state.set_day_order(DAY, &tasks(&["task-3", "task-1"]))
        })
        .unwrap();
    phone_sync.flush().unwrap();

    fs::write(&truncated_path, &whole).unwrap();
    fs::rename(&placeholder, &evicted_path).unwrap();
    fs::write(&evicted_path, &evicted).unwrap();
    let outcome = phone_sync.pull().unwrap();
    assert_eq!(outcome.applied, 2);
    assert!(outcome.pending.is_empty(), "{:?}", outcome.pending);
    assert_eq!(
        phone_sync.pull().unwrap().applied,
        0,
        "a second pull applies nothing"
    );

    let mac_outcome = mac_sync.pull().unwrap();
    assert_eq!(
        mac_outcome.applied, 2,
        "the conflict copy and the phone's file"
    );
    assert_eq!(mac_sync.heads(), phone_sync.heads());
    assert_eq!(
        mac_sync.state().ritual_answers(DAY, "shutdown"),
        phone_sync.state().ritual_answers(DAY, "shutdown")
    );
    assert_eq!(
        mac_sync.state().day_order(DAY),
        phone_sync.state().day_order(DAY)
    );
    assert_eq!(
        mac_sync.state().day_order(DAY),
        tasks(&["task-3", "task-1"])
    );
}

#[test]
fn edits_are_batched_into_one_file_per_burst() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    let mut sync = mac.create(&root);
    let start = Instant::now();
    for (offset, question) in ["a", "b", "c"].iter().enumerate() {
        let at = start + Duration::from_secs(offset as u64);
        sync.edit(at, |state| {
            state.set_ritual_answer(DAY, "planning", question, "x")
        })
        .unwrap();
        assert_eq!(
            sync.flush_if_due(at).unwrap(),
            None,
            "the burst is still going"
        );
    }
    assert!(sync.has_unflushed_edits());
    let written = sync
        .flush_if_due(start + Duration::from_secs(4))
        .unwrap()
        .expect("quiet for the flush window");
    assert!(written.ends_with("0002.bin"));
    assert!(!sync.has_unflushed_edits());
    assert_eq!(sync.flush().unwrap(), None, "nothing left to write");

    // Edits that never pause are written after the longest delay.
    let start = start + Duration::from_secs(100);
    let mut written = None;
    for second in 0..=10 {
        let at = start + Duration::from_secs(second);
        sync.edit(at, |state| {
            state.set_rollover_streak(&TaskId::new("task-1"), 1 + second as u32)
        })
        .unwrap();
        written = written.or(sync.flush_if_due(at).unwrap());
    }
    assert!(written.unwrap().ends_with("0003.bin"));

    let phone = mac.another();
    let phone_sync = phone.open(&root);
    assert_eq!(phone_sync.state().ritual_answers(DAY, "planning").len(), 3);
}

#[test]
fn a_failed_edit_leaves_nothing_behind() {
    let temp = tempfile::tempdir().unwrap();
    let mac = Device::first();
    let mut sync = mac.create(&temp.path().join("vault"));
    let result: Result<(), _> = sync.edit(Instant::now(), |state| {
        state.set_day_order(DAY, &tasks(&["task-9"]))?;
        Err(app_state::AppStateError::IdSpaceExhausted("task-"))
    });
    assert!(result.is_err());
    assert!(sync.state().day_order(DAY).is_empty());
    assert!(!sync.has_unflushed_edits());
}

#[test]
fn a_change_file_is_unreadable_without_the_key() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    let mut sync = mac.create(&root);
    let path = answer(&mut sync, "wins", MARKER);

    let bytes = fs::read(&path).unwrap();
    assert!(
        !bytes
            .windows(MARKER.len())
            .any(|window| window == MARKER.as_bytes()),
        "the plaintext never reaches the disk"
    );

    let stranger = Device::first();
    let (stranger_sync, outcome) = WorkspaceSync::open(
        plain(),
        &root,
        stranger.id(),
        DataKey::generate().unwrap(),
        config(),
    )
    .unwrap();
    assert_eq!(outcome.applied, 0);
    assert_eq!(outcome.pending.len(), 2);
    assert!(
        outcome
            .pending
            .iter()
            .all(|file| file.reason == NotArrived::Undecryptable)
    );
    assert!(
        stranger_sync
            .state()
            .ritual_answers(DAY, "shutdown")
            .is_empty()
    );
}

#[test]
fn deleting_the_workspace_destroys_the_key_for_every_device() {
    let mac = Device::first();
    let phone = mac.another();
    create_workspace_key(&mac.secrets, &workspace()).unwrap();
    app_state::delete_workspace(&mac.secrets, &workspace()).unwrap();
    assert!(
        workspace_key(&phone.secrets, &workspace())
            .unwrap()
            .is_none()
    );
    assert!(
        phone
            .secrets
            .list(app_state::SecretKind::WorkspaceDataKey)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn counters_never_collide_across_two_devices() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    let phone = mac.another();
    let mut mac_sync = mac.create(&root);

    // The phone starts before any of the Mac's files reached it.
    let offline_root = temp.path().join("offline");
    let mut phone_sync = phone.open(&offline_root);

    let mut minted = BTreeSet::new();
    mint_round(&mut mac_sync, &mut minted);
    mint_round(&mut phone_sync, &mut minted);
    assert!(minted.contains("task-1"), "the creator holds block 0");

    // The phone's folder reaches the shared sync folder; both see every claim.
    let phone_folder = sync_folder(&offline_root).join(phone.id().as_str());
    let shared_phone_folder = sync_folder(&root).join(phone.id().as_str());
    fs::create_dir_all(&shared_phone_folder).unwrap();
    for name in files_in(&phone_folder) {
        fs::copy(phone_folder.join(&name), shared_phone_folder.join(&name)).unwrap();
    }
    let mut phone_sync = phone.open(&root);
    mac_sync.pull().unwrap();
    mint_round(&mut mac_sync, &mut minted);
    mint_round(&mut phone_sync, &mut minted);
    assert_eq!(minted.len(), 4 * 3 * 600);

    let mac_blocks = mac_sync.state().id_blocks(&mac.id());
    let phone_blocks = phone_sync.state().id_blocks(&phone.id());
    assert!(mac_blocks.iter().all(|block| !phone_blocks.contains(block)));
}

#[test]
fn compaction_preserves_the_merged_state_and_tombstones_its_numbers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    let phone = mac.another();
    let mut mac_sync = mac.create(&root);
    let mut phone_sync = phone.open(&root);
    plan(&mut mac_sync, &["task-1"]);
    answer(&mut phone_sync, "focus", "Reviews");
    let first = sync_folder(&root).join(mac.id().as_str()).join("0001.bin");
    let first_bytes = fs::read(&first).unwrap();
    answer(&mut mac_sync, "wins", "Shipped");
    mac_sync.pull().unwrap();

    let CompactionOutcome::Compacted(compaction) = mac_sync.compact().unwrap() else {
        panic!("the Mac has written files");
    };
    assert_eq!(compaction.covers_through, 3);
    assert_eq!(compaction.removed, 3);
    let mac_dir = sync_folder(&root).join(mac.id().as_str());
    assert_eq!(files_in(&mac_dir), ["snapshot-0003.bin"]);
    assert_eq!(
        files_in(&sync_folder(&root).join(phone.id().as_str())).len(),
        1,
        "only the Mac's folder is compacted"
    );

    let laptop = mac.another();
    let mut laptop_sync = laptop.open(&root);
    assert_eq!(laptop_sync.heads(), mac_sync.heads());
    assert_eq!(laptop_sync.state().day_order(DAY), tasks(&["task-1"]));
    assert_eq!(laptop_sync.state().ritual_answers(DAY, "shutdown").len(), 2);

    // A sync client brings back a deleted file: it is harmless to readers,
    // and the Mac deletes it again and never reuses its number.
    fs::write(&first, &first_bytes).unwrap();
    assert_eq!(laptop_sync.pull().unwrap().pending, []);
    drop(mac_sync);
    let mut mac_sync = mac.open(&root);
    assert_eq!(files_in(&mac_dir), ["snapshot-0003.bin"]);
    let next = plan(&mut mac_sync, &["task-1", "task-4"]);
    assert!(next.ends_with("0004.bin"));
    laptop_sync.pull().unwrap();
    assert_eq!(
        laptop_sync.state().day_order(DAY),
        tasks(&["task-1", "task-4"])
    );

    // A second compaction replaces the first snapshot.
    let CompactionOutcome::Compacted(again) = mac_sync.compact().unwrap() else {
        panic!("the Mac wrote 0004");
    };
    assert_eq!(mac_sync.compact().unwrap(), CompactionOutcome::NothingNew);
    assert_eq!(files_in(&mac_dir), ["snapshot-0004.bin"]);
    assert_eq!(again.removed, 2);
    let mut fresh = mac.another().open(&root);
    assert_eq!(fresh.heads(), mac_sync.heads());
}

#[test]
fn creating_over_an_existing_sync_folder_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let mac = Device::first();
    mac.create(&root);
    let phone = mac.another();
    assert!(matches!(
        WorkspaceSync::create(plain(), &root, phone.id(), mac.key(), config()),
        Err(app_state::AppStateError::WorkspaceExists(_))
    ));
}
