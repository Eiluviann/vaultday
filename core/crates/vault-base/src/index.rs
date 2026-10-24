//! `FileIndex`: the per-file modification-time cache the app refreshes
//! before a query. It lives in memory; [`crate::IndexStore`] persists it per
//! device and [`crate::VaultWatcher`] keeps it current.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use thiserror::Error;

use crate::file_access::{DirEntry, EntryKind, FileAccess, FileAccessError};
use crate::workspace::{SETTINGS_FILE_NAME, WORKSPACE_DIR_NAME, find_linked_folders};

/// Which files an index covers. Globs match root-relative paths with `/`
/// separators; `*` does not cross a `/`, `**` does. A folder is pruned when an
/// exclude pattern matches its path with a trailing `/` (so `**/Archive/**`
/// prunes every `Archive` folder). Folders that are themselves workspaces
/// (they hold `.vaultday/settings.json`) are always pruned: an index never
/// reads another workspace's notes. The root's linked folders (see
/// [`crate::linked_folders`]) are walked through their links, so their files
/// keep root-relative paths; no other symlink is followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexScope {
    /// Glob patterns a file must match.
    pub include: Vec<String>,
    /// Glob patterns that exclude a file or prune a folder.
    pub exclude: Vec<String>,
    /// Only files modified within this many days of the refresh time, when set.
    pub edited_within_days: Option<u32>,
}

/// What the index remembers about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    /// Last modification time.
    pub modified: SystemTime,
    /// Size in bytes.
    pub len: u64,
}

/// The in-memory index over one root.
#[derive(Debug, Clone)]
pub struct FileIndex {
    root: PathBuf,
    scope: IndexScope,
    include: GlobSet,
    exclude: GlobSet,
    entries: BTreeMap<PathBuf, FileStamp>,
}

/// What a refresh changed, as root-relative paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexDelta {
    /// Files seen for the first time.
    pub added: Vec<PathBuf>,
    /// Files whose stamp changed.
    pub changed: Vec<PathBuf>,
    /// Files no longer present or no longer in scope.
    pub removed: Vec<PathBuf>,
}

/// Why an index could not be built.
#[derive(Debug, Error)]
pub enum IndexError {
    /// A glob pattern does not compile.
    #[error("glob pattern {pattern:?} is invalid: {source}")]
    InvalidGlob {
        /// The pattern.
        pattern: String,
        /// The compile error.
        #[source]
        source: globset::Error,
    },
}

const SECONDS_PER_DAY: u64 = 86_400;

impl Default for IndexScope {
    fn default() -> Self {
        IndexScope {
            include: vec!["**/*.md".to_owned()],
            exclude: vec![
                "**/.obsidian/**".to_owned(),
                "**/.vaultday/**".to_owned(),
                "**/.trash/**".to_owned(),
                "**/_templates/**".to_owned(),
                "**/.git/**".to_owned(),
                "**/node_modules/**".to_owned(),
            ],
            edited_within_days: None,
        }
    }
}

impl IndexScope {
    /// The default scope narrowed to files edited in the last `days` days.
    #[must_use]
    pub fn edited_within(days: u32) -> IndexScope {
        IndexScope {
            edited_within_days: Some(days),
            ..IndexScope::default()
        }
    }
}

impl FileIndex {
    /// Creates an empty index over `root` with the given scope.
    ///
    /// # Errors
    /// [`IndexError::InvalidGlob`] when a pattern does not compile.
    pub fn new(root: &Path, scope: IndexScope) -> Result<FileIndex, IndexError> {
        Ok(FileIndex {
            root: root.to_path_buf(),
            include: compile_globs(&scope.include)?,
            exclude: compile_globs(&scope.exclude)?,
            scope,
            entries: BTreeMap::new(),
        })
    }

    /// The indexed root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The scope the index was built with.
    #[must_use]
    pub fn scope(&self) -> &IndexScope {
        &self.scope
    }

    /// Every indexed file with its stamp, as root-relative paths in order.
    pub fn files(&self) -> impl Iterator<Item = (&Path, &FileStamp)> {
        self.entries
            .iter()
            .map(|(path, stamp)| (path.as_path(), stamp))
    }

    /// The stamp of one root-relative path.
    #[must_use]
    pub fn stamp(&self, relative_path: &Path) -> Option<&FileStamp> {
        self.entries.get(relative_path)
    }

    /// Number of indexed files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// True when the root-relative path matches the include globs and none of
    /// the exclude globs, and its modification time lies in the edited-within
    /// window when one is set.
    #[must_use]
    pub fn is_in_scope(
        &self,
        relative_path: &Path,
        modified: Option<SystemTime>,
        now: SystemTime,
    ) -> bool {
        let key = glob_key(relative_path);
        if !self.include.is_match(&key) || self.exclude.is_match(&key) {
            return false;
        }
        match self.scope.edited_within_days {
            None => true,
            Some(days) => modified.is_some_and(|modified| {
                let window = Duration::from_secs(u64::from(days) * SECONDS_PER_DAY);
                now.duration_since(modified)
                    .map_or(true, |age| age <= window)
            }),
        }
    }

    /// Walks the root, updates the stamps and reports what changed.
    ///
    /// # Errors
    /// File errors other than a folder vanishing mid-walk.
    pub fn refresh(
        &mut self,
        file_access: &dyn FileAccess,
        now: SystemTime,
    ) -> Result<IndexDelta, FileAccessError> {
        let links = self.followed_links(file_access)?;
        let seen = self.collect_below(&self.root, &links, file_access, now)?;
        let mut delta = IndexDelta::default();
        for (path, stamp) in &seen {
            match self.entries.get(path) {
                None => delta.added.push(path.clone()),
                Some(previous) if previous != stamp => delta.changed.push(path.clone()),
                Some(_) => {}
            }
        }
        for path in self.entries.keys() {
            if !seen.contains_key(path) {
                delta.removed.push(path.clone());
            }
        }
        self.entries = seen;
        Ok(delta)
    }

    /// Updates the stamps of just the given root-relative paths, the cheap
    /// refresh a file watcher feeds. A path that is gone is dropped together
    /// with anything below it; a folder is re-walked; a file is stamped when
    /// it is in scope and dropped when it is not. As in [`FileIndex::refresh`],
    /// nothing inside a nested workspace is indexed.
    ///
    /// # Errors
    /// File errors other than a path vanishing mid-update.
    pub fn apply_changes(
        &mut self,
        file_access: &dyn FileAccess,
        relative_paths: &[PathBuf],
        now: SystemTime,
    ) -> Result<IndexDelta, FileAccessError> {
        let mut delta = IndexDelta::default();
        let links = self.followed_links(file_access)?;
        for relative in relative_paths {
            let absolute = self.root.join(relative);
            let seen = if self.is_through_unfollowed_link(relative, &links, file_access)?
                || self.is_inside_foreign_workspace(relative, file_access)?
            {
                    BTreeMap::new()
                } else {
                    self.collect_changed(relative, &absolute, &links, file_access, now)?
                };
            let stale: Vec<PathBuf> = self
                .entries
                .keys()
                .filter(|known| known.starts_with(relative) && !seen.contains_key(*known))
                .cloned()
                .collect();
            for path in stale {
                self.entries.remove(&path);
                delta.removed.push(path);
            }
            for (path, stamp) in seen {
                match self.entries.insert(path.clone(), stamp) {
                    None => delta.added.push(path),
                    Some(previous) if previous != stamp => delta.changed.push(path),
                    Some(_) => {}
                }
            }
        }
        Ok(delta)
    }

    /// Rebuilds an index from stamps read back from a store. Entries outside
    /// the scope are dropped, so a stale store can never widen an index.
    ///
    /// # Errors
    /// [`IndexError::InvalidGlob`] when a pattern does not compile.
    pub fn from_entries(
        root: &Path,
        scope: IndexScope,
        entries: impl IntoIterator<Item = (PathBuf, FileStamp)>,
    ) -> Result<FileIndex, IndexError> {
        let mut index = FileIndex::new(root, scope)?;
        // The edited-within window depends on the clock and is re-applied by
        // the next refresh; the globs are checked here.
        index.entries = entries
            .into_iter()
            .filter(|(path, _)| {
                let key = glob_key(path);
                index.include.is_match(&key) && !index.exclude.is_match(&key)
            })
            .collect();
        Ok(index)
    }

    /// The links of the root's linked folders, the only symlinks a walk follows.
    fn followed_links(
        &self,
        file_access: &dyn FileAccess,
    ) -> Result<Vec<PathBuf>, FileAccessError> {
        Ok(find_linked_folders(&self.root, file_access)?
            .into_iter()
            .map(|linked| linked.link)
            .collect())
    }

    /// Walks `start` (a folder below the root) and stamps every file in scope.
    fn collect_below(
        &self,
        start: &Path,
        links: &[PathBuf],
        file_access: &dyn FileAccess,
        now: SystemTime,
    ) -> Result<BTreeMap<PathBuf, FileStamp>, FileAccessError> {
        let mut seen: BTreeMap<PathBuf, FileStamp> = BTreeMap::new();
        let mut pending = vec![start.to_path_buf()];
        while let Some(folder) = pending.pop() {
            let entries = match file_access.read_dir(&folder) {
                Ok(entries) => entries,
                Err(FileAccessError::NotFound(_)) => continue,
                // A linked folder this process may not read is left out, not fatal.
                Err(_) if links.iter().any(|link| folder.starts_with(link)) => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let Ok(relative) = entry.path.strip_prefix(&self.root) else {
                    continue;
                };
                match entry.kind {
                    EntryKind::Directory => {
                        let is_excluded = self.exclude.is_match(format!("{}/", glob_key(relative)));
                        if !is_excluded && !is_foreign_workspace(&entry.path, file_access)? {
                            pending.push(entry.path.clone());
                        }
                    }
                    EntryKind::File => {
                        if self.is_in_scope(relative, entry.modified, now) {
                            seen.insert(relative.to_path_buf(), stamp_of(&entry));
                        }
                    }
                    EntryKind::Other => {
                        let is_excluded = self.exclude.is_match(format!("{}/", glob_key(relative)));
                        if !is_excluded && links.contains(&entry.path) {
                            pending.push(entry.path.clone());
                        }
                    }
                }
            }
        }
        Ok(seen)
    }

    /// The stamps a changed path contributes: a folder's files, a file's own
    /// stamp when it is in scope, nothing when the path is gone.
    fn collect_changed(
        &self,
        relative: &Path,
        absolute: &Path,
        links: &[PathBuf],
        file_access: &dyn FileAccess,
        now: SystemTime,
    ) -> Result<BTreeMap<PathBuf, FileStamp>, FileAccessError> {
        let mut seen = BTreeMap::new();
        match file_access.stat(absolute)? {
            Some(entry) if entry.kind == EntryKind::Directory => {
                seen = self.collect_below(absolute, links, file_access, now)?;
            }
            Some(entry) if entry.kind == EntryKind::File => {
                if self.is_in_scope(relative, entry.modified, now) {
                    seen.insert(relative.to_path_buf(), stamp_of(&entry));
                }
            }
            _ => {}
        }
        Ok(seen)
    }

    /// True when `relative` or a folder above it, below the root, is a
    /// symlink other than a linked folder's link. A walk never enters one, and
    /// a watcher that follows links (inotify does) reports the files below it.
    fn is_through_unfollowed_link(
        &self,
        relative: &Path,
        links: &[PathBuf],
        file_access: &dyn FileAccess,
    ) -> Result<bool, FileAccessError> {
        for folder in relative.ancestors() {
            if folder.as_os_str().is_empty() {
                break;
            }
            let absolute = self.root.join(folder);
            if file_access.link_target(&absolute)?.is_some() && !links.contains(&absolute) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// True when `relative` or a folder above it, below the root, holds
    /// another workspace's settings.
    fn is_inside_foreign_workspace(
        &self,
        relative: &Path,
        file_access: &dyn FileAccess,
    ) -> Result<bool, FileAccessError> {
        for folder in relative.ancestors() {
            if folder.as_os_str().is_empty() {
                break;
            }
            if is_foreign_workspace(&self.root.join(folder), file_access)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn stamp_of(entry: &DirEntry) -> FileStamp {
    FileStamp {
        modified: entry.modified.unwrap_or(SystemTime::UNIX_EPOCH),
        len: entry.len,
    }
}

/// True when `folder` holds `.vaultday/settings.json`, so it belongs to another workspace.
fn is_foreign_workspace(
    folder: &Path,
    file_access: &dyn FileAccess,
) -> Result<bool, FileAccessError> {
    let marker = folder.join(WORKSPACE_DIR_NAME).join(SETTINGS_FILE_NAME);
    Ok(file_access
        .stat(&marker)?
        .is_some_and(|entry| entry.kind == EntryKind::File))
}

fn compile_globs(patterns: &[String]) -> Result<GlobSet, IndexError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|source| IndexError::InvalidGlob {
                pattern: pattern.clone(),
                source,
            })?;
        builder.add(glob);
    }
    builder.build().map_err(|source| IndexError::InvalidGlob {
        pattern: patterns.join(", "),
        source,
    })
}

/// A root-relative path as a `/`-joined string, whatever the platform separator.
fn glob_key(relative_path: &Path) -> String {
    relative_path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::{FileIndex, IndexScope};
    use crate::file_access::PlainFileAccess;
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    #[test]
    fn scope_applies_include_exclude_and_the_edited_window() {
        let index =
            FileIndex::new(Path::new("/vault"), IndexScope::edited_within(7)).expect("index");
        let now = SystemTime::now();
        let fresh = Some(now - Duration::from_secs(86_400));
        let stale = Some(now - Duration::from_secs(30 * 86_400));
        assert!(index.is_in_scope(Path::new("Projects/a.md"), fresh, now));
        assert!(!index.is_in_scope(Path::new("Projects/a.md"), stale, now));
        assert!(!index.is_in_scope(Path::new("Projects/a.md"), None, now));
        assert!(!index.is_in_scope(Path::new(".obsidian/a.md"), fresh, now));
        assert!(!index.is_in_scope(Path::new("Clients/Acme/.obsidian/a.md"), fresh, now));
        assert!(!index.is_in_scope(Path::new("Clients/Acme/_templates/story.md"), fresh, now));
        assert!(!index.is_in_scope(Path::new("Projects/a.txt"), fresh, now));
        assert!(index.is_in_scope(Path::new("top.md"), fresh, now));
        let narrow = FileIndex::new(
            Path::new("/vault"),
            IndexScope {
                include: vec!["Archive/*.md".to_owned()],
                exclude: Vec::new(),
                edited_within_days: None,
            },
        )
        .expect("index");
        assert!(narrow.is_in_scope(Path::new("Archive/a.md"), None, now));
        assert!(
            !narrow.is_in_scope(Path::new("Archive/sub/a.md"), None, now),
            "`*` does not cross a slash"
        );
        assert!(
            FileIndex::new(
                Path::new("/vault"),
                IndexScope {
                    include: vec!["[".to_owned()],
                    ..IndexScope::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn refresh_reports_added_changed_and_removed() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path();
        fs::create_dir_all(root.join("Projects")).expect("mkdir");
        fs::create_dir_all(root.join(".obsidian")).expect("mkdir");
        fs::write(root.join("Projects/a.md"), "a").expect("write");
        fs::write(root.join(".obsidian/app.json"), "{}").expect("write");
        fs::write(root.join("notes.txt"), "x").expect("write");
        let mut index = FileIndex::new(root, IndexScope::default()).expect("index");
        let access = PlainFileAccess;

        let first = index.refresh(&access, SystemTime::now()).expect("refresh");
        assert_eq!(first.added, [Path::new("Projects/a.md")]);
        assert!(first.changed.is_empty() && first.removed.is_empty());
        assert_eq!(index.len(), 1);

        fs::write(root.join("Projects/a.md"), "a longer body").expect("write");
        fs::write(root.join("b.md"), "b").expect("write");
        let second = index.refresh(&access, SystemTime::now()).expect("refresh");
        assert_eq!(second.added, [Path::new("b.md")]);
        assert_eq!(second.changed, [Path::new("Projects/a.md")]);

        fs::remove_file(root.join("b.md")).expect("remove");
        let third = index.refresh(&access, SystemTime::now()).expect("refresh");
        assert_eq!(third.removed, [Path::new("b.md")]);
        assert!(index.stamp(Path::new("Projects/a.md")).is_some());
    }

    #[test]
    fn nested_workspaces_and_excluded_folders_are_pruned() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path();
        fs::create_dir_all(root.join("Clients/Acme/.vaultday")).expect("mkdir");
        fs::create_dir_all(root.join("Clients/Acme/Projects")).expect("mkdir");
        fs::create_dir_all(root.join("Notes/.obsidian")).expect("mkdir");
        fs::create_dir_all(root.join("Archive/old")).expect("mkdir");
        fs::write(root.join("Clients/Acme/.vaultday/settings.json"), "{}").expect("write");
        fs::write(root.join("Clients/Acme/Projects/Portal.md"), "x").expect("write");
        fs::write(root.join("Clients/readme.md"), "x").expect("write");
        fs::write(root.join("Notes/.obsidian/workspace.md"), "x").expect("write");
        fs::write(root.join("Notes/a.md"), "x").expect("write");
        fs::write(root.join("Archive/old/b.md"), "x").expect("write");
        let mut scope = IndexScope::default();
        scope.exclude.push("**/Archive/**".to_owned());
        let mut index = FileIndex::new(root, scope).expect("index");
        let _ = index
            .refresh(&PlainFileAccess, SystemTime::now())
            .expect("refresh");
        let files: Vec<&Path> = index.files().map(|(path, _)| path).collect();
        assert_eq!(
            files,
            [Path::new("Clients/readme.md"), Path::new("Notes/a.md")]
        );
    }
}
