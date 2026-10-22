//! Conflict copies (`Note 2.md`) against sandbox copies of the mock vault:
//! every branch of the merge, then the removal of the copy.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use common::{WEB_157, read, sandbox, snapshot, workspace, write};
use vault_base::{
    ConflictCopy, FileIndex, IndexScope, PlainFileAccess, VaultWriter, conflict_copies_in_index,
    find_conflict_copies,
};

const WEB_157_COPY: &str =
    "Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup 2.md";
const WEB_157_COPY_3: &str =
    "Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup 3.md";

const TASK_8: &str = "- [ ] Draft the confirmation mail  [id:: task-8] [created:: 2026-10-19]";

fn indexed(root: &Path) -> FileIndex {
    let mut index = FileIndex::new(root, IndexScope::default()).unwrap();
    index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    index
}

/// The original story with `replacements` applied to its text.
fn copy_of_web_157(root: &Path, replacements: &[(&str, &str)]) {
    let mut text = read(root, WEB_157);
    for (from, to) in replacements {
        assert!(text.contains(from), "{from:?} is in the story");
        text = text.replacen(from, to, 1);
    }
    write(root, WEB_157_COPY, &text);
}

#[test]
fn a_line_only_in_the_copy_is_appended_under_its_section() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let added = "- [ ] Test the double opt-in  [id:: task-30] [created:: 2026-10-23]";
    copy_of_web_157(&sandbox.root, &[(TASK_8, &format!("{TASK_8}\n{added}"))]);

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap();

    assert_eq!(report.appended.len(), 1);
    assert_eq!(report.appended[0].new_line, added);
    assert!(report.conflicts.is_empty());
    assert!(report.copy_removed);
    assert!(
        read(&sandbox.root, WEB_157).contains(&format!("{TASK_8}\n{added}\n\n## Pending Input")),
        "after the last Next Steps task, before the next section"
    );
    assert!(!sandbox.root.join(WEB_157_COPY).exists());
}

#[test]
fn a_line_changed_only_in_the_copy_is_still_a_conflict() {
    // Without the version both devices started from, a tick in the copy and
    // an untick in the note look the same, so neither side wins.
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let ticked = format!("{} [completion:: 2026-10-23]", TASK_8.replace("[ ]", "[x]"));
    copy_of_web_157(&sandbox.root, &[(TASK_8, &ticked)]);
    let before = snapshot(&sandbox.root);

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap();

    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].original_line, TASK_8);
    assert_eq!(report.conflicts[0].copy_line, ticked);
    assert!(!report.copy_removed);
    assert_eq!(snapshot(&sandbox.root), before);
}

#[test]
fn a_line_changed_on_both_sides_is_reported_and_nothing_is_lost() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    // The other device changed the line one way...
    let their_edit = TASK_8.replace("confirmation mail", "opt-in mail");
    copy_of_web_157(&sandbox.root, &[(TASK_8, &their_edit)]);
    // ...while the note moved on.
    let own_edit = TASK_8.replace("confirmation mail", "welcome mail");
    write(
        &sandbox.root,
        WEB_157,
        &read(&sandbox.root, WEB_157).replace(TASK_8, &own_edit),
    );
    let before = snapshot(&sandbox.root);

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap();

    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].task_id, "task-8");
    assert_eq!(report.conflicts[0].original_line, own_edit);
    assert_eq!(report.conflicts[0].copy_line, their_edit);
    assert!(!report.is_clean());
    assert!(!report.copy_removed);
    assert_eq!(
        snapshot(&sandbox.root),
        before,
        "both versions stay as they were"
    );
}

#[test]
fn an_identical_copy_is_simply_removed() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    copy_of_web_157(&sandbox.root, &[]);
    let before = read(&sandbox.root, WEB_157);
    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap();
    assert!(report.appended.is_empty() && report.is_clean());
    assert!(report.copy_removed);
    assert_eq!(read(&sandbox.root, WEB_157), before);
    assert!(!sandbox.root.join(WEB_157_COPY).exists());
}

#[test]
fn text_outside_the_tasks_keeps_the_copy_and_the_note_untouched() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let added = "- [ ] Test the double opt-in  [id:: task-30] [created:: 2026-10-23]";
    copy_of_web_157(
        &sandbox.root,
        &[
            (
                "Footer signup box with double opt-in.",
                "Footer signup box with double opt-in, now with a checkbox.",
            ),
            (TASK_8, &format!("{TASK_8}\n{added}")),
        ],
    );
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let before = snapshot(&sandbox.root);

    for _ in 0..2 {
        let report = writer
            .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
            .unwrap();
        assert!(report.appended.is_empty(), "a merge is all or nothing");
        assert_eq!(
            report.unmerged_lines,
            ["Footer signup box with double opt-in, now with a checkbox."]
        );
        assert!(!report.copy_removed, "the prose of the copy would be lost");
        assert_eq!(
            snapshot(&sandbox.root),
            before,
            "merging again changes nothing"
        );
    }
}

#[test]
fn a_subtask_only_in_the_copy_goes_under_its_parent_and_a_missing_section_is_created() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    write(
        &sandbox.root,
        "Notes/Plan.md",
        "# Plan\n\n## Next Steps\n- [ ] Parent  [id:: a-1]\n- [ ] Sibling  [id:: a-2]\n",
    );
    write(
        &sandbox.root,
        "Notes/Plan 2.md",
        "# Plan\n\n## Next Steps\n- [ ] Parent  [id:: a-1]\n\t- [ ] Child  [id:: a-3]\n- [ ] Sibling  [id:: a-2]\n\n## Pending Input\n- [ ] Waiting  [id:: a-4] [assigned_to:: Lee]\n",
    );

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new("Notes/Plan.md"), Path::new("Notes/Plan 2.md"))
        .unwrap();

    assert_eq!(report.appended.len(), 3, "child, heading and waiting item");
    assert_eq!(
        read(&sandbox.root, "Notes/Plan.md"),
        "# Plan\n\n## Next Steps\n- [ ] Parent  [id:: a-1]\n\t- [ ] Child  [id:: a-3]\n- [ ] Sibling  [id:: a-2]\n\n## Pending Input\n- [ ] Waiting  [id:: a-4] [assigned_to:: Lee]\n"
    );
    assert!(report.copy_removed);
}

#[test]
fn every_copy_beside_a_note_is_found_and_merged_lowest_number_first() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let first_addition = "- [ ] From device two  [id:: task-40]";
    let second_addition = "- [ ] From device three  [id:: task-41]";
    copy_of_web_157(
        &sandbox.root,
        &[(TASK_8, &format!("{TASK_8}\n{first_addition}"))],
    );
    write(
        &sandbox.root,
        WEB_157_COPY_3,
        &read(&sandbox.root, WEB_157).replacen(TASK_8, &format!("{TASK_8}\n{second_addition}"), 1),
    );
    write(&sandbox.root, "Notes/Plan 2.md", "# unrelated\n");
    let index_with_copies = indexed(&sandbox.root);

    let from_index = conflict_copies_in_index(&index_with_copies);
    assert_eq!(
        from_index
            .iter()
            .map(|found| (found.original.clone(), found.copy.clone(), found.number))
            .collect::<Vec<_>>(),
        [
            (PathBuf::from(WEB_157), PathBuf::from(WEB_157_COPY), 2),
            (PathBuf::from(WEB_157), PathBuf::from(WEB_157_COPY_3), 3),
        ],
        "`Plan 2.md` has no `Plan.md` beside it, so it is an ordinary note"
    );

    let beside = find_conflict_copies(&PlainFileAccess, &sandbox.root.join(WEB_157)).unwrap();
    assert_eq!(
        beside
            .iter()
            .map(|found: &ConflictCopy| found.number)
            .collect::<Vec<_>>(),
        [2, 3]
    );

    let reports = VaultWriter::new(&PlainFileAccess, &workspace)
        .resolve_conflict_copies(Path::new(WEB_157))
        .unwrap();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.copy_removed));
    let note = read(&sandbox.root, WEB_157);
    assert!(note.contains(&format!("{TASK_8}\n{first_addition}\n{second_addition}\n")));
    assert!(
        find_conflict_copies(&PlainFileAccess, &sandbox.root.join(WEB_157))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn copies_that_changed_the_same_line_each_report_their_conflict() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    copy_of_web_157(
        &sandbox.root,
        &[(TASK_8, &TASK_8.replace("confirmation", "welcome"))],
    );
    write(
        &sandbox.root,
        WEB_157_COPY_3,
        &read(&sandbox.root, WEB_157).replacen(
            TASK_8,
            &TASK_8.replace("confirmation", "reminder"),
            1,
        ),
    );
    let before = snapshot(&sandbox.root);
    let reports = VaultWriter::new(&PlainFileAccess, &workspace)
        .resolve_conflict_copies(Path::new(WEB_157))
        .unwrap();
    assert_eq!(reports.len(), 2);
    assert!(
        reports
            .iter()
            .all(|report| report.conflicts.len() == 1 && !report.copy_removed)
    );
    assert_eq!(snapshot(&sandbox.root), before);
}

#[test]
fn a_missing_copy_or_note_is_an_error_not_a_merge() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let missing_copy = writer
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap_err();
    assert!(missing_copy.is_not_found());
    write(&sandbox.root, "Notes/Gone 2.md", "# x\n");
    let missing_note = writer
        .merge_conflict_copy(Path::new("Notes/Gone.md"), Path::new("Notes/Gone 2.md"))
        .unwrap_err();
    assert!(missing_note.is_not_found());
}

#[test]
fn a_local_edit_is_not_overwritten_by_the_copy() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let their_edit = TASK_8.replace("confirmation mail", "opt-in mail");
    copy_of_web_157(&sandbox.root, &[(TASK_8, &their_edit)]);
    // This device edits the note too. Whether or not the index has seen
    // that edit, the copy must not win.
    writer
        .set_text(
            &vault_base::TaskTarget::new(WEB_157, "task-8"),
            "Draft the welcome mail",
        )
        .unwrap();
    let before = snapshot(&sandbox.root);

    let report = writer
        .merge_conflict_copy(Path::new(WEB_157), Path::new(WEB_157_COPY))
        .unwrap();

    assert_eq!(report.conflicts.len(), 1);
    assert!(!report.copy_removed);
    assert_eq!(snapshot(&sandbox.root), before, "both versions survive");
}

#[test]
fn a_task_without_an_id_that_differs_is_not_duplicated_into_the_note() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    write(
        &sandbox.root,
        "Notes/Plan.md",
        "# Plan\n\n## Next Steps\n- [ ] Typed on the phone\n- [ ] Parent  [id:: a-1]\n",
    );
    write(
        &sandbox.root,
        "Notes/Plan 2.md",
        "# Plan\n\n## Next Steps\n- [x] Typed on the phone\n- [ ] Parent  [id:: a-1]\n",
    );
    let before = snapshot(&sandbox.root);

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new("Notes/Plan.md"), Path::new("Notes/Plan 2.md"))
        .unwrap();

    assert_eq!(report.unmerged_lines, ["- [x] Typed on the phone"]);
    assert!(!report.copy_removed);
    assert_eq!(snapshot(&sandbox.root), before);
}

#[test]
fn a_note_that_only_looks_like_a_copy_is_not_merged_into_its_namesake() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    write(
        &sandbox.root,
        "Notes/Sprint.md",
        "# Sprint\n\n## Next Steps\n- [ ] Plan the sprint  [id:: s-1]\n",
    );
    write(
        &sandbox.root,
        "Notes/Sprint 2.md",
        "# Sprint 2\n\n## Next Steps\n- [ ] Ship the second sprint  [id:: s-2]\n",
    );
    let before = snapshot(&sandbox.root);

    let report = VaultWriter::new(&PlainFileAccess, &workspace)
        .merge_conflict_copy(Path::new("Notes/Sprint.md"), Path::new("Notes/Sprint 2.md"))
        .unwrap();

    assert_eq!(report.unmerged_lines, ["# Sprint 2"]);
    assert!(report.appended.is_empty());
    assert!(!report.copy_removed);
    assert_eq!(
        snapshot(&sandbox.root),
        before,
        "nothing moves between the notes"
    );
}
