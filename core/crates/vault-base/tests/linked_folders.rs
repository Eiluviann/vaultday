//! The one-root folder model: a workspace is its root, and folders join it
//! as symlinks directly in the root. Every case builds its folders in a temp
//! dir, since symlinks in a committed fixture do not survive every checkout.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use vault_base::{
    FileIndex, FolderRefusal, IndexScope, OpError, PlacedWorkspace, PlainFileAccess, VaultWatcher,
    Workspace, WorkspaceKind, WorkspaceSet, WorkspaceSettings, check_folder_for_workspace,
    check_new_workspace_root, linked_folders, resolve_workspace,
};

/// A temp dir with a life workspace `Home` and a folder `Elsewhere/Shared`
/// outside it, linked into the root as `Home/Shared`.
struct Layout {
    _directory: tempfile::TempDir,
    base: PathBuf,
    home: PathBuf,
    shared: PathBuf,
}

fn layout() -> Layout {
    let directory = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(directory.path()).unwrap();
    let home = base.join("Home");
    make_workspace(&home, "Home", WorkspaceKind::Life);
    write(&home, "Notes/Groceries.md", "# Groceries\n- [ ] milk\n");
    let shared = base.join("Elsewhere/Shared");
    write(&shared, "Recipe.md", "# Recipe\n- [ ] bake\n");
    symlink(&shared, home.join("Shared")).unwrap();
    Layout {
        _directory: directory,
        base,
        home,
        shared,
    }
}

fn make_workspace(root: &Path, name: &str, kind: WorkspaceKind) {
    let settings = WorkspaceSettings::new(name, kind).to_json().unwrap();
    write(root, ".vaultday/settings.json", &settings);
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn indexed_files(root: &Path) -> Vec<PathBuf> {
    let mut index = FileIndex::new(root, IndexScope::default()).unwrap();
    let _ = index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    index.files().map(|(path, _)| path.to_path_buf()).collect()
}

fn placed(workspace: &Workspace, id: &str) -> PlacedWorkspace {
    PlacedWorkspace {
        id: id.to_owned(),
        name: workspace.settings.name.clone(),
        kind: workspace.settings.kind,
        root: workspace.root.clone(),
        linked_targets: workspace
            .linked_folders
            .iter()
            .map(|linked| linked.target.clone())
            .collect(),
    }
}

#[test]
fn a_linked_folder_is_indexed_under_the_link_and_resolves_to_the_root() {
    let layout = layout();
    let home = Workspace::open(&layout.home, &PlainFileAccess).unwrap();
    assert_eq!(home.linked_folders.len(), 1);
    assert_eq!(home.linked_folders[0].name, "Shared");
    assert_eq!(home.linked_folders[0].link, layout.home.join("Shared"));
    assert_eq!(home.linked_folders[0].target, layout.shared);

    assert_eq!(
        indexed_files(&layout.home),
        vec![
            PathBuf::from("Notes/Groceries.md"),
            PathBuf::from("Shared/Recipe.md")
        ],
        "the linked note keeps its path through the link, once"
    );

    let set = WorkspaceSet::from_workspaces(vec![home]).unwrap();
    let through_link = layout.home.join("Shared/Recipe.md");
    let real = layout.shared.join("Recipe.md");
    assert_eq!(set.resolve(&through_link).unwrap().settings.name, "Home");
    assert_eq!(
        set.resolve(&real).unwrap().settings.name,
        "Home",
        "the target's real path belongs to the workspace that links it"
    );
    assert!(set.resolve(&layout.base.join("Elsewhere")).is_none());
    assert_eq!(
        resolve_workspace(&through_link, &PlainFileAccess)
            .unwrap()
            .settings
            .name,
        "Home"
    );
}

#[test]
fn links_into_the_root_cycles_duplicates_and_deeper_links_are_not_followed() {
    let layout = layout();
    // A link to a folder already inside the root would index it twice.
    symlink(layout.home.join("Notes"), layout.home.join("Notes again")).unwrap();
    // A link to the root's parent would loop.
    symlink(&layout.base, layout.home.join("Up")).unwrap();
    // A second link to the same target.
    symlink(&layout.shared, layout.home.join("Shared twice")).unwrap();
    // A link below the top level.
    let deep = layout.base.join("Deep");
    write(&deep, "Hidden.md", "# Hidden\n");
    symlink(&deep, layout.home.join("Notes/Deep")).unwrap();
    // A link to a file.
    symlink(
        layout.shared.join("Recipe.md"),
        layout.home.join("Recipe.md"),
    )
    .unwrap();

    let names: Vec<String> = linked_folders(&layout.home, &PlainFileAccess)
        .unwrap()
        .into_iter()
        .map(|linked| linked.name)
        .collect();
    assert_eq!(names, ["Shared"]);
    assert_eq!(
        indexed_files(&layout.home),
        vec![
            PathBuf::from("Notes/Groceries.md"),
            PathBuf::from("Shared/Recipe.md")
        ]
    );
}

#[test]
fn a_link_to_another_workspaces_folder_is_not_followed() {
    let layout = layout();
    let acme = layout.base.join("Acme");
    make_workspace(&acme, "Acme", WorkspaceKind::Work);
    write(&acme, "Projects/Portal.md", "# Portal\n");
    symlink(acme.join("Projects"), layout.home.join("Acme projects")).unwrap();
    symlink(&acme, layout.home.join("Acme")).unwrap();

    let names: Vec<String> = linked_folders(&layout.home, &PlainFileAccess)
        .unwrap()
        .into_iter()
        .map(|linked| linked.name)
        .collect();
    assert_eq!(names, ["Shared"]);
    assert!(
        !indexed_files(&layout.home)
            .iter()
            .any(|path| path.ends_with("Portal.md")),
        "a client's notes never reach another workspace's index"
    );
}

#[test]
fn a_change_event_on_a_link_that_is_not_followed_is_ignored() {
    let layout = layout();
    let mut index = FileIndex::new(&layout.home, IndexScope::default()).unwrap();
    let _ = index.refresh(&PlainFileAccess, SystemTime::now()).unwrap();
    let deep = layout.base.join("Deep");
    write(&deep, "Hidden.md", "# Hidden\n");
    symlink(&deep, layout.home.join("Notes/Deep")).unwrap();

    let delta = index
        .apply_changes(
            &PlainFileAccess,
            &[PathBuf::from("Notes/Deep")],
            SystemTime::now(),
        )
        .unwrap();
    assert!(delta.added.is_empty(), "{delta:?}");

    symlink(&deep, layout.home.join("Deep")).unwrap();
    let delta = index
        .apply_changes(
            &PlainFileAccess,
            &[PathBuf::from("Deep")],
            SystemTime::now(),
        )
        .unwrap();
    assert_eq!(delta.added, vec![PathBuf::from("Deep/Hidden.md")]);
}

#[test]
fn the_watcher_reports_a_change_in_a_linked_folder_under_the_link() {
    let layout = layout();
    let watcher =
        VaultWatcher::start(&PlainFileAccess, &layout.home, Duration::from_millis(150)).unwrap();
    write(&layout.shared, "Fresh.md", "# Fresh\n");

    let expected = PathBuf::from("Shared/Fresh.md");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen = BTreeSet::new();
    while Instant::now() < deadline && !seen.contains(&expected) {
        if let Some(batch) = watcher.recv_timeout(Duration::from_millis(200)) {
            seen.extend(batch.changed);
        }
    }
    assert!(seen.contains(&expected), "{seen:?}");
    assert!(
        !seen.iter().any(|path| path.is_absolute()),
        "no event leaks under the target's real path: {seen:?}"
    );
}

#[test]
fn a_folder_offered_to_a_workspace_may_not_belong_to_or_hold_another() {
    let layout = layout();
    let acme_root = layout.base.join("Acme");
    make_workspace(&acme_root, "Acme", WorkspaceKind::Work);
    let home = Workspace::open(&layout.home, &PlainFileAccess).unwrap();
    let acme = Workspace::open(&acme_root, &PlainFileAccess).unwrap();
    let existing = [placed(&home, "ws-home"), placed(&acme, "ws-acme")];
    let free = layout.base.join("Free");
    fs::create_dir_all(&free).unwrap();

    assert_eq!(
        check_folder_for_workspace(&free, "ws-acme", &existing),
        Ok(())
    );
    assert_eq!(
        check_folder_for_workspace(&acme_root.join("Projects"), "ws-home", &existing),
        Err(FolderRefusal::BelongsTo {
            folder: acme_root.join("Projects"),
            workspace: "Acme".to_owned(),
        })
    );
    assert_eq!(
        check_folder_for_workspace(&layout.shared.join("Sub"), "ws-acme", &existing),
        Err(FolderRefusal::BelongsTo {
            folder: layout.shared.join("Sub"),
            workspace: "Home".to_owned(),
        }),
        "a folder inside another workspace's linked folder belongs to it"
    );
    assert_eq!(
        check_folder_for_workspace(&layout.shared, "ws-home", &existing),
        Err(FolderRefusal::AlreadyInWorkspace {
            folder: layout.shared.clone(),
            workspace: "Home".to_owned(),
        })
    );
    assert_eq!(
        check_folder_for_workspace(&layout.home.join("Notes"), "ws-home", &existing),
        Err(FolderRefusal::AlreadyInWorkspace {
            folder: layout.home.join("Notes"),
            workspace: "Home".to_owned(),
        })
    );
    assert_eq!(
        check_folder_for_workspace(&layout.base.join("Elsewhere"), "ws-acme", &existing),
        Err(FolderRefusal::HoldsWorkspace {
            folder: layout.base.join("Elsewhere"),
            workspace: "Home".to_owned(),
        }),
        "a folder holding a linked folder would index it twice"
    );
    assert!(matches!(
        check_folder_for_workspace(&layout.base, "ws-home", &existing),
        Err(FolderRefusal::HoldsWorkspace { .. })
    ));
    let refusal = check_folder_for_workspace(&acme_root, "ws-home", &existing).unwrap_err();
    assert_eq!(
        refusal.to_string(),
        format!("{} already belongs to Acme", acme_root.display())
    );
    assert_eq!(refusal.workspace_name(), "Acme");
}

#[test]
fn a_new_root_keeps_the_client_rule_and_may_not_take_a_linked_folder() {
    let layout = layout();
    let home = Workspace::open(&layout.home, &PlainFileAccess).unwrap();
    let existing = [placed(&home, "ws-home")];

    assert_eq!(
        check_new_workspace_root(&layout.home, WorkspaceKind::Work, &existing),
        Err(FolderRefusal::AlreadyAWorkspace {
            folder: layout.home.clone(),
            workspace: "Home".to_owned(),
        })
    );
    assert_eq!(
        check_new_workspace_root(
            &layout.home.join("Coaching"),
            WorkspaceKind::Life,
            &existing
        ),
        Ok(()),
        "roots may still nest; the nearest .vaultday wins"
    );
    assert!(matches!(
        check_new_workspace_root(&layout.shared, WorkspaceKind::Life, &existing),
        Err(FolderRefusal::BelongsTo { .. })
    ));
    assert!(matches!(
        check_new_workspace_root(
            &layout.base.join("Elsewhere"),
            WorkspaceKind::Life,
            &existing
        ),
        Err(FolderRefusal::HoldsWorkspace { .. })
    ));

    let clients_root = layout.base.join("Clients");
    make_workspace(&clients_root, "Clients", WorkspaceKind::Work);
    let clients = Workspace::open(&clients_root, &PlainFileAccess).unwrap();
    let existing = [placed(&home, "ws-home"), placed(&clients, "ws-clients")];
    assert!(matches!(
        check_new_workspace_root(&clients_root.join("Acme"), WorkspaceKind::Work, &existing),
        Err(FolderRefusal::OverlappingWork { .. })
    ));
    assert_eq!(
        check_new_workspace_root(&clients_root.join("Acme"), WorkspaceKind::Life, &existing),
        Ok(())
    );
}

#[test]
fn an_inactive_workspace_is_refused_by_the_cli_resolution_and_keeps_its_files() {
    let layout = layout();
    let coaching = layout.home.join("Coaching");
    let mut settings = WorkspaceSettings::new("Coaching", WorkspaceKind::Life);
    settings.active = false;
    write(
        &coaching,
        ".vaultday/settings.json",
        &settings.to_json().unwrap(),
    );
    write(&coaching, "Plan.md", "# Plan\n");

    let text = fs::read_to_string(coaching.join(".vaultday/settings.json")).unwrap();
    assert!(text.contains("\"active\": false"), "{text}");
    let home_text = fs::read_to_string(layout.home.join(".vaultday/settings.json")).unwrap();
    assert!(
        !home_text.contains("active"),
        "an active workspace's manifest does not spell the default"
    );

    let error = resolve_workspace(&coaching.join("Plan.md"), &PlainFileAccess).unwrap_err();
    assert!(matches!(error, OpError::InactiveWorkspace(ref name) if name == "Coaching"));
    assert_eq!(error.code(), "inactive-workspace");
    assert_eq!(
        error.to_string(),
        "Coaching is inactive; reactivate it in Vaultday"
    );
    assert!(
        !indexed_files(&layout.home)
            .iter()
            .any(|path| path.starts_with("Coaching")),
        "the outer workspace does not take the inactive one's notes"
    );
    assert!(
        !Workspace::open(&coaching, &PlainFileAccess)
            .unwrap()
            .settings
            .active
    );
}

#[test]
fn a_linked_folder_that_cannot_be_read_is_skipped_not_fatal() {
    use std::os::unix::fs::PermissionsExt;
    let layout = layout();
    let locked = layout.base.join("Locked");
    write(&locked, "Secret.md", "# Secret\n");
    symlink(&locked, layout.home.join("Locked")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let opened = Workspace::open(&layout.home, &PlainFileAccess);
    let files = {
        let mut index = FileIndex::new(&layout.home, IndexScope::default()).unwrap();
        index.refresh(&PlainFileAccess, SystemTime::now()).map(|_| {
            index
                .files()
                .map(|(path, _)| path.to_path_buf())
                .collect::<Vec<_>>()
        })
    };
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(opened.is_ok(), "{opened:?}");
    assert_eq!(
        files.expect("the rest of the workspace still indexes"),
        vec![
            PathBuf::from("Notes/Groceries.md"),
            PathBuf::from("Shared/Recipe.md")
        ]
    );
}
