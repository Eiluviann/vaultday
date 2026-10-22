//! The persisted parse index on a sandbox copy of the mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use common::{WEB_142, read, sandbox, write};
use vault_base::{
    FileIndex, FileStamp, INDEX_STORE_SCHEMA_VERSION, IndexScope, IndexStore, IndexStoreError,
    PlainFileAccess,
};

fn refreshed(root: &std::path::Path, scope: IndexScope) -> FileIndex {
    let mut index = FileIndex::new(root, scope).unwrap();
    index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    index
}

fn stamps(index: &FileIndex) -> Vec<(PathBuf, FileStamp)> {
    index
        .files()
        .map(|(path, stamp)| (path.to_path_buf(), *stamp))
        .collect()
}

#[test]
fn an_index_survives_a_save_and_a_load_in_a_new_process() {
    let sandbox = sandbox();
    let store_folder = tempfile::tempdir().unwrap();
    let store_path = store_folder.path().join("state/index.sqlite3");
    let index = refreshed(&sandbox.root, IndexScope::default());
    assert!(!index.is_empty());

    IndexStore::open(&store_path, &PlainFileAccess)
        .unwrap()
        .save(&index)
        .unwrap();

    // A fresh handle, as after a relaunch.
    let store = IndexStore::open(&store_path, &PlainFileAccess).unwrap();
    let loaded = store
        .load(&sandbox.root, IndexScope::default())
        .unwrap()
        .expect("saved index");
    assert_eq!(stamps(&loaded), stamps(&index));
    assert_eq!(loaded.root(), index.root());

    // The loaded index only reports what changed since it was saved.
    let mut loaded = loaded;
    let delta = loaded.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    assert_eq!(delta, vault_base::IndexDelta::default());
    write(
        &sandbox.root,
        WEB_142,
        &format!("{}one more line\n", read(&sandbox.root, WEB_142)),
    );
    write(&sandbox.root, "Inbox/Fresh.md", "# fresh\n");
    let delta = loaded.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    assert_eq!(delta.changed, [PathBuf::from(WEB_142)]);
    assert_eq!(delta.added, [PathBuf::from("Inbox/Fresh.md")]);
    assert!(delta.removed.is_empty());
}

#[test]
fn saving_again_replaces_the_previous_index_and_vaults_are_kept_apart() {
    let sandbox = sandbox();
    let other = common::sandbox();
    let store_folder = tempfile::tempdir().unwrap();
    let mut store =
        IndexStore::open(&store_folder.path().join("index.sqlite3"), &PlainFileAccess).unwrap();

    let mut index = refreshed(&sandbox.root, IndexScope::default());
    let other_index = refreshed(&other.root, IndexScope::default());
    store.save(&index).unwrap();
    store.save(&other_index).unwrap();

    std::fs::remove_file(sandbox.root.join("Home.md")).unwrap();
    index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    store.save(&index).unwrap();

    let loaded = store
        .load(&sandbox.root, IndexScope::default())
        .unwrap()
        .unwrap();
    assert_eq!(stamps(&loaded), stamps(&index));
    assert!(loaded.stamp(std::path::Path::new("Home.md")).is_none());
    let loaded_other = store
        .load(&other.root, IndexScope::default())
        .unwrap()
        .unwrap();
    assert_eq!(stamps(&loaded_other), stamps(&other_index));

    store.forget(&sandbox.root).unwrap();
    assert!(
        store
            .load(&sandbox.root, IndexScope::default())
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .load(&other.root, IndexScope::default())
            .unwrap()
            .is_some()
    );
}

#[test]
fn nothing_saved_or_a_different_scope_is_a_cache_miss() {
    let sandbox = sandbox();
    let store_folder = tempfile::tempdir().unwrap();
    let mut store =
        IndexStore::open(&store_folder.path().join("index.sqlite3"), &PlainFileAccess).unwrap();
    assert!(
        store
            .load(&sandbox.root, IndexScope::default())
            .unwrap()
            .is_none()
    );
    store
        .save(&refreshed(&sandbox.root, IndexScope::default()))
        .unwrap();
    assert!(
        store
            .load(&sandbox.root, IndexScope::edited_within(7))
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .load(&sandbox.root, IndexScope::default())
            .unwrap()
            .is_some()
    );
}

#[test]
fn the_store_may_not_live_inside_the_vault() {
    let sandbox = sandbox();
    let index = refreshed(&sandbox.root, IndexScope::default());
    let mut store = IndexStore::open(
        &sandbox.root.join(".vaultday/index.sqlite3"),
        &PlainFileAccess,
    )
    .unwrap();
    let error = store.save(&index).unwrap_err();
    assert!(
        matches!(error, IndexStoreError::InsideVault { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("outside"));
}

#[test]
fn a_store_from_another_schema_version_is_emptied() {
    let sandbox = sandbox();
    let store_folder = tempfile::tempdir().unwrap();
    let store_path = store_folder.path().join("index.sqlite3");
    {
        let mut store = IndexStore::open(&store_path, &PlainFileAccess).unwrap();
        store
            .save(&refreshed(&sandbox.root, IndexScope::default()))
            .unwrap();
    }
    {
        let connection = rusqlite::Connection::open(&store_path).unwrap();
        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, INDEX_STORE_SCHEMA_VERSION);
        connection
            .pragma_update(None, "user_version", INDEX_STORE_SCHEMA_VERSION + 1)
            .unwrap();
    }
    let store = IndexStore::open(&store_path, &PlainFileAccess).unwrap();
    assert!(
        store
            .load(&sandbox.root, IndexScope::default())
            .unwrap()
            .is_none()
    );
    let connection = rusqlite::Connection::open(&store_path).unwrap();
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, INDEX_STORE_SCHEMA_VERSION);
}

#[test]
fn modification_times_keep_their_precision() {
    let sandbox = sandbox();
    let store_folder = tempfile::tempdir().unwrap();
    let mut store =
        IndexStore::open(&store_folder.path().join("index.sqlite3"), &PlainFileAccess).unwrap();
    let precise = SystemTime::UNIX_EPOCH + Duration::new(1_790_000_000, 987_654_321);
    let index = FileIndex::from_entries(
        &sandbox.root,
        IndexScope::default(),
        [
            (
                PathBuf::from("a.md"),
                FileStamp {
                    modified: precise,
                    len: 42,
                },
            ),
            (
                PathBuf::from(".obsidian/app.md"),
                FileStamp {
                    modified: precise,
                    len: 1,
                },
            ),
        ],
    )
    .unwrap();
    assert_eq!(index.len(), 1, "out-of-scope entries are dropped");
    store.save(&index).unwrap();
    let loaded = store
        .load(&sandbox.root, IndexScope::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.stamp(std::path::Path::new("a.md")),
        Some(&FileStamp {
            modified: precise,
            len: 42
        })
    );
}
