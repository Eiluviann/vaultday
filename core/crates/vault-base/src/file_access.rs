//! The `FileAccess` port: how the core touches the file system. Desktop and
//! tests use the plain-path implementation; sandboxed apps supply their own
//! (security-scoped bookmarks on Apple).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use thiserror::Error;

/// What kind of entry a directory listing returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symlink or anything else.
    Other,
}

/// One entry of a directory listing or a `stat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Absolute path of the entry.
    pub path: PathBuf,
    /// File, directory or other.
    pub kind: EntryKind,
    /// Last modification time when the platform reports one.
    pub modified: Option<SystemTime>,
    /// Size in bytes, 0 for directories.
    pub len: u64,
}

/// Keeps a watch alive; dropping it stops the watch. The base crate ships a
/// no-op watch, the watcher itself arrives with the index persistence.
#[derive(Debug)]
pub struct WatchHandle {
    _private: (),
}

/// Callback a watch fires with the changed path.
pub type WatchCallback = Box<dyn Fn(&Path) + Send + Sync>;

/// Why a file operation failed.
#[derive(Debug, Error)]
pub enum FileAccessError {
    /// The path does not exist.
    #[error("{0} does not exist")]
    NotFound(PathBuf),
    /// The file is not UTF-8.
    #[error("{0} is not UTF-8 text")]
    InvalidUtf8(PathBuf),
    /// Any other I/O failure.
    #[error("{operation} {path} failed: {source}")]
    Io {
        /// What was attempted.
        operation: &'static str,
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
}

/// The file system as the core sees it.
pub trait FileAccess: Send + Sync {
    /// Lists the entries of a directory in no particular order. The plain
    /// implementation does not follow symlinks: a link is reported as
    /// [`EntryKind::Other`], so linked notes and folders stay out of walks,
    /// while [`FileAccess::stat`] follows links.
    ///
    /// # Errors
    /// [`FileAccessError::NotFound`] when the directory is missing, otherwise I/O errors.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FileAccessError>;

    /// Describes one path, `None` when nothing exists there.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn stat(&self, path: &Path) -> Result<Option<DirEntry>, FileAccessError>;

    /// Reads a UTF-8 text file.
    ///
    /// # Errors
    /// [`FileAccessError::NotFound`], [`FileAccessError::InvalidUtf8`] or I/O errors.
    fn read_to_string(&self, path: &Path) -> Result<String, FileAccessError>;

    /// Writes a file so that readers see either the old or the new content,
    /// never a partial write. The plain implementation writes a temporary file
    /// beside the target and renames it over, keeping the target's
    /// permissions; a target that is a symlink is replaced by a regular file.
    ///
    /// # Errors
    /// I/O errors from the temporary file or the rename.
    fn write_atomic(&self, path: &Path, contents: &str) -> Result<(), FileAccessError>;

    /// Starts watching a path. The base implementation never fires; the
    /// returned handle is still the contract for callers.
    ///
    /// # Errors
    /// Implementations may fail to register a watch.
    fn watch(&self, path: &Path, on_change: WatchCallback) -> Result<WatchHandle, FileAccessError> {
        let _ = (path, on_change);
        Ok(WatchHandle { _private: () })
    }
}

/// `FileAccess` over plain paths with `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainFileAccess;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl FileAccess for PlainFileAccess {
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FileAccessError> {
        let entries = fs::read_dir(path).map_err(|source| io_error("read_dir", path, source))?;
        let mut listing = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| io_error("read_dir", path, source))?;
            let entry_path = entry.path();
            // An entry that vanishes between the listing and its metadata
            // (editor temp files, sync clients) is not an error for the folder.
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => return Err(io_error("stat", &entry_path, source)),
            };
            listing.push(dir_entry(entry_path, &metadata));
        }
        Ok(listing)
    }

    fn stat(&self, path: &Path) -> Result<Option<DirEntry>, FileAccessError> {
        match fs::metadata(path) {
            Ok(metadata) => Ok(Some(dir_entry(path.to_path_buf(), &metadata))),
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(None)
            }
            Err(source) => Err(io_error("stat", path, source)),
        }
    }

    fn read_to_string(&self, path: &Path) -> Result<String, FileAccessError> {
        let bytes = fs::read(path).map_err(|source| io_error("read", path, source))?;
        String::from_utf8(bytes).map_err(|_| FileAccessError::InvalidUtf8(path.to_path_buf()))
    }

    fn write_atomic(&self, path: &Path, contents: &str) -> Result<(), FileAccessError> {
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        if path.file_name().is_none() {
            return Err(io_error(
                "write",
                path,
                io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"),
            ));
        }
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = directory.join(format!(".vaultday-{}-{counter}.tmp", std::process::id()));
        let existing_permissions = fs::metadata(path)
            .ok()
            .map(|metadata| metadata.permissions());
        if let Err(source) = write_and_sync(&temp_path, contents, existing_permissions) {
            let _ = fs::remove_file(&temp_path);
            return Err(io_error("write", path, source));
        }
        if let Err(source) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(io_error("rename", path, source));
        }
        // Make the rename itself durable where the platform allows a
        // directory to be opened; elsewhere the rename is already visible.
        if let Ok(directory_handle) = fs::File::open(directory) {
            let _ = directory_handle.sync_all();
        }
        Ok(())
    }
}

fn write_and_sync(
    temp_path: &Path,
    contents: &str,
    permissions: Option<fs::Permissions>,
) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::File::create_new(temp_path)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

fn dir_entry(path: PathBuf, metadata: &fs::Metadata) -> DirEntry {
    let kind = if metadata.is_file() {
        EntryKind::File
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::Other
    };
    DirEntry {
        path,
        kind,
        modified: metadata.modified().ok(),
        len: metadata.len(),
    }
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> FileAccessError {
    if source.kind() == io::ErrorKind::NotFound {
        return FileAccessError::NotFound(path.to_path_buf());
    }
    FileAccessError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{EntryKind, FileAccess, FileAccessError, PlainFileAccess};

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("note.md");
        let access = PlainFileAccess;
        access.write_atomic(&path, "one\n").expect("first write");
        access.write_atomic(&path, "two\n").expect("second write");
        assert_eq!(access.read_to_string(&path).expect("read"), "two\n");
        let names: Vec<String> = access
            .read_dir(directory.path())
            .expect("list")
            .into_iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(names, ["note.md"]);
        let stat = access.stat(&path).expect("stat").expect("exists");
        assert_eq!(stat.kind, EntryKind::File);
        assert_eq!(stat.len, 4);
        assert!(
            access
                .stat(&directory.path().join("missing"))
                .expect("stat")
                .is_none()
        );
    }

    #[test]
    fn missing_paths_report_not_found() {
        let access = PlainFileAccess;
        let missing = std::env::temp_dir().join("vaultday-definitely-missing-dir");
        assert!(matches!(
            access.read_dir(&missing),
            Err(FileAccessError::NotFound(_))
        ));
        assert!(matches!(
            access.read_to_string(&missing),
            Err(FileAccessError::NotFound(_))
        ));
    }

    #[test]
    fn the_default_watch_is_a_no_op() {
        let handle = PlainFileAccess
            .watch(std::path::Path::new("."), Box::new(|_| {}))
            .expect("watch");
        assert!(format!("{handle:?}").contains("WatchHandle"));
    }
}
