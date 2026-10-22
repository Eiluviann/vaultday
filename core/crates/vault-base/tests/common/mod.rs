//! Shared helpers for the writer, watcher and index-store tests. Every test
//! works on a copy of `core/fixtures/mock-vault` in a temp dir.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use vault_base::{PlainFileAccess, Workspace, normalize_path};

/// A sandbox copy of the mock vault; the temp dir goes away with the value.
pub struct Sandbox {
    _directory: tempfile::TempDir,
    pub root: PathBuf,
}

pub fn mock_vault_source() -> PathBuf {
    normalize_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mock-vault"))
}

pub fn sandbox() -> Sandbox {
    let directory = tempfile::tempdir().unwrap();
    // Resolve symlinks (macOS temp dirs live under /var -> /private/var) so
    // watcher events and paths compare equal.
    let root = fs::canonicalize(directory.path()).unwrap().join("vault");
    copy_dir(&mock_vault_source(), &root);
    Sandbox {
        _directory: directory,
        root,
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn workspace(root: &Path) -> Workspace {
    Workspace::open(root, &PlainFileAccess).unwrap()
}

pub fn read(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).unwrap()
}

pub fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub const WEB_142: &str = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md";
pub const WEB_157: &str = "Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup.md";
pub const ACME_9: &str =
    "Projects/Acme portal/ACME-9 Onboarding emails/ACME-9 Onboarding emails.md";
pub const ACME_12: &str = "Projects/Acme portal/ACME-12 Invoice export/ACME-12 Invoice export.md";

/// Every file below `root` as relative path to text.
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(text) = fs::read_to_string(&path) {
                files.insert(path.strip_prefix(root).unwrap().to_path_buf(), text);
            }
        }
    }
    files
}

/// What differs in one file: the lines removed and the lines added, after
/// trimming the common head and tail.
#[derive(Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: PathBuf,
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

pub fn diff(before: &BTreeMap<PathBuf, String>, after: &BTreeMap<PathBuf, String>) -> Vec<Changed> {
    let mut changed = Vec::new();
    for path in before
        .keys()
        .chain(after.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let old: Vec<&str> = before
            .get(path)
            .map_or_else(Vec::new, |t| t.split_inclusive('\n').collect());
        let new: Vec<&str> = after
            .get(path)
            .map_or_else(Vec::new, |t| t.split_inclusive('\n').collect());
        if old == new {
            continue;
        }
        let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let tail = old[head..]
            .iter()
            .rev()
            .zip(new[head..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        changed.push(Changed {
            path: path.clone(),
            removed: old[head..old.len() - tail]
                .iter()
                .map(|l| (*l).to_owned())
                .collect(),
            added: new[head..new.len() - tail]
                .iter()
                .map(|l| (*l).to_owned())
                .collect(),
        });
    }
    changed
}

/// The 1-based line number of the first line that contains `needle`.
pub fn line_number_of(text: &str, needle: &str) -> usize {
    text.lines().position(|line| line.contains(needle)).unwrap() + 1
}

/// The 1-based numbers of the lines that differ between two texts of equal
/// line count.
pub fn differing_line_numbers(before: &str, after: &str) -> Vec<usize> {
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    assert_eq!(old.len(), new.len(), "line counts differ");
    old.iter()
        .zip(&new)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, _)| index + 1)
        .collect()
}
