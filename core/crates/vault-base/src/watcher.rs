//! File watching through the [`FileAccess`] port. Raw events arrive in bursts
//! (an editor save is a rename plus a write, a sync client touches many files
//! at once), so the watcher collects the changed paths and delivers them as
//! one [`WatchBatch`] once the folder has been quiet for the debounce window.
//! Paths under `.vaultday/sync/` and `.obsidian/`, and the temporary files of
//! the atomic writer, never reach a batch.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::file_access::{FileAccess, FileAccessError, WatchHandle};
use crate::index::{FileIndex, IndexDelta};
use crate::workspace::{OBSIDIAN_DIR_NAME, WORKSPACE_DIR_NAME, find_linked_folders};

/// How long the folder must be quiet before a batch is delivered.
pub const DEFAULT_WATCH_DEBOUNCE: Duration = Duration::from_millis(300);

const SYNC_DIR_NAME: &str = "sync";
const TEMP_FILE_PREFIX: &str = ".vaultday-";
const TEMP_FILE_SUFFIX: &str = ".tmp";

/// The paths that changed during one burst, root-relative, sorted, without
/// duplicates. A path may be a file or a folder, and may no longer exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchBatch {
    /// The changed paths.
    pub changed: Vec<PathBuf>,
}

/// A running watch over a root. Dropping it stops the watch and the thread
/// that debounces its events.
#[derive(Debug)]
pub struct VaultWatcher {
    _handles: Vec<WatchHandle>,
    batches: Receiver<WatchBatch>,
}

impl VaultWatcher {
    /// Starts watching `root` and each of its linked folders. A linked
    /// folder is watched through its link, so its events arrive under the
    /// link's path and the target's real path never reaches a batch. A link
    /// made after the start is watched from the next start.
    ///
    /// # Errors
    /// File errors from registering the watch.
    pub fn start(
        file_access: &dyn FileAccess,
        root: &Path,
        debounce: Duration,
    ) -> Result<VaultWatcher, FileAccessError> {
        let (raw_sender, raw_receiver) = mpsc::channel::<PathBuf>();
        let (batch_sender, batches) = mpsc::channel::<WatchBatch>();
        let raw_sender = std::sync::Arc::new(std::sync::Mutex::new(raw_sender));
        let watched_root = root.to_path_buf();
        let mut watched = vec![root.to_path_buf()];
        watched.extend(
            find_linked_folders(root, file_access)?
                .into_iter()
                .map(|linked| linked.link),
        );
        let mut handles = Vec::with_capacity(watched.len());
        for path in &watched {
            let sender = std::sync::Arc::clone(&raw_sender);
            handles.push(file_access.watch(
                path,
                Box::new(move |path| {
                    if let Ok(sender) = sender.lock() {
                        let _ = sender.send(path.to_path_buf());
                    }
                }),
            )?);
        }
        thread::spawn(move || {
            debounce_events(&watched_root, &raw_receiver, &batch_sender, debounce)
        });
        Ok(VaultWatcher {
            _handles: handles,
            batches,
        })
    }

    /// Waits up to `timeout` for the next batch.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<WatchBatch> {
        self.batches.recv_timeout(timeout).ok()
    }

    /// The next batch if one is ready.
    #[must_use]
    pub fn try_recv(&self) -> Option<WatchBatch> {
        self.batches.try_recv().ok()
    }

    /// Waits up to `timeout` for a batch and feeds it to `index`, returning
    /// what changed in the index; `None` when no batch arrived.
    ///
    /// # Errors
    /// File errors from stamping the changed paths.
    pub fn feed(
        &self,
        index: &mut FileIndex,
        file_access: &dyn FileAccess,
        timeout: Duration,
    ) -> Result<Option<IndexDelta>, FileAccessError> {
        let Some(batch) = self.recv_timeout(timeout) else {
            return Ok(None);
        };
        index
            .apply_changes(file_access, &batch.changed, SystemTime::now())
            .map(Some)
    }
}

/// True for paths the watcher drops: anything inside `.obsidian/`, anything
/// inside a `.vaultday/sync/` folder, and the atomic writer's temporary files.
/// `relative_path` is relative to the watched root.
#[must_use]
pub fn is_ignored(relative_path: &Path) -> bool {
    let names: Vec<&str> = relative_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    let is_temp_file = names
        .last()
        .is_some_and(|name| name.starts_with(TEMP_FILE_PREFIX) && name.ends_with(TEMP_FILE_SUFFIX));
    let is_in_obsidian = names.contains(&OBSIDIAN_DIR_NAME);
    let is_in_sync = names
        .windows(2)
        .any(|pair| pair == [WORKSPACE_DIR_NAME, SYNC_DIR_NAME]);
    is_temp_file || is_in_obsidian || is_in_sync
}

fn debounce_events(
    root: &Path,
    raw: &Receiver<PathBuf>,
    batches: &mpsc::Sender<WatchBatch>,
    debounce: Duration,
) {
    let mut pending: BTreeSet<PathBuf> = BTreeSet::new();
    loop {
        let event = if pending.is_empty() {
            match raw.recv() {
                Ok(path) => Some(path),
                Err(_) => return,
            }
        } else {
            match raw.recv_timeout(debounce) {
                Ok(path) => Some(path),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    deliver(batches, &mut pending);
                    return;
                }
            }
        };
        match event {
            Some(path) => {
                if let Some(relative) = relative_to_root(root, &path)
                    && !is_ignored(&relative)
                {
                    pending.insert(relative);
                }
            }
            None => {
                if !deliver(batches, &mut pending) {
                    return;
                }
            }
        }
    }
}

/// Sends the pending paths as one batch; false when nobody listens any more.
fn deliver(batches: &mpsc::Sender<WatchBatch>, pending: &mut BTreeSet<PathBuf>) -> bool {
    let changed: Vec<PathBuf> = std::mem::take(pending).into_iter().collect();
    if changed.is_empty() {
        return true;
    }
    batches.send(WatchBatch { changed }).is_ok()
}

fn relative_to_root(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::is_ignored;
    use std::path::Path;

    #[test]
    fn ignores_sync_obsidian_and_writer_temp_files() {
        for ignored in [
            ".vaultday/sync/device-a/0001.bin",
            "Clients/Acme/.vaultday/sync/device-a/0001.bin",
            ".obsidian/workspace.json",
            "Projects/.obsidian/app.json",
            "Projects/.vaultday-123-4.tmp",
        ] {
            assert!(is_ignored(Path::new(ignored)), "{ignored}");
        }
        for watched in [
            ".vaultday/settings.json",
            "Projects/Website/Website.md",
            "Log/2026-10-23.md",
            "sync/notes.md",
            "Projects/.vaultday-notes.md",
        ] {
            assert!(!is_ignored(Path::new(watched)), "{watched}");
        }
    }
}
