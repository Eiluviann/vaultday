//! Workspace resolution: a folder is a workspace when it holds
//! `.vaultday/settings.json`. `Workspace.md` is never read at runtime.
//!
//! Paths are compared lexically after `.` and `..` are resolved and relative
//! paths are anchored at the current directory. Symlinks and case-insensitive
//! file systems are not reconciled here: callers that mix aliases of one
//! location canonicalise their paths first.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::file_access::{EntryKind, FileAccess, FileAccessError};
use crate::settings::{SettingsError, WorkspaceKind, WorkspaceSettings};

/// The hidden folder that marks a workspace root.
pub const WORKSPACE_DIR_NAME: &str = ".vaultday";
/// The manifest inside the workspace folder.
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// The folder that marks an Obsidian vault root.
pub const OBSIDIAN_DIR_NAME: &str = ".obsidian";

/// Folders a discovery walk never descends into.
const SKIPPED_DIRECTORY_NAMES: [&str; 6] = [
    ".obsidian",
    ".git",
    ".trash",
    "node_modules",
    "target",
    ".vaultday",
];

/// The one-line notice for a workspace outside its Obsidian vault.
pub const OBSIDIAN_LINK_FALLBACK_NOTICE: &str =
    "This workspace sits outside its Obsidian vault, so Open in Obsidian may not find the note";

/// An opened workspace.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    /// The folder holding `.vaultday/`.
    pub root: PathBuf,
    /// The nearest ancestor-or-self holding `.obsidian/`, else the root.
    pub vault_root: PathBuf,
    /// The parsed manifest.
    pub settings: WorkspaceSettings,
}

/// Every workspace reachable from the granted sources, with the deepest-match rule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceSet {
    workspaces: Vec<Workspace>,
}

/// Why a workspace could not be opened or a set could not be formed.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    /// The folder has no `.vaultday/settings.json`.
    #[error("{0} is not a workspace: it has no .vaultday/settings.json")]
    NotAWorkspace(PathBuf),
    /// The manifest was refused.
    #[error("{path}: {source}")]
    Settings {
        /// The manifest path.
        path: PathBuf,
        /// The refusal.
        #[source]
        source: SettingsError,
    },
    /// The file system failed.
    #[error(transparent)]
    FileAccess(#[from] FileAccessError),
    /// Two work workspaces nest, which the client-separation rule forbids.
    #[error("work workspaces may not overlap: {inner} lies inside {outer}")]
    OverlappingWorkWorkspaces {
        /// The enclosing work workspace root.
        outer: PathBuf,
        /// The nested work workspace root.
        inner: PathBuf,
    },
}

impl Workspace {
    /// Opens the workspace rooted at `root`.
    ///
    /// # Errors
    /// [`WorkspaceError::NotAWorkspace`] without a manifest, [`WorkspaceError::Settings`]
    /// when the manifest is refused, file errors otherwise.
    pub fn open(root: &Path, file_access: &dyn FileAccess) -> Result<Workspace, WorkspaceError> {
        let root = normalize_path(root);
        let root = root.as_path();
        let settings_path = settings_path(root);
        let text = match file_access.read_to_string(&settings_path) {
            Ok(text) => text,
            Err(FileAccessError::NotFound(_)) => {
                return Err(WorkspaceError::NotAWorkspace(root.to_path_buf()));
            }
            Err(error) => return Err(error.into()),
        };
        let settings =
            WorkspaceSettings::from_json(&text).map_err(|source| WorkspaceError::Settings {
                path: settings_path,
                source,
            })?;
        Ok(Workspace {
            root: root.to_path_buf(),
            vault_root: find_vault_root(root, file_access)?,
            settings,
        })
    }

    /// The path of this workspace's manifest.
    #[must_use]
    pub fn settings_path(&self) -> PathBuf {
        settings_path(&self.root)
    }

    /// The workspace's log folder as an absolute path.
    #[must_use]
    pub fn log_folder_path(&self) -> PathBuf {
        self.root.join(&self.settings.log_folder)
    }

    /// True for a client workspace.
    #[must_use]
    pub fn is_work(&self) -> bool {
        self.settings.kind == WorkspaceKind::Work
    }

    /// True when `path` lies inside this workspace (root included).
    #[must_use]
    pub fn contains(&self, path: &Path) -> bool {
        normalize_path(path).starts_with(&self.root)
    }

    /// What a preview says when [`Workspace::obsidian_link`] has to fall
    /// back: the workspace root lies outside the folder holding `.obsidian/`,
    /// so the link names the note from the vault folder's name and the path
    /// below the workspace only, and Obsidian may not find it. `None` when
    /// the root is inside the vault, the usual case.
    #[must_use]
    pub fn obsidian_link_notice(&self) -> Option<&'static str> {
        (!normalize_path(&self.root).starts_with(normalize_path(&self.vault_root)))
            .then_some(OBSIDIAN_LINK_FALLBACK_NOTICE)
    }

    /// The `obsidian://open` link for the note at `relative_path` (relative to
    /// the workspace root). Obsidian names the vault by the folder holding
    /// `.obsidian/`, which can lie above the workspace root; the file is the
    /// path from that folder without `.md`. Every character outside the
    /// unreserved set is percent-encoded.
    #[must_use]
    pub fn obsidian_link(&self, relative_path: &Path) -> String {
        let from_vault_root = self
            .root
            .strip_prefix(&self.vault_root)
            .unwrap_or(Path::new(""))
            .join(relative_path);
        let file = from_vault_root.to_string_lossy().replace('\\', "/");
        let file = file.strip_suffix(".md").unwrap_or(&file);
        let vault = self
            .vault_root
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        format!(
            "obsidian://open?vault={}&file={}",
            percent_encode(&vault),
            percent_encode(file)
        )
    }

    /// The `obsidian://open` link that opens the note at `relative_path` at
    /// its heading `heading`: the file parameter carries `#heading`.
    #[must_use]
    pub fn obsidian_link_at(&self, relative_path: &Path, heading: &str) -> String {
        format!(
            "{}{}",
            self.obsidian_link(relative_path),
            percent_encode(&format!("#{heading}"))
        )
    }
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

impl WorkspaceSet {
    /// Builds a set from already-opened workspaces, refusing nested work
    /// workspaces. Duplicate roots collapse to one.
    ///
    /// # Errors
    /// [`WorkspaceError::OverlappingWorkWorkspaces`] when two work workspaces nest.
    pub fn from_workspaces(workspaces: Vec<Workspace>) -> Result<WorkspaceSet, WorkspaceError> {
        let mut unique: Vec<Workspace> = Vec::with_capacity(workspaces.len());
        for workspace in workspaces {
            if !unique.iter().any(|known| known.root == workspace.root) {
                unique.push(workspace);
            }
        }
        unique.sort_by(|a, b| {
            a.root
                .components()
                .count()
                .cmp(&b.root.components().count())
                .then_with(|| a.root.cmp(&b.root))
        });
        for (position, outer) in unique.iter().enumerate() {
            if !outer.is_work() {
                continue;
            }
            let nested_work = unique
                .get(position + 1..)
                .unwrap_or_default()
                .iter()
                .find(|inner| {
                    inner.is_work()
                        && inner.root != outer.root
                        && inner.root.starts_with(&outer.root)
                });
            if let Some(inner) = nested_work {
                return Err(WorkspaceError::OverlappingWorkWorkspaces {
                    outer: outer.root.clone(),
                    inner: inner.root.clone(),
                });
            }
        }
        Ok(WorkspaceSet { workspaces: unique })
    }

    /// Walks every granted source folder, opens each workspace found and
    /// forms the set. A source that is itself a workspace counts too.
    ///
    /// # Errors
    /// Any error opening a workspace, file errors, or the overlap refusal.
    pub fn discover(
        sources: &[PathBuf],
        file_access: &dyn FileAccess,
    ) -> Result<WorkspaceSet, WorkspaceError> {
        let mut workspaces = Vec::new();
        for source in sources {
            for root in find_workspace_roots_below(&normalize_path(source), file_access)? {
                workspaces.push(Workspace::open(&root, file_access)?);
            }
        }
        WorkspaceSet::from_workspaces(workspaces)
    }

    /// The workspace whose root is the deepest ancestor-or-self of `path`.
    #[must_use]
    pub fn resolve(&self, path: &Path) -> Option<&Workspace> {
        let path = normalize_path(path);
        self.workspaces
            .iter()
            .filter(|workspace| path.starts_with(&workspace.root))
            .max_by_key(|workspace| workspace.root.components().count())
    }

    /// Every workspace, shallowest root first.
    pub fn iter(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.iter()
    }

    /// Number of workspaces in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.workspaces.len()
    }

    /// True when the set holds no workspace.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.workspaces.is_empty()
    }
}

/// Walks up from `start` (a file or folder) to the nearest folder holding
/// `.vaultday/settings.json`. The result is normalised like [`Workspace::root`],
/// so a relative `start` still yields an absolute root.
///
/// # Errors
/// File errors other than "not found".
pub fn find_workspace_root(
    start: &Path,
    file_access: &dyn FileAccess,
) -> Result<Option<PathBuf>, WorkspaceError> {
    let start = normalize_path(start);
    for ancestor in start.ancestors() {
        if is_workspace_root(ancestor, file_access)? {
            return Ok(Some(ancestor.to_path_buf()));
        }
    }
    Ok(None)
}

/// The nearest ancestor-or-self of `workspace_root` that holds `.obsidian/`,
/// falling back to the workspace root itself. The result is normalised.
///
/// # Errors
/// File errors other than "not found".
pub fn find_vault_root(
    workspace_root: &Path,
    file_access: &dyn FileAccess,
) -> Result<PathBuf, WorkspaceError> {
    let workspace_root = normalize_path(workspace_root);
    for ancestor in workspace_root.ancestors() {
        let marker = ancestor.join(OBSIDIAN_DIR_NAME);
        if file_access
            .stat(&marker)?
            .is_some_and(|entry| entry.kind == EntryKind::Directory)
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(workspace_root)
}

/// True when `folder` holds `.vaultday/settings.json`.
///
/// # Errors
/// File errors other than "not found".
pub fn is_workspace_root(
    folder: &Path,
    file_access: &dyn FileAccess,
) -> Result<bool, WorkspaceError> {
    Ok(file_access
        .stat(&settings_path(folder))?
        .is_some_and(|entry| entry.kind == EntryKind::File))
}

fn settings_path(root: &Path) -> PathBuf {
    root.join(WORKSPACE_DIR_NAME).join(SETTINGS_FILE_NAME)
}

/// Anchors a relative path at the current directory and resolves `.` and `..`
/// lexically. Nothing is read from disk.
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Every workspace root at or below `source`, found by walking the tree.
fn find_workspace_roots_below(
    source: &Path,
    file_access: &dyn FileAccess,
) -> Result<Vec<PathBuf>, WorkspaceError> {
    let mut roots = Vec::new();
    let mut pending = vec![source.to_path_buf()];
    while let Some(folder) = pending.pop() {
        if is_workspace_root(&folder, file_access)? {
            roots.push(folder.clone());
        }
        let entries = match file_access.read_dir(&folder) {
            Ok(entries) => entries,
            Err(FileAccessError::NotFound(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            if entry.kind != EntryKind::Directory {
                continue;
            }
            let name = entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if SKIPPED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }
            pending.push(entry.path);
        }
    }
    roots.sort();
    Ok(roots)
}
