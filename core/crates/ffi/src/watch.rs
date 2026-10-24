//! Vault change notifications for the app: the PR 5 watcher
//! ([`vault_base::VaultWatcher`]) over the workspace root, debounced, handed
//! to a platform listener so a surface that holds a snapshot (Browse) can
//! read the vault again when a note changes outside the app.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use vault_base::{PlainFileAccess, VaultWatcher};

use crate::{CoreError, WorkspaceHandle};

/// How often the watch thread looks at its stop flag.
const POLL: Duration = Duration::from_millis(200);

/// Told when files under a watched workspace changed. Called on a
/// background thread, once per debounced burst.
#[uniffi::export(with_foreign)]
pub trait VaultChangeListener: Send + Sync {
    /// The paths that changed in one burst, relative to the workspace root.
    fn vault_changed(&self, paths: Vec<String>);
}

/// A running watch over one workspace. It stops on [`VaultChangeWatch::stop`]
/// or when the object is released.
#[derive(Debug, uniffi::Object)]
pub struct VaultChangeWatch {
    is_stopped: Arc<AtomicBool>,
}

#[uniffi::export]
impl VaultChangeWatch {
    /// Watches `workspace`'s root and the linked folders it follows, delivering a batch to `listener` once the
    /// folder has been quiet for `debounce_ms`. Paths the watcher ignores
    /// (`.obsidian/`, `.vaultday/sync/`, the writer's temporary files) never
    /// arrive.
    ///
    /// # Errors
    /// [`CoreError::FileAccess`] when the watch cannot be registered.
    #[uniffi::constructor]
    pub fn start(
        workspace: &WorkspaceHandle,
        debounce_ms: u32,
        listener: Arc<dyn VaultChangeListener>,
    ) -> Result<Arc<VaultChangeWatch>, CoreError> {
        let watcher = VaultWatcher::start(
            &PlainFileAccess,
            &workspace.workspace.root,
            &workspace.workspace.linked_targets(),
            Duration::from_millis(u64::from(debounce_ms)),
        )
        .map_err(|error| CoreError::FileAccess(error.to_string()))?;
        let is_stopped = Arc::new(AtomicBool::new(false));
        let thread_stop = is_stopped.clone();
        thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                let Some(batch) = watcher.recv_timeout(POLL) else {
                    continue;
                };
                if thread_stop.load(Ordering::Acquire) {
                    break;
                }
                let paths = batch
                    .changed
                    .iter()
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
                    .collect();
                listener.vault_changed(paths);
            }
        });
        Ok(Arc::new(VaultChangeWatch { is_stopped }))
    }

    /// Stops the watch; no batch is delivered after it returns but one
    /// already being handed over.
    pub fn stop(&self) {
        self.is_stopped.store(true, Ordering::Release);
    }
}

impl Drop for VaultChangeWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::time::Instant;

    use super::*;
    use crate::open_workspace;

    #[derive(Default)]
    struct Recorder {
        batches: Mutex<Vec<Vec<String>>>,
    }

    impl VaultChangeListener for Recorder {
        fn vault_changed(&self, paths: Vec<String>) {
            self.batches.lock().unwrap().push(paths);
        }
    }

    fn wait_for(recorder: &Recorder, wanted: impl Fn(&[Vec<String>]) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if wanted(&recorder.batches.lock().unwrap()) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn a_change_outside_the_app_arrives_as_one_debounced_batch_and_stop_ends_it() {
        let sandbox = tempfile::tempdir().unwrap();
        let mock =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mock-vault");
        copy_dir(&mock, sandbox.path());
        let workspace =
            open_workspace(sandbox.path().to_string_lossy().into_owned(), Vec::new()).unwrap();
        let recorder = Arc::new(Recorder::default());
        let watch = VaultChangeWatch::start(&workspace, 100, recorder.clone()).unwrap();
        thread::sleep(Duration::from_millis(300));
        let note = workspace.workspace.root.join("Inbox/Outside edit.md");
        fs::write(&note, "- [ ] Typed in another editor\n").unwrap();
        assert!(
            wait_for(&recorder, |batches| batches
                .iter()
                .flatten()
                .any(|path| path.ends_with("Outside edit.md"))),
            "the edit arrives"
        );
        watch.stop();
        thread::sleep(Duration::from_millis(400));
        let seen = recorder.batches.lock().unwrap().len();
        fs::write(&note, "- [ ] Typed again\n").unwrap();
        thread::sleep(Duration::from_millis(800));
        assert_eq!(
            recorder.batches.lock().unwrap().len(),
            seen,
            "nothing after stop"
        );
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
