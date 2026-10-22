//! The persisted parse index: a small SQLite store, one per device, that
//! remembers each vault's [`FileIndex`] between runs so a launch only re-stats
//! files instead of treating the whole vault as new. It is a cache. The app
//! decides where it lives (the App Group container on the Mac, the XDG state
//! folder on Linux) and it must never sit inside the vault, which this module
//! enforces when saving. A store written by another schema version is emptied
//! and rebuilt, never migrated.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::file_access::{FileAccess, FileAccessError};
use crate::index::{FileIndex, FileStamp, IndexError, IndexScope};
use crate::workspace::normalize_path;

/// The schema version of the store, kept in SQLite's `user_version`.
pub const INDEX_STORE_SCHEMA_VERSION: i32 = 1;

/// Why the store could not be used.
#[derive(Debug, Error)]
pub enum IndexStoreError {
    /// The store path lies inside the vault it would cache.
    #[error("the index store {store} lies inside the vault {vault}; it must be outside")]
    InsideVault {
        /// The store file.
        store: PathBuf,
        /// The vault root.
        vault: PathBuf,
    },
    /// A path is not valid UTF-8 and cannot be a key.
    #[error("{0} is not valid UTF-8")]
    NonUtf8Path(PathBuf),
    /// SQLite failed.
    #[error("index store failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The folder of the store could not be created.
    #[error(transparent)]
    File(#[from] FileAccessError),
    /// A stored scope no longer compiles.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// The scope could not be encoded or decoded.
    #[error("stored index scope is unreadable: {0}")]
    Scope(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct StoredScope {
    include: Vec<String>,
    exclude: Vec<String>,
    edited_within_days: Option<u32>,
}

/// The per-device index store.
#[derive(Debug)]
pub struct IndexStore {
    connection: Connection,
    path: PathBuf,
}

impl IndexStore {
    /// Opens (creating it and its folder when missing) the store at `path`.
    /// A store with another schema version is emptied.
    ///
    /// # Errors
    /// [`IndexStoreError`] when the folder or the database cannot be created.
    pub fn open(path: &Path, file_access: &dyn FileAccess) -> Result<IndexStore, IndexStoreError> {
        let path = normalize_path(path);
        if let Some(folder) = path.parent() {
            file_access.create_dir_all(folder)?;
        }
        let connection = Connection::open(&path)?;
        let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != INDEX_STORE_SCHEMA_VERSION {
            connection.execute_batch(
                "DROP TABLE IF EXISTS index_files; DROP TABLE IF EXISTS index_roots;",
            )?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS index_roots (
                root TEXT PRIMARY KEY NOT NULL,
                scope TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS index_files (
                root TEXT NOT NULL REFERENCES index_roots(root) ON DELETE CASCADE,
                path TEXT NOT NULL,
                modified_seconds INTEGER NOT NULL,
                modified_nanos INTEGER NOT NULL,
                len INTEGER NOT NULL,
                PRIMARY KEY (root, path)
            ) WITHOUT ROWID;",
        )?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "user_version", INDEX_STORE_SCHEMA_VERSION)?;
        Ok(IndexStore { connection, path })
    }

    /// The store file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replaces what the store holds for `index`'s root with the index.
    ///
    /// # Errors
    /// [`IndexStoreError::InsideVault`] when the store lies inside the root,
    /// otherwise SQLite errors.
    pub fn save(&mut self, index: &FileIndex) -> Result<(), IndexStoreError> {
        let root = normalize_path(index.root());
        if self.path.starts_with(&root) {
            return Err(IndexStoreError::InsideVault {
                store: self.path.clone(),
                vault: root,
            });
        }
        let root_key = utf8(&root)?;
        let scope = serde_json::to_string(&StoredScope::from(index.scope()))?;
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM index_files WHERE root = ?1", params![root_key])?;
        transaction.execute("DELETE FROM index_roots WHERE root = ?1", params![root_key])?;
        transaction.execute(
            "INSERT INTO index_roots (root, scope) VALUES (?1, ?2)",
            params![root_key, scope],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO index_files (root, path, modified_seconds, modified_nanos, len)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (relative, stamp) in index.files() {
                let (seconds, nanos) = split_time(stamp.modified);
                insert.execute(params![
                    root_key,
                    utf8(relative)?,
                    seconds,
                    nanos,
                    i64::try_from(stamp.len).unwrap_or(i64::MAX),
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// The saved index for `root`, `None` when nothing was saved or it was
    /// saved with a different `scope` (a changed scope invalidates the cache).
    ///
    /// # Errors
    /// SQLite errors, or a stored scope that cannot be decoded.
    pub fn load(
        &self,
        root: &Path,
        scope: IndexScope,
    ) -> Result<Option<FileIndex>, IndexStoreError> {
        let root = normalize_path(root);
        let root_key = utf8(&root)?;
        let stored_scope: Option<String> = self
            .connection
            .query_row(
                "SELECT scope FROM index_roots WHERE root = ?1",
                params![root_key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(stored_scope) = stored_scope else {
            return Ok(None);
        };
        let stored: StoredScope = serde_json::from_str(&stored_scope)?;
        if stored != StoredScope::from(&scope) {
            return Ok(None);
        }
        let mut statement = self.connection.prepare(
            "SELECT path, modified_seconds, modified_nanos, len FROM index_files
             WHERE root = ?1 ORDER BY path",
        )?;
        let rows = statement.query_map(params![root_key], |row| {
            let path: String = row.get(0)?;
            let seconds: i64 = row.get(1)?;
            let nanos: i64 = row.get(2)?;
            let len: i64 = row.get(3)?;
            Ok((path, seconds, nanos, len))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (path, seconds, nanos, len) = row?;
            entries.push((
                PathBuf::from(path),
                FileStamp {
                    modified: join_time(seconds, nanos),
                    len: u64::try_from(len).unwrap_or(0),
                },
            ));
        }
        Ok(Some(FileIndex::from_entries(&root, scope, entries)?))
    }

    /// Drops everything saved for `root`.
    ///
    /// # Errors
    /// SQLite errors.
    pub fn forget(&mut self, root: &Path) -> Result<(), IndexStoreError> {
        let root = normalize_path(root);
        let root_key = utf8(&root)?;
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM index_files WHERE root = ?1", params![root_key])?;
        transaction.execute("DELETE FROM index_roots WHERE root = ?1", params![root_key])?;
        transaction.commit()?;
        Ok(())
    }
}

impl From<&IndexScope> for StoredScope {
    fn from(scope: &IndexScope) -> StoredScope {
        StoredScope {
            include: scope.include.clone(),
            exclude: scope.exclude.clone(),
            edited_within_days: scope.edited_within_days,
        }
    }
}

fn utf8(path: &Path) -> Result<&str, IndexStoreError> {
    path.to_str()
        .ok_or_else(|| IndexStoreError::NonUtf8Path(path.to_path_buf()))
}

fn split_time(time: SystemTime) -> (i64, i64) {
    let since_epoch = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    (
        i64::try_from(since_epoch.as_secs()).unwrap_or(i64::MAX),
        i64::from(since_epoch.subsec_nanos()),
    )
}

fn join_time(seconds: i64, nanos: i64) -> SystemTime {
    let seconds = u64::try_from(seconds).unwrap_or(0);
    let nanos = u32::try_from(nanos).unwrap_or(0);
    SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos)
}

#[cfg(test)]
mod tests {
    use super::{join_time, split_time};
    use std::time::{Duration, SystemTime};

    #[test]
    fn times_survive_the_round_trip_to_the_nanosecond() {
        let time = SystemTime::UNIX_EPOCH + Duration::new(1_790_000_000, 123_456_789);
        let (seconds, nanos) = split_time(time);
        assert_eq!(join_time(seconds, nanos), time);
    }
}
