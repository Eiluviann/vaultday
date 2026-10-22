//! iCloud (and other sync clients) leave a conflict copy beside a note that two
//! devices edited at once: `Note 2.md`, `Note 3.md`. This module finds them
//! and merges them back into the note by task id, then removes the copy.
//!
//! The merge rules, per task line of the copy:
//! - the id is in the note and the lines are equal: nothing to do;
//! - the id is not in the note: the copy's line is appended under its section
//!   (under its parent when the parent is in the note);
//! - the id is in the note and the lines differ: a conflict. Without the
//!   version both devices started from there is no telling which side made
//!   the change, so neither line wins;
//! - the line has no id and the note lacks it: it cannot be matched, so it is
//!   left unmerged rather than added as a near-duplicate.
//!
//! A merge is all or nothing. When the copy has a conflict, an unmatched task
//! line, or text outside task lines that the note lacks (prose, frontmatter,
//! another title), the note is not touched and the copy stays, so nothing is
//! lost and a note that merely looks like a copy (`Sprint 2.md` beside
//! `Sprint.md`) is never folded into its namesake. Otherwise the copy's new
//! tasks are added and the copy is removed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::buffer::LineBuffer;
use crate::file_access::{EntryKind, FileAccess, FileAccessError};
use crate::index::FileIndex;
use crate::scan::{Document, LineKind, Task};
use crate::writer::{
    VaultWriter, WriteError, WriteReceipt, find_task_by_id, insert_into_section, receipts_from,
    subtree_end,
};

/// A conflict copy and the note it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictCopy {
    /// The note, relative to the folder the search started from.
    pub original: PathBuf,
    /// The copy, `<stem> <number>.md` beside the note.
    pub copy: PathBuf,
    /// The number in the copy's name: 2, 3, …
    pub number: u32,
}

/// A task line that changed on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    /// The task id.
    pub task_id: String,
    /// The line in the note.
    pub original_line: String,
    /// The line in the copy.
    pub copy_line: String,
}

/// What merging one copy did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReport {
    /// The note, relative to the workspace root.
    pub original: PathBuf,
    /// The copy, relative to the workspace root.
    pub copy: PathBuf,
    /// Lines (and section headings) added to the note; empty unless the
    /// merge was clean.
    pub appended: Vec<WriteReceipt>,
    /// Task lines whose note and copy versions differ; the note keeps its own.
    pub conflicts: Vec<MergeConflict>,
    /// Non-blank copy lines the note does not contain and the merge cannot
    /// place: prose, frontmatter, headings, and task lines without an id.
    pub unmerged_lines: Vec<String>,
    /// True when the copy was deleted.
    pub copy_removed: bool,
}

impl MergeReport {
    /// True when nothing is left to resolve by hand.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.unmerged_lines.is_empty()
    }
}

/// `Plan 2.md` → `Plan.md` and 2. `None` for names that are not conflict copies.
#[must_use]
pub fn conflict_copy_original(copy: &Path) -> Option<(PathBuf, u32)> {
    if copy.extension()? != "md" {
        return None;
    }
    let stem = copy.file_stem()?.to_str()?;
    let (original_stem, number_text) = stem.rsplit_once(' ')?;
    if original_stem.is_empty() || !number_text.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let number: u32 = number_text.parse().ok()?;
    if number < 2 {
        return None;
    }
    Some((copy.with_file_name(format!("{original_stem}.md")), number))
}

/// Every conflict copy in the index whose note is in the index too.
#[must_use]
pub fn conflict_copies_in_index(index: &FileIndex) -> Vec<ConflictCopy> {
    let mut copies: Vec<ConflictCopy> = index
        .files()
        .filter_map(|(path, _)| {
            let (original, number) = conflict_copy_original(path)?;
            index.stamp(&original)?;
            Some(ConflictCopy {
                original,
                copy: path.to_path_buf(),
                number,
            })
        })
        .collect();
    copies.sort_by(|a, b| (&a.original, a.number).cmp(&(&b.original, b.number)));
    copies
}

/// The conflict copies beside `note` (a path the file access can list),
/// lowest number first.
///
/// # Errors
/// File errors from listing the folder.
pub fn find_conflict_copies(
    file_access: &dyn FileAccess,
    note: &Path,
) -> Result<Vec<ConflictCopy>, FileAccessError> {
    let (Some(folder), Some(stem)) = (note.parent(), note.file_stem().and_then(|s| s.to_str()))
    else {
        return Ok(Vec::new());
    };
    let mut copies: Vec<ConflictCopy> = file_access
        .read_dir(folder)?
        .into_iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .filter_map(|entry| {
            let (original, number) = conflict_copy_original(&entry.path)?;
            (original.file_stem().and_then(|s| s.to_str()) == Some(stem)).then_some(ConflictCopy {
                original: note.to_path_buf(),
                copy: entry.path,
                number,
            })
        })
        .collect();
    copies.sort_by_key(|copy| copy.number);
    Ok(copies)
}

impl VaultWriter<'_> {
    /// Merges one conflict copy into its note and removes the copy when the
    /// merge was clean; see the module rules.
    ///
    /// # Errors
    /// [`WriteError`] when either file is missing or unreadable, or the note
    /// cannot be written. Conflicts are not errors: they are in the report.
    pub fn merge_conflict_copy(
        &self,
        original: &Path,
        copy: &Path,
    ) -> Result<MergeReport, WriteError> {
        let (original_relative, original_absolute) = self.resolve(original)?;
        let (copy_relative, copy_absolute) = self.resolve(copy)?;
        let copy_text = match self.file_access.read_to_string(&copy_absolute) {
            Ok(text) => text,
            Err(FileAccessError::NotFound(_)) => {
                return Err(WriteError::NoteNotFound(copy_relative));
            }
            Err(error) => return Err(error.into()),
        };
        let copy_document = Document::parse(&copy_text);
        let mut report = MergeReport {
            original: original_relative.clone(),
            copy: copy_relative,
            appended: Vec::new(),
            conflicts: Vec::new(),
            unmerged_lines: Vec::new(),
            copy_removed: false,
        };
        self.rewrite(&original_relative, &original_absolute, |buffer| {
            let outcome = merge_into(buffer, &copy_document, &original_relative)?;
            report.appended = receipts_from(buffer, &original_relative);
            report.conflicts = outcome.conflicts;
            report.unmerged_lines = outcome.unmerged_lines;
            Ok(())
        })?;
        if report.is_clean() {
            self.file_access.remove_file(&copy_absolute)?;
            report.copy_removed = true;
        }
        Ok(report)
    }

    /// Merges every conflict copy beside `note`, lowest number first, and
    /// returns one report per copy.
    ///
    /// # Errors
    /// As [`VaultWriter::merge_conflict_copy`]; copies merged before the
    /// failure stay merged.
    pub fn resolve_conflict_copies(&self, note: &Path) -> Result<Vec<MergeReport>, WriteError> {
        let (relative, absolute) = self.resolve(note)?;
        let mut reports = Vec::new();
        for found in find_conflict_copies(self.file_access, &absolute)? {
            let copy = found.copy.strip_prefix(&self.root).unwrap_or(&found.copy);
            reports.push(self.merge_conflict_copy(&relative, copy)?);
        }
        Ok(reports)
    }
}

struct MergeOutcome {
    conflicts: Vec<MergeConflict>,
    unmerged_lines: Vec<String>,
}

fn merge_into(
    buffer: &mut LineBuffer,
    copy: &Document,
    relative: &Path,
) -> Result<MergeOutcome, WriteError> {
    let original = buffer.document();
    let mut conflicts = Vec::new();
    let mut to_append: Vec<&Task> = Vec::new();
    for copy_task in copy.tasks() {
        let Some(id) = copy_task.id() else {
            continue;
        };
        match find_task_by_id(&original, id) {
            Some(original_task) => {
                let original_line = line_text(&original, original_task.line);
                let copy_line = line_text(copy, copy_task.line);
                if original_line != copy_line {
                    conflicts.push(MergeConflict {
                        task_id: id.to_owned(),
                        original_line: original_line.to_owned(),
                        copy_line: copy_line.to_owned(),
                    });
                }
            }
            None => to_append.push(copy_task),
        }
    }
    let mut merged = buffer.clone();
    for copy_task in to_append {
        append_task(&mut merged, copy, copy_task, relative)?;
    }
    let unmerged_lines = unmerged_copy_lines(copy, &merged.document());
    if conflicts.is_empty() && unmerged_lines.is_empty() {
        *buffer = merged;
    }
    Ok(MergeOutcome {
        conflicts,
        unmerged_lines,
    })
}

/// The copy's non-blank lines the merged note lacks, other than task lines
/// with an id (those are conflicts or were appended).
fn unmerged_copy_lines(copy: &Document, merged: &Document) -> Vec<String> {
    let merged_texts: BTreeSet<&str> = merged
        .lines()
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    copy.lines()
        .iter()
        .filter(|line| match line.kind {
            LineKind::Task(task_index) => copy
                .tasks()
                .get(task_index)
                .is_none_or(|task| task.id().is_none()),
            _ => true,
        })
        .map(|line| line.text.as_str())
        .filter(|text| !text.trim().is_empty() && !merged_texts.contains(text))
        .map(str::to_owned)
        .collect()
}

/// Adds the copy's task line verbatim: under its parent when the parent is in
/// the note, otherwise at the end of its section.
fn append_task(
    buffer: &mut LineBuffer,
    copy: &Document,
    copy_task: &Task,
    relative: &Path,
) -> Result<(), WriteError> {
    let line = line_text(copy, copy_task.line).to_owned();
    let document = buffer.document();
    let parent_in_note = copy_task
        .parent
        .and_then(|parent| copy.tasks().get(parent))
        .and_then(|parent| parent.id())
        .and_then(|parent_id| find_task_by_id(&document, parent_id))
        .filter(|parent| !document.is_model_text(parent.line));
    if let Some(parent) = parent_in_note {
        let at = subtree_end(buffer, parent);
        buffer.insert(at, line);
        return Ok(());
    }
    match section_title_at(copy, copy_task.line) {
        Some(title) => insert_into_section(buffer, &title, line, relative),
        None => {
            buffer.insert(buffer.len(), line);
            Ok(())
        }
    }
}

fn section_title_at(document: &Document, line: usize) -> Option<String> {
    document
        .sections()
        .into_iter()
        .filter(|section| section.heading_line.is_some() && section.contains(line))
        .max_by_key(|section| section.start)
        .map(|section| section.title)
}

fn line_text(document: &Document, index: usize) -> &str {
    document
        .lines()
        .get(index)
        .map(|line| line.text.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{conflict_copy_original, section_title_at};
    use crate::scan::Document;
    use std::path::{Path, PathBuf};

    #[test]
    fn recognises_numbered_copies_only() {
        assert_eq!(
            conflict_copy_original(Path::new("A/Plan 2.md")),
            Some((PathBuf::from("A/Plan.md"), 2))
        );
        assert_eq!(
            conflict_copy_original(Path::new("Plan 3.md")),
            Some((PathBuf::from("Plan.md"), 3))
        );
        assert_eq!(
            conflict_copy_original(Path::new("Plan Q2 12.md")),
            Some((PathBuf::from("Plan Q2.md"), 12))
        );
        for not_a_copy in [
            "Plan.md",
            "Plan 1.md",
            "Plan 2.txt",
            "Plan two.md",
            " 2.md",
            "2.md",
        ] {
            assert_eq!(
                conflict_copy_original(Path::new(not_a_copy)),
                None,
                "{not_a_copy}"
            );
        }
    }

    #[test]
    fn the_section_of_a_line_is_the_deepest_heading_above_it() {
        let document = Document::parse("# T\n## A\n### B\n- [ ] x\n## C\n- [ ] y\n");
        assert_eq!(section_title_at(&document, 3).as_deref(), Some("B"));
        assert_eq!(section_title_at(&document, 5).as_deref(), Some("C"));
        assert_eq!(section_title_at(&Document::parse("- [ ] z\n"), 0), None);
    }
}
