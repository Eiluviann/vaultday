//! The writer against sandbox copies of the mock vault: the round trip from
//! the design note, every operation, the day log, and the failure cases.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use common::{
    ACME_9, ACME_12, WEB_142, WEB_157, diff, differing_line_numbers, line_number_of, read, sandbox,
    snapshot, workspace, write,
};
use vault_base::{
    Date, DirEntry, FileAccess, FileAccessError, IndexScope, LogAppend, LogKind, NewStory, NewTask,
    PlainFileAccess, TaskEdit, TaskSection, TaskTarget, Time, Vault, VaultWriter, WatchCallback,
    WatchHandle, WriteError, WriteQueue,
};

fn date(text: &str) -> Date {
    Date::parse(text).unwrap()
}

fn new_task(text: &str, id: &str, assigned_to: Option<&str>) -> NewTask {
    NewTask {
        text: text.to_owned(),
        id: id.to_owned(),
        created: date("2026-10-23"),
        assigned_to: assigned_to.map(str::to_owned),
        scheduled: None,
        estimate: None,
        due: None,
        reminder: None,
        source: None,
    }
}

#[test]
fn round_trip_ticking_a_pending_input_item_changes_exactly_one_line() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let git_baseline = commit_baseline(&sandbox.root);
    let before = snapshot(&sandbox.root);

    let receipt = VaultWriter::new(&PlainFileAccess, &workspace)
        .tick_done(&TaskTarget::new(WEB_142, "task-6"), date("2026-10-23"))
        .unwrap();

    assert_eq!(receipt.path, Path::new(WEB_142));
    assert_eq!(
        receipt.line_number,
        line_number_of(&read(&sandbox.root, WEB_142), "task-6")
    );
    assert_eq!(
        receipt.old_line.as_deref(),
        Some(
            "- [ ] Final copy for the order summary  [id:: task-6] [created:: 2026-10-10] [assigned_to:: Dana]"
        )
    );
    assert_eq!(
        receipt.new_line,
        "- [x] Final copy for the order summary  [id:: task-6] [created:: 2026-10-10] [assigned_to:: Dana] [completion:: 2026-10-23]"
    );

    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].path, Path::new(WEB_142));
    assert_eq!(
        changes[0].removed,
        [format!("{}\n", receipt.old_line.unwrap())]
    );
    assert_eq!(changes[0].added, [format!("{}\n", receipt.new_line)]);

    if git_baseline {
        let numstat = git(&sandbox.root, &["diff", "--numstat"]);
        assert_eq!(numstat.lines().count(), 1, "{numstat}");
        assert!(numstat.starts_with("1\t1\t"), "{numstat}");
    }

    // The vault reads the new state back.
    let vault = Vault::open(
        &workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    let ticked = vault.task_by_id("task-6").unwrap().task;
    assert!(!ticked.is_open());
    assert_eq!(ticked.field("completion"), Some("2026-10-23"));
}

#[test]
fn untick_restores_the_original_bytes() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let before = snapshot(&sandbox.root);
    let target = TaskTarget::new(WEB_142, "task-6");
    writer.tick_done(&target, date("2026-10-23")).unwrap();
    // Ticking twice keeps the first completion date.
    let second = writer.tick_done(&target, date("2026-11-01")).unwrap();
    assert_eq!(second.old_line.as_deref(), Some(second.new_line.as_str()));
    writer.untick(&target).unwrap();
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

#[test]
fn field_and_text_edits_replace_only_their_line() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let target = TaskTarget::new(WEB_142, "task-5");
    let original = "- [ ] Write the end-to-end checkout test  [id:: task-5] [created:: 2026-10-21] [due:: 2026-10-28]";

    let receipt = writer.set_due(&target, Some(date("2026-11-02"))).unwrap();
    assert_eq!(receipt.old_line.as_deref(), Some(original));
    assert_eq!(
        receipt.new_line,
        "- [ ] Write the end-to-end checkout test  [id:: task-5] [created:: 2026-10-21] [due:: 2026-11-02]"
    );
    let receipt = writer
        .set_scheduled(&target, Some(date("2026-10-24")))
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Write the end-to-end checkout test  [id:: task-5] [created:: 2026-10-21] [due:: 2026-11-02] [scheduled:: 2026-10-24]"
    );
    let receipt = writer.set_estimate(&target, Some("90m")).unwrap();
    assert!(
        receipt
            .new_line
            .ends_with("[scheduled:: 2026-10-24] [estimate:: 90m]")
    );
    let receipt = writer
        .set_text(&target, "Write the checkout end-to-end test")
        .unwrap();
    assert!(
        receipt
            .new_line
            .starts_with("- [ ] Write the checkout end-to-end test  [id:: task-5]")
    );
    let receipt = writer.set_due(&target, None).unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Write the checkout end-to-end test  [id:: task-5] [created:: 2026-10-21] [scheduled:: 2026-10-24] [estimate:: 90m]"
    );
    writer.set_scheduled(&target, None).unwrap();
    let receipt = writer.set_estimate(&target, None).unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Write the checkout end-to-end test  [id:: task-5] [created:: 2026-10-21]"
    );
}

#[test]
fn editing_a_line_inside_a_model_text_block_leaves_the_markers_alone() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    VaultWriter::new(&PlainFileAccess, &workspace)
        .set_text(
            &TaskTarget::new(WEB_142, "task-1"),
            "Extract the address form",
        )
        .unwrap();
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].removed.len(), 1);
    assert_eq!(changes[0].added.len(), 1);
    let note = read(&sandbox.root, WEB_142);
    assert!(note.contains("%% ai:begin fable · 2026-10-20 · 3f9a1c2b7e4d %%\n"));
    assert!(note.contains("\n%% ai:end %%\n"));
}

#[test]
fn failures_say_what_was_not_found_or_what_conflicted() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let before = snapshot(&sandbox.root);

    let missing_task = writer
        .tick_done(&TaskTarget::new(WEB_142, "task-999"), date("2026-10-23"))
        .unwrap_err();
    assert!(matches!(&missing_task, WriteError::TaskNotFound { id, .. } if id == "task-999"));
    assert!(missing_task.is_not_found());
    assert!(missing_task.to_string().contains("task-999"));

    let missing_note = writer
        .untick(&TaskTarget::new("Projects/Nowhere.md", "task-1"))
        .unwrap_err();
    assert!(
        matches!(&missing_note, WriteError::NoteNotFound(path) if path == Path::new("Projects/Nowhere.md"))
    );
    assert!(missing_note.is_not_found());

    // The fenced example task in WEB-142 is not a task: it has no id and sits in a fence.
    let stale = writer
        .set_text(
            &TaskTarget::new(WEB_142, "task-5")
                .expecting_line("- [ ] an older version of the line"),
            "x",
        )
        .unwrap_err();
    assert!(
        matches!(&stale, WriteError::TaskLineChanged { found, .. } if found.contains("task-5"))
    );
    assert!(stale.is_conflict());

    write(
        &sandbox.root,
        "Notes/Twice.md",
        "# T\n## Next Steps\n- [ ] one [id:: dup]\n- [ ] two [id:: dup]\n",
    );
    let duplicate = writer
        .untick(&TaskTarget::new("Notes/Twice.md", "dup"))
        .unwrap_err();
    assert!(matches!(&duplicate, WriteError::DuplicateTaskId { lines, .. } if lines == &[3, 4]));
    assert!(duplicate.is_conflict());

    let invalid = writer
        .set_text(
            &TaskTarget::new(WEB_142, "task-5"),
            "see [due:: 2026-01-01]",
        )
        .unwrap_err();
    assert!(matches!(invalid, WriteError::InvalidInput(_)));

    let mut expected = before;
    expected.insert(
        PathBuf::from("Notes/Twice.md"),
        "# T\n## Next Steps\n- [ ] one [id:: dup]\n- [ ] two [id:: dup]\n".to_owned(),
    );
    assert!(
        diff(&expected, &snapshot(&sandbox.root)).is_empty(),
        "failures write nothing"
    );
}

#[test]
fn paths_outside_the_workspace_are_refused() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    for outside in ["../escape.md", "/etc/hosts", "a/../../escape.md", ""] {
        let error = writer
            .untick(&TaskTarget::new(outside, "task-1"))
            .unwrap_err();
        assert!(
            matches!(error, WriteError::OutsideWorkspace(_)),
            "{outside}"
        );
    }
    // An absolute path inside the root is fine.
    let absolute = sandbox.root.join(WEB_142);
    assert!(
        writer
            .set_due(&TaskTarget::new(absolute, "task-5"), None)
            .is_ok()
    );
}

#[test]
fn a_task_is_added_after_the_last_content_of_its_section() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let before = snapshot(&sandbox.root);

    let receipt = writer
        .add_task(
            Path::new(WEB_142),
            TaskSection::NextSteps,
            &new_task("Review the copy", "task-20", None),
        )
        .unwrap();
    assert_eq!(receipt.old_line, None);
    assert_eq!(
        receipt.new_line,
        "- [ ] Review the copy [id:: task-20] [created:: 2026-10-23]"
    );
    let note = read(&sandbox.root, WEB_142);
    assert_eq!(receipt.line_number, line_number_of(&note, "task-20"));
    assert!(note.contains(
        "[id:: task-5] [created:: 2026-10-21] [due:: 2026-10-28]\n- [ ] Review the copy"
    ));

    let receipt = writer
        .add_task(
            Path::new(WEB_142),
            TaskSection::PendingInput,
            &new_task("Legal sign-off", "task-21", Some("Dana")),
        )
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Legal sign-off [id:: task-21] [created:: 2026-10-23] [assigned_to:: Dana]"
    );
    // After the indented prose that belongs to the last Pending Input task.
    let note = read(&sandbox.root, WEB_142);
    assert!(note.contains("\t- Rounds half up per line, confirmed by mail\n- [ ] Legal sign-off"));

    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    let after = read(&sandbox.root, WEB_142);
    let mut expected_removal = after.clone();
    for added in [
        "- [ ] Review the copy [id:: task-20] [created:: 2026-10-23]\n",
        "- [ ] Legal sign-off [id:: task-21] [created:: 2026-10-23] [assigned_to:: Dana]\n",
    ] {
        expected_removal = expected_removal.replacen(added, "", 1);
    }
    assert_eq!(
        expected_removal,
        before[Path::new(WEB_142)],
        "only the two lines were added"
    );
}

#[test]
fn an_empty_section_gets_its_first_task_under_the_heading() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    VaultWriter::new(&PlainFileAccess, &workspace)
        .add_task(
            Path::new(ACME_12),
            TaskSection::NextSteps,
            &new_task("Archive the export", "task-22", None),
        )
        .unwrap();
    assert!(read(&sandbox.root, ACME_12).contains(
        "## Next Steps\n- [ ] Archive the export [id:: task-22] [created:: 2026-10-23]\n\n## Pending Input\n"
    ));
}

#[test]
fn an_imported_reminder_carries_its_due_day_and_source() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let task = NewTask {
        scheduled: Some(date("2026-10-26")),
        due: Some(date("2026-10-30")),
        reminder: Some("x-apple-reminder-7F3A".to_owned()),
        source: None,
        ..new_task("Renew the passport", "task-23", None)
    };
    let receipt = VaultWriter::new(&PlainFileAccess, &workspace)
        .add_task(Path::new(ACME_12), TaskSection::NextSteps, &task)
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Renew the passport [id:: task-23] [created:: 2026-10-23] [scheduled:: 2026-10-26] [due:: 2026-10-30] [reminder:: x-apple-reminder-7F3A]"
    );
    let bracketed = NewTask {
        reminder: Some("a]b".to_owned()),
        source: None,
        ..new_task("Bad source", "task-24", None)
    };
    assert!(matches!(
        VaultWriter::new(&PlainFileAccess, &workspace).add_task(
            Path::new(ACME_12),
            TaskSection::NextSteps,
            &bracketed
        ),
        Err(WriteError::InvalidInput(_))
    ));
}

#[test]
fn a_triaged_task_ends_with_its_signal_source() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let task = NewTask {
        estimate: Some("30m".to_owned()),
        source: Some("sig-0123456789abcdef".to_owned()),
        ..new_task("Answer the tier question", "task-25", Some("Lena Ortiz"))
    };
    let receipt = writer
        .add_task(Path::new(ACME_12), TaskSection::PendingInput, &task)
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "- [ ] Answer the tier question [id:: task-25] [created:: 2026-10-23] [estimate:: 30m] [assigned_to:: Lena Ortiz] [source:: sig-0123456789abcdef]"
    );
    let bracketed = NewTask {
        source: Some("sig-[x]".to_owned()),
        ..new_task("Bad signal", "task-26", None)
    };
    assert!(matches!(
        writer.add_task(Path::new(ACME_12), TaskSection::NextSteps, &bracketed),
        Err(WriteError::InvalidInput(_))
    ));
}

#[test]
fn the_intake_note_is_created_once_and_reads_as_a_story_without_a_number() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let path = Path::new("Inbox/Reminders.md");
    assert!(writer.create_intake_note(path, "Reminders").unwrap());
    writer
        .add_task(
            path,
            TaskSection::NextSteps,
            &new_task("Call the bank", "task-25", None),
        )
        .unwrap();
    assert!(
        !writer.create_intake_note(path, "Reminders").unwrap(),
        "an existing note is kept"
    );
    assert_eq!(
        read(&sandbox.root, "Inbox/Reminders.md"),
        "---\ntype: story\n---\n# Reminders\n\n## Next Steps\n- [ ] Call the bank [id:: task-25] [created:: 2026-10-23]\n"
    );
    let vault = vault_base::ops::open_vault(&workspace, &PlainFileAccess).unwrap();
    let story = vault
        .stories()
        .into_iter()
        .find(|entry| entry.path == path)
        .expect("the note reads as a story");
    assert_eq!(story.story.number, None);
    assert_eq!(story.story.next_steps.len(), 1);
    assert!(matches!(
        writer.create_intake_note(Path::new("../outside.md"), "Reminders"),
        Err(WriteError::OutsideWorkspace(_))
    ));
}

#[test]
fn a_duplicate_id_or_bad_text_is_refused() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let duplicate = writer
        .add_task(
            Path::new(WEB_157),
            TaskSection::NextSteps,
            &new_task("Again", "task-8", None),
        )
        .unwrap_err();
    assert!(matches!(&duplicate, WriteError::IdAlreadyUsed { id, .. } if id == "task-8"));
    for (text, id) in [
        ("", "task-30"),
        ("a\nb", "task-30"),
        ("ok", "has space"),
        ("ok", ""),
    ] {
        let error = writer
            .add_task(
                Path::new(WEB_157),
                TaskSection::NextSteps,
                &new_task(text, id, None),
            )
            .unwrap_err();
        assert!(
            matches!(error, WriteError::InvalidInput(_)),
            "{text:?} {id:?}"
        );
    }
}

#[test]
fn a_missing_section_is_created_at_the_right_place() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);

    write(
        &sandbox.root,
        "Notes/Bare.md",
        "---\ntype: story\nnumber: X-1\n---\n# T\n\nBody\n",
    );
    writer
        .add_task(
            Path::new("Notes/Bare.md"),
            TaskSection::NextSteps,
            &new_task("a", "t-1", None),
        )
        .unwrap();
    writer
        .add_task(
            Path::new("Notes/Bare.md"),
            TaskSection::PendingInput,
            &new_task("b", "t-2", Some("Lee")),
        )
        .unwrap();
    assert_eq!(
        read(&sandbox.root, "Notes/Bare.md"),
        "---\ntype: story\nnumber: X-1\n---\n# T\n\nBody\n\n## Next Steps\n- [ ] a [id:: t-1] [created:: 2026-10-23]\n\n## Pending Input\n- [ ] b [id:: t-2] [created:: 2026-10-23] [assigned_to:: Lee]\n"
    );

    write(
        &sandbox.root,
        "Notes/OnlyPending.md",
        "# T\n\n## Pending Input\n- [ ] x [id:: p-1]\n",
    );
    writer
        .add_task(
            Path::new("Notes/OnlyPending.md"),
            TaskSection::NextSteps,
            &new_task("n", "t-3", None),
        )
        .unwrap();
    assert_eq!(
        read(&sandbox.root, "Notes/OnlyPending.md"),
        "# T\n\n## Next Steps\n- [ ] n [id:: t-3] [created:: 2026-10-23]\n\n## Pending Input\n- [ ] x [id:: p-1]\n"
    );
}

#[test]
fn nothing_is_inserted_into_a_model_text_block() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);

    write(
        &sandbox.root,
        "Notes/Block.md",
        "# T\n\n%% ai:begin fable · 2026-10-01 · aaaaaaaaaaaa %%\n## Next Steps\n- [ ] model task [id:: m-1]\n%% ai:end %%\n",
    );
    writer
        .add_task(
            Path::new("Notes/Block.md"),
            TaskSection::NextSteps,
            &new_task("mine", "t-4", None),
        )
        .unwrap();
    assert_eq!(
        read(&sandbox.root, "Notes/Block.md"),
        "# T\n\n%% ai:begin fable · 2026-10-01 · aaaaaaaaaaaa %%\n## Next Steps\n- [ ] model task [id:: m-1]\n%% ai:end %%\n- [ ] mine [id:: t-4] [created:: 2026-10-23]\n"
    );

    write(
        &sandbox.root,
        "Notes/WholeNote.md",
        "# T\n\n%% ai:note fable · 2026-10-01 · aaaaaaaaaaaa %%\n## Next Steps\n- [ ] model task [id:: m-1]\n",
    );
    let error = writer
        .add_task(
            Path::new("Notes/WholeNote.md"),
            TaskSection::NextSteps,
            &new_task("mine", "t-5", None),
        )
        .unwrap_err();
    assert!(matches!(error, WriteError::InsideModelText { .. }));
    assert!(error.is_conflict());
}

#[test]
fn a_subtask_goes_after_the_last_child_with_the_siblings_indent() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);

    let receipt = writer
        .add_subtask(
            &TaskTarget::new(WEB_142, "task-2"),
            &new_task("Rotate the keys", "task-23", None),
        )
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "\t- [ ] Rotate the keys [id:: task-23] [created:: 2026-10-23]"
    );
    let note = read(&sandbox.root, WEB_142);
    assert!(note.contains(
        "[id:: task-4] [created:: 2026-10-21] [estimate:: 45m]\n\t- [ ] Rotate the keys [id:: task-23]"
    ));
    assert_eq!(receipt.line_number, line_number_of(&note, "task-23"));

    // A parent without children gets one tab.
    let receipt = writer
        .add_subtask(
            &TaskTarget::new(WEB_142, "task-5"),
            &new_task("Mock the gateway", "task-24", None),
        )
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "\t- [ ] Mock the gateway [id:: task-24] [created:: 2026-10-23]"
    );
    assert!(read(&sandbox.root, WEB_142).contains("[due:: 2026-10-28]\n\t- [ ] Mock the gateway"));
    let vault = Vault::open(
        &workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    assert_eq!(vault.task_by_id("task-24").unwrap().task.depth, 1);

    // Model text is never extended, and a missing parent is not found.
    let in_model_text = writer
        .add_subtask(
            &TaskTarget::new(WEB_142, "task-1"),
            &new_task("x", "task-25", None),
        )
        .unwrap_err();
    assert!(matches!(in_model_text, WriteError::InsideModelText { .. }));
    let orphan = writer
        .add_subtask(
            &TaskTarget::new(WEB_142, "task-404"),
            &new_task("x", "task-25", None),
        )
        .unwrap_err();
    assert!(orphan.is_not_found());
}

#[test]
fn a_first_subtask_uses_the_indent_the_note_nests_with() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    write(
        &sandbox.root,
        "Notes/Spaces.md",
        "## Next Steps\n- [ ] Parent  [id:: s-1]\n    - [ ] Child  [id:: s-2]\n- [ ] Alone  [id:: s-3]\n",
    );
    let receipt = VaultWriter::new(&PlainFileAccess, &workspace)
        .add_subtask(
            &TaskTarget::new("Notes/Spaces.md", "s-3"),
            &new_task("First child", "s-4", None),
        )
        .unwrap();
    assert_eq!(
        receipt.new_line,
        "    - [ ] First child [id:: s-4] [created:: 2026-10-23]"
    );
}

#[test]
fn a_story_is_created_from_the_workspace_template() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let story = NewStory {
        number: "WEB-200".to_owned(),
        title: "Dark mode".to_owned(),
        project_folder: PathBuf::from("Projects/Website"),
    };

    let receipt = writer.new_story(&story).unwrap();
    let path = "Projects/Website/WEB-200 Dark mode/WEB-200 Dark mode.md";
    assert_eq!(receipt.path, Path::new(path));
    assert_eq!(receipt.line_number, 11);
    assert_eq!(receipt.new_line, "# Dark mode");
    assert_eq!(
        read(&sandbox.root, path),
        "---\ntype: story\nnumber: WEB-200\nstatus: draft\npriority:\ndue:\nestimate:\nsort_index:\nnote_system: \"1.0\"\n---\n# Dark mode\n\n## Next Steps\n- [ ] \n\n## Pending Input\n- [ ] \n"
    );

    let vault = Vault::open(
        &workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    let created = vault.story_by_number("WEB-200").unwrap();
    assert_eq!(created.story.title.as_deref(), Some("Dark mode"));

    // The template's empty checkbox is filled by the first task.
    let filled = writer
        .add_task(
            Path::new(path),
            TaskSection::NextSteps,
            &new_task("Pick the palette", "task-26", None),
        )
        .unwrap();
    assert_eq!(filled.old_line.as_deref(), Some("- [ ] "));

    assert!(matches!(
        writer.new_story(&story).unwrap_err(),
        WriteError::AlreadyExists(_)
    ));
    for (number, title) in [("", "T"), ("A B", "T"), ("A-1", ""), ("A-1", "...")] {
        let invalid = NewStory {
            number: number.to_owned(),
            title: title.to_owned(),
            project_folder: PathBuf::from("Projects"),
        };
        assert!(
            matches!(
                writer.new_story(&invalid).unwrap_err(),
                WriteError::InvalidInput(_)
            ),
            "{number:?} {title:?}"
        );
    }
}

#[test]
fn story_templates_with_placeholders_and_a_missing_template() {
    let sandbox = sandbox();
    let mut workspace = workspace(&sandbox.root);
    write(
        &sandbox.root,
        "_templates/placeholders.md",
        "---\ntype: story\nnumber: {{number}}\n---\n# {{title}}\n\n## Next Steps\n",
    );
    workspace.settings.story_template = Some("_templates/placeholders.md".to_owned());
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let receipt = writer
        .new_story(&NewStory {
            number: "ACME-30".to_owned(),
            title: "Pay: now/later".to_owned(),
            project_folder: PathBuf::from("Projects/Acme portal"),
        })
        .unwrap();
    assert_eq!(
        receipt.path,
        Path::new("Projects/Acme portal/ACME-30 Pay- now-later/ACME-30 Pay- now-later.md")
    );
    assert_eq!(
        read(&sandbox.root, receipt.path.to_str().unwrap()),
        "---\ntype: story\nnumber: ACME-30\n---\n# Pay: now/later\n\n## Next Steps\n"
    );

    workspace.settings.story_template = Some("_templates/missing.md".to_owned());
    let missing = VaultWriter::new(&PlainFileAccess, &workspace)
        .new_story(&NewStory {
            number: "ACME-31".to_owned(),
            title: "x".to_owned(),
            project_folder: PathBuf::from("Projects"),
        })
        .unwrap_err();
    assert!(
        matches!(&missing, WriteError::TemplateNotFound(path) if path == Path::new("_templates/missing.md"))
    );
    assert!(missing.is_not_found());

    workspace.settings.story_template = None;
    let default = VaultWriter::new(&PlainFileAccess, &workspace)
        .new_story(&NewStory {
            number: "ACME-32".to_owned(),
            title: "Built-in".to_owned(),
            project_folder: PathBuf::from("Projects"),
        })
        .unwrap();
    assert!(read(&sandbox.root, default.path.to_str().unwrap()).contains("number: ACME-32\n"));
}

#[test]
fn a_log_line_is_appended_to_an_existing_day_log_inside_its_section() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let entry = LogAppend::new(Time::new(15, 5).unwrap(), "Use a single-page checkout")
        .with_kind(LogKind::Decision)
        .with_story("WEB-142")
        .with_task("task-5");

    let receipt = VaultWriter::new(&PlainFileAccess, &workspace)
        .append_log(date("2026-10-22"), &entry)
        .unwrap();

    let expected = "- 15:05 decision · Use a single-page checkout [story:: WEB-142] [kind:: decision] [task:: task-5]";
    assert_eq!(receipt.path, Path::new("Log/2026-10-22.md"));
    assert_eq!(receipt.new_line, expected);
    assert_eq!(receipt.old_line, None);
    let note = read(&sandbox.root, "Log/2026-10-22.md");
    assert_eq!(receipt.line_number, line_number_of(&note, "15:05"));
    // Before the Journal section, after the last log line.
    assert!(note.contains(&format!("[task:: task-9]\n{expected}\n\n## Journal\n")));
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].added, [format!("{expected}\n")]);
    assert!(changes[0].removed.is_empty());
}

#[test]
fn a_new_day_gets_a_log_file_with_the_conventions_header() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let first =
        LogAppend::new(Time::new(9, 0).unwrap(), "Planned the day").with_kind(LogKind::Status);
    let receipt = writer.append_log(date("2026-10-23"), &first).unwrap();
    assert_eq!(receipt.path, Path::new("Log/2026-10-23.md"));
    assert_eq!(receipt.line_number, 8);
    let second = LogAppend::new(Time::new(9, 30).unwrap(), "Coupon field stays")
        .with_kind(LogKind::Decision)
        .with_story("WEB-142")
        .with_id("log-1");
    writer.append_log(date("2026-10-23"), &second).unwrap();

    assert_eq!(
        read(&sandbox.root, "Log/2026-10-23.md"),
        "---\ntype: log\ndate: 2026-10-23\n---\n# 2026-10-23\n\n## Log\n- 09:00 status · Planned the day [kind:: status]\n- 09:30 decision · Coupon field stays [story:: WEB-142] [kind:: decision] [id:: log-1]\n"
    );

    // The queries read today's lines back.
    let vault = Vault::open(
        &workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    let lines = vault.log_lines_on(date("2026-10-23"));
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].line.kind, Some(LogKind::Decision));
    assert_eq!(lines[1].line.story(), Some("WEB-142"));
    assert_eq!(lines[1].line.text, "decision · Coupon field stays");

    let invalid = writer
        .append_log(
            date("2026-10-23"),
            &LogAppend::new(Time::new(1, 0).unwrap(), "a | b"),
        )
        .unwrap_err();
    assert!(matches!(invalid, WriteError::Log(_)));
}

#[test]
fn a_custom_log_folder_and_a_note_without_a_log_section() {
    let sandbox = sandbox();
    let mut workspace = workspace(&sandbox.root);
    workspace.settings.log_folder = "Journal/Days".to_owned();
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let entry = LogAppend::new(Time::new(8, 0).unwrap(), "Started");
    let receipt = writer.append_log(date("2026-10-23"), &entry).unwrap();
    assert_eq!(receipt.path, Path::new("Journal/Days/2026-10-23.md"));

    write(
        &sandbox.root,
        "Journal/Days/2026-10-24.md",
        "---\ntype: log\ndate: 2026-10-24\n---\n# 2026-10-24\n",
    );
    writer.append_log(date("2026-10-24"), &entry).unwrap();
    assert_eq!(
        read(&sandbox.root, "Journal/Days/2026-10-24.md"),
        "---\ntype: log\ndate: 2026-10-24\n---\n# 2026-10-24\n\n## Log\n- 08:00 Started\n"
    );
}

#[test]
fn crlf_notes_keep_their_terminators() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    write(
        &sandbox.root,
        "Notes/Windows.md",
        "# T\r\n## Next Steps\r\n- [ ] a [id:: w-1]\r\n- [ ] b [id:: w-2]\r\n",
    );
    writer
        .tick_done(
            &TaskTarget::new("Notes/Windows.md", "w-1"),
            date("2026-10-23"),
        )
        .unwrap();
    writer
        .add_task(
            Path::new("Notes/Windows.md"),
            TaskSection::NextSteps,
            &new_task("c", "w-3", None),
        )
        .unwrap();
    assert_eq!(
        read(&sandbox.root, "Notes/Windows.md"),
        "# T\r\n## Next Steps\r\n- [x] a [id:: w-1] [completion:: 2026-10-23]\r\n- [ ] b [id:: w-2]\r\n- [ ] c [id:: w-3] [created:: 2026-10-23]\r\n"
    );
}

#[test]
fn a_burst_of_edits_is_coalesced_and_written_once() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let before = snapshot(&sandbox.root);
    let start = Instant::now();
    let at = |milliseconds: u64| start + Duration::from_millis(milliseconds);
    let mut queue = WriteQueue::new(Duration::from_millis(400));

    queue.push(WEB_142, "task-5", TaskEdit::Text("Draft".to_owned()), at(0));
    queue.push(
        WEB_142,
        "task-5",
        TaskEdit::Text("Draft the plan".to_owned()),
        at(100),
    );
    queue.push(
        WEB_142,
        "task-5",
        TaskEdit::Due(Some(date("2026-11-03"))),
        at(150),
    );
    queue.push(
        WEB_142,
        "task-6",
        TaskEdit::Done(date("2026-10-23")),
        at(200),
    );
    queue.push(WEB_142, "task-6", TaskEdit::Reopen, at(220));
    queue.push(
        WEB_142,
        "task-6",
        TaskEdit::Done(date("2026-10-24")),
        at(240),
    );
    assert_eq!(
        queue.pending(),
        3,
        "later edits of one kind replace earlier ones"
    );
    assert_eq!(queue.next_due(), Some(at(640)));

    assert!(queue.flush_due(&writer, at(639)).is_empty());
    assert_eq!(
        snapshot(&sandbox.root),
        before,
        "nothing is written inside the window"
    );

    let outcomes = queue.flush_due(&writer, at(640));
    assert_eq!(outcomes.len(), 3);
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    assert_eq!(queue.pending(), 0);
    assert_eq!(queue.next_due(), None);

    let note = read(&sandbox.root, WEB_142);
    let changed_lines = differing_line_numbers(&before[Path::new(WEB_142)], &note);
    assert_eq!(
        changed_lines,
        [
            line_number_of(&note, "task-5"),
            line_number_of(&note, "task-6")
        ]
    );
    assert!(
        note.contains(
            "- [ ] Draft the plan  [id:: task-5] [created:: 2026-10-21] [due:: 2026-11-03]"
        )
    );
    assert!(note.contains("[assigned_to:: Dana] [completion:: 2026-10-24]"));
}

#[test]
fn a_queued_edit_that_cannot_apply_is_reported_without_blocking_the_others() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let writer = VaultWriter::new(&PlainFileAccess, &workspace);
    let mut queue = WriteQueue::default();
    let now = Instant::now();
    queue.push(WEB_142, "task-404", TaskEdit::Reopen, now);
    queue.push(
        WEB_142,
        "task-5",
        TaskEdit::Estimate(Some("1h".to_owned())),
        now,
    );
    queue.push("Projects/Nowhere.md", "task-1", TaskEdit::Reopen, now);
    let outcomes = queue.flush_all(&writer);
    assert_eq!(outcomes.len(), 3);
    let by_id = |id: &str| outcomes.iter().find(|outcome| outcome.id == id).unwrap();
    assert!(
        by_id("task-404")
            .result
            .as_ref()
            .unwrap_err()
            .is_not_found()
    );
    assert!(by_id("task-5").result.is_ok());
    assert!(matches!(
        by_id("task-1").result.as_ref().unwrap_err(),
        WriteError::NoteNotFound(_)
    ));
}

/// Changes the note right after the writer read it, to simulate a sync client
/// or another device writing at the same time.
struct RacyFileAccess {
    inner: PlainFileAccess,
    target: PathBuf,
    remaining_races: AtomicUsize,
    has_raced: AtomicBool,
}

impl FileAccess for RacyFileAccess {
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FileAccessError> {
        self.inner.read_dir(path)
    }
    fn stat(&self, path: &Path) -> Result<Option<DirEntry>, FileAccessError> {
        self.inner.stat(path)
    }
    fn read_to_string(&self, path: &Path) -> Result<String, FileAccessError> {
        let text = self.inner.read_to_string(path)?;
        if path == self.target {
            let remaining = self.remaining_races.load(Ordering::SeqCst);
            if remaining > 0 {
                self.remaining_races.store(remaining - 1, Ordering::SeqCst);
                self.has_raced.store(true, Ordering::SeqCst);
                self.inner
                    .write_atomic(path, &format!("{text}Concurrent line {remaining}\n"))?;
            }
        }
        Ok(text)
    }
    fn write_atomic(&self, path: &Path, contents: &str) -> Result<(), FileAccessError> {
        self.inner.write_atomic(path, contents)
    }
    fn remove_file(&self, path: &Path) -> Result<(), FileAccessError> {
        self.inner.remove_file(path)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), FileAccessError> {
        self.inner.create_dir_all(path)
    }
    fn watch(&self, path: &Path, on_change: WatchCallback) -> Result<WatchHandle, FileAccessError> {
        self.inner.watch(path, on_change)
    }
}

fn racy(sandbox_root: &Path, races: usize) -> RacyFileAccess {
    RacyFileAccess {
        inner: PlainFileAccess,
        target: sandbox_root.join(WEB_142),
        remaining_races: AtomicUsize::new(races),
        has_raced: AtomicBool::new(false),
    }
}

#[test]
fn a_write_that_races_another_writer_starts_over_from_a_fresh_read() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let access = racy(&sandbox.root, 1);
    let writer = VaultWriter::new(&access, &workspace);
    writer
        .tick_done(&TaskTarget::new(WEB_142, "task-6"), date("2026-10-23"))
        .unwrap();
    assert!(access.has_raced.load(Ordering::SeqCst));
    let note = read(&sandbox.root, WEB_142);
    assert!(
        note.ends_with("Concurrent line 1\n"),
        "the other writer's line survives"
    );
    assert!(note.contains("[x] Final copy"), "and so does the tick");
}

#[test]
fn a_file_that_never_settles_is_not_written() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let access = racy(&sandbox.root, 100);
    let writer = VaultWriter::new(&access, &workspace);
    let error = writer
        .tick_done(&TaskTarget::new(WEB_142, "task-6"), date("2026-10-23"))
        .unwrap_err();
    assert!(matches!(error, WriteError::FileChangedWhileWriting(_)));
    assert!(error.is_conflict());
    assert!(!read(&sandbox.root, WEB_142).contains("[x] Final copy"));
}

#[test]
fn the_other_story_notes_are_untouched_by_a_write_to_one() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    VaultWriter::new(&PlainFileAccess, &workspace)
        .add_task(
            Path::new(ACME_9),
            TaskSection::PendingInput,
            &new_task("Sign-off", "task-27", Some("Morgan")),
        )
        .unwrap();
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, Path::new(ACME_9));
}

/// Commits the sandbox so `git diff` can show what the writer changed. False
/// when git is not installed; the test then relies on its own line diff.
fn commit_baseline(root: &Path) -> bool {
    if Command::new("git").arg("--version").output().is_err() {
        return false;
    }
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-q",
            "-m",
            "baseline",
        ],
    );
    true
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
