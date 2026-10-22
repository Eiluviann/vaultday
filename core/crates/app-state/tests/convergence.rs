//! Two devices over one sync folder, each writing real change files, converge.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use app_state::{AppState, DeviceId, SessionEntry};

fn session(id: &str, target: &str) -> SessionEntry {
    SessionEntry {
        id: id.to_owned(),
        target: target.to_owned(),
        started_at: format!("2026-10-22T09:00:00+02:00#{id}"),
    }
}

fn open(root: &Path, device: &str) -> AppState {
    AppState::open(root, DeviceId::new(device).unwrap()).expect("state opens")
}

#[test]
fn two_devices_appending_apart_converge_through_change_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".vaultday").join("sync");

    let mut mac = open(&root, "mac-a");
    let mut linux = open(&root, "linux-b");

    let mac_file = mac.append_session(&session("ses-1", "task-1")).unwrap();
    let linux_file = linux.append_session(&session("ses-2", "WEB-142")).unwrap();
    assert_eq!(mac_file, root.join("mac-a").join("0001.bin"));
    assert_eq!(linux_file, root.join("linux-b").join("0001.bin"));
    assert_eq!(mac.sessions().len(), 1, "nothing pulled yet");

    assert_eq!(mac.pull().unwrap().applied, 1);
    assert_eq!(linux.pull().unwrap().applied, 1);
    assert_eq!(
        mac.pull().unwrap().applied,
        0,
        "a second pull applies nothing"
    );

    assert_eq!(mac.heads(), linux.heads());
    assert_eq!(mac.sessions(), linux.sessions());
    let mut ids: Vec<String> = mac.sessions().into_iter().map(|s| s.id).collect();
    ids.sort();
    assert_eq!(ids, ["ses-1", "ses-2"]);
}

#[test]
fn a_fresh_device_loads_everything_and_a_reopened_one_continues_numbering() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("sync");

    let mut mac = open(&root, "mac-a");
    mac.append_session(&session("ses-1", "task-1")).unwrap();
    mac.append_session(&session("ses-2", "task-2")).unwrap();
    drop(mac);

    let mut mac_again = open(&root, "mac-a");
    let third = mac_again
        .append_session(&session("ses-3", "task-3"))
        .unwrap();
    assert_eq!(third.file_name().unwrap(), "0003.bin");

    let mut phone = open(&root, "phone-c");
    let ids: Vec<String> = phone.sessions().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, ["ses-1", "ses-2", "ses-3"]);
    assert_eq!(phone.heads(), mac_again.heads());
    assert!(fs::read_dir(root.join("phone-c")).unwrap().next().is_none());
}

#[test]
fn a_change_file_that_arrives_late_is_still_applied() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("sync");
    let mut mac = open(&root, "mac-a");
    for id in ["ses-1", "ses-2", "ses-3"] {
        mac.append_session(&session(id, "task")).unwrap();
    }
    // The sync service delivers 0003 before 0002.
    let second = root.join("mac-a").join("0002.bin");
    let held_back = fs::read(&second).unwrap();
    fs::remove_file(&second).unwrap();

    let mut linux = open(&root, "linux-b");
    assert_eq!(ids(&linux), ["ses-1"], "stops at the gap");

    fs::write(&second, held_back).unwrap();
    assert_eq!(linux.pull().unwrap().applied, 2);
    assert_eq!(ids(&linux), ["ses-1", "ses-2", "ses-3"]);
    assert_eq!(linux.heads(), mac.heads());
}

#[test]
fn a_truncated_change_file_stalls_its_device_until_it_is_whole() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("sync");
    let mut mac = open(&root, "mac-a");
    let first = mac.append_session(&session("ses-1", "task-1")).unwrap();
    let whole = fs::read(&first).unwrap();
    fs::write(&first, &whole[..whole.len() / 2]).unwrap();
    let mut phone = open(&root, "phone-c");
    phone.append_session(&session("ses-9", "task-9")).unwrap();

    let mut linux = open(&root, "linux-b");
    assert_eq!(ids(&linux), ["ses-9"], "other devices still load");
    let outcome = linux.pull().unwrap();
    assert_eq!(outcome.applied, 0);
    assert_eq!(outcome.stalled.len(), 1);
    assert!(
        outcome.stalled[0].to_string().contains("0001.bin"),
        "{}",
        outcome.stalled[0]
    );

    fs::write(&first, whole).unwrap();
    let outcome = linux.pull().unwrap();
    assert_eq!(outcome.applied, 1);
    assert!(outcome.stalled.is_empty());
    let mut all = ids(&linux);
    all.sort();
    assert_eq!(all, ["ses-1", "ses-9"]);
}

fn ids(state: &AppState) -> Vec<String> {
    state.sessions().into_iter().map(|s| s.id).collect()
}
