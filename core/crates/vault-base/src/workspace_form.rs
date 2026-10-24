//! What the onboarding Configure sheet and the folder steps need from the
//! manifest: the form-level checks with one message per field, creating and
//! updating `.vaultday/settings.json` without losing the keys this build does
//! not know, the one-time prefill from a `Workspace.md`, and the folder
//! overlap rule.
//!
//! The runtime never reads `Workspace.md`: [`prefill_from_note`] runs once,
//! when a folder without `settings.json` becomes a workspace, and its answer
//! lands in the manifest the app owns from then on.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::file_access::{FileAccess, FileAccessError};
use crate::scan::Document;
use crate::settings::{MAX_HOURS_PER_DAY, SettingsError, WorkspaceKind, WorkspaceSettings};
use crate::workspace::{SETTINGS_FILE_NAME, WORKSPACE_DIR_NAME, normalize_path};

/// The note a prefill reads, at the workspace root.
pub const WORKSPACE_NOTE_FILE_NAME: &str = "Workspace.md";
/// Longest workspace name the form accepts.
pub const MAX_NAME_CHARS: usize = 80;

/// One thing wrong with one field of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIssue {
    /// The `settings.json` key the message is about.
    pub field: &'static str,
    /// What to tell the user.
    pub message: String,
}

/// Why a manifest could not be created or updated.
#[derive(Debug, Error)]
pub enum WorkspaceFormError {
    /// One or more fields failed the form checks; nothing was written.
    #[error("{}", join_issues(.0))]
    Invalid(Vec<FieldIssue>),
    /// The folder already holds a manifest, so it cannot be created again.
    #[error("{0} already holds .vaultday/settings.json")]
    AlreadyAWorkspace(PathBuf),
    /// The folder holds no manifest to update.
    #[error("{0} has no .vaultday/settings.json")]
    NotAWorkspace(PathBuf),
    /// The existing manifest was refused by the schema.
    #[error("{0}")]
    Settings(#[from] SettingsError),
    /// The file system failed.
    #[error(transparent)]
    FileAccess(#[from] FileAccessError),
}

/// The values a `Workspace.md` offered, each present only when the note says it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspacePrefill {
    /// The name: the frontmatter `name`, else the first `# ` heading.
    pub name: Option<String>,
    /// Accent token.
    pub accent: Option<String>,
    /// Ticket pattern.
    pub ticket_pattern: Option<String>,
    /// Ticket link template.
    pub ticket_url: Option<String>,
    /// Hosts.
    pub hosts: Vec<String>,
    /// Browser.
    pub browser: Option<String>,
    /// Working hours a day.
    pub hours_per_day: Option<f64>,
    /// Log folder.
    pub log_folder: Option<String>,
    /// Story template.
    pub story_template: Option<String>,
}

/// Where a workspace lives, for the placement rule: its root and the real
/// targets of its linked folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedWorkspace {
    /// The workspace's id.
    pub id: String,
    /// The workspace's name.
    pub name: String,
    /// Whether the workspace is client work.
    pub kind: WorkspaceKind,
    /// The folder holding `.vaultday/`.
    pub root: PathBuf,
    /// The folders linked into the root, with every symlink resolved.
    pub linked_targets: Vec<PathBuf>,
}

/// A folder that may not become a workspace root, or join a workspace.
/// Each message names the folder and the workspace it clashes with.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FolderRefusal {
    /// The folder already is a workspace's root.
    #[error("{folder} is already the workspace {workspace}")]
    AlreadyAWorkspace {
        /// The folder.
        folder: PathBuf,
        /// The workspace rooted there.
        workspace: String,
    },
    /// The folder is already part of the workspace it is offered to.
    #[error("{folder} is already part of {workspace}")]
    AlreadyInWorkspace {
        /// The folder.
        folder: PathBuf,
        /// The workspace.
        workspace: String,
    },
    /// The folder lies inside another workspace's root or linked folder.
    #[error("{folder} already belongs to {workspace}")]
    BelongsTo {
        /// The folder.
        folder: PathBuf,
        /// The workspace that holds it.
        workspace: String,
    },
    /// The folder holds a workspace's root or linked folder, whose files
    /// would then be read twice.
    #[error("{folder} holds files of {workspace}")]
    HoldsWorkspace {
        /// The folder.
        folder: PathBuf,
        /// The workspace whose files it holds.
        workspace: String,
    },
    /// Two client workspace roots would nest.
    #[error(
        "{inner} and {outer} would overlap, and client workspaces may not share files: {workspace} is work too"
    )]
    OverlappingWork {
        /// The enclosing folder.
        outer: PathBuf,
        /// The enclosed folder.
        inner: PathBuf,
        /// The client workspace already there.
        workspace: String,
    },
}

impl FolderRefusal {
    /// The name of the workspace the folder clashes with, for a popup's title.
    #[must_use]
    pub fn workspace_name(&self) -> &str {
        match self {
            FolderRefusal::AlreadyAWorkspace { workspace, .. }
            | FolderRefusal::AlreadyInWorkspace { workspace, .. }
            | FolderRefusal::BelongsTo { workspace, .. }
            | FolderRefusal::HoldsWorkspace { workspace, .. }
            | FolderRefusal::OverlappingWork { workspace, .. } => workspace,
        }
    }
}

/// Every problem with the form's values, in field order. Stricter than
/// [`WorkspaceSettings::from_json`], which has to accept what other builds
/// and hand edits wrote: these are the rules the app holds its own writes to.
#[must_use]
pub fn form_issues(settings: &WorkspaceSettings) -> Vec<FieldIssue> {
    let mut issues = Vec::new();
    let name = settings.name.trim();
    if name.is_empty() {
        issues.push(issue("name", "Give the workspace a name"));
    } else if name.chars().count() > MAX_NAME_CHARS {
        issues.push(issue(
            "name",
            format!("The name may be at most {MAX_NAME_CHARS} characters"),
        ));
    }
    if let Some(accent) = &settings.accent
        && (accent.trim().is_empty() || accent.chars().any(char::is_whitespace))
    {
        issues.push(issue("accent", "The color is a single word"));
    }
    if let Some(url) = &settings.ticket_url
        && !url.contains("{ticket}")
    {
        issues.push(issue(
            "ticket_url",
            "The link needs {ticket} where the ticket number goes",
        ));
    }
    for host in &settings.hosts {
        if !is_plain_host(host) {
            issues.push(issue(
                "hosts",
                format!("{host:?} is not a host name; write it like tracker.example.com"),
            ));
        }
    }
    if let Some(browser) = &settings.browser
        && browser.trim().is_empty()
    {
        issues.push(issue(
            "browser",
            "Name a browser, or leave it empty for the default",
        ));
    }
    if !settings.hours_per_day.is_finite()
        || settings.hours_per_day <= 0.0
        || settings.hours_per_day > MAX_HOURS_PER_DAY
    {
        issues.push(issue(
            "hours_per_day",
            format!("Working hours must be above 0 and at most {MAX_HOURS_PER_DAY}"),
        ));
    }
    if !is_plain_relative_folder(&settings.log_folder) {
        issues.push(issue(
            "log_folder",
            "The log folder is a folder inside the workspace, without . or .. in it",
        ));
    }
    if let Some(template) = &settings.story_template
        && !is_plain_relative_folder(template)
    {
        issues.push(issue(
            "story_template",
            "The story template is a note inside the workspace, without . or .. in its path",
        ));
    }
    issues
}

/// The settings for a folder becoming a workspace: the user's `name` and
/// `kind`, and, when the folder has a `Workspace.md`, whatever else that note
/// offers (its name only when `name` is blank). A note value the form would
/// refuse is left out, so a hand-written note never keeps the folder from
/// becoming a workspace; the user sets it in the Configure sheet instead.
#[must_use]
pub fn new_workspace_settings(
    name: &str,
    kind: WorkspaceKind,
    note: Option<&WorkspacePrefill>,
) -> WorkspaceSettings {
    let chosen_name = if name.trim().is_empty() {
        note.and_then(|note| note.name.clone()).unwrap_or_default()
    } else {
        name.trim().to_owned()
    };
    let mut settings = WorkspaceSettings::new(chosen_name, kind);
    let Some(note) = note else {
        return settings;
    };
    settings.accent.clone_from(&note.accent);
    settings.ticket_pattern.clone_from(&note.ticket_pattern);
    settings.ticket_url.clone_from(&note.ticket_url);
    settings.hosts.clone_from(&note.hosts);
    settings.browser.clone_from(&note.browser);
    if let Some(hours) = note.hours_per_day {
        settings.hours_per_day = hours;
    }
    if let Some(log_folder) = &note.log_folder {
        settings.log_folder.clone_from(log_folder);
    }
    settings.story_template.clone_from(&note.story_template);
    leave_out_refused_note_values(&mut settings);
    settings
}

/// Reads what a `Workspace.md` offers: frontmatter keys named like the
/// manifest's, and the first `# ` heading as the name.
#[must_use]
pub fn prefill_from_note(text: &str) -> WorkspacePrefill {
    let document = Document::parse(text);
    let frontmatter = document.frontmatter();
    let scalar = |key: &str| {
        frontmatter
            .and_then(|fields| fields.scalar(key))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let heading_name = document
        .headings()
        .find(|(_, heading)| heading.level == 1)
        .map(|(_, heading)| heading.title.clone())
        .filter(|title| !title.is_empty());
    WorkspacePrefill {
        name: scalar("name").or(heading_name),
        accent: scalar("accent"),
        ticket_pattern: scalar("ticket_pattern"),
        ticket_url: scalar("ticket_url"),
        hosts: frontmatter
            .map(|fields| fields.list("hosts"))
            .unwrap_or_default(),
        browser: scalar("browser"),
        hours_per_day: scalar("hours_per_day").and_then(|text| text.parse().ok()),
        log_folder: scalar("log_folder"),
        story_template: scalar("story_template"),
    }
}

/// Reads the folder's `Workspace.md`, when there is one.
///
/// # Errors
/// File errors other than "not found".
pub fn read_note_prefill(
    root: &Path,
    file_access: &dyn FileAccess,
) -> Result<Option<WorkspacePrefill>, FileAccessError> {
    match file_access.read_to_string(&root.join(WORKSPACE_NOTE_FILE_NAME)) {
        Ok(text) => Ok(Some(prefill_from_note(&text))),
        Err(FileAccessError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Writes a new `.vaultday/settings.json` in `root`, creating `.vaultday/`.
///
/// # Errors
/// [`WorkspaceFormError::Invalid`] with every issue, [`WorkspaceFormError::AlreadyAWorkspace`]
/// when a manifest exists, schema and file errors otherwise.
pub fn create_settings_file(
    root: &Path,
    settings: &WorkspaceSettings,
    file_access: &dyn FileAccess,
) -> Result<(), WorkspaceFormError> {
    let issues = form_issues(settings);
    if !issues.is_empty() {
        return Err(WorkspaceFormError::Invalid(issues));
    }
    let root = normalize_path(root);
    let path = settings_file(&root);
    if file_access.stat(&path)?.is_some() {
        return Err(WorkspaceFormError::AlreadyAWorkspace(root));
    }
    let text = settings.to_json()?;
    file_access.create_dir_all(&root.join(WORKSPACE_DIR_NAME))?;
    file_access.write_atomic(&path, &text)?;
    Ok(())
}

/// Reads the manifest, lets `edit` change it, checks the form rules and
/// writes it back. Keys this build does not know, and the `features`
/// namespace, are carried through untouched.
///
/// # Errors
/// [`WorkspaceFormError::NotAWorkspace`] without a manifest, [`WorkspaceFormError::Invalid`]
/// with every issue (the file stays as it was), schema and file errors otherwise.
pub fn update_settings_file(
    root: &Path,
    file_access: &dyn FileAccess,
    edit: impl FnOnce(&mut WorkspaceSettings),
) -> Result<WorkspaceSettings, WorkspaceFormError> {
    let root = normalize_path(root);
    let path = settings_file(&root);
    let text = match file_access.read_to_string(&path) {
        Ok(text) => text,
        Err(FileAccessError::NotFound(_)) => return Err(WorkspaceFormError::NotAWorkspace(root)),
        Err(error) => return Err(error.into()),
    };
    let mut settings = WorkspaceSettings::from_json(&text)?;
    edit(&mut settings);
    let issues = form_issues(&settings);
    if !issues.is_empty() {
        return Err(WorkspaceFormError::Invalid(issues));
    }
    file_access.write_atomic(&path, &settings.to_json()?)?;
    Ok(settings)
}

/// Whether `candidate` may become a new workspace root of `kind`, given the
/// workspaces already placed.
///
/// A root may be taken once, and may not lie inside or hold another
/// workspace's linked folder. Roots may nest (the nearest `.vaultday/` owns
/// a file), except two client roots.
///
/// # Errors
/// The first [`FolderRefusal`] that applies.
pub fn check_new_workspace_root(
    candidate: &Path,
    kind: WorkspaceKind,
    existing: &[PlacedWorkspace],
) -> Result<(), FolderRefusal> {
    let candidate = normalize_path(candidate);
    if let Some(workspace) = existing
        .iter()
        .find(|workspace| normalize_path(&workspace.root) == candidate)
    {
        return Err(FolderRefusal::AlreadyAWorkspace {
            folder: candidate,
            workspace: workspace.name.clone(),
        });
    }
    if let Some(refusal) = linked_folder_clash(&candidate, existing) {
        return Err(refusal);
    }
    for workspace in existing {
        if kind != WorkspaceKind::Work || workspace.kind != WorkspaceKind::Work {
            continue;
        }
        let root = normalize_path(&workspace.root);
        if root.starts_with(&candidate) {
            return Err(FolderRefusal::OverlappingWork {
                outer: candidate,
                inner: root,
                workspace: workspace.name.clone(),
            });
        }
        if candidate.starts_with(&root) {
            return Err(FolderRefusal::OverlappingWork {
                outer: root,
                inner: candidate,
                workspace: workspace.name.clone(),
            });
        }
    }
    Ok(())
}

/// Whether `candidate` may join the workspace `workspace_id`, linked or
/// moved into its root, given the workspaces already placed.
///
/// Each file belongs to at most one workspace, so the folder may not lie
/// inside any workspace's root or linked folder (its own included: it is
/// already part of it), and may not hold one.
///
/// # Errors
/// The first [`FolderRefusal`] that applies.
pub fn check_folder_for_workspace(
    candidate: &Path,
    workspace_id: &str,
    existing: &[PlacedWorkspace],
) -> Result<(), FolderRefusal> {
    let candidate = normalize_path(candidate);
    let holder = existing
        .iter()
        .flat_map(|workspace| areas(workspace).map(move |area| (workspace, area)))
        .filter(|(_, area)| candidate.starts_with(area))
        .max_by_key(|(_, area)| area.components().count());
    if let Some((workspace, _)) = holder {
        return Err(if workspace.id == workspace_id {
            FolderRefusal::AlreadyInWorkspace {
                folder: candidate,
                workspace: workspace.name.clone(),
            }
        } else {
            FolderRefusal::BelongsTo {
                folder: candidate,
                workspace: workspace.name.clone(),
            }
        });
    }
    if let Some(workspace) = existing
        .iter()
        .find(|workspace| areas(workspace).any(|area| area.starts_with(&candidate)))
    {
        return Err(FolderRefusal::HoldsWorkspace {
            folder: candidate,
            workspace: workspace.name.clone(),
        });
    }
    Ok(())
}

/// The refusal when `candidate` lies inside or holds a workspace's linked folder.
fn linked_folder_clash(candidate: &Path, existing: &[PlacedWorkspace]) -> Option<FolderRefusal> {
    for workspace in existing {
        for target in &workspace.linked_targets {
            let target = normalize_path(target);
            if candidate.starts_with(&target) {
                return Some(FolderRefusal::BelongsTo {
                    folder: candidate.to_path_buf(),
                    workspace: workspace.name.clone(),
                });
            }
            if target.starts_with(candidate) {
                return Some(FolderRefusal::HoldsWorkspace {
                    folder: candidate.to_path_buf(),
                    workspace: workspace.name.clone(),
                });
            }
        }
    }
    None
}

/// A workspace's root and linked targets, normalised.
fn areas(workspace: &PlacedWorkspace) -> impl Iterator<Item = PathBuf> + '_ {
    std::iter::once(&workspace.root)
        .chain(&workspace.linked_targets)
        .map(|path| normalize_path(path))
}

fn leave_out_refused_note_values(settings: &mut WorkspaceSettings) {
    let defaults = WorkspaceSettings::new(String::new(), settings.kind);
    for issue in form_issues(settings) {
        match issue.field {
            "accent" => settings.accent = None,
            "ticket_url" => settings.ticket_url = None,
            "hosts" => settings.hosts.retain(|host| is_plain_host(host)),
            "browser" => settings.browser = None,
            "hours_per_day" => settings.hours_per_day = defaults.hours_per_day,
            "log_folder" => settings.log_folder.clone_from(&defaults.log_folder),
            "story_template" => settings.story_template = None,
            _ => {}
        }
    }
}

fn settings_file(root: &Path) -> PathBuf {
    root.join(WORKSPACE_DIR_NAME).join(SETTINGS_FILE_NAME)
}

fn issue(field: &'static str, message: impl Into<String>) -> FieldIssue {
    FieldIssue {
        field,
        message: message.into(),
    }
}

fn join_issues(issues: &[FieldIssue]) -> String {
    issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

fn is_plain_host(host: &str) -> bool {
    !host.is_empty()
        && !host.contains("://")
        && host.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | ':')
        })
}

fn is_plain_relative_folder(text: &str) -> bool {
    !text.is_empty()
        && !text.ends_with(['/', '\\'])
        && Path::new(text)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::file_access::PlainFileAccess;

    fn valid_settings() -> WorkspaceSettings {
        WorkspaceSettings::new("Freelancing", WorkspaceKind::Work)
    }

    fn fields(issues: &[FieldIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.field).collect()
    }

    #[test]
    fn a_minimal_workspace_has_no_issues() {
        assert!(form_issues(&valid_settings()).is_empty());
    }

    #[test]
    fn every_bad_field_gets_its_own_message() {
        let mut settings = valid_settings();
        settings.name = "  ".to_owned();
        settings.accent = Some("deep blue".to_owned());
        settings.ticket_url = Some("https://tracker.example.test/browse/".to_owned());
        settings.hosts = vec![
            "https://tracker.example.test".to_owned(),
            "ok.example.test".to_owned(),
        ];
        settings.browser = Some(" ".to_owned());
        settings.hours_per_day = 25.0;
        settings.log_folder = "../Log".to_owned();
        settings.story_template = Some("/etc/story.md".to_owned());
        let issues = form_issues(&settings);
        assert_eq!(
            fields(&issues),
            [
                "name",
                "accent",
                "ticket_url",
                "hosts",
                "browser",
                "hours_per_day",
                "log_folder",
                "story_template"
            ]
        );
        assert!(issues.iter().all(|issue| !issue.message.is_empty()));
        assert!(issues[2].message.contains("{ticket}"));
    }

    #[test]
    fn nan_and_zero_hours_are_refused() {
        for hours in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
            let mut settings = valid_settings();
            settings.hours_per_day = hours;
            assert_eq!(
                fields(&form_issues(&settings)),
                ["hours_per_day"],
                "{hours}"
            );
        }
    }

    #[test]
    fn the_note_prefill_reads_frontmatter_and_the_first_heading() {
        let note = "---\ntype: workspace\nticket_pattern: \"(WEB|ACME)-\\\\d+\"\nhosts:\n  - tracker.example.test\nhours_per_day: 6.5\naccent: plum\n---\n# Mock Studio\n\n## Elsewhere\n";
        let prefill = prefill_from_note(note);
        assert_eq!(prefill.name.as_deref(), Some("Mock Studio"));
        assert_eq!(prefill.ticket_pattern.as_deref(), Some("(WEB|ACME)-\\d+"));
        assert_eq!(prefill.hosts, ["tracker.example.test"]);
        assert_eq!(prefill.hours_per_day, Some(6.5));
        assert_eq!(prefill.accent.as_deref(), Some("plum"));
        assert_eq!(prefill.browser, None);
        let plain = prefill_from_note("# Only a heading\n");
        assert_eq!(plain.name.as_deref(), Some("Only a heading"));
        assert_eq!(prefill_from_note(""), WorkspacePrefill::default());
    }

    #[test]
    fn a_blank_name_takes_the_notes_name_and_a_given_name_wins() {
        let note = prefill_from_note("---\nlog_folder: Journal\n---\n# From the note\n");
        let from_note = new_workspace_settings(" ", WorkspaceKind::Life, Some(&note));
        assert_eq!(from_note.name, "From the note");
        assert_eq!(from_note.log_folder, "Journal");
        assert_eq!(from_note.kind, WorkspaceKind::Life);
        let given = new_workspace_settings("Chosen", WorkspaceKind::Work, Some(&note));
        assert_eq!(given.name, "Chosen");
    }

    #[test]
    fn a_note_value_the_form_refuses_is_left_out_and_the_rest_is_kept() {
        let note = prefill_from_note(
            "---\naccent: deep blue\nticket_url: https://tracker.example.test/browse/\nhosts:\n  - https://tracker.example.test\nbrowser: firefox\nhours_per_day: 30\nlog_folder: ../Log\nstory_template: _templates/story.md\n---\n# Noted\n",
        );
        let settings = new_workspace_settings("", WorkspaceKind::Work, Some(&note));
        assert!(
            form_issues(&settings).is_empty(),
            "{:?}",
            form_issues(&settings)
        );
        let defaults = WorkspaceSettings::new("Noted", WorkspaceKind::Work);
        assert_eq!(settings.name, "Noted");
        assert_eq!(settings.accent, None);
        assert_eq!(settings.ticket_url, None);
        assert!(settings.hosts.is_empty());
        assert_eq!(settings.hours_per_day, defaults.hours_per_day);
        assert_eq!(settings.log_folder, defaults.log_folder);
        assert_eq!(settings.browser.as_deref(), Some("firefox"));
        assert_eq!(
            settings.story_template.as_deref(),
            Some("_templates/story.md")
        );
    }

    #[test]
    fn create_then_update_keeps_unknown_keys_and_refuses_bad_edits_untouched() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path();
        create_settings_file(root, &valid_settings(), &PlainFileAccess).expect("created");
        let path = settings_file(root);
        let seeded = fs::read_to_string(&path).expect("read").replace(
            "\"features\": {}",
            "\"features\": {}, \"x_future\": {\"kept\": [1, 2]}",
        );
        fs::write(&path, &seeded).expect("seed unknown key");

        let updated = update_settings_file(root, &PlainFileAccess, |settings| {
            settings.name = "Renamed".to_owned();
            settings.hosts = vec!["tracker.example.test".to_owned()];
        })
        .expect("updated");
        assert_eq!(updated.name, "Renamed");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"x_future\""), "{text}");
        assert!(text.contains("\"kept\""), "{text}");

        let before = fs::read_to_string(&path).expect("read");
        let error = update_settings_file(root, &PlainFileAccess, |settings| {
            settings.hours_per_day = 99.0;
        })
        .expect_err("refused");
        assert!(
            matches!(&error, WorkspaceFormError::Invalid(issues) if fields(issues) == ["hours_per_day"])
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), before);

        let again =
            create_settings_file(root, &valid_settings(), &PlainFileAccess).expect_err("exists");
        assert!(matches!(again, WorkspaceFormError::AlreadyAWorkspace(_)));
        let missing = tempfile::tempdir().expect("temp dir");
        let none =
            update_settings_file(missing.path(), &PlainFileAccess, |_| {}).expect_err("none");
        assert!(matches!(none, WorkspaceFormError::NotAWorkspace(_)));
    }

    #[test]
    fn a_secret_in_features_is_refused_on_update() {
        let folder = tempfile::tempdir().expect("temp dir");
        create_settings_file(folder.path(), &valid_settings(), &PlainFileAccess).expect("created");
        let error = update_settings_file(folder.path(), &PlainFileAccess, |settings| {
            settings
                .features
                .insert("mail".to_owned(), serde_json::json!({ "api_token": "abc" }));
        })
        .expect_err("refused");
        assert!(matches!(
            error,
            WorkspaceFormError::Settings(SettingsError::SecretKey { .. })
        ));
    }

    fn placed(root: &str, workspace: &str, kind: WorkspaceKind, links: &[&str]) -> PlacedWorkspace {
        PlacedWorkspace {
            id: format!("ws-{workspace}"),
            name: workspace.to_owned(),
            kind,
            root: PathBuf::from(root),
            linked_targets: links.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_root_is_taken_once() {
        let existing = [placed("/v/a", "A", WorkspaceKind::Work, &[])];
        let error = check_new_workspace_root(Path::new("/v/a/"), WorkspaceKind::Life, &existing)
            .expect_err("refused");
        assert!(matches!(error, FolderRefusal::AlreadyAWorkspace { .. }));
        assert_eq!(error.to_string(), "/v/a is already the workspace A");
    }

    #[test]
    fn client_roots_may_not_nest_either_way_round() {
        let existing = [placed("/v/clients/a", "A", WorkspaceKind::Work, &[])];
        let inner = check_new_workspace_root(
            Path::new("/v/clients/a/docs"),
            WorkspaceKind::Work,
            &existing,
        )
        .expect_err("inner refused");
        assert!(inner.to_string().contains("overlap"), "{inner}");
        assert_eq!(inner.workspace_name(), "A");
        let outer =
            check_new_workspace_root(Path::new("/v/clients"), WorkspaceKind::Work, &existing)
                .expect_err("outer refused");
        assert!(matches!(outer, FolderRefusal::OverlappingWork { .. }));
    }

    #[test]
    fn a_folder_joins_one_workspace_only() {
        let existing = [
            placed(
                "/v/clients/a",
                "A",
                WorkspaceKind::Work,
                &["/elsewhere/specs"],
            ),
            placed("/v/life", "Home", WorkspaceKind::Life, &[]),
        ];
        assert!(check_folder_for_workspace(Path::new("/free"), "ws-A", &existing).is_ok());
        assert!(matches!(
            check_folder_for_workspace(Path::new("/v/clients/a/docs"), "ws-A", &existing),
            Err(FolderRefusal::AlreadyInWorkspace { .. })
        ));
        assert!(matches!(
            check_folder_for_workspace(Path::new("/elsewhere/specs/old"), "ws-Home", &existing),
            Err(FolderRefusal::BelongsTo { .. })
        ));
        assert!(matches!(
            check_folder_for_workspace(Path::new("/v"), "ws-Home", &existing),
            Err(FolderRefusal::HoldsWorkspace { .. })
        ));
    }
}
