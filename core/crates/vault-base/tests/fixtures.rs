//! Tests against the synthetic fixtures under `core/fixtures`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use vault_base::{
    Date, Document, FileIndex, IndexScope, MarkerKind, NoteType, PlainFileAccess, StoryStatus,
    TaskStatus, Vault, Workspace, WorkspaceError, WorkspaceKind, WorkspaceSet, WorkspaceSettings,
    find_vault_root, find_workspace_root, normalize_path,
};

fn fixtures_root() -> PathBuf {
    normalize_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

fn mock_vault() -> PathBuf {
    fixtures_root().join("mock-vault")
}

fn two_workspaces() -> PathBuf {
    fixtures_root().join("two-workspaces")
}

fn every_file_below(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).expect("fixture folder") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn open_mock_vault() -> Vault {
    let workspace = Workspace::open(&mock_vault(), &PlainFileAccess).expect("mock vault opens");
    Vault::open(
        &workspace,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .expect("vault loads")
}

fn date(text: &str) -> Date {
    Date::parse(text).expect("fixture date")
}

#[test]
fn every_fixture_file_round_trips_byte_for_byte() {
    let mut checked = 0;
    for root in [mock_vault(), two_workspaces()] {
        for path in every_file_below(&root) {
            let source = fs::read_to_string(&path).expect("fixture is UTF-8");
            let document = Document::parse(&source);
            assert_eq!(document.render(), source, "{}", path.display());
            let rendered_lines: String = document
                .lines()
                .iter()
                .map(|line| format!("{}{}", line.text, line.ending.as_str()))
                .collect();
            assert_eq!(rendered_lines, source, "{}", path.display());
            checked += 1;
        }
    }
    assert!(
        checked >= 20,
        "expected the fixtures to hold at least 20 files, found {checked}"
    );
}

#[test]
fn crlf_and_missing_final_newline_survive_and_still_parse() {
    let clipped = fs::read_to_string(mock_vault().join("Inbox/Clipped article.md")).unwrap();
    assert!(clipped.contains("\r\n") && !clipped.ends_with('\n'));
    let document = Document::parse(&clipped);
    assert_eq!(document.render(), clipped);
    assert_eq!(
        document.frontmatter().unwrap().scalar("type").as_deref(),
        Some("clipping")
    );
    assert_eq!(document.tasks().len(), 1);
    assert_eq!(document.tasks()[0].id(), Some("task-14"));

    let quick = fs::read_to_string(mock_vault().join("Inbox/Quick capture.md")).unwrap();
    let document = Document::parse(&quick);
    assert!(document.frontmatter().is_none());
    assert_eq!(document.tasks().len(), 1);
    assert_eq!(document.tasks()[0].id(), None);
}

#[test]
fn markers_inside_sections_pass_through_and_tasks_inside_them_are_still_found() {
    let path = mock_vault()
        .join("Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md");
    let source = fs::read_to_string(path).unwrap();
    let document = Document::parse(&source);
    let kinds: Vec<&MarkerKind> = document.markers().map(|(_, marker)| &marker.kind).collect();
    assert_eq!(kinds, [&MarkerKind::Begin, &MarkerKind::End]);
    let units = document.provenance_units();
    assert_eq!(units.len(), 1);
    let implementation_tasks = document.tasks_under("Implementation");
    assert_eq!(implementation_tasks.len(), 1);
    assert!(document.is_model_text(implementation_tasks[0].line));
    assert_eq!(implementation_tasks[0].status, TaskStatus::Done);
    let fenced_marker_lines = document
        .lines()
        .iter()
        .filter(|line| {
            line.text.contains("%% ai:begin") && matches!(line.kind, vault_base::LineKind::Fenced)
        })
        .count();
    assert_eq!(
        fenced_marker_lines, 1,
        "the marker quoted in the fence is not a marker"
    );
    assert_eq!(document.tasks_under("Next Steps").len(), 4);
    assert_eq!(document.tasks_under("Pending Input").len(), 2);
    let nested: Vec<usize> = document
        .tasks_under("Next Steps")
        .iter()
        .map(|task| task.depth)
        .collect();
    assert_eq!(nested, [0, 1, 1, 0]);

    let note = fs::read_to_string(
        mock_vault().join("Projects/Website/WEB-142 Checkout redesign/Research notes.md"),
    )
    .unwrap();
    let document = Document::parse(&note);
    assert_eq!(document.provenance_units().len(), 1);
    assert!(document.is_model_text(document.lines().len() - 1));
}

#[test]
fn the_mock_workspace_settings_keep_unknown_keys_through_a_rewrite() {
    let text = fs::read_to_string(mock_vault().join(".vaultday/settings.json")).unwrap();
    let settings = WorkspaceSettings::from_json(&text).expect("fixture settings are valid");
    assert_eq!(settings.name, "Mock Studio");
    assert_eq!(settings.kind, WorkspaceKind::Work);
    assert_eq!(settings.ticket_pattern.as_deref(), Some("(WEB|ACME)-\\d+"));
    assert!(settings.extra.contains_key("x_future_setting"));
    let rewritten = settings.to_json().unwrap();
    assert_eq!(
        rewritten, text,
        "the fixture is written in the canonical shape"
    );
    assert_eq!(WorkspaceSettings::from_json(&rewritten).unwrap(), settings);
}

#[test]
fn the_committed_json_schema_is_current() {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/settings.schema.json");
    let mut expected = serde_json::to_string_pretty(&WorkspaceSettings::json_schema()).unwrap();
    expected.push('\n');
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        fs::write(&schema_path, &expected).unwrap();
    }
    let committed = fs::read_to_string(&schema_path).expect("schema/settings.schema.json exists");
    assert_eq!(
        committed, expected,
        "run `UPDATE_SCHEMA=1 cargo test` to regenerate the schema file"
    );
}

#[test]
fn the_mock_vault_loads_with_its_types() {
    let vault = open_mock_vault();
    let types: Vec<(String, NoteType)> = vault
        .notes()
        .map(|note| {
            (
                note.relative_path.to_string_lossy().into_owned(),
                note.note_type.clone(),
            )
        })
        .collect();
    assert!(
        types
            .iter()
            .any(|(path, kind)| path == "Home.md" && *kind == NoteType::Other("home".to_owned()))
    );
    assert!(
        types
            .iter()
            .any(|(path, kind)| path == "Inbox/Quick capture.md" && *kind == NoteType::Untyped)
    );
    assert!(
        !types
            .iter()
            .any(|(path, _)| path.starts_with(".obsidian") || path.starts_with(".vaultday"))
    );
    assert_eq!(
        types
            .iter()
            .filter(|(_, kind)| *kind == NoteType::Story)
            .count(),
        4
    );
    assert!(
        !types.iter().any(|(path, _)| path.starts_with("_templates")),
        "templates are not notes"
    );
    assert_eq!(
        types
            .iter()
            .filter(|(_, kind)| *kind == NoteType::Project)
            .count(),
        2
    );
    assert_eq!(
        types
            .iter()
            .filter(|(_, kind)| *kind == NoteType::Log)
            .count(),
        2
    );
}

#[test]
fn stories_come_with_next_steps_and_pending_input() {
    let vault = open_mock_vault();
    let stories = vault.stories();
    let numbers: Vec<Option<&str>> = stories
        .iter()
        .map(|entry| entry.story.number.as_deref())
        .collect();
    assert_eq!(
        numbers,
        [
            Some("WEB-142"),
            Some("WEB-157"),
            Some("ACME-9"),
            Some("ACME-12")
        ]
    );

    let checkout = vault.story_by_number("WEB-142").unwrap();
    assert_eq!(checkout.story.status, Some(StoryStatus::InProgress));
    assert_eq!(checkout.story.due, Some(date("2026-10-30")));
    assert_eq!(checkout.story.aliases, ["Checkout v2", "Cart redesign"]);
    assert_eq!(checkout.story.source_id.as_deref(), Some("10142"));
    assert_eq!(checkout.story.next_steps.len(), 4);
    assert_eq!(checkout.story.pending_input.len(), 2);
    assert_eq!(checkout.story.next_steps[2].status, TaskStatus::InProgress);
    assert_eq!(checkout.story.next_steps[2].field("estimate"), Some("45m"));

    let newsletter = vault.story_by_number("WEB-157").unwrap();
    assert_eq!(newsletter.story.due, None, "an empty due key is no date");
    assert_eq!(newsletter.story.next_steps.len(), 1);

    let export = vault.story_by_number("ACME-12").unwrap();
    assert_eq!(export.story.sort_index, None);
    assert!(export.story.next_steps.is_empty());
    assert!(export.story.status.unwrap().is_closed());

    let projects = vault.projects();
    let keys: Vec<Option<&str>> = projects.iter().map(|(_, hub)| hub.key.as_deref()).collect();
    assert_eq!(keys, [Some("ACME"), Some("WEB")]);
}

#[test]
fn a_task_is_found_by_id() {
    let vault = open_mock_vault();
    let task = vault.task_by_id("task-3").unwrap();
    assert!(task.path.ends_with("WEB-142 Checkout redesign.md"));
    assert_eq!(task.task.depth, 1);
    assert_eq!(
        task.task.text,
        "Add the gateway test keys to the `.env.example`"
    );
    assert!(vault.task_by_id("task-999").is_none());
    assert_eq!(
        vault.task_by_id("task-14").unwrap().path,
        Path::new("Inbox/Clipped article.md")
    );
}

#[test]
fn todays_log_lines_come_in_time_order() {
    let vault = open_mock_vault();
    let today = vault.log_lines_on(date("2026-10-22"));
    let summary: Vec<(String, &str, Option<&str>)> = today
        .iter()
        .map(|entry| {
            (
                entry.line.time.to_string(),
                entry.line.kind.as_ref().map_or("", |kind| kind.as_str()),
                entry.line.task(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("08:45".to_owned(), "done", Some("task-1")),
            ("09:30".to_owned(), "session", Some("task-5")),
            ("11:15".to_owned(), "done", None),
            ("14:00".to_owned(), "note", Some("task-9")),
        ]
    );
    assert_eq!(today[2].line.project(), Some("Website"));
    assert_eq!(today[2].line.field("agent"), Some("claude-fable-5-1"));
    assert_eq!(vault.log_lines_on(date("2026-10-21")).len(), 4);
    assert!(vault.log_lines_on(date("2026-10-23")).is_empty());
}

#[test]
fn waiting_on_lists_open_tasks_assigned_to_others_with_their_age() {
    let vault = open_mock_vault();
    let waiting = vault.waiting_on("me", date("2026-10-22"));
    let summary: Vec<(&str, Option<i64>, Option<&str>)> = waiting
        .iter()
        .map(|item| (item.assigned_to, item.age_days, item.task.id()))
        .collect();
    assert_eq!(
        summary,
        [
            ("Priya", Some(21), Some("task-11")),
            ("Dana", Some(12), Some("task-6")),
            ("Lee", Some(7), Some("task-9")),
            ("Morgan", None, None),
        ]
    );
    assert_eq!(waiting[1].story_number, Some("WEB-142"));
    assert_eq!(waiting[0].story_number, None, "hub tasks have no story");
    assert!(
        vault
            .waiting_on("Dana", date("2026-10-22"))
            .iter()
            .all(|item| item.assigned_to != "Dana")
    );
}

#[test]
fn file_index_scope_narrows_to_recent_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("old.md"), "old").unwrap();
    fs::write(root.join("new.md"), "new").unwrap();
    let now = SystemTime::now();
    fs::File::open(root.join("old.md"))
        .unwrap()
        .set_modified(now - Duration::from_secs(30 * 86_400))
        .unwrap();
    fs::File::open(root.join("new.md"))
        .unwrap()
        .set_modified(now - Duration::from_secs(86_400))
        .unwrap();
    let mut recent = FileIndex::new(root, IndexScope::edited_within(7)).unwrap();
    let _ = recent.refresh(&PlainFileAccess, now).unwrap();
    let files: Vec<&Path> = recent.files().map(|(path, _)| path).collect();
    assert_eq!(files, [Path::new("new.md")]);

    let mut full = FileIndex::new(&mock_vault(), IndexScope::default()).unwrap();
    let delta = full.refresh(&PlainFileAccess, now).unwrap();
    assert_eq!(delta.added.len(), full.len());
    assert!(full.stamp(Path::new("Log/2026-10-22.md")).is_some());
    assert!(full.stamp(Path::new(".obsidian/app.json")).is_none());
    assert!(full.stamp(Path::new("_templates/story.md")).is_none());
}

#[test]
fn an_enclosing_workspace_never_reads_a_nested_one() {
    let root = two_workspaces();
    let home = Workspace::open(&root, &PlainFileAccess).unwrap();
    let home_vault = Vault::open(
        &home,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    let paths: Vec<&Path> = home_vault
        .notes()
        .map(|note| note.relative_path.as_path())
        .collect();
    assert_eq!(paths, [Path::new("Notes/Groceries.md")]);
    assert!(
        home_vault.projects().is_empty(),
        "the Acme hub belongs to the Acme workspace"
    );

    let acme = Workspace::open(&root.join("Clients/Acme"), &PlainFileAccess).unwrap();
    let acme_vault = Vault::open(
        &acme,
        IndexScope::default(),
        &PlainFileAccess,
        SystemTime::now(),
    )
    .unwrap();
    let paths: Vec<&Path> = acme_vault
        .notes()
        .map(|note| note.relative_path.as_path())
        .collect();
    assert_eq!(paths, [Path::new("Projects/Portal.md")]);
    assert_eq!(acme_vault.projects().len(), 1);
}

#[test]
fn a_non_utf8_note_is_skipped_and_reported() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("good.md"), "# ok\n").unwrap();
    fs::write(directory.path().join("bad.md"), [0xff, 0xfe, b'x']).unwrap();
    let mut index = FileIndex::new(directory.path(), IndexScope::default()).unwrap();
    let _ = index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    let vault = Vault::load(&index, "Log", &PlainFileAccess).unwrap();
    assert_eq!(vault.len(), 1);
    assert_eq!(vault.unreadable().len(), 1);
    assert_eq!(vault.unreadable()[0].relative_path, Path::new("bad.md"));
}

#[test]
fn the_deepest_workspace_wins_and_the_vault_root_is_the_obsidian_folder() {
    let root = two_workspaces();
    let set = WorkspaceSet::discover(std::slice::from_ref(&root), &PlainFileAccess)
        .expect("both workspaces open");
    assert_eq!(set.len(), 2);
    let acme_note = root.join("Clients/Acme/Projects/Portal.md");
    let home_note = root.join("Notes/Groceries.md");
    assert_eq!(set.resolve(&acme_note).unwrap().settings.name, "Acme");
    assert_eq!(set.resolve(&home_note).unwrap().settings.name, "Home");
    assert_eq!(
        set.resolve(&root.join("Clients")).unwrap().settings.name,
        "Home"
    );
    assert!(set.resolve(Path::new("/elsewhere/note.md")).is_none());

    assert_eq!(
        find_workspace_root(&acme_note, &PlainFileAccess).unwrap(),
        Some(root.join("Clients/Acme"))
    );
    assert_eq!(
        find_workspace_root(&home_note, &PlainFileAccess).unwrap(),
        Some(root.clone())
    );

    let acme = set.resolve(&acme_note).unwrap();
    assert_eq!(
        acme.vault_root, root,
        "the nearest .obsidian ancestor is the outer vault"
    );
    assert_eq!(find_vault_root(&acme.root, &PlainFileAccess).unwrap(), root);

    let mock = Workspace::open(&mock_vault(), &PlainFileAccess).unwrap();
    assert_eq!(mock.vault_root, mock_vault());
    assert_eq!(mock.log_folder_path(), mock_vault().join("Log"));
}

#[test]
fn a_relative_start_still_yields_an_absolute_workspace_root() {
    // Cargo runs integration tests from the crate folder.
    let relative = Path::new("../../fixtures/mock-vault/Projects/Website/Website.md");
    assert!(relative.is_relative() && relative.exists());
    let root = find_workspace_root(relative, &PlainFileAccess).unwrap();
    assert_eq!(root, Some(mock_vault()));
    assert_eq!(
        find_vault_root(Path::new("../../fixtures/mock-vault"), &PlainFileAccess).unwrap(),
        mock_vault()
    );
}

#[test]
fn a_duplicate_task_id_resolves_to_the_first_in_path_then_line_order() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("b.md"),
        "- [ ] second file [id:: dup]\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("a.md"),
        "- [x] done without an id\n- [ ] first [id:: dup]\n- [ ] later [id:: dup]\n",
    )
    .unwrap();
    let mut index = FileIndex::new(directory.path(), IndexScope::default()).unwrap();
    let _ = index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    let vault = Vault::load(&index, "Log", &PlainFileAccess).unwrap();
    let found = vault.task_by_id("dup").unwrap();
    assert_eq!(
        (found.path, found.task.text.as_str()),
        (Path::new("a.md"), "first")
    );
    assert_eq!(
        vault
            .tasks()
            .filter(|task_ref| task_ref.task.id() == Some("dup"))
            .count(),
        3
    );
    assert_eq!(vault.tasks().next().unwrap().task.id(), None);
}

#[test]
fn a_folder_without_a_manifest_is_not_a_workspace() {
    let error = Workspace::open(&two_workspaces().join("Notes"), &PlainFileAccess).unwrap_err();
    assert!(matches!(error, WorkspaceError::NotAWorkspace(_)), "{error}");
    assert_eq!(
        find_workspace_root(Path::new("/"), &PlainFileAccess).unwrap(),
        None
    );
}

#[test]
fn nested_work_workspaces_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let outer = directory.path().join("Clients");
    let inner = outer.join("Acme");
    for (root, name) in [(&outer, "Clients"), (&inner, "Acme")] {
        fs::create_dir_all(root.join(".vaultday")).unwrap();
        let settings = WorkspaceSettings::new(name, WorkspaceKind::Work)
            .to_json()
            .unwrap();
        fs::write(root.join(".vaultday/settings.json"), settings).unwrap();
    }
    let error =
        WorkspaceSet::discover(&[directory.path().to_path_buf()], &PlainFileAccess).unwrap_err();
    match error {
        WorkspaceError::OverlappingWorkWorkspaces {
            outer: found_outer,
            inner: found_inner,
        } => {
            assert_eq!(found_outer, normalize_path(&outer));
            assert_eq!(found_inner, normalize_path(&inner));
        }
        other => panic!("expected the overlap refusal, got {other}"),
    }

    let life_outer = WorkspaceSettings::new("Home", WorkspaceKind::Life)
        .to_json()
        .unwrap();
    fs::write(outer.join(".vaultday/settings.json"), life_outer).unwrap();
    let set = WorkspaceSet::discover(&[directory.path().to_path_buf()], &PlainFileAccess)
        .expect("life around work is fine");
    assert_eq!(set.len(), 2);
    let a_file_twice = [directory.path().to_path_buf(), outer.clone()];
    assert_eq!(
        WorkspaceSet::discover(&a_file_twice, &PlainFileAccess)
            .unwrap()
            .len(),
        2,
        "duplicate roots collapse"
    );
}

#[test]
fn a_refused_manifest_names_the_file() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join(".vaultday")).unwrap();
    fs::write(
        directory.path().join(".vaultday/settings.json"),
        r#"{"schema_version": 1, "name": "x", "kind": "work", "tokens": {"tracker": "abc"}}"#,
    )
    .unwrap();
    let error = Workspace::open(directory.path(), &PlainFileAccess).unwrap_err();
    assert!(matches!(error, WorkspaceError::Settings { .. }));
    assert!(
        error.to_string().contains("settings.json") && error.to_string().contains("tokens"),
        "{error}"
    );
}
