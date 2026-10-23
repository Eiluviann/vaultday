//! Where volatile state lives: the parse index, caches and window state.
//! The caller gives the folder (the App Group container on the Mac,
//! `$XDG_STATE_HOME` on Linux later); it is never inside a workspace folder,
//! so nothing volatile syncs with the vault.

use std::path::{Component, Path, PathBuf};

use domain::workspace::WorkspaceId;

use crate::error::AppStateError;

/// Lines the onboarding Sync and backup step offers for a vault that is a
/// git repository. The change files themselves must sync, so only the
/// in-flight temporary files are ignored.
pub const GITIGNORE_SNIPPET: &str = "\
# Vaultday: the encrypted app state in .vaultday/sync must sync with the vault;
# only its in-flight temporary files stay out of git.
.vaultday/sync/**/*.tmp
";

/// A volatile state folder checked to be outside every workspace folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolatileStateDir(PathBuf);

impl VolatileStateDir {
    /// Accepts `path` as the volatile state folder when it is inside none of
    /// `workspace_roots`. Paths are compared made absolute and lexically
    /// normalised, and also resolved through symlinks where they exist.
    ///
    /// # Errors
    /// [`AppStateError::VolatileInsideWorkspace`] when it is inside one.
    pub fn new(path: &Path, workspace_roots: &[&Path]) -> Result<VolatileStateDir, AppStateError> {
        let candidate = comparable_forms(path);
        for workspace in workspace_roots {
            let is_inside = comparable_forms(workspace)
                .iter()
                .any(|root| candidate.iter().any(|form| form.starts_with(root)));
            if is_inside {
                return Err(AppStateError::VolatileInsideWorkspace {
                    path: path.to_path_buf(),
                    workspace: workspace.to_path_buf(),
                });
            }
        }
        Ok(VolatileStateDir(normalise(path)))
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The folder for one workspace's volatile state.
    #[must_use]
    pub fn for_workspace(&self, workspace: &WorkspaceId) -> PathBuf {
        self.0.join("workspaces").join(workspace.as_str())
    }
}

/// The lexical form, plus the resolved form when the path exists.
fn comparable_forms(path: &Path) -> Vec<PathBuf> {
    let lexical = normalise(path);
    let mut forms = vec![lexical.clone()];
    if let Ok(resolved) = lexical.canonicalize() {
        forms.push(resolved);
    }
    forms
}

/// Absolute, with `.` dropped and `..` applied.
fn normalise(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut normalised = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalised.pop();
            }
            other => normalised.push(other),
        }
    }
    normalised
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volatile_state_is_refused_inside_a_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("vault/Clients/Acme");
        std::fs::create_dir_all(&workspace).unwrap();
        let container = temp.path().join("container");
        let accepted = VolatileStateDir::new(&container, &[&workspace]).unwrap();
        assert_eq!(
            accepted.for_workspace(&WorkspaceId::new("acme")),
            normalise(&container).join("workspaces/acme")
        );
        for inside in [
            workspace.join(".vaultday/cache"),
            workspace.clone(),
            temp.path().join("vault/Clients/x/../Acme/./index"),
        ] {
            assert!(
                matches!(
                    VolatileStateDir::new(&inside, &[&workspace]),
                    Err(AppStateError::VolatileInsideWorkspace { .. })
                ),
                "{}",
                inside.display()
            );
        }
        assert!(
            VolatileStateDir::new(&temp.path().join("vault/Clients/Acme2"), &[&workspace]).is_ok(),
            "a sibling with a common prefix is not inside"
        );
    }

    #[test]
    fn the_gitignore_snippet_ignores_only_temporary_files() {
        let rules: Vec<&str> = GITIGNORE_SNIPPET
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .collect();
        assert_eq!(rules, [".vaultday/sync/**/*.tmp"]);
    }
}
