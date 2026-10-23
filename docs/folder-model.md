# One root folder per workspace

Decided by the user on 2026-10-24 (feedback round 1, items 6, 7, 8, 10 and 12). A workspace owns exactly one folder, its **root**: the folder holding `.vaultday/settings.json`. Any other folder joins the workspace on the Folders step in one of two ways, chosen per folder:

- **Link**: Vaultday makes a symlink in the root that points at the folder, which stays where it is.
- **Move**: Vaultday moves the folder into the root.

New folders are made on the Folders step, inside the root. The Workspaces step only creates a workspace (in a new or existing empty folder) or imports a folder that already holds `.vaultday/`.

## Linked folders

A linked folder is a symlink to a directory **directly in the root** (`<root>/<name> -> <target>`). Symlinks deeper in the tree are not followed, as before. A top-level link is skipped, never followed, when its target

- lies inside any workspace root (the root itself included), so a file is never indexed twice,
- is the root or one of its ancestors (a cycle),
- a second link already points at the same target,
- or cannot be read by this process (no grant in the sandbox, a locked folder): one such link never takes the workspace down.

The file system is the record: nothing in `settings.json` lists folders, so the manifest keeps one meaning (this folder is a workspace) and a hand-made link in the root works like one the app made. The core reads the links when it opens a workspace (`Workspace::linked_folders`).

## Workspace resolution in the core

- A workspace is its root. The old model, where a workspace was a set of granted sources and the deepest matching source won, reduces to roots: `WorkspaceSet::resolve(path)` takes the nearest root at or above the path, and failing that the workspace whose linked folder holds it (by the target's real path), so a note reached through a link and the same note reached by its real path both resolve to the root's workspace.
- Roots may still nest (a coaching workspace inside a personal vault): the nearest `.vaultday/` wins and the outer index prunes the inner root, as before. Two client (work) roots still may not nest.
- **Overlap rule**, now one rule for every kind because each file belongs to at most one workspace: a folder offered to a workspace (to link or move) is refused when it lies inside a workspace root or a linked folder, or holds one. A folder offered as a new workspace root is refused when it is already a root, lies inside or holds a linked folder, or nests with another client root while both are work. The refusal names the workspace, which the app shows in a popup (item 7).
- `core/fixtures/two-workspaces` stays as it is: a life root with a work root nested below it is still allowed and still the deepest-root case. The link cases are built in temporary folders by the tests (symlinks in a committed fixture do not survive every checkout and copy helper).
- The CLI resolves by walking up from its real working directory first, so a folder that lies in a workspace always gets that workspace, whatever link led there. Only when the real directory is in no workspace does it try the shell's logical directory (`$PWD`, when it names the same folder), and only through a link that sits directly in a workspace root: `cd <root>/<link>` resolves to the root's workspace. Started in a link's real target without going through the link, it finds no workspace, since the CLI does not read the app document. That case is a known limit.

## Index and watcher

- The index walks the root as before and also descends into each followed link, keeping the logical path (`<link>/<file>`), so reads and writes go through the link. Writes follow the link to the target because the path does.
- Folders holding another `.vaultday/` are pruned inside linked folders too.
- The watcher watches the root and each followed link's target, and reports the target's events under the link path, so the index never sees the real path twice. When the open workspace's links change, the app restarts its watch, so a newly linked folder is watched at once.
- A change event on a symlink that is not a followed link is ignored, which also closes the old gap where `apply_changes` followed links a full refresh skipped.

## Folder grants (sandbox)

- The root has one security-scoped bookmark, as before.
- A **linked** folder whose target lies outside the root needs its own bookmark: the sandbox resolves the link and checks the target's path. The open panel that picks the folder gives the access; the bookmark is stored as a folder connection, a grant and a source of the workspace in the app document, exactly as a "further folder" was. Its meaning narrows to "the grant for a link target". The app keeps every link grant of the open workspace accessed, not only the root's.
- A **moved** folder needs no bookmark: it is inside the root.
- A link is only ever removed after checking it still is a symlink; a real folder that took its place is left alone and reported.
- A link grant whose link is gone (removed by hand, or not carried by a sync service such as iCloud Drive, which does not sync symlinks) shows on the Folders step as "Link missing" with Link again and Remove, never dropped silently.
- iCloud Drive and Obsidian Sync do not carry symlinks: a linked folder is reachable on this Mac only. Move is the choice for a folder the iPhone has to see.

## Migration

A workspace from an earlier build may list further folders that are neither the root nor a link target. On the first launch after the update, Vaultday asks once, in an alert inside the window, whether to **link** those folders (the default; nothing moves) or **move** them into their roots. The answer converts every listed folder and is stored per device (`folders_converted`), so the question never comes back. A further folder that already lies inside its own root just loses its grant; one that lies inside another workspace's root cannot be linked without indexing it twice, so it is left out and named in the result. Before the answer, nothing is changed.

## Adding a workspace writes nothing until Done (item 8)

Add asks for the folder, checks it (placement rule, empty for Create, `.vaultday/` present for Import) and opens Configure on a draft. Only Done writes: `settings.json` for a new workspace (with the sheet's values), the edited values for an imported one, the workspace id file, and the connection, grant and source in the app document. Cancel releases the folder's access and leaves no trace. Opening a workspace no longer adds a source either: registration is the one writer, which closes the path that could list one root under two workspace ids.

## Deactivating a workspace (item 12)

- `settings.json` carries `"active": false` for an inactive workspace (absent means active). The flag lives in the manifest because the CLI reads only the manifest; the app document still holds the workspace's connection, grant and source, and the grant stays.
- Inactive workspaces stay on the setup Workspaces and Folders steps and in Settings, with the badge "Inactive", and leave the switcher, the sidebar, Today, All, Browse, Hours, the palette, quick add and every planning view. The app never opens one at launch.
- The CLI refuses an inactive workspace with "<name> is inactive; reactivate it in Vaultday". It does not fall through to an outer workspace: the inactive workspace still owns its files.
- A running timer on the open workspace is stopped first, and the stop writes the usual day-log line; then the workspace closes and the next active one opens.

## Moving workspaces and folders (item 20)

- **A workspace card dragged to the other sphere** changes the workspace's `kind` in `settings.json`; the app document keeps no kind, so nothing else is written. A move to Work that would nest two client roots is refused with a popup naming the other workspace, as a new root would be. The card's `…` menu has the same move (Move to Work / Move to Life) for the keyboard.
- **A folder row dragged to another workspace** re-homes the folder. A linked folder can be linked into the target root or moved into it, asked on drop; the old link and its grant go either way. A plain folder inside the source root can only move: linked from another root it would belong to two workspaces. A workspace's root is not draggable and has no Move to. The placement rule runs on drop with the folder's current workspace left out, so a folder holding another workspace is still refused. The row's `…` menu has Move to for the keyboard.
- The drop target shows a dashed border only while a drag hovers over it (the design system's `dropIndicator`), never at rest.
