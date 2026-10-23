//! The byte-level file port of app state as a foreign trait: the Mac app
//! implements [`SyncFilesPort`] with file coordination inside its granted
//! folders, and [`ForeignSyncFiles`] adapts it to [`app_state::SyncFiles`].

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;

use app_state::{PlainSyncFiles, SyncEntry as CoreSyncEntry, SyncFiles};

/// One entry of a folder listing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncEntry {
    /// Absolute path of the entry.
    pub path: String,
    /// Whether the entry is a folder.
    pub is_dir: bool,
}

/// Why a file operation failed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FileFailure {
    /// The file or folder does not exist.
    #[error("{message}")]
    NotFound {
        /// The platform's message.
        message: String,
    },
    /// The platform refused or failed; a folder outside every grant lands here.
    #[error("{message}")]
    Failed {
        /// The platform's message.
        message: String,
    },
}

/// Byte-level file access for app state; the contract is
/// [`app_state::SyncFiles`]'s. Paths are absolute.
#[uniffi::export(with_foreign)]
pub trait SyncFilesPort: Send + Sync {
    /// Lists a folder; a missing folder lists empty.
    ///
    /// # Errors
    /// Failures other than "not found".
    fn list(&self, dir: String) -> Result<Vec<SyncEntry>, FileFailure>;

    /// Reads a whole file.
    ///
    /// # Errors
    /// [`FileFailure::NotFound`] for a missing file, otherwise [`FileFailure::Failed`].
    fn read(&self, path: String) -> Result<Vec<u8>, FileFailure>;

    /// Writes a file atomically, creating missing parent folders.
    ///
    /// # Errors
    /// The write or the rename failed.
    fn write_atomic(&self, path: String, bytes: Vec<u8>) -> Result<(), FileFailure>;

    /// Deletes a file; a missing one is not an error.
    ///
    /// # Errors
    /// Failures other than "not found".
    fn remove(&self, path: String) -> Result<(), FileFailure>;

    /// Asks the sync provider to download the file a placeholder stands for.
    ///
    /// # Errors
    /// The download could not be started.
    fn request_download(&self, placeholder: String) -> Result<(), FileFailure>;
}

/// [`app_state::SyncFiles`] over a platform [`SyncFilesPort`].
pub struct ForeignSyncFiles(Arc<dyn SyncFilesPort>);

/// The plain `std::fs` file port, for tests and hosts without a sandbox.
#[derive(Debug, Default)]
struct PlainFilesPort;

/// File access over plain paths, without coordination or a sandbox grant;
/// tests on the Swift side use it as the reference port.
#[uniffi::export]
#[must_use]
pub fn plain_sync_files() -> Arc<dyn SyncFilesPort> {
    Arc::new(PlainFilesPort)
}

impl ForeignSyncFiles {
    /// Adapts `port`.
    #[must_use]
    pub fn new(port: Arc<dyn SyncFilesPort>) -> ForeignSyncFiles {
        ForeignSyncFiles(port)
    }
}

impl fmt::Debug for ForeignSyncFiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ForeignSyncFiles(..)")
    }
}

impl SyncFiles for ForeignSyncFiles {
    fn list(&self, dir: &Path) -> io::Result<Vec<CoreSyncEntry>> {
        let entries = self.0.list(path_text(dir)).map_err(io_error)?;
        Ok(entries
            .into_iter()
            .map(|entry| CoreSyncEntry {
                path: entry.path.into(),
                is_dir: entry.is_dir,
            })
            .collect())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0.read(path_text(path)).map_err(io_error)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.0
            .write_atomic(path_text(path), bytes.to_vec())
            .map_err(io_error)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.0.remove(path_text(path)).map_err(io_error)
    }

    fn request_download(&self, placeholder: &Path) -> io::Result<()> {
        self.0
            .request_download(path_text(placeholder))
            .map_err(io_error)
    }
}

impl SyncFilesPort for PlainFilesPort {
    fn list(&self, dir: String) -> Result<Vec<SyncEntry>, FileFailure> {
        let entries = PlainSyncFiles.list(dir.as_ref()).map_err(file_failure)?;
        Ok(entries
            .into_iter()
            .map(|entry| SyncEntry {
                path: path_text(&entry.path),
                is_dir: entry.is_dir,
            })
            .collect())
    }

    fn read(&self, path: String) -> Result<Vec<u8>, FileFailure> {
        PlainSyncFiles.read(path.as_ref()).map_err(file_failure)
    }

    fn write_atomic(&self, path: String, bytes: Vec<u8>) -> Result<(), FileFailure> {
        PlainSyncFiles
            .write_atomic(path.as_ref(), &bytes)
            .map_err(file_failure)
    }

    fn remove(&self, path: String) -> Result<(), FileFailure> {
        PlainSyncFiles.remove(path.as_ref()).map_err(file_failure)
    }

    fn request_download(&self, _placeholder: String) -> Result<(), FileFailure> {
        Ok(())
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for FileFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        FileFailure::Failed {
            message: error.reason,
        }
    }
}

pub(crate) fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn io_error(failure: FileFailure) -> io::Error {
    match failure {
        FileFailure::NotFound { message } => io::Error::new(io::ErrorKind::NotFound, message),
        FileFailure::Failed { message } => io::Error::other(message),
    }
}

fn file_failure(error: io::Error) -> FileFailure {
    let message = error.to_string();
    if error.kind() == io::ErrorKind::NotFound {
        FileFailure::NotFound { message }
    } else {
        FileFailure::Failed { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_port_round_trips_through_the_adapter() {
        let temp = tempfile::tempdir().unwrap();
        let files = ForeignSyncFiles::new(plain_sync_files());
        let path = temp.path().join("sync/dev/0001.bin");
        files.write_atomic(&path, b"sealed").unwrap();
        assert_eq!(files.read(&path).unwrap(), b"sealed");
        assert_eq!(files.list(path.parent().unwrap()).unwrap().len(), 1);
        files.remove(&path).unwrap();
        files.remove(&path).unwrap();
        let missing = files.read(&path).unwrap_err();
        assert_eq!(
            missing.kind(),
            io::ErrorKind::NotFound,
            "not found survives the bridge"
        );
        assert!(files.list(&temp.path().join("none")).unwrap().is_empty());
        files.request_download(&path).unwrap();
    }
}
