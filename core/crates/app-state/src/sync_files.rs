//! The byte-level file port the sync folder and the signed snapshots go
//! through. Desktop and tests use [`PlainSyncFiles`]; the Mac app supplies one
//! that coordinates reads and writes (`NSFileCoordinator`) inside the granted
//! folder and starts downloads of evicted iCloud files.
//!
//! It is separate from `vault_base::FileAccess` because that port carries
//! UTF-8 text only; see the crate's `SECURITY.md` for the plan to fold the
//! byte operations into it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One entry of a folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntry {
    /// Absolute path of the entry.
    pub path: PathBuf,
    /// Whether the entry is a folder.
    pub is_dir: bool,
}

/// Byte-level file access for app state.
pub trait SyncFiles: Send + Sync {
    /// Lists a folder in no particular order; a missing folder lists empty.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn list(&self, dir: &Path) -> io::Result<Vec<SyncEntry>>;

    /// Reads a whole file.
    ///
    /// # Errors
    /// I/O errors, "not found" included.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Writes a file so that readers see the old content or the new, never
    /// part of it: a temporary sibling, flushed to disk, renamed over the
    /// target. Missing parent folders are created.
    ///
    /// # Errors
    /// I/O errors from the temporary file or the rename.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Deletes a file; one that is already gone is not an error.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// Asks the sync provider to download a file it has evicted, given the
    /// path of its placeholder (`.0003.bin.icloud` for `0003.bin`). The
    /// default does nothing, for providers without placeholders.
    ///
    /// # Errors
    /// Implementations may fail to start the download.
    fn request_download(&self, placeholder: &Path) -> io::Result<()> {
        let _ = placeholder;
        Ok(())
    }
}

/// [`SyncFiles`] over plain paths with `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainSyncFiles;

/// Suffix of in-flight writes; readers skip these, and the `.gitignore`
/// snippet keeps them out of git.
pub(crate) const TEMP_SUFFIX: &str = ".tmp";
/// Suffix of an iCloud placeholder: `.0003.bin.icloud` stands for `0003.bin`.
pub(crate) const ICLOUD_PLACEHOLDER_SUFFIX: &str = ".icloud";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl SyncFiles for PlainSyncFiles {
    fn list(&self, dir: &Path) -> io::Result<Vec<SyncEntry>> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut listing = Vec::new();
        for entry in entries {
            let entry = entry?;
            // An entry that vanishes between the listing and its type (a sync
            // client's own temp file) is not an error for the folder.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            listing.push(SyncEntry {
                path: entry.path(),
                is_dir: file_type.is_dir(),
            });
        }
        Ok(listing)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path has no file name",
            ));
        };
        fs::create_dir_all(dir)?;
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = dir.join(format!(
            ".{}.{}-{counter}{TEMP_SUFFIX}",
            name.to_string_lossy(),
            std::process::id()
        ));
        let written = write_and_sync(&temp_path, bytes).and_then(|()| fs::rename(&temp_path, path));
        if written.is_err() {
            let _ = fs::remove_file(&temp_path);
            return written;
        }
        // Make the rename durable where a folder can be opened and synced.
        if let Ok(dir_handle) = fs::File::open(dir) {
            let _ = dir_handle.sync_all();
        }
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_create_folders_and_leave_no_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sync/dev/0001.bin");
        PlainSyncFiles.write_atomic(&path, b"one").unwrap();
        PlainSyncFiles.write_atomic(&path, b"two").unwrap();
        assert_eq!(PlainSyncFiles.read(&path).unwrap(), b"two");
        let listing = PlainSyncFiles.list(path.parent().unwrap()).unwrap();
        assert_eq!(listing.len(), 1);
        assert!(!listing[0].is_dir);
        PlainSyncFiles.remove(&path).unwrap();
        PlainSyncFiles.remove(&path).unwrap();
        assert!(
            PlainSyncFiles
                .list(&temp.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }
}
