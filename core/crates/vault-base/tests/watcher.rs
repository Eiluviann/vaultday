//! The watcher on a sandbox copy of the mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use common::{WEB_142, read, sandbox, write};
use vault_base::{FileIndex, IndexScope, PlainFileAccess, VaultWatcher, WatchBatch};

const DEBOUNCE: Duration = Duration::from_millis(150);
const PATIENCE: Duration = Duration::from_secs(15);

/// Collects batches until `done` holds for the paths seen so far or the time runs out.
fn collect_until(
    watcher: &VaultWatcher,
    done: impl Fn(&BTreeSet<PathBuf>) -> bool,
) -> (BTreeSet<PathBuf>, Vec<WatchBatch>) {
    let deadline = Instant::now() + PATIENCE;
    let mut seen = BTreeSet::new();
    let mut batches = Vec::new();
    while Instant::now() < deadline && !done(&seen) {
        if let Some(batch) = watcher.recv_timeout(Duration::from_millis(200)) {
            seen.extend(batch.changed.iter().cloned());
            batches.push(batch);
        }
    }
    (seen, batches)
}

/// Whatever else is still in flight after the expected paths arrived.
fn drain(watcher: &VaultWatcher, seen: &mut BTreeSet<PathBuf>) {
    while let Some(batch) = watcher.recv_timeout(DEBOUNCE * 6) {
        seen.extend(batch.changed);
    }
}

#[test]
fn a_burst_becomes_one_batch_and_ignored_folders_stay_silent() {
    let sandbox = sandbox();
    let watcher = VaultWatcher::start(&PlainFileAccess, &sandbox.root, &[], DEBOUNCE).unwrap();
    let story = PathBuf::from(WEB_142);

    // Ignored first, so any leak would show up before the real change.
    write(
        &sandbox.root,
        ".vaultday/sync/device-a/0001.bin",
        "not a note",
    );
    write(&sandbox.root, ".obsidian/workspace.json", "{}");
    let original = read(&sandbox.root, WEB_142);
    for round in 0..8 {
        write(
            &sandbox.root,
            WEB_142,
            &format!("{original}burst {round}\n"),
        );
    }

    let (mut seen, batches) = collect_until(&watcher, |seen| seen.contains(&story));
    drain(&watcher, &mut seen);
    assert!(
        seen.contains(&story),
        "the edited note is reported: {seen:?}"
    );
    assert!(
        seen.iter()
            .all(|path| { !path.starts_with(".obsidian") && !path.starts_with(".vaultday/sync") }),
        "ignored folders leaked: {seen:?}"
    );
    assert!(
        batches.len() <= 2,
        "eight writes within the window are one burst, got {} batches: {batches:?}",
        batches.len()
    );
    assert!(
        batches.iter().all(|batch| {
            let mut sorted = batch.changed.clone();
            sorted.sort();
            sorted.dedup();
            sorted == batch.changed
        }),
        "batches are sorted without duplicates"
    );
}

#[test]
fn the_watcher_feeds_the_index() {
    let sandbox = sandbox();
    let mut index = FileIndex::new(&sandbox.root, IndexScope::default()).unwrap();
    index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    let indexed_before = index.len();
    let watcher = VaultWatcher::start(&PlainFileAccess, &sandbox.root, &[], DEBOUNCE).unwrap();

    write(&sandbox.root, "Projects/Website/New note.md", "# New\n");
    let created = PathBuf::from("Projects/Website/New note.md");
    let delta = feed_until(&watcher, &mut index, |_, delta| {
        delta.added.contains(&created)
    });
    assert!(delta.added.contains(&created), "{delta:?}");
    assert_eq!(index.len(), indexed_before + 1);
    assert!(index.stamp(&created).is_some());

    write(
        &sandbox.root,
        "Projects/Website/New note.md",
        "# New and longer\n",
    );
    let delta = feed_until(&watcher, &mut index, |_, delta| {
        delta.changed.contains(&created)
    });
    assert!(delta.changed.contains(&created), "{delta:?}");

    fs::remove_file(sandbox.root.join(&created)).unwrap();
    let delta = feed_until(&watcher, &mut index, |_, delta| {
        delta.removed.contains(&created)
    });
    assert!(delta.removed.contains(&created), "{delta:?}");
    assert_eq!(index.len(), indexed_before);

    // A whole folder arriving at once is walked.
    write(&sandbox.root, "Projects/Fresh/One.md", "# One\n");
    write(&sandbox.root, "Projects/Fresh/Two.md", "# Two\n");
    let delta = feed_until(&watcher, &mut index, |index, _| {
        index
            .stamp(&PathBuf::from("Projects/Fresh/Two.md"))
            .is_some()
            && index
                .stamp(&PathBuf::from("Projects/Fresh/One.md"))
                .is_some()
    });
    assert!(delta.added.len() >= 2, "{delta:?}");
    assert!(
        index
            .stamp(&PathBuf::from("Projects/Fresh/Two.md"))
            .is_some()
    );
}

/// Feeds batches to the index and merges their deltas until `done` holds.
fn feed_until(
    watcher: &VaultWatcher,
    index: &mut FileIndex,
    done: impl Fn(&FileIndex, &vault_base::IndexDelta) -> bool,
) -> vault_base::IndexDelta {
    let deadline = Instant::now() + PATIENCE;
    let mut total = vault_base::IndexDelta::default();
    while Instant::now() < deadline && !done(index, &total) {
        if let Some(delta) = watcher
            .feed(index, &PlainFileAccess, Duration::from_millis(200))
            .unwrap()
        {
            total.added.extend(delta.added);
            total.changed.extend(delta.changed);
            total.removed.extend(delta.removed);
        }
    }
    total
}

#[test]
fn stopping_the_watcher_ends_its_thread_and_delivery() {
    let sandbox = sandbox();
    let watcher = VaultWatcher::start(&PlainFileAccess, &sandbox.root, &[], DEBOUNCE).unwrap();
    drop(watcher);
    write(&sandbox.root, "After.md", "# after\n");
}

#[test]
fn watching_a_missing_folder_fails() {
    let sandbox = sandbox();
    assert!(
        VaultWatcher::start(
            &PlainFileAccess,
            &sandbox.root.join("nowhere"),
            &[],
            DEBOUNCE
        )
        .is_err()
    );
}

#[test]
fn changes_inside_a_nested_workspace_stay_out_of_the_outer_index() {
    let sandbox = sandbox();
    write(
        &sandbox.root,
        "Clients/Acme/.vaultday/settings.json",
        "{\"schema_version\": 1, \"name\": \"Acme\", \"kind\": \"work\"}\n",
    );
    write(
        &sandbox.root,
        "Clients/Acme/Projects/Portal.md",
        "# Portal\n",
    );
    let mut index = FileIndex::new(&sandbox.root, IndexScope::default()).unwrap();
    index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    let refreshed: Vec<PathBuf> = index.files().map(|(path, _)| path.to_path_buf()).collect();

    let delta = index
        .apply_changes(
            &PlainFileAccess,
            &[
                PathBuf::from("Clients/Acme/Projects/Portal.md"),
                PathBuf::from("Clients/Acme/Projects"),
            ],
            SystemTime::now(),
        )
        .unwrap();

    assert!(delta.added.is_empty(), "{delta:?}");
    let applied: Vec<PathBuf> = index.files().map(|(path, _)| path.to_path_buf()).collect();
    assert_eq!(
        applied, refreshed,
        "a watcher batch agrees with a full refresh"
    );
}
