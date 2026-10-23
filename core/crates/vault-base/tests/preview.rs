//! The read-only previews against the synthetic mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use vault_base::ops::open_vault;
use vault_base::{Date, NoteBlock, PlainFileAccess, TaskStatus};

use common::{sandbox, workspace};

fn day(text: &str) -> Date {
    Date::parse(text).unwrap()
}

#[test]
fn a_story_preview_holds_the_note_blocks_the_waiting_items_and_the_activity() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();

    let preview = vault
        .story_preview("WEB-142", day("2026-10-22"), 10)
        .unwrap();

    assert_eq!(preview.title.as_deref(), Some("Checkout redesign"));
    assert_eq!(preview.status.as_deref(), Some("in-progress"));
    assert_eq!(preview.priority.as_deref(), Some("high"));
    assert_eq!(preview.due, Some(day("2026-10-30")));
    assert_eq!(preview.estimate.as_deref(), Some("3d"));
    let headings: Vec<&str> = preview
        .blocks
        .iter()
        .filter_map(|block| match block {
            NoteBlock::Heading { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(headings, ["Background", "Implementation", "Notes"]);
    let rendered = format!("{:?}", preview.blocks);
    assert!(!rendered.contains("ai:end"), "{rendered}");
    assert!(!rendered.contains("Wire the payment form"), "{rendered}");
    assert!(
        preview.blocks.contains(&NoteBlock::Paragraph(
            "Render the address and payment forms side by side; validate on blur.".to_owned()
        )),
        "{rendered}"
    );

    assert_eq!(preview.waiting.len(), 1);
    assert_eq!(preview.waiting[0].task_id.as_deref(), Some("task-6"));
    assert_eq!(preview.waiting[0].assigned_to, "Dana");
    assert_eq!(preview.waiting[0].age_days, Some(12));

    let lines: Vec<(String, &str)> = preview
        .activity
        .iter()
        .map(|entry| {
            (
                format!("{} {}", entry.date, entry.time),
                entry.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        lines[0],
        (
            "2026-10-22 09:30".to_owned(),
            "Session on the end-to-end test"
        )
    );
    assert_eq!(
        lines.last().map(|line| line.1),
        Some("Planned the checkout work")
    );
}

#[test]
fn the_activity_is_newest_first_and_capped() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();

    let preview = vault
        .story_preview("WEB-142", day("2026-10-22"), 2)
        .unwrap();

    assert_eq!(preview.activity.len(), 2);
    assert!(
        preview.activity[0].date >= preview.activity[1].date,
        "{:?}",
        preview.activity
    );
}

#[test]
fn a_task_preview_lists_its_subtasks_and_its_own_activity() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();

    let preview = vault.task_preview("task-2", day("2026-10-22"), 5).unwrap();

    let subtasks: Vec<(Option<&str>, &str, TaskStatus)> = preview
        .subtasks
        .iter()
        .map(|subtask| (subtask.id.as_deref(), subtask.text.as_str(), subtask.status))
        .collect();
    assert_eq!(
        subtasks,
        [
            (
                Some("task-3"),
                "Add the gateway test keys to the `.env.example`",
                TaskStatus::Open
            ),
            (
                Some("task-4"),
                "Handle the declined-card response",
                TaskStatus::InProgress
            ),
        ]
    );
    assert_eq!(preview.subtasks[1].estimate.as_deref(), Some("45m"));
    assert_eq!(preview.task_activity.len(), 2);
    assert_eq!(
        preview.task_activity[0].text,
        "session · Wire the payment form to the gateway sandbox · 1h 30m"
    );
    assert_eq!(preview.task_activity[1].text, "Session on the payment form");
    assert_eq!(
        preview
            .story
            .as_ref()
            .and_then(|story| story.number.as_deref()),
        Some("WEB-142")
    );

    let with_activity = vault.task_preview("task-5", day("2026-10-22"), 5).unwrap();
    assert_eq!(with_activity.task_activity.len(), 2);
    assert_eq!(
        with_activity.task_activity[0].kind.as_deref(),
        Some("session")
    );
}

#[test]
fn an_unknown_task_or_story_has_no_preview() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();

    assert!(
        vault
            .task_preview("task-999", day("2026-10-22"), 5)
            .is_none()
    );
    assert!(
        vault
            .story_preview("WEB-999", day("2026-10-22"), 5)
            .is_none()
    );
}

#[test]
fn the_obsidian_link_names_the_vault_folder_and_encodes_the_note_path() {
    let sandbox = sandbox();
    let mut workspace = workspace(&sandbox.root);
    let note = Path::new(common::WEB_142);

    let link = workspace.obsidian_link(note);

    let folder = sandbox.root.file_name().unwrap().to_string_lossy();
    assert_eq!(
        link,
        format!(
            "obsidian://open?vault={folder}&file=Projects%2FWebsite%2FWEB-142%20Checkout%20redesign%2FWEB-142%20Checkout%20redesign"
        )
    );

    // A workspace below the folder that holds `.obsidian/` links from that folder.
    workspace.vault_root = sandbox.root.parent().unwrap().to_path_buf();
    let nested = workspace.obsidian_link(Path::new("Log/2026-10-22.md"));
    let parent = sandbox
        .root
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    assert_eq!(
        nested,
        format!("obsidian://open?vault={parent}&file=vault%2FLog%2F2026-10-22")
    );
}
