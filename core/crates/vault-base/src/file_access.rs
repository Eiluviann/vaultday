//! The `FileAccess` port: how the core touches the file system. Desktop and
//! tests use the plain-path implementation; sandboxed apps supply their own
//! (security-scoped bookmarks on Apple).

use std::any::Any;
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

/// Keeps a watch alive; dropping it stops the watch.
#[derive(Debug)]
pub struct WatchHandle {
    _guard: Option<Box<dyn Any + Send>>,
}

impl WatchHandle {
    /// A handle that keeps `guard` alive until it is dropped. Implementations
    /// of [`FileAccess::watch`] put whatever owns their watch in the guard.
    #[must_use]
    pub fn new(guard: Box<dyn Any + Send>) -> WatchHandle {
        WatchHandle {
            _guard: Some(guard),
        }
    }

    /// A handle for a watch that never fires.
    #[must_use]
    pub fn inert() -> WatchHandle {
        WatchHandle { _guard: None }
    }
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
    /// [`EntryKind::Other`], so linked notes and folders stay out of walks
    /// unless a walk follows a workspace's linked folders on purpose, while
    /// [`FileAccess::stat`] follows links.
    ///
    /// # Errors
    /// [`FileAccessError::NotFound`] when the directory is missing, otherwise I/O errors.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FileAccessError>;

    /// Describes one path, `None` when nothing exists there.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn stat(&self, path: &Path) -> Result<Option<DirEntry>, FileAccessError>;

    /// What the symlink at `path` points at, as written in the link; `None`
    /// when `path` is not a symlink or does not exist. Ports without symlinks
    /// keep the default, which never reports one, so no link is followed.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn link_target(&self, path: &Path) -> Result<Option<PathBuf>, FileAccessError> {
        let _ = path;
        Ok(None)
    }

    /// `path` with every symlink resolved, `None` when nothing exists there.
    /// The default returns the path as it is.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FileAccessError> {
        Ok(Some(path.to_path_buf()))
    }

    /// Reads a UTF-8 text file.
    ///
    /// # Errors
    /// [`FileAccessError::NotFound`], [`FileAccessError::InvalidUtf8`] or I/O errors.
    fn read_to_string(&self, path: &Path) -> Result<String, FileAccessError>;

    /// Writes a file so that readers see either the old or the new content,
    /// never a partial write. The plain implementation writes a temporary file
    /// beside the target and renames it over, keeping the target's
    /// permissions; a target that is a symlink is written through to the file
    /// it points at, so the link stays a link.
    ///
    /// # Errors
    /// I/O errors from the temporary file or the rename.
    fn write_atomic(&self, path: &Path, contents: &str) -> Result<(), FileAccessError>;

    /// Deletes a file. Removing a file that is already gone is not an error,
    /// so a sync client that removed it first does not fail the caller.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn remove_file(&self, path: &Path) -> Result<(), FileAccessError>;

    /// Creates a directory and its missing parents; an existing directory is fine.
    ///
    /// # Errors
    /// I/O errors from creating the directories.
    fn create_dir_all(&self, path: &Path) -> Result<(), FileAccessError>;

    /// Starts watching a path and everything below it; `on_change` receives
    /// the path of every file system event, under the path that was passed in.
    /// Events arrive in bursts and may repeat, so consumers debounce them (see
    /// [`crate::watcher`]). Dropping the handle stops the watch. The default
    /// implementation never fires, for ports with no change notification.
    ///
    /// # Errors
    /// Implementations may fail to register a watch.
    fn watch(&self, path: &Path, on_change: WatchCallback) -> Result<WatchHandle, FileAccessError> {
        let _ = (path, on_change);
        Ok(WatchHandle::inert())
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

    fn link_target(&self, path: &Path) -> Result<Option<PathBuf>, FileAccessError> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => fs::read_link(path)
                .map(Some)
                .map_err(|source| io_error("read_link", path, source)),
            Ok(_) => Ok(None),
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

    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FileAccessError> {
        match fs::canonicalize(path) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(None)
            }
            Err(source) => Err(io_error("canonicalize", path, source)),
        }
    }

    fn read_to_string(&self, path: &Path) -> Result<String, FileAccessError> {
        let bytes = fs::read(path).map_err(|source| io_error("read", path, source))?;
        String::from_utf8(bytes).map_err(|_| FileAccessError::InvalidUtf8(path.to_path_buf()))
    }

    fn write_atomic(&self, path: &Path, contents: &str) -> Result<(), FileAccessError> {
        // A rename over a symlink would replace the link with a plain file and
        // leave its target stale, so the write goes to the target.
        let resolved = symlink_target(path).map_err(|source| io_error("write", path, source))?;
        let path = resolved.as_path();
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

    fn remove_file(&self, path: &Path) -> Result<(), FileAccessError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(io_error("remove", path, source)),
        }
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FileAccessError> {
        fs::create_dir_all(path).map_err(|source| io_error("create_dir_all", path, source))
    }

    fn watch(&self, path: &Path, on_change: WatchCallback) -> Result<WatchHandle, FileAccessError> {
        // Some platforms report events under the resolved path (`/private/var`
        // for `/var` on macOS); consumers get them under the path they passed.
        let canonical = fs::canonicalize(path).map_err(|source| io_error("watch", path, source))?;
        let requested = path.to_path_buf();
        let handler = move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else {
                return;
            };
            if matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            for event_path in &event.paths {
                match event_path.strip_prefix(&canonical) {
                    Ok(relative) => on_change(&requested.join(relative)),
                    Err(_) => on_change(event_path),
                }
            }
        };
        let mut watcher = notify::recommended_watcher(handler)
            .map_err(|source| notify_error("watch", path, source))?;
        notify::Watcher::watch(&mut watcher, path, notify::RecursiveMode::Recursive)
            .map_err(|source| notify_error("watch", path, source))?;
        Ok(WatchHandle::new(Box::new(watcher)))
    }
}

fn symlink_target(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
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

fn notify_error(operation: &'static str, path: &Path, source: notify::Error) -> FileAccessError {
    FileAccessError::Io {
        operation,
        path: path.to_path_buf(),
        source: io::Error::other(source),
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

    #[cfg(unix)]
    #[test]
    fn atomic_write_through_a_symlink_updates_the_target_and_keeps_the_link() {
        let directory = tempfile::tempdir().expect("tempdir");
        let access = PlainFileAccess;
        let shared = directory.path().join("shared");
        access.create_dir_all(&shared).expect("create");
        let target = shared.join("note.md");
        access.write_atomic(&target, "one\n").expect("write");
        let link = directory.path().join("link.md");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        access
            .write_atomic(&link, "two\n")
            .expect("write through link");

        let link_metadata = std::fs::symlink_metadata(&link).expect("link metadata");
        assert!(link_metadata.file_type().is_symlink(), "the link survives");
        assert_eq!(access.read_to_string(&target).expect("read"), "two\n");
    }

    #[test]
    fn remove_file_is_idempotent_and_create_dir_all_nests() {
        let directory = tempfile::tempdir().expect("tempdir");
        let access = PlainFileAccess;
        let nested = directory.path().join("a/b/c");
        access.create_dir_all(&nested).expect("create");
        access.create_dir_all(&nested).expect("create again");
        let file = nested.join("note.md");
        access.write_atomic(&file, "x\n").expect("write");
        access.remove_file(&file).expect("remove");
        access.remove_file(&file).expect("remove again");
        assert!(access.stat(&file).expect("stat").is_none());
    }

    #[test]
    fn watching_a_missing_path_fails() {
        let missing = std::env::temp_dir().join("vaultday-definitely-missing-dir");
        assert!(PlainFileAccess.watch(&missing, Box::new(|_| {})).is_err());
    }

    #[test]
    fn an_inert_watch_handle_debugs_as_a_handle() {
        assert!(format!("{:?}", super::WatchHandle::inert()).contains("WatchHandle"));
    }
}
