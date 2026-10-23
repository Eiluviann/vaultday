//! The shared operations, run against a sandbox copy of the mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{WEB_142, diff, read, sandbox, snapshot, workspace};
use vault_base::ops::{
    AddTaskRequest, LogRequest, OpError, SessionLogRequest, add_story_task, append_day_log,
    append_session_log, build_report, edit_task, format_task_line, next_task_id, open_vault,
    resolve_workspace, restore_task_line, task_by_id, tick_task_done, waiting_items,
};
use vault_base::writer::TaskEdit;
use vault_base::{Date, PlainFileAccess, Time};

fn date(text: &str) -> Date {
    Date::parse(text).unwrap()
}

fn log_request(kind: &str, text: &str) -> LogRequest {
    LogRequest {
        date: date("2026-10-23"),
        time: Time::new(9, 5).unwrap(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        story: None,
        task: None,
        agent: None,
    }
}

#[test]
fn the_deepest_workspace_wins_and_a_story_folder_resolves_to_its_workspace() {
    let sandbox = sandbox();
    let story_folder = sandbox
        .root
        .join("Projects/Website/WEB-142 Checkout redesign");
    let resolved = resolve_workspace(&story_folder, &PlainFileAccess).unwrap();
    assert_eq!(resolved.root, sandbox.root);

    let nested = sandbox.root.join("Clients/Inner");
    std::fs::create_dir_all(nested.join(".vaultday")).unwrap();
    std::fs::write(
        nested.join(".vaultday/settings.json"),
        r#"{"schema_version": 1, "name": "Inner", "kind": "life"}"#,
    )
    .unwrap();
    let resolved = resolve_workspace(&nested, &PlainFileAccess).unwrap();
    assert_eq!(resolved.root, nested);
    assert_eq!(resolved.settings.name, "Inner");
}

#[test]
fn outside_a_workspace_the_path_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let error = resolve_workspace(directory.path(), &PlainFileAccess).unwrap_err();
    assert!(matches!(error, OpError::NotAWorkspace(_)), "{error}");
    assert!(error.to_string().contains("not a workspace"));
}

#[test]
fn waiting_items_list_open_tasks_assigned_to_others_oldest_first() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let items = waiting_items(&vault, date("2026-10-22"));
    let waiting: Vec<(Option<&str>, &str, Option<i64>)> = items
        .iter()
        .map(|item| {
            (
                item.story_number.as_deref(),
                item.assigned_to.as_str(),
                item.age_days,
            )
        })
        .collect();
    // The project hub's question is the oldest; the story's comes after it.
    assert_eq!(waiting.first(), Some(&(None, "Priya", Some(21))));
    assert!(waiting.contains(&(Some("WEB-142"), "Dana", Some(12))));
    assert!(waiting.iter().any(|(_, who, _)| *who == "Morgan"));
    assert!(waiting.iter().all(|(_, who, _)| *who != "me"));
}

#[test]
fn a_task_is_found_by_id_with_its_section() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let task = task_by_id(&vault, "task-6").unwrap();
    assert_eq!(task.story_number.as_deref(), Some("WEB-142"));
    assert_eq!(task.section.as_deref(), Some("pending-input"));
    assert_eq!(task.assigned_to.as_deref(), Some("Dana"));
    assert_eq!(task.status, "open");
    assert_eq!(task.relative_path, WEB_142);
    let missing = task_by_id(&vault, "task-999").unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)));
}

#[test]
fn ticking_a_task_changes_exactly_one_line() {
    let sandbox = sandbox();
    let before = snapshot(&sandbox.root);
    let change = tick_task_done(
        &workspace(&sandbox.root),
        &PlainFileAccess,
        "task-6",
        date("2026-10-23"),
    )
    .unwrap();
    assert_eq!(change.relative_path, WEB_142);
    assert_eq!(
        change.new_line,
        "- [x] Final copy for the order summary  [id:: task-6] [created:: 2026-10-10] [assigned_to:: Dana] [completion:: 2026-10-23]"
    );
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].removed.len(), 1);
    assert_eq!(changes[0].added.len(), 1);

    let missing = tick_task_done(
        &workspace(&sandbox.root),
        &PlainFileAccess,
        "task-999",
        date("2026-10-23"),
    )
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)));
}

#[test]
fn the_next_task_id_is_one_past_the_highest_in_the_vault() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let highest = vault
        .tasks()
        .filter_map(|found| found.task.id()?.strip_prefix("task-")?.parse::<u64>().ok())
        .max()
        .unwrap();
    assert_eq!(next_task_id(&vault), format!("task-{}", highest + 1));
}

fn add_request(section: &str, text: &str) -> AddTaskRequest {
    AddTaskRequest {
        story_number: "WEB-142".to_owned(),
        section: section.to_owned(),
        text: text.to_owned(),
        assigned_to: None,
        id: None,
        created: date("2026-10-23"),
        scheduled: None,
        estimate: None,
        begin_marker: None,
    }
}

#[test]
fn a_task_is_added_under_the_named_section_with_an_allocated_id() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    let expected_id = next_task_id(&vault);
    let added = add_story_task(
        &workspace,
        &PlainFileAccess,
        &add_request("next-steps", "Review the copy"),
    )
    .unwrap();
    assert_eq!(added.id, expected_id);
    assert_eq!(
        added.change.new_line,
        format!("- [ ] Review the copy [id:: {expected_id}] [created:: 2026-10-23]")
    );
    let note = read(&sandbox.root, WEB_142);
    assert!(note.contains(&format!(
        "[due:: 2026-10-28]\n- [ ] Review the copy [id:: {expected_id}]"
    )));

    let mut waiting = add_request("pending-input", "Legal sign-off");
    waiting.assigned_to = Some("Dana".to_owned());
    let added = add_story_task(&workspace, &PlainFileAccess, &waiting).unwrap();
    assert!(added.change.new_line.ends_with("[assigned_to:: Dana]"));
}

#[test]
fn a_planned_task_carries_its_day_and_estimate_and_changes_one_line() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let mut request = add_request("pending-input", "Chase the gateway keys");
    request.id = Some("task-40".to_owned());
    request.scheduled = Some(date("2026-10-24"));
    request.estimate = Some("15m".to_owned());
    request.assigned_to = Some("Lee".to_owned());
    let added = add_story_task(&workspace, &PlainFileAccess, &request).unwrap();
    assert_eq!(
        added.change.new_line,
        "- [ ] Chase the gateway keys [id:: task-40] [created:: 2026-10-23] [scheduled:: 2026-10-24] [estimate:: 15m] [assigned_to:: Lee]"
    );
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1, "{changes:?}");

    let mut bracketed = add_request("next-steps", "Other");
    bracketed.estimate = Some("[1h]".to_owned());
    assert!(matches!(
        add_story_task(&workspace, &PlainFileAccess, &bracketed).unwrap_err(),
        OpError::InvalidInput(_)
    ));
}

#[test]
fn adding_a_task_refuses_unknown_stories_sections_and_text() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let mut request = add_request("next-steps", "x");
    request.story_number = "NOPE-1".to_owned();
    assert!(matches!(
        add_story_task(&workspace, &PlainFileAccess, &request).unwrap_err(),
        OpError::NotFound(_)
    ));
    assert!(matches!(
        add_story_task(&workspace, &PlainFileAccess, &add_request("backlog", "x")).unwrap_err(),
        OpError::InvalidInput(_)
    ));
    assert!(matches!(
        add_story_task(
            &workspace,
            &PlainFileAccess,
            &add_request("next-steps", "a [b:: c]")
        )
        .unwrap_err(),
        OpError::InvalidInput(_)
    ));
    let mut taken = add_request("next-steps", "again");
    taken.id = Some("task-5".to_owned());
    assert!(matches!(
        add_story_task(&workspace, &PlainFileAccess, &taken).unwrap_err(),
        OpError::Conflict(_)
    ));
}

#[test]
fn a_model_written_task_is_wrapped_in_its_marker_block() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let line = format_task_line(
        "Draft the release notes",
        "task-40",
        date("2026-10-23"),
        None,
    )
    .unwrap();
    let marker = "%% ai:begin claude-fable-5-1 · 2026-10-23 · 0123456789ab %%";
    let mut request = add_request("next-steps", "Draft the release notes");
    request.id = Some("task-40".to_owned());
    request.begin_marker = Some(marker.to_owned());
    let added = add_story_task(&workspace, &PlainFileAccess, &request).unwrap();
    assert_eq!(added.change.new_line, line);
    let note = read(&sandbox.root, WEB_142);
    assert!(note.contains(&format!(
        "[due:: 2026-10-28]\n{marker}\n{line}\n%% ai:end %%\n"
    )));

    request.id = Some("task-41".to_owned());
    request.begin_marker = Some("not a marker".to_owned());
    assert!(matches!(
        add_story_task(&workspace, &PlainFileAccess, &request).unwrap_err(),
        OpError::InvalidInput(_)
    ));
}

#[test]
fn a_model_task_fills_the_template_placeholder_instead_of_leaving_it() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let template_story = "---\ntype: story\nnumber: NEW-1\n---\n# New\n\n## Next Steps\n- [ ] \n\n## Pending Input\n- [ ] \n";
    common::write(
        &sandbox.root,
        "Projects/Website/NEW-1 New/NEW-1 New.md",
        template_story,
    );
    let mut request = add_request("pending-input", "Ask legal");
    request.story_number = "NEW-1".to_owned();
    request.id = Some("task-50".to_owned());
    request.begin_marker =
        Some("%% ai:begin claude-fable-5-1 · 2026-10-23 · 0123456789ab %%".to_owned());
    add_story_task(&workspace, &PlainFileAccess, &request).unwrap();
    assert_eq!(
        read(&sandbox.root, "Projects/Website/NEW-1 New/NEW-1 New.md"),
        "---\ntype: story\nnumber: NEW-1\n---\n# New\n\n## Next Steps\n- [ ] \n\n## Pending Input\n%% ai:begin claude-fable-5-1 · 2026-10-23 · 0123456789ab %%\n- [ ] Ask legal [id:: task-50] [created:: 2026-10-23]\n%% ai:end %%\n"
    );
}

#[test]
fn a_decision_is_appended_in_the_documented_format() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let mut request = log_request("decision", "Use a single-page checkout");
    request.date = date("2026-10-22");
    request.story = Some("WEB-142".to_owned());
    request.task = Some("task-5".to_owned());
    let change = append_day_log(&workspace, &PlainFileAccess, &request).unwrap();
    let expected = "- 09:05 decision · Use a single-page checkout [story:: WEB-142] [kind:: decision] [task:: task-5]";
    assert_eq!(change.new_line, expected);
    assert_eq!(change.relative_path, "Log/2026-10-22.md");
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].added, [format!("{expected}\n")]);
}

#[test]
fn a_model_line_names_its_agent_and_facts_do_not() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let mut request = log_request("note", "Checked the gateway docs");
    request.agent = Some("claude-fable-5-1".to_owned());
    let change = append_day_log(&workspace, &PlainFileAccess, &request).unwrap();
    assert_eq!(
        change.new_line,
        "- 09:05 note · Checked the gateway docs [kind:: note] [agent:: claude-fable-5-1]"
    );
    request.agent = None;
    request.text = "Fact line".to_owned();
    let change = append_day_log(&workspace, &PlainFileAccess, &request).unwrap();
    assert!(!change.new_line.contains("agent"));
}

fn session_request(minutes: u64) -> SessionLogRequest {
    SessionLogRequest {
        date: date("2026-10-23"),
        time: Time::new(10, 0).unwrap(),
        session: "ses-41".to_owned(),
        task: Some("task-5".to_owned()),
        story: Some("WEB-142".to_owned()),
        fallback_text: "task-5".to_owned(),
        minutes,
    }
}

#[test]
fn a_stopped_session_is_logged_once_with_its_task_text_and_time() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let change = append_session_log(&workspace, &PlainFileAccess, &session_request(65))
        .unwrap()
        .unwrap();
    assert!(
        change.new_line.starts_with("- 10:00 session · "),
        "{}",
        change.new_line
    );
    assert!(
        change
            .new_line
            .contains(" · 1h 5m [story:: WEB-142] [kind:: session] [id:: ses-41] [task:: task-5]"),
        "{}",
        change.new_line
    );
    assert!(
        !change.new_line.contains("session · task-5 ·"),
        "the task's text, not its id"
    );
    assert_eq!(change.relative_path, "Log/2026-10-23.md");
    let again = append_session_log(&workspace, &PlainFileAccess, &session_request(70)).unwrap();
    assert_eq!(again, None, "the same session is never logged twice");
    let changes = diff(&before, &snapshot(&sandbox.root));
    assert_eq!(changes.len(), 1, "one new day log");
    let session_lines: Vec<&String> = changes[0]
        .added
        .iter()
        .filter(|line| line.contains("[kind:: session]"))
        .collect();
    assert_eq!(session_lines, [&format!("{}\n", change.new_line)]);
}

#[test]
fn a_session_without_a_vault_task_uses_its_fallback_text() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let request = SessionLogRequest {
        task: None,
        story: None,
        fallback_text: "Standup | team [sync]".to_owned(),
        minutes: 0,
        ..session_request(0)
    };
    let change = append_session_log(&workspace, &PlainFileAccess, &request)
        .unwrap()
        .unwrap();
    assert_eq!(
        change.new_line,
        "- 10:00 session · Standup / team (sync) · 0m [kind:: session] [id:: ses-41]"
    );
}

#[test]
fn a_log_line_with_a_bad_kind_story_task_or_text_is_refused_and_writes_nothing() {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let before = snapshot(&sandbox.root);
    let mut unknown_story = log_request("note", "x");
    unknown_story.story = Some("NOPE-1".to_owned());
    let mut unknown_task = log_request("note", "x");
    unknown_task.task = Some("task-999".to_owned());
    for (request, wanted_not_found) in [
        (log_request("celebration", "x"), false),
        (log_request("note", "has | pipe"), false),
        (log_request("note", "  "), false),
        (unknown_story, true),
        (unknown_task, true),
    ] {
        let error = append_day_log(&workspace, &PlainFileAccess, &request).unwrap_err();
        assert_eq!(
            matches!(error, OpError::NotFound(_)),
            wanted_not_found,
            "{error}"
        );
        assert_eq!(
            matches!(error, OpError::InvalidInput(_)),
            !wanted_not_found,
            "{error}"
        );
    }
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}

fn report(subject: &str, from: &str, to: &str) -> Result<vault_base::ops::Report, OpError> {
    let sandbox = sandbox();
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    build_report(&workspace, &vault, subject, date(from), date(to))
}

#[test]
fn a_story_report_carries_frontmatter_task_state_and_its_log_lines() {
    let report = report("WEB-142", "2026-10-21", "2026-10-22").unwrap();
    assert_eq!(report.subject_kind, "story");
    assert_eq!(report.stories.len(), 1);
    let story = &report.stories[0];
    assert_eq!(story.number.as_deref(), Some("WEB-142"));
    assert_eq!(story.status.as_deref(), Some("in-progress"));
    let aliases = story
        .frontmatter
        .iter()
        .find(|entry| entry.key == "aliases")
        .unwrap();
    assert_eq!(aliases.values, ["Checkout v2", "Cart redesign"]);
    let statuses: Vec<&str> = story
        .next_steps
        .iter()
        .map(|task| task.status.as_str())
        .collect();
    assert_eq!(statuses, ["open", "open", "in-progress", "open"]);
    let pending: Vec<&str> = story
        .pending_input
        .iter()
        .map(|task| task.status.as_str())
        .collect();
    assert_eq!(pending, ["open", "done"]);
    assert!(story.note_text.contains("%% ai:begin fable"));
    assert!(
        report
            .log
            .iter()
            .all(|line| line.story.as_deref() == Some("WEB-142") || line.task.is_some())
    );
    let kinds: Vec<&str> = report
        .log
        .iter()
        .filter_map(|line| line.kind.as_deref())
        .collect();
    assert_eq!(
        kinds,
        [
            "status", "decision", "session", "blocker", "done", "session"
        ]
    );
}

#[test]
fn a_client_report_returns_every_event_kind_in_the_fixture() {
    for subject in ["Website", "WEB", "website"] {
        let report = report(subject, "2026-10-21", "2026-10-22").unwrap();
        assert_eq!(report.subject_kind, "client");
        let numbers: Vec<Option<&str>> =
            report.stories.iter().map(|s| s.number.as_deref()).collect();
        assert_eq!(numbers, [Some("WEB-142"), Some("WEB-157")]);
        let mut kinds: Vec<&str> = report
            .log
            .iter()
            .filter_map(|line| line.kind.as_deref())
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(
            kinds,
            ["blocker", "decision", "done", "note", "session", "status"]
        );
        // The project-tagged, model-written line is included and attributed.
        assert!(
            report
                .log
                .iter()
                .any(|line| line.project.as_deref() == Some("Website")
                    && line.agent.as_deref() == Some("claude-fable-5-1"))
        );
    }
}

#[test]
fn the_range_limits_the_log_and_a_workspace_name_covers_every_story() {
    let one_day = report("Website", "2026-10-21", "2026-10-21").unwrap();
    assert!(one_day.log.iter().all(|line| line.date == "2026-10-21"));
    let all = report("Mock Studio", "2026-10-01", "2026-10-31").unwrap();
    assert_eq!(all.stories.len(), 4);
}

#[test]
fn a_report_refuses_unknown_subjects_and_reversed_ranges() {
    assert!(matches!(
        report("Nobody", "2026-10-21", "2026-10-22").unwrap_err(),
        OpError::NotFound(_)
    ));
    assert!(matches!(
        report("WEB-142", "2026-10-22", "2026-10-21").unwrap_err(),
        OpError::InvalidInput(_)
    ));
}

#[test]
fn operations_never_read_the_sync_folder() {
    let sandbox = sandbox();
    common::write(
        &sandbox.root,
        ".vaultday/sync/device/0001.md",
        "---\ntype: story\nnumber: HIDDEN-1\n---\n# Hidden\n",
    );
    let workspace = workspace(&sandbox.root);
    let vault = open_vault(&workspace, &PlainFileAccess).unwrap();
    assert!(vault.story_by_number("HIDDEN-1").is_none());
}

#[test]
fn an_edit_and_its_inverse_leave_the_vault_as_it_was() {
    let sandbox = sandbox();
    let before = snapshot(&sandbox.root);
    let root = workspace(&sandbox.root);
    let scheduled = edit_task(
        &root,
        &PlainFileAccess,
        "task-6",
        &TaskEdit::Schedule(Some(date("2026-10-24"))),
    )
    .unwrap();
    assert!(scheduled.new_line.contains("[scheduled:: 2026-10-24]"));
    assert_eq!(diff(&before, &snapshot(&sandbox.root)).len(), 1);

    edit_task(&root, &PlainFileAccess, "task-6", &TaskEdit::Schedule(None)).unwrap();
    edit_task(
        &root,
        &PlainFileAccess,
        "task-6",
        &TaskEdit::Done(date("2026-10-23")),
    )
    .unwrap();
    edit_task(&root, &PlainFileAccess, "task-6", &TaskEdit::Reopen).unwrap();
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());

    let missing = edit_task(&root, &PlainFileAccess, "task-999", &TaskEdit::Reopen).unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)));
}

#[test]
fn restoring_a_receipts_old_line_undoes_an_edit_byte_for_byte() {
    let sandbox = sandbox();
    let before = snapshot(&sandbox.root);
    let root = workspace(&sandbox.root);
    // Clearing a field and setting it again would move it to the end of the line.
    let cleared = edit_task(&root, &PlainFileAccess, "task-2", &TaskEdit::Schedule(None)).unwrap();
    let old = cleared.old_line.clone().unwrap();
    restore_task_line(&root, &PlainFileAccess, "task-2", &cleared.new_line, &old).unwrap();
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());

    let stale =
        restore_task_line(&root, &PlainFileAccess, "task-2", &cleared.new_line, &old).unwrap_err();
    assert!(matches!(stale, OpError::Conflict(_)), "{stale:?}");
    let without_id =
        restore_task_line(&root, &PlainFileAccess, "task-2", &old, "- [ ] Lost").unwrap_err();
    assert!(
        matches!(without_id, OpError::InvalidInput(_)),
        "{without_id:?}"
    );
    let two_lines = restore_task_line(
        &root,
        &PlainFileAccess,
        "task-2",
        &old,
        &format!("{old}\n- [ ] x"),
    )
    .unwrap_err();
    assert!(
        matches!(two_lines, OpError::InvalidInput(_)),
        "{two_lines:?}"
    );
    assert!(diff(&before, &snapshot(&sandbox.root)).is_empty());
}
