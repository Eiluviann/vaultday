//! The settings form of the onboarding Configure sheet, creating a workspace
//! in a folder, discovering workspaces for a restore, the folders linked into
//! a root, the active flag and the folder placement rule. Records are plain; the Swift models copy them out so views never see
//! them. Every write goes through `vault_base::workspace_form`, which keeps
//! the keys this build does not know.

use std::path::{Path, PathBuf};

use app_state::settings_guard::secret_like_fields;
use domain::ticket::TicketPattern;
use vault_base::{
    FieldIssue, FolderRefusal, PlacedWorkspace, PlainFileAccess, WorkspaceError,
    WorkspaceFormError, WorkspaceKind, WorkspaceSet, WorkspaceSettings, check_folder_for_workspace,
    check_new_workspace_root, create_settings_file, find_workspace_root, form_issues,
    linked_folders, new_workspace_settings, read_note_prefill, update_settings_file,
    workspace::is_workspace_root,
};

use crate::CoreError;

/// Whether a workspace holds client work or the user's own life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WorkspaceKindView {
    /// A client workspace.
    Work,
    /// A personal workspace.
    Life,
}

/// The fields of `.vaultday/settings.json` the Configure sheet edits.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WorkspaceFormView {
    /// Display name.
    pub name: String,
    /// Work or life.
    pub kind: WorkspaceKindView,
    /// Accent color token.
    pub accent: Option<String>,
    /// Ticket number pattern, a regular expression.
    pub ticket_pattern: Option<String>,
    /// Ticket link template with `{ticket}`.
    pub ticket_url: Option<String>,
    /// Hosts the workspace's links point at.
    pub hosts: Vec<String>,
    /// Browser for links; `None` is the system default.
    pub browser: Option<String>,
    /// Working hours a day.
    pub hours_per_day: f64,
    /// Log folder inside the workspace.
    pub log_folder: String,
    /// Story template inside the workspace.
    pub story_template: Option<String>,
}

/// One thing wrong with one field.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FieldIssueView {
    /// The `settings.json` key.
    pub field: String,
    /// What to tell the user.
    pub message: String,
}

/// A workspace found in a folder.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkspaceBriefView {
    /// The workspace folder.
    pub root: String,
    /// Its name.
    pub name: String,
    /// Work or life.
    pub kind: WorkspaceKindView,
}

/// Where a workspace lives, for the placement rule.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlacedWorkspaceView {
    /// The workspace id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Work or life.
    pub kind: WorkspaceKindView,
    /// The folder holding `.vaultday/`.
    pub root: String,
    /// The real folders its links point at.
    pub linked_targets: Vec<String>,
}

/// Why a folder may not be placed where the user asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FolderRefusalKindView {
    /// It already is a workspace's root.
    AlreadyAWorkspace,
    /// It is already part of the workspace it was offered to.
    AlreadyInWorkspace,
    /// It lies inside another workspace.
    BelongsTo,
    /// It holds another workspace's files.
    HoldsWorkspace,
    /// Two client workspaces would nest.
    OverlappingWork,
}

/// A refused folder, with the workspace it clashes with for the popup.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderRefusalView {
    /// What kind of clash.
    pub kind: FolderRefusalKindView,
    /// The workspace the folder clashes with.
    pub workspace_name: String,
    /// The whole reason, in words for the user.
    pub message: String,
}

/// A folder linked into a workspace root.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LinkedFolderView {
    /// The link's name in the root.
    pub name: String,
    /// The link, `<root>/<name>`.
    pub link: String,
    /// The real folder it points at.
    pub target: String,
}

/// Every problem with the form's values, one message per field, in field order.
#[uniffi::export]
pub fn workspace_form_issues(form: WorkspaceFormView) -> Vec<FieldIssueView> {
    all_issues(&form)
}

/// Reads the folder's manifest as the form shows it.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] without a manifest, [`CoreError::InvalidSettings`]
/// when the manifest is refused.
#[uniffi::export]
pub fn read_workspace_form(root: String) -> Result<WorkspaceFormView, CoreError> {
    let workspace =
        vault_base::Workspace::open(Path::new(&root), &PlainFileAccess).map_err(workspace_error)?;
    Ok(form_view(&workspace.settings))
}

/// Makes the folder a workspace by writing `.vaultday/settings.json`. When the
/// folder holds a `Workspace.md` its values prefill the manifest, once: the
/// note is read here and never again. A blank `name` takes the note's name,
/// else the folder's; a note value the form would refuse is left out.
///
/// # Errors
/// [`CoreError::InvalidSettings`] with the field messages, [`CoreError::Conflict`]
/// when the folder already is a workspace, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn create_workspace(
    root: String,
    name: String,
    kind: WorkspaceKindView,
) -> Result<WorkspaceFormView, CoreError> {
    let root = PathBuf::from(root);
    refuse_existing_workspace(&root)?;
    let settings = prefilled_settings(&root, &name, kind)?;
    create_settings_file(&root, &settings, &PlainFileAccess).map_err(form_error)?;
    Ok(form_view(&settings))
}

/// The manifest [`create_workspace`] writes: the note's values once, the
/// name from the argument, the note or the folder, and nothing the form or
/// the secret guard would refuse.
fn prefilled_settings(
    root: &Path,
    name: &str,
    kind: WorkspaceKindView,
) -> Result<WorkspaceSettings, CoreError> {
    let prefill = read_note_prefill(root, &PlainFileAccess)
        .map_err(|error| CoreError::FileAccess(error.to_string()))?;
    let mut settings = new_workspace_settings(name, core_kind(kind), prefill.as_ref());
    if secret_issues(&settings)
        .iter()
        .any(|issue| issue.field == "name")
    {
        settings.name.clear();
    }
    if settings.name.is_empty() {
        settings.name = root
            .file_name()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    if pattern_issue(&settings).is_some() {
        settings.ticket_pattern = None;
    }
    leave_out_secret_shaped_values(&mut settings);
    Ok(settings)
}

/// The form a new workspace in `root` starts from, as [`create_workspace`]
/// would write it, without writing anything: Configure shows it as a draft
/// and only Done creates the workspace.
///
/// # Errors
/// [`CoreError::Conflict`] when the folder already is a workspace,
/// [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn new_workspace_form(
    root: String,
    kind: WorkspaceKindView,
) -> Result<WorkspaceFormView, CoreError> {
    let root = PathBuf::from(root);
    refuse_existing_workspace(&root)?;
    Ok(form_view(&prefilled_settings(&root, "", kind)?))
}

/// Makes the folder a workspace with the form's values, the Done of a
/// draft. Keys the form does not show take their defaults.
///
/// # Errors
/// [`CoreError::InvalidSettings`] with the field messages, [`CoreError::Conflict`]
/// when the folder already is a workspace, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn create_workspace_from_form(
    root: String,
    form: WorkspaceFormView,
) -> Result<WorkspaceFormView, CoreError> {
    let root = PathBuf::from(root);
    refuse_existing_workspace(&root)?;
    let issues = all_issues(&form);
    if !issues.is_empty() {
        return Err(CoreError::InvalidSettings(join_messages(&issues)));
    }
    let mut settings = settings_from_form(&form);
    settings.name = settings.name.trim().to_owned();
    create_settings_file(&root, &settings, &PlainFileAccess).map_err(form_error)?;
    Ok(form_view(&settings))
}

/// Whether the workspace in `root` is active; a deactivated one stays in the
/// app but leaves every surface and the CLI.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] without a manifest, [`CoreError::InvalidSettings`]
/// when the manifest is refused.
#[uniffi::export]
pub fn workspace_is_active(root: String) -> Result<bool, CoreError> {
    let workspace =
        vault_base::Workspace::open(Path::new(&root), &PlainFileAccess).map_err(workspace_error)?;
    Ok(workspace.settings.active)
}

/// Deactivates or reactivates the workspace in `root` by writing `active` in
/// its manifest; every other key stays as it was.
///
/// # Errors
/// [`CoreError::NotAWorkspace`] without a manifest, [`CoreError::InvalidSettings`]
/// when the manifest is refused, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn set_workspace_active(root: String, is_active: bool) -> Result<(), CoreError> {
    update_settings_file(Path::new(&root), &PlainFileAccess, |settings| {
        settings.active = is_active;
    })
    .map_err(form_error)?;
    Ok(())
}

/// The folders linked into the workspace root: its top-level symlinks to
/// folders that belong to no workspace yet, by name.
///
/// # Errors
/// [`CoreError::FileAccess`] when the root cannot be read.
#[uniffi::export]
pub fn workspace_linked_folders(root: String) -> Result<Vec<LinkedFolderView>, CoreError> {
    let linked = linked_folders(Path::new(&root), &PlainFileAccess).map_err(workspace_error)?;
    Ok(linked
        .into_iter()
        .map(|folder| LinkedFolderView {
            name: folder.name,
            link: folder.link.to_string_lossy().into_owned(),
            target: folder.target.to_string_lossy().into_owned(),
        })
        .collect())
}

/// Writes the form into the folder's manifest. Keys this build does not know
/// are kept; a form with problems writes nothing.
///
/// # Errors
/// [`CoreError::InvalidSettings`] with the field messages, [`CoreError::NotAWorkspace`]
/// without a manifest, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn update_workspace_form(
    root: String,
    form: WorkspaceFormView,
) -> Result<WorkspaceFormView, CoreError> {
    let issues = all_issues(&form);
    if !issues.is_empty() {
        return Err(CoreError::InvalidSettings(join_messages(&issues)));
    }
    let settings = update_settings_file(Path::new(&root), &PlainFileAccess, |settings| {
        settings.name = form.name.trim().to_owned();
        settings.kind = core_kind(form.kind);
        settings.accent = form.accent;
        settings.ticket_pattern = form.ticket_pattern;
        settings.ticket_url = form.ticket_url;
        settings.hosts = form.hosts;
        settings.browser = form.browser;
        settings.hours_per_day = form.hours_per_day;
        settings.log_folder = form.log_folder;
        settings.story_template = form.story_template;
    })
    .map_err(form_error)?;
    Ok(form_view(&settings))
}

/// Whether the folder holds `.vaultday/settings.json`.
///
/// # Errors
/// [`CoreError::FileAccess`] on file errors other than "not found".
#[uniffi::export]
pub fn is_workspace_folder(path: String) -> Result<bool, CoreError> {
    is_workspace_root(Path::new(&path), &PlainFileAccess)
        .map_err(|error| CoreError::FileAccess(error.to_string()))
}

/// Every workspace at or below the given folders, for a restore.
///
/// # Errors
/// [`CoreError::InvalidSettings`] when a manifest is refused, [`CoreError::Conflict`]
/// when two client workspaces nest, [`CoreError::FileAccess`] otherwise.
#[uniffi::export]
pub fn discover_workspaces(folders: Vec<String>) -> Result<Vec<WorkspaceBriefView>, CoreError> {
    let sources: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    let set = WorkspaceSet::discover(&sources, &PlainFileAccess).map_err(workspace_error)?;
    Ok(set
        .iter()
        .map(|workspace| WorkspaceBriefView {
            root: workspace.root.to_string_lossy().into_owned(),
            name: workspace.settings.name.clone(),
            kind: kind_view(workspace.settings.kind),
        })
        .collect())
}

/// Whether `candidate` may become the root of a new workspace of `kind`;
/// `None` when it may, else the refusal with the workspace it clashes with.
#[uniffi::export]
pub fn check_workspace_root_placement(
    candidate: String,
    kind: WorkspaceKindView,
    workspaces: Vec<PlacedWorkspaceView>,
) -> Option<FolderRefusalView> {
    let placed = placed_workspaces(workspaces);
    check_new_workspace_root(Path::new(&candidate), core_kind(kind), &placed)
        .err()
        .map(|refusal| refusal_view(&refusal))
}

/// Whether `candidate` may join the workspace `workspace_id`, linked or moved
/// into its root; `None` when it may. A folder inside a workspace the app
/// does not know yet (a `.vaultday/` at or above it) is refused too, except
/// the root in `leaving_root`: a folder moving out of a root to another
/// workspace, whose caller leaves that workspace's claim on it out of
/// `workspaces`.
///
/// # Errors
/// [`CoreError::FileAccess`] when the folders above it cannot be read,
/// [`CoreError::InvalidSettings`] when such a workspace's manifest is refused.
#[uniffi::export]
pub fn check_folder_placement(
    candidate: String,
    workspace_id: String,
    workspaces: Vec<PlacedWorkspaceView>,
    leaving_root: Option<String>,
) -> Result<Option<FolderRefusalView>, CoreError> {
    let candidate = PathBuf::from(candidate);
    let placed = placed_workspaces(workspaces);
    if let Err(refusal) = check_folder_for_workspace(&candidate, &workspace_id, &placed) {
        return Ok(Some(refusal_view(&refusal)));
    }
    let Some(root) = find_workspace_root(&candidate, &PlainFileAccess).map_err(workspace_error)?
    else {
        return Ok(None);
    };
    let is_leaving =
        leaving_root.is_some_and(|leaving| vault_base::normalize_path(Path::new(&leaving)) == root);
    if is_leaving {
        return Ok(None);
    }
    let workspace =
        vault_base::Workspace::open(&root, &PlainFileAccess).map_err(workspace_error)?;
    Ok(Some(refusal_view(&FolderRefusal::BelongsTo {
        folder: vault_base::normalize_path(&candidate),
        workspace: workspace.settings.name,
    })))
}

fn placed_workspaces(workspaces: Vec<PlacedWorkspaceView>) -> Vec<PlacedWorkspace> {
    workspaces
        .into_iter()
        .map(|workspace| PlacedWorkspace {
            id: workspace.id,
            name: workspace.name,
            kind: core_kind(workspace.kind),
            root: PathBuf::from(workspace.root),
            linked_targets: workspace
                .linked_targets
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        })
        .collect()
}

fn refusal_view(refusal: &FolderRefusal) -> FolderRefusalView {
    FolderRefusalView {
        kind: match refusal {
            FolderRefusal::AlreadyAWorkspace { .. } => FolderRefusalKindView::AlreadyAWorkspace,
            FolderRefusal::AlreadyInWorkspace { .. } => FolderRefusalKindView::AlreadyInWorkspace,
            FolderRefusal::BelongsTo { .. } => FolderRefusalKindView::BelongsTo,
            FolderRefusal::HoldsWorkspace { .. } => FolderRefusalKindView::HoldsWorkspace,
            FolderRefusal::OverlappingWork { .. } => FolderRefusalKindView::OverlappingWork,
        },
        workspace_name: refusal.workspace_name().to_owned(),
        message: refusal.to_string(),
    }
}

fn refuse_existing_workspace(root: &Path) -> Result<(), CoreError> {
    if is_workspace_root(root, &PlainFileAccess)
        .map_err(|error| CoreError::FileAccess(error.to_string()))?
    {
        return Err(CoreError::Conflict(format!(
            "{} already holds .vaultday/settings.json",
            root.display()
        )));
    }
    Ok(())
}

fn all_issues(form: &WorkspaceFormView) -> Vec<FieldIssueView> {
    let settings = settings_from_form(form);
    let mut issues = form_issues(&settings);
    issues.extend(pattern_issue(&settings));
    issues.extend(secret_issues(&settings));
    issues.sort_by_key(|issue| field_rank(issue.field));
    issues.into_iter().map(issue_view).collect()
}

fn settings_from_form(form: &WorkspaceFormView) -> WorkspaceSettings {
    let mut settings = WorkspaceSettings::new(form.name.clone(), core_kind(form.kind));
    settings.accent.clone_from(&form.accent);
    settings.ticket_pattern.clone_from(&form.ticket_pattern);
    settings.ticket_url.clone_from(&form.ticket_url);
    settings.hosts.clone_from(&form.hosts);
    settings.browser.clone_from(&form.browser);
    settings.hours_per_day = form.hours_per_day;
    settings.log_folder.clone_from(&form.log_folder);
    settings.story_template.clone_from(&form.story_template);
    settings
}

fn pattern_issue(settings: &WorkspaceSettings) -> Option<FieldIssue> {
    let pattern = settings.ticket_pattern.as_deref()?;
    TicketPattern::new(pattern).err().map(|error| FieldIssue {
        field: "ticket_pattern",
        message: format!("The ticket pattern is not a valid regular expression: {error}"),
    })
}

/// One issue per field whose value has the form of a secret, as the
/// `settings_guard` reads the file these settings would write.
/// `settings.json` syncs as plain text with the vault, so a key pasted into
/// the wrong field must not land there.
fn secret_issues(settings: &WorkspaceSettings) -> Vec<FieldIssue> {
    let Ok(value) = serde_json::to_value(settings) else {
        return Vec::new();
    };
    let mut fields: Vec<&'static str> = secret_like_fields(&value)
        .iter()
        .filter_map(|path| {
            let top = path.split(['.', '[']).next()?;
            FIELD_ORDER.iter().copied().find(|field| *field == top)
        })
        .collect();
    fields.dedup();
    fields
        .into_iter()
        .map(|field| FieldIssue {
            field,
            message: format!(
                "The {} looks like a key or a password; settings.json syncs as plain text, so secrets stay out of it",
                field.replace('_', " ")
            ),
        })
        .collect()
}

/// Clears what a note offered in a secret's form, as a refused note value is
/// left out, so the folder still becomes a workspace.
fn leave_out_secret_shaped_values(settings: &mut WorkspaceSettings) {
    for issue in secret_issues(settings) {
        match issue.field {
            "accent" => settings.accent = None,
            "ticket_pattern" => settings.ticket_pattern = None,
            "ticket_url" => settings.ticket_url = None,
            "hosts" => settings.hosts.clear(),
            "browser" => settings.browser = None,
            "log_folder" => {
                settings.log_folder =
                    WorkspaceSettings::new(String::new(), settings.kind).log_folder;
            }
            "story_template" => settings.story_template = None,
            _ => {}
        }
    }
}

const FIELD_ORDER: [&str; 9] = [
    "name",
    "accent",
    "ticket_pattern",
    "ticket_url",
    "hosts",
    "browser",
    "hours_per_day",
    "log_folder",
    "story_template",
];

fn field_rank(field: &str) -> usize {
    FIELD_ORDER
        .iter()
        .position(|known| *known == field)
        .unwrap_or(FIELD_ORDER.len())
}

fn issue_view(issue: FieldIssue) -> FieldIssueView {
    FieldIssueView {
        field: issue.field.to_owned(),
        message: issue.message,
    }
}

fn join_messages(issues: &[FieldIssueView]) -> String {
    issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

fn form_view(settings: &WorkspaceSettings) -> WorkspaceFormView {
    WorkspaceFormView {
        name: settings.name.clone(),
        kind: kind_view(settings.kind),
        accent: settings.accent.clone(),
        ticket_pattern: settings.ticket_pattern.clone(),
        ticket_url: settings.ticket_url.clone(),
        hosts: settings.hosts.clone(),
        browser: settings.browser.clone(),
        hours_per_day: settings.hours_per_day,
        log_folder: settings.log_folder.clone(),
        story_template: settings.story_template.clone(),
    }
}

fn kind_view(kind: WorkspaceKind) -> WorkspaceKindView {
    match kind {
        WorkspaceKind::Work => WorkspaceKindView::Work,
        WorkspaceKind::Life => WorkspaceKindView::Life,
    }
}

fn core_kind(kind: WorkspaceKindView) -> WorkspaceKind {
    match kind {
        WorkspaceKindView::Work => WorkspaceKind::Work,
        WorkspaceKindView::Life => WorkspaceKind::Life,
    }
}

fn form_error(error: WorkspaceFormError) -> CoreError {
    let message = error.to_string();
    match error {
        WorkspaceFormError::Invalid(_) | WorkspaceFormError::Settings(_) => {
            CoreError::InvalidSettings(message)
        }
        WorkspaceFormError::AlreadyAWorkspace(_) => CoreError::Conflict(message),
        WorkspaceFormError::NotAWorkspace(_) => CoreError::NotAWorkspace(message),
        WorkspaceFormError::FileAccess(_) => CoreError::FileAccess(message),
    }
}

fn workspace_error(error: WorkspaceError) -> CoreError {
    let message = error.to_string();
    match error {
        WorkspaceError::NotAWorkspace(_) => CoreError::NotAWorkspace(message),
        WorkspaceError::Settings { .. } => CoreError::InvalidSettings(message),
        WorkspaceError::FileAccess(_) => CoreError::FileAccess(message),
        WorkspaceError::OverlappingWorkWorkspaces { .. } => CoreError::Conflict(message),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn form(name: &str) -> WorkspaceFormView {
        WorkspaceFormView {
            name: name.to_owned(),
            kind: WorkspaceKindView::Work,
            accent: None,
            ticket_pattern: None,
            ticket_url: None,
            hosts: Vec::new(),
            browser: None,
            hours_per_day: 8.0,
            log_folder: "Log".to_owned(),
            story_template: None,
        }
    }

    #[test]
    fn a_bad_ticket_pattern_is_reported_in_field_order() {
        let mut bad = form("Studio");
        bad.ticket_pattern = Some("(unclosed".to_owned());
        bad.hours_per_day = 30.0;
        let issues = workspace_form_issues(bad);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert_eq!(fields, ["ticket_pattern", "hours_per_day"]);
        assert!(issues[0].message.contains("regular expression"));
    }

    #[test]
    fn creating_prefills_once_from_the_note_and_the_note_is_never_read_again() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path();
        fs::write(
            root.join("Workspace.md"),
            "---\nhosts:\n  - tracker.example.test\nhours_per_day: 6\n---\n# Prefilled Name\n",
        )
        .expect("note");
        let created = create_workspace(
            root.to_string_lossy().into_owned(),
            String::new(),
            WorkspaceKindView::Work,
        )
        .expect("created");
        assert_eq!(created.name, "Prefilled Name");
        assert_eq!(created.hosts, ["tracker.example.test"]);
        assert_eq!(created.hours_per_day, 6.0);

        // The note changes, then becomes unreadable: nothing reads it again.
        fs::write(root.join("Workspace.md"), "# Changed\n").expect("note");
        let read = read_workspace_form(root.to_string_lossy().into_owned()).expect("read");
        assert_eq!(read.name, "Prefilled Name");
        let again = create_workspace(
            root.to_string_lossy().into_owned(),
            "Other".to_owned(),
            WorkspaceKindView::Work,
        )
        .expect_err("already a workspace");
        assert!(matches!(again, CoreError::Conflict(_)));
        let after = read_workspace_form(root.to_string_lossy().into_owned()).expect("read");
        assert_eq!(after.name, "Prefilled Name");
    }

    #[test]
    fn a_note_the_form_refuses_still_lets_the_folder_become_a_workspace() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().join("Studio notes");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("Workspace.md"),
            "---\nticket_pattern: \"(unclosed\"\nhosts:\n  - https://tracker.example.test\nhours_per_day: 6\n---\nNo heading here.\n",
        )
        .expect("note");
        let created = create_workspace(
            root.to_string_lossy().into_owned(),
            String::new(),
            WorkspaceKindView::Work,
        )
        .expect("created despite the note's bad values");
        assert_eq!(created.name, "Studio notes");
        assert_eq!(created.ticket_pattern, None);
        assert!(created.hosts.is_empty());
        assert_eq!(created.hours_per_day, 6.0);
    }

    #[test]
    fn an_update_keeps_unknown_keys_and_a_refused_update_writes_nothing() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().to_string_lossy().into_owned();
        create_workspace(root.clone(), "Studio".to_owned(), WorkspaceKindView::Work)
            .expect("created");
        let path = folder.path().join(".vaultday/settings.json");
        let seeded = fs::read_to_string(&path)
            .expect("read")
            .replace("\"features\": {}", "\"features\": {}, \"x_future\": 7");
        fs::write(&path, seeded).expect("seed");
        let mut edited = form("Studio 2");
        edited.hosts = vec!["tracker.example.test".to_owned()];
        update_workspace_form(root.clone(), edited).expect("updated");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"x_future\": 7"), "{text}");
        assert!(text.contains("Studio 2"));

        let mut broken = form("Studio 3");
        broken.log_folder = "../out".to_owned();
        let error = update_workspace_form(root, broken).expect_err("refused");
        assert!(
            matches!(error, CoreError::InvalidSettings(message) if message.contains("log folder"))
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn discovery_lists_workspaces_and_the_overlap_check_speaks_plainly() {
        let folder = tempfile::tempdir().expect("temp dir");
        let one = folder.path().join("one");
        let two = folder.path().join("two");
        fs::create_dir_all(&one).expect("one");
        fs::create_dir_all(&two).expect("two");
        create_workspace(
            one.to_string_lossy().into_owned(),
            "One".to_owned(),
            WorkspaceKindView::Work,
        )
        .expect("one");
        create_workspace(
            two.to_string_lossy().into_owned(),
            "Two".to_owned(),
            WorkspaceKindView::Life,
        )
        .expect("two");
        let found =
            discover_workspaces(vec![folder.path().to_string_lossy().into_owned()]).expect("found");
        let names: Vec<&str> = found.iter().map(|brief| brief.name.as_str()).collect();
        assert_eq!(names, ["One", "Two"]);
        assert!(is_workspace_folder(one.to_string_lossy().into_owned()).expect("is"));
        assert!(
            !is_workspace_folder(folder.path().to_string_lossy().into_owned()).expect("is not")
        );

        let placed = vec![PlacedWorkspaceView {
            id: "ws-one".to_owned(),
            name: "One".to_owned(),
            kind: WorkspaceKindView::Work,
            root: one.to_string_lossy().into_owned(),
            linked_targets: Vec::new(),
        }];
        let nested = one.join("docs").to_string_lossy().into_owned();
        let refused =
            check_workspace_root_placement(nested.clone(), WorkspaceKindView::Work, placed.clone())
                .expect("refused");
        assert_eq!(refused.kind, FolderRefusalKindView::OverlappingWork);
        assert_eq!(refused.workspace_name, "One");
        assert!(refused.message.contains("overlap"), "{}", refused.message);
        assert_eq!(
            check_workspace_root_placement(nested.clone(), WorkspaceKindView::Life, placed.clone()),
            None,
            "life nests"
        );
        let belongs = check_folder_placement(nested.clone(), "ws-other".to_owned(), placed, None)
            .expect("checked")
            .expect("refused");
        assert_eq!(
            check_folder_placement(
                nested,
                "ws-other".to_owned(),
                Vec::new(),
                Some(one.to_string_lossy().into_owned())
            )
            .expect("checked"),
            None,
            "a folder leaving its root is not held back by that root"
        );
        assert_eq!(belongs.kind, FolderRefusalKindView::BelongsTo);
        let unknown = check_folder_placement(
            two.join("notes").to_string_lossy().into_owned(),
            "ws-one".to_owned(),
            Vec::new(),
            None,
        )
        .expect("checked")
        .expect("a workspace the app does not know still owns its folder");
        assert_eq!(unknown.workspace_name, "Two");
    }

    #[test]
    fn a_draft_writes_nothing_until_it_is_created() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().join("Studio");
        fs::create_dir_all(&root).expect("root");
        let root_text = root.to_string_lossy().into_owned();
        let mut draft =
            new_workspace_form(root_text.clone(), WorkspaceKindView::Life).expect("draft");
        assert_eq!(draft.name, "Studio");
        assert!(!root.join(".vaultday").exists(), "a draft writes nothing");

        draft.name = "  ".to_owned();
        assert!(matches!(
            create_workspace_from_form(root_text.clone(), draft.clone()),
            Err(CoreError::InvalidSettings(_))
        ));
        assert!(!root.join(".vaultday").exists());
        draft.name = "Studio Life".to_owned();
        draft.accent = Some("green".to_owned());
        let created = create_workspace_from_form(root_text.clone(), draft).expect("created");
        assert_eq!(created.name, "Studio Life");
        assert_eq!(written_settings(&root)["accent"], "green");
        assert!(matches!(
            new_workspace_form(root_text.clone(), WorkspaceKindView::Life),
            Err(CoreError::Conflict(_))
        ));

        assert!(workspace_is_active(root_text.clone()).expect("read"));
        set_workspace_active(root_text.clone(), false).expect("deactivated");
        assert!(!workspace_is_active(root_text.clone()).expect("read"));
        assert_eq!(
            written_settings(&root)["accent"],
            "green",
            "other keys stay"
        );
        set_workspace_active(root_text.clone(), true).expect("reactivated");
        assert!(written_settings(&root).get("active").is_none());
    }

    const SYNTHETIC_KEY: &str = "sk-ant-api03-synthetic-not-a-real-key";

    fn written_settings(root: &Path) -> serde_json::Value {
        let text = fs::read_to_string(root.join(".vaultday/settings.json")).expect("settings.json");
        serde_json::from_str(&text).expect("json")
    }

    #[test]
    fn a_settings_file_the_steps_write_passes_the_secret_guard() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().to_string_lossy().into_owned();
        create_workspace(root.clone(), "Studio".to_owned(), WorkspaceKindView::Work)
            .expect("created");
        let mut every_field = form("Studio");
        every_field.accent = Some("blue".to_owned());
        every_field.ticket_pattern = Some("WEB-\\d+".to_owned());
        every_field.ticket_url = Some("https://tracker.example.test/browse/{ticket}".to_owned());
        every_field.hosts = vec!["tracker.example.test".to_owned()];
        every_field.browser = Some("Safari".to_owned());
        every_field.story_template = Some("Templates/Story.md".to_owned());
        update_workspace_form(root, every_field).expect("updated");

        let settings = written_settings(folder.path());
        assert_eq!(
            app_state::settings_guard::secret_like_fields(&settings),
            Vec::<String>::new()
        );
        let keys = settings.as_object().expect("object");
        for forbidden in vault_base::settings::FORBIDDEN_SETTINGS_KEYS {
            assert!(
                !keys.keys().any(|key| key.eq_ignore_ascii_case(forbidden)),
                "{forbidden} is in settings.json"
            );
        }
    }

    #[test]
    fn a_value_shaped_like_a_secret_is_refused_and_nothing_is_written() {
        let folder = tempfile::tempdir().expect("temp dir");
        let root = folder.path().to_string_lossy().into_owned();
        create_workspace(root.clone(), "Studio".to_owned(), WorkspaceKindView::Work)
            .expect("created");
        let before = written_settings(folder.path());
        let mut pasted = form("Studio");
        pasted.story_template = Some(SYNTHETIC_KEY.to_owned());

        let issues = workspace_form_issues(pasted.clone());
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert_eq!(fields, ["story_template"]);
        let refused = update_workspace_form(root, pasted).expect_err("refused");
        assert!(matches!(refused, CoreError::InvalidSettings(_)));
        assert_eq!(written_settings(folder.path()), before);
    }

    #[test]
    fn a_note_value_shaped_like_a_secret_is_left_out_when_the_folder_becomes_a_workspace() {
        let folder = tempfile::tempdir().expect("temp dir");
        fs::write(
            folder.path().join("Workspace.md"),
            format!("---\nbrowser: {SYNTHETIC_KEY}\nhours_per_day: 6\n---\n# Studio\n"),
        )
        .expect("note");
        let created = create_workspace(
            folder.path().to_string_lossy().into_owned(),
            String::new(),
            WorkspaceKindView::Work,
        )
        .expect("created");
        assert_eq!(created.browser, None);
        assert_eq!(created.hours_per_day, 6.0);
        let text = fs::read_to_string(folder.path().join(".vaultday/settings.json"))
            .expect("settings.json");
        assert!(!text.contains(SYNTHETIC_KEY));
    }
}
