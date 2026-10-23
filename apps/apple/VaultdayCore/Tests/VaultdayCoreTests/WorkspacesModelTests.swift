import Foundation
import XCTest
@testable import VaultdayCore

/// The Workspaces and Folders steps over the mock vault and temporary folders:
/// a workspace is one root folder, and other folders are linked or moved into it.
@MainActor
final class WorkspacesModelTests: XCTestCase {
	private let forbiddenKeys = ["security", "tokens", "rates", "models", "integrations"]
	private let knownKeys: Set<String> = [
		"schema_version", "name", "kind", "accent", "ticket_pattern", "ticket_url", "hosts", "browser",
		"hours_per_day", "log_folder", "story_template", "features",
	]

	// MARK: Adding a workspace

	func testAddingTheMockVaultListsItAndARelaunchRestoresIt() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: vault, sphere: .life))
		XCTAssertEqual(entry.name, "Mock Studio")
		XCTAssertEqual(entry.sphere, .work, "the manifest's own kind wins over the sphere the Add key sat in")
		XCTAssertTrue(entry.id.hasPrefix("ws-"))
		XCTAssertTrue(entry.isActive)
		XCTAssertEqual(entry.folders.map(\.role), [.root])
		XCTAssertEqual(entry.folders.first?.kindLabel, "Obsidian vault")

		let relaunched = WorkspacesModel(session: try harness.relaunched().session)
		XCTAssertEqual(relaunched.entries, workspaces.entries)
		XCTAssertEqual(relaunched.problems, [])
	}

	func testCreatingWritesNothingUntilDoneAndThenTheSheetsValues() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "Freelancing")
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		var changes = 0
		workspaces.onChange = { changes += 1 }

		let draft = try draft(workspaces.beginWorkspace(folder: folder, sphere: .work, choice: .create))
		XCTAssertFalse(draft.isImport)
		XCTAssertEqual(draft.configure.name, "Freelancing")
		XCTAssertFalse(FileManager.default.fileExists(atPath: folder.appending(path: ".vaultday").path), "a draft writes no manifest")
		XCTAssertEqual(workspaces.entries, [], "a draft is not listed")
		XCTAssertEqual(harness.session.appState.allSources(), [], "a draft is not in the app document")
		XCTAssertEqual(changes, 0)

		draft.configure.name = "Freelance work"
		draft.configure.accent = "green"
		XCTAssertTrue(draft.configure.save(), draft.configure.errorMessage ?? "")
		XCTAssertTrue(draft.isCommitted)
		let json = try TestFixtures.settingsJSON(in: folder)
		XCTAssertEqual(json["name"] as? String, "Freelance work")
		XCTAssertEqual(json["accent"] as? String, "green")
		XCTAssertEqual(json["kind"] as? String, "work")
		XCTAssertEqual(json["schema_version"] as? Int, 1)
		XCTAssertTrue(Set(json.keys).isSubset(of: knownKeys.union(["accent"])), "unexpected keys: \(Set(json.keys).subtracting(knownKeys))")
		for key in forbiddenKeys { XCTAssertNil(json[key]) }
		XCTAssertEqual(workspaces.entries.map(\.name), ["Freelance work"])
		XCTAssertEqual(changes, 1)
	}

	func testCancellingADraftLeavesNoTrace() throws {
		let created = try TestFixtures.emptyFolder(self, named: "Fresh")
		let imported = try TestFixtures.copyOfMockVault(self)
		let manifestBefore = try Data(contentsOf: imported.appending(path: ".vaultday/settings.json"))
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)

		let fresh = try draft(workspaces.beginWorkspace(folder: created, sphere: .life, choice: .create))
		fresh.configure.name = "Changed then cancelled"
		workspaces.cancel(fresh)
		let vault = try draft(workspaces.beginWorkspace(folder: imported, sphere: .work, choice: .import))
		vault.configure.name = "Changed then cancelled"
		workspaces.cancel(vault)

		XCTAssertFalse(FileManager.default.fileExists(atPath: created.appending(path: ".vaultday").path))
		XCTAssertEqual(try Data(contentsOf: imported.appending(path: ".vaultday/settings.json")), manifestBefore)
		XCTAssertFalse(FileManager.default.fileExists(atPath: imported.appending(path: ".vaultday/sync/workspace.id").path), "no workspace id is claimed")
		XCTAssertEqual(workspaces.entries, [])
		XCTAssertEqual(harness.session.appState.folderGrants(), [])
		XCTAssertEqual(WorkspacesModel(session: try harness.relaunched().session).entries, [])
	}

	func testTheSameFolderAddedTwiceIsOneWorkspace() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let first = try draft(workspaces.beginWorkspace(folder: vault, sphere: .work, choice: .import))
		workspaces.cancel(first)
		XCTAssertNotNil(workspaces.addWorkspace(folder: vault, sphere: .work))
		let opened = WorkspaceModel(session: harness.session)
		opened.restoreAtLaunch()
		XCTAssertEqual(opened.phase, .open)
		opened.close()
		opened.restoreAtLaunch()

		guard case let .refused(problem) = workspaces.beginWorkspace(folder: vault, sphere: .work, choice: .import) else {
			return XCTFail("the same root twice is refused")
		}
		XCTAssertEqual(problem.workspaceName, "Mock Studio")
		workspaces.reload()
		XCTAssertEqual(workspaces.entries.count, 1)
		XCTAssertEqual(Set(harness.session.appState.allSources().map(\.workspace)).count, 1, "one root, one workspace id")
	}

	func testCreateNeedsAnEmptyFolderAndImportNeedsAVaultdayFolder() throws {
		let notes = try TestFixtures.emptyFolder(self, named: "Notes")
		try Data("# A note\n".utf8).write(to: notes.appending(path: "note.md"))
		let empty = try TestFixtures.emptyFolder(self, named: "Empty")
		try Data().write(to: empty.appending(path: ".DS_Store"))
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)

		guard case let .refused(notEmpty) = workspaces.beginWorkspace(folder: notes, sphere: .life, choice: .create) else {
			return XCTFail("a folder with notes is not a new workspace")
		}
		XCTAssertEqual(notEmpty.title, "Notes is not empty")
		guard case let .refused(noVaultday) = workspaces.beginWorkspace(folder: empty, sphere: .life, choice: .import) else {
			return XCTFail("an import needs a .vaultday folder")
		}
		XCTAssertEqual(noVaultday.title, "Empty holds no workspace")
		XCTAssertFalse(FileManager.default.fileExists(atPath: notes.appending(path: ".vaultday").path))

		let created = try draft(workspaces.beginWorkspace(folder: empty, sphere: .life, choice: .create))
		XCTAssertFalse(created.isImport, "hidden files do not count")
		workspaces.cancel(created)
	}

	func testCreateOnAFolderThatHoldsAWorkspaceImportsIt() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		let draft = try draft(workspaces.beginWorkspace(folder: vault, sphere: .life, choice: .create))
		XCTAssertTrue(draft.isImport)
		XCTAssertEqual(draft.configure.name, "Mock Studio")
		workspaces.cancel(draft)
	}

	func testTheStepsAddKeyTakesTheNotesNameAndSkipsTheValuesTheFormRefuses() throws {
		let folder = try TestFixtures.emptyFolder(self, named: "Studio")
		try Data("---\nhosts:\n  - https://tracker.example.test\nhours_per_day: 6\n---\n# Studio From Note\n".utf8).write(to: folder.appending(path: "Workspace.md"))
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)

		let entry = workspaces.addWorkspace(folder: folder, sphere: .work)
		XCTAssertNil(workspaces.message)
		XCTAssertEqual(entry?.name, "Studio From Note", "a folder holding only Workspace.md counts as empty")
		let json = try TestFixtures.settingsJSON(in: folder)
		XCTAssertNil(json["hosts"], "the URL is left out, not written as a host")
		XCTAssertEqual(json["hours_per_day"] as? Double, 6)
	}

	func testARootTakenOrNestedClientWorkIsRefusedWithAPopupNamingTheWorkspace() throws {
		let outer = try TestFixtures.emptyFolder(self, named: "clients")
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		XCTAssertNotNil(workspaces.addWorkspace(folder: outer, sphere: .work, name: "Clients"))
		let inner = outer.appending(path: "acme", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: inner, withIntermediateDirectories: true)

		guard case let .refused(again) = workspaces.beginWorkspace(folder: outer, sphere: .work, choice: .import) else {
			return XCTFail("the same root twice is refused")
		}
		XCTAssertEqual(again.workspaceName, "Clients")
		XCTAssertEqual(again.title, "clients is already Clients")
		guard case let .refused(nested) = workspaces.beginWorkspace(folder: inner, sphere: .work, choice: .create) else {
			return XCTFail("client roots may not nest")
		}
		XCTAssertTrue(nested.message.contains("overlap"), nested.message)
		XCTAssertFalse(FileManager.default.fileExists(atPath: inner.appending(path: ".vaultday").path), "a refused folder gets no manifest")
		XCTAssertNotNil(workspaces.addWorkspace(folder: inner, sphere: .life, name: "Acme life"), "a life workspace may nest inside client work")
		XCTAssertEqual(workspaces.entries.count, 2)
	}

	// MARK: Folders

	func testAFolderOutsideTheRootIsLinkedAndKeepsItsGrant() throws {
		let (harness, workspaces, entry, root) = try personalWorkspace()
		let recipes = try TestFixtures.emptyFolder(self, named: "Recipes")
		try Data("# Bread\n".utf8).write(to: recipes.appending(path: "bread.md"))

		let pending = try ready(workspaces.offerFolder(recipes, to: entry.id))
		XCTAssertTrue(workspaces.linkFolder(pending), workspaces.message ?? "")

		let link = root.appending(path: "Recipes")
		XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: link.path), recipes.resolvingSymlinksInPath().path)
		XCTAssertTrue(FileManager.default.fileExists(atPath: recipes.appending(path: "bread.md").path), "the folder stays where it is")
		let updated = try XCTUnwrap(workspaces.entry(id: entry.id))
		XCTAssertEqual(updated.folders.map(\.role), [.root, .linked(link: root.resolvingSymlinksInPath().appending(path: "Recipes").path)])
		XCTAssertEqual(updated.folders.last?.kindLabel, "Linked folder")
		XCTAssertEqual(harness.session.appState.folderGrants().count, 2, "the link's target keeps a grant of its own")

		XCTAssertEqual(try workspaceLinkedFolders(root: root.path).map(\.name), ["Recipes"])

		workspaces.unlinkFolder(try XCTUnwrap(updated.folders.last).id, from: entry.id)
		XCTAssertFalse(FileManager.default.fileExists(atPath: link.path))
		XCTAssertTrue(FileManager.default.fileExists(atPath: recipes.appending(path: "bread.md").path), "unlinking keeps the notes")
		XCTAssertEqual(try XCTUnwrap(workspaces.entry(id: entry.id)).folders.map(\.role), [.root])
		XCTAssertEqual(harness.session.appState.folderGrants().count, 1)
	}

	func testAFolderMovedIntoTheRootNeedsNoGrant() throws {
		let (harness, workspaces, entry, root) = try personalWorkspace()
		let journal = try TestFixtures.emptyFolder(self, named: "Journal")
		try Data("# Day\n".utf8).write(to: journal.appending(path: "day.md"))

		let pending = try ready(workspaces.offerFolder(journal, to: entry.id))
		XCTAssertTrue(workspaces.moveFolder(pending), workspaces.message ?? "")

		XCTAssertFalse(FileManager.default.fileExists(atPath: journal.path))
		XCTAssertTrue(FileManager.default.fileExists(atPath: root.appending(path: "Journal/day.md").path))
		let updated = try XCTUnwrap(workspaces.entry(id: entry.id))
		XCTAssertEqual(updated.folders.map(\.role), [.root])
		XCTAssertEqual(updated.subfolders, ["Journal"])
		XCTAssertEqual(harness.session.appState.folderGrants().count, 1)
	}

	func testANewFolderIsMadeInsideTheRoot() throws {
		let (_, workspaces, entry, root) = try personalWorkspace()
		XCTAssertTrue(workspaces.createFolder(named: "Recipes", in: entry.id))
		XCTAssertTrue(FileManager.default.fileExists(atPath: root.appending(path: "Recipes").path))
		XCTAssertEqual(try XCTUnwrap(workspaces.entry(id: entry.id)).subfolders, ["Recipes"])

		XCTAssertFalse(workspaces.createFolder(named: "Recipes", in: entry.id))
		XCTAssertEqual(workspaces.message, "Personal already has a folder named Recipes")
		XCTAssertFalse(workspaces.createFolder(named: "a/b", in: entry.id))
		XCTAssertFalse(workspaces.createFolder(named: "  ", in: entry.id))
	}

	func testAFolderThatBelongsToAWorkspaceIsRefusedWithItsName() throws {
		let (_, workspaces, entry, root) = try personalWorkspace()
		let vault = try TestFixtures.copyOfMockVault(self)
		XCTAssertNotNil(workspaces.addWorkspace(folder: vault, sphere: .work))

		guard case let .refused(other) = workspaces.offerFolder(vault.appending(path: "Projects"), to: entry.id) else {
			return XCTFail("another workspace's folder is refused")
		}
		XCTAssertEqual(other.workspaceName, "Mock Studio")
		XCTAssertEqual(other.title, "Projects belongs to Mock Studio")
		XCTAssertTrue(other.message.hasSuffix("already belongs to Mock Studio"), other.message)

		try FileManager.default.createDirectory(at: root.appending(path: "Inside"), withIntermediateDirectories: true)
		guard case let .refused(own) = workspaces.offerFolder(root.appending(path: "Inside"), to: entry.id) else {
			return XCTFail("a folder already in the root is refused")
		}
		XCTAssertEqual(own.title, "Inside is already in Personal")
		XCTAssertFalse(FileManager.default.fileExists(atPath: root.appending(path: "Projects").path))
	}

	// MARK: Moving between spheres and workspaces

	func testAWorkspaceMovedToTheOtherSphereChangesItsKind() throws {
		let (_, workspaces, entry, root) = try personalWorkspace()
		var changes = 0
		workspaces.onChange = { changes += 1 }

		XCTAssertNil(workspaces.moveWorkspace(entry.id, to: .work))
		XCTAssertEqual(workspaces.entry(id: entry.id)?.sphere, .work)
		XCTAssertEqual(try TestFixtures.settingsJSON(in: root)["kind"] as? String, "work")
		XCTAssertEqual(try TestFixtures.settingsJSON(in: root)["name"] as? String, "Personal", "every other key stays")
		XCTAssertEqual(changes, 1)
		XCTAssertNil(workspaces.moveWorkspace(entry.id, to: .work), "a move to its own sphere does nothing")
		XCTAssertEqual(changes, 1)
	}

	func testAMoveToWorkThatWouldNestClientWorkIsRefused() throws {
		let outer = try TestFixtures.emptyFolder(self, named: "clients")
		let workspaces = WorkspacesModel(session: try OnboardingHarness(self).session)
		XCTAssertNotNil(workspaces.addWorkspace(folder: outer, sphere: .work, name: "Clients"))
		let inner = outer.appending(path: "acme", directoryHint: .isDirectory)
		try FileManager.default.createDirectory(at: inner, withIntermediateDirectories: true)
		let acme = try XCTUnwrap(workspaces.addWorkspace(folder: inner, sphere: .life, name: "Acme life"))

		let problem = try XCTUnwrap(workspaces.moveWorkspace(acme.id, to: .work))
		XCTAssertEqual(problem.workspaceName, "Clients")
		XCTAssertTrue(problem.message.contains("overlap"), problem.message)
		XCTAssertEqual(workspaces.entry(id: acme.id)?.sphere, .life)
		XCTAssertEqual(try TestFixtures.settingsJSON(in: inner)["kind"] as? String, "life")
	}

	func testALinkedFolderMovesToAnotherWorkspaceAsALinkOrIntoItsRoot() throws {
		let (harness, workspaces, personal, personalRoot) = try personalWorkspace()
		let studioRoot = try TestFixtures.emptyFolder(self, named: "Studio")
		let studio = try XCTUnwrap(workspaces.addWorkspace(folder: studioRoot, sphere: .work, name: "Studio"))
		let recipes = try TestFixtures.emptyFolder(self, named: "Recipes")
		XCTAssertTrue(workspaces.linkFolder(try ready(workspaces.offerFolder(recipes, to: personal.id))))
		let linked = try XCTUnwrap(workspaces.entry(id: personal.id)?.folders.last)

		let offer = try ready(workspaces.offerMove(.linked(folderId: linked.id), from: personal.id, to: studio.id))
		XCTAssertTrue(offer.canLink)
		XCTAssertTrue(workspaces.linkFolder(offer), workspaces.message ?? "")
		XCTAssertFalse(FileManager.default.fileExists(atPath: personalRoot.appending(path: "Recipes").path), "the old link goes")
		XCTAssertNotNil(try? FileManager.default.destinationOfSymbolicLink(atPath: studioRoot.appending(path: "Recipes").path))
		XCTAssertEqual(workspaces.entry(id: personal.id)?.folders.map(\.role), [.root])
		XCTAssertEqual(workspaces.entry(id: studio.id)?.folders.count, 2)
		XCTAssertEqual(harness.session.appState.folderGrants().count, 3, "two roots and the one target")

		let back = try XCTUnwrap(workspaces.entry(id: studio.id)?.folders.last)
		let move = try ready(workspaces.offerMove(.linked(folderId: back.id), from: studio.id, to: personal.id))
		XCTAssertTrue(workspaces.moveFolder(move), workspaces.message ?? "")
		XCTAssertFalse(FileManager.default.fileExists(atPath: recipes.path), "the folder itself moved")
		XCTAssertEqual(workspaces.entry(id: personal.id)?.subfolders, ["Recipes"])
		XCTAssertEqual(workspaces.entry(id: studio.id)?.folders.map(\.role), [.root])
		XCTAssertEqual(harness.session.appState.folderGrants().count, 2)
	}

	func testAFolderInsideARootMovesToAnotherRootButCannotBeLinked() throws {
		let (_, workspaces, personal, personalRoot) = try personalWorkspace()
		let studioRoot = try TestFixtures.emptyFolder(self, named: "Studio")
		let studio = try XCTUnwrap(workspaces.addWorkspace(folder: studioRoot, sphere: .work, name: "Studio"))
		XCTAssertTrue(workspaces.createFolder(named: "Journal", in: personal.id))

		let offer = try ready(workspaces.offerMove(.inside(name: "Journal"), from: personal.id, to: studio.id))
		XCTAssertFalse(offer.canLink, "a folder inside one root cannot be linked into another")
		XCTAssertFalse(workspaces.linkFolder(offer))
		XCTAssertTrue(workspaces.moveFolder(offer), workspaces.message ?? "")
		XCTAssertFalse(FileManager.default.fileExists(atPath: personalRoot.appending(path: "Journal").path))
		XCTAssertEqual(workspaces.entry(id: studio.id)?.subfolders, ["Journal"])
	}

	func testARootFolderCannotBeMoved() throws {
		let (_, workspaces, personal, _) = try personalWorkspace()
		let studio = try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Studio"), sphere: .work, name: "Studio"))
		guard case .failed = workspaces.offerMove(.linked(folderId: personal.rootConnection), from: personal.id, to: studio.id) else {
			return XCTFail("a workspace's own folder stays its root")
		}
		guard case .failed = workspaces.offerMove(.inside(name: ".vaultday"), from: personal.id, to: studio.id) else {
			return XCTFail("only plain folders inside the root move")
		}
	}

	func testDragItemsSurviveTheirTextForm() {
		let items: [SetupDragItem] = [
			.workspace(id: "ws-1"),
			.folder(workspaceId: "ws-1", origin: .linked(folderId: "con-2")),
			.folder(workspaceId: "ws-1", origin: .inside(name: "Recipes and more")),
		]
		for item in items {
			XCTAssertEqual(SetupDragItem(text: item.text), item)
		}
		XCTAssertNil(SetupDragItem(text: "plain text from elsewhere"))
	}

	// MARK: Converting further folders

	func testFurtherFoldersOfAnEarlierBuildAreAskedAboutOnceAndLinked() throws {
		let harness = try OnboardingHarness(self)
		let (workspaceId, root, docs) = try workspaceWithAFurtherFolder(harness)

		let workspaces = WorkspacesModel(session: harness.session)
		XCTAssertEqual(workspaces.pendingConversion.map(\.path), [docs.resolvingSymlinksInPath().path])
		XCTAssertEqual(workspaces.entry(id: workspaceId)?.folders.map(\.role), [.root, .unlinked])

		workspaces.convertFolders(.link)
		XCTAssertEqual(workspaces.pendingConversion, [])
		XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: root.appending(path: "docs").path), docs.resolvingSymlinksInPath().path)
		XCTAssertEqual(workspaces.entry(id: workspaceId)?.folders.count, 2)
		XCTAssertTrue(harness.session.appState.foldersConverted())

		try FileManager.default.removeItem(at: root.appending(path: "docs"))
		let relaunched = WorkspacesModel(session: try harness.relaunched().session)
		XCTAssertEqual(relaunched.pendingConversion, [], "asked once: a link removed later shows as missing, no new question")
		XCTAssertEqual(relaunched.entry(id: workspaceId)?.folders.last?.kindLabel, "Link missing")
	}

	func testFurtherFoldersCanBeMovedIntoTheRootInstead() throws {
		let harness = try OnboardingHarness(self)
		let (workspaceId, root, docs) = try workspaceWithAFurtherFolder(harness)
		let workspaces = WorkspacesModel(session: harness.session)

		workspaces.convertFolders(.move)
		XCTAssertFalse(FileManager.default.fileExists(atPath: docs.path))
		XCTAssertTrue(FileManager.default.fileExists(atPath: root.appending(path: "docs/a.md").path))
		XCTAssertEqual(workspaces.entry(id: workspaceId)?.folders.map(\.role), [.root])
		XCTAssertEqual(harness.session.appState.folderGrants().count, 1, "a moved folder needs no grant")
	}

	func testANewDeviceWithNothingToConvertIsNeverAsked() throws {
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		XCTAssertEqual(workspaces.pendingConversion, [])
		XCTAssertTrue(harness.session.appState.foldersConverted())
	}

	// MARK: Deactivating

	func testAnInactiveWorkspaceStaysListedButLeavesTheActiveListAndLaunch() throws {
		let harness = try OnboardingHarness(self)
		let vault = try TestFixtures.copyOfMockVault(self)
		let workspaces = WorkspacesModel(session: harness.session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: vault, sphere: .work))
		let life = try XCTUnwrap(workspaces.addWorkspace(folder: try TestFixtures.emptyFolder(self, named: "Home"), sphere: .life))
		var changes = 0
		workspaces.onChange = { changes += 1 }

		workspaces.setActive(false, workspaceId: entry.id)
		XCTAssertEqual(changes, 1)
		XCTAssertEqual(workspaces.entries.map(\.id), [entry.id, life.id], "the steps and Settings still list it")
		XCTAssertEqual(workspaces.entry(id: entry.id)?.isActive, false)
		XCTAssertEqual(workspaces.activeEntries.map(\.id), [life.id])
		XCTAssertEqual(harness.session.appState.folderGrants().count, 2, "its grant stays")
		XCTAssertEqual(try TestFixtures.settingsJSON(in: vault)["active"] as? Bool, false)

		try harness.session.edit { try $0.setLastSingleWorkspace(workspace: entry.id) }
		let opened = WorkspaceModel(session: harness.session)
		opened.restoreAtLaunch()
		XCTAssertEqual(opened.phase, .open)
		XCTAssertEqual(opened.workspaceId, life.id, "launch never opens an inactive workspace")

		workspaces.setActive(true, workspaceId: entry.id)
		XCTAssertEqual(workspaces.activeEntries.map(\.id), [entry.id, life.id])
		XCTAssertNil(try TestFixtures.settingsJSON(in: vault)["active"])
	}

	// MARK: Removing

	func testRemovingAWorkspaceDestroysItsKeyArchivesItsSyncFolderAndAllowsAddingItAgain() throws {
		let vault = try TestFixtures.copyOfMockVault(self)
		let harness = try OnboardingHarness(self)
		let workspaces = WorkspacesModel(session: harness.session)
		var changes = 0
		workspaces.onChange = { changes += 1 }
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: vault, sphere: .work))
		// Start the workspace's app state so there is a key and a sync folder to remove.
		let opened = WorkspaceModel(session: harness.session)
		opened.restoreAtLaunch()
		XCTAssertEqual(opened.phase, .open)
		opened.close()
		XCTAssertTrue(harness.keychain.storedItems.keys.contains { $0.account == entry.id })

		workspaces.removeWorkspace(id: entry.id)

		XCTAssertEqual(workspaces.entries, [])
		XCTAssertFalse(harness.keychain.storedItems.keys.contains { $0.account == entry.id })
		let vaultday = try FileManager.default.contentsOfDirectory(atPath: vault.appending(path: ".vaultday").path)
		XCTAssertTrue(vaultday.contains { $0.hasPrefix("sync-removed-") })
		XCTAssertGreaterThanOrEqual(changes, 2)
		XCTAssertNotNil(workspaces.addWorkspace(folder: vault, sphere: .work))
	}

	func testAWorkspaceFolderThatHoldsAnotherDevicesStateIsRefusedUntilItArrives() throws {
		// The folder holds app state from another device but not its workspace.id yet.
		let vault = try TestFixtures.copyOfMockVault(self)
		let other = try OnboardingHarness(self)
		let first = WorkspacesModel(session: other.session)
		XCTAssertNotNil(first.addWorkspace(folder: vault, sphere: .work))
		let opened = WorkspaceModel(session: other.session)
		opened.restoreAtLaunch()
		opened.close()
		try FileManager.default.removeItem(at: vault.appending(path: ".vaultday/sync/workspace.id"))

		let second = WorkspacesModel(session: try OnboardingHarness(self).session)
		XCTAssertNil(second.addWorkspace(folder: vault, sphere: .work))
		XCTAssertNotNil(second.message)
		XCTAssertEqual(second.entries, [])
	}

	// MARK: Helpers

	private struct UnexpectedOutcome: Error, CustomStringConvertible {
		let description: String
	}

	private func draft(_ outcome: WorkspaceDraftOutcome) throws -> WorkspaceDraft {
		switch outcome {
		case let .draft(draft): return draft
		case let .refused(problem): throw UnexpectedOutcome(description: "expected a draft, got a refusal: \(problem.message)")
		case let .failed(reason): throw UnexpectedOutcome(description: "expected a draft, got a failure: \(reason)")
		}
	}

	private func ready(_ outcome: FolderOffer) throws -> PendingFolder {
		switch outcome {
		case let .ready(pending): return pending
		case let .refused(problem): throw UnexpectedOutcome(description: "expected the folder to be accepted, got: \(problem.message)")
		case let .failed(reason): throw UnexpectedOutcome(description: "expected the folder to be accepted, got: \(reason)")
		}
	}

	private func personalWorkspace() throws -> (OnboardingHarness, WorkspacesModel, WorkspaceEntry, URL) {
		let harness = try OnboardingHarness(self)
		let root = try TestFixtures.emptyFolder(self, named: "Personal")
		let workspaces = WorkspacesModel(session: harness.session)
		let entry = try XCTUnwrap(workspaces.addWorkspace(folder: root, sphere: .life, name: "Personal"))
		return (harness, workspaces, entry, root)
	}

	/// A workspace as an earlier build left it: its root plus a further
	/// folder granted beside it, with no link in the root.
	private func workspaceWithAFurtherFolder(_ harness: OnboardingHarness) throws -> (String, URL, URL) {
		let root = try TestFixtures.emptyFolder(self, named: "Freelancing")
		let docs = try TestFixtures.emptyFolder(self, named: "docs")
		try Data("# Doc\n".utf8).write(to: docs.appending(path: "a.md"))
		_ = try createWorkspace(root: root.path, name: "Freelancing", kind: .work)
		let session = harness.session
		session.environment.files.allow(root.resolvingSymlinksInPath())
		let workspaceId = try claimWorkspaceStateId(files: session.environment.files, workspaceRoot: root.resolvingSymlinksInPath().path)
		for (connection, folder) in [("con-root", root), ("con-docs", docs)] {
			let canonical = folder.resolvingSymlinksInPath()
			try session.edit { appState in
				try appState.putConnection(connection: ConnectionView(id: connection, kind: .folder, label: canonical.lastPathComponent))
				try appState.putFolderGrant(grant: FolderGrantView(connection: connection, bookmark: try session.environment.grants.makeBookmark(for: canonical), path: canonical.path))
				try appState.addSource(source: SourceView(workspace: workspaceId, connection: connection, subpath: ""))
			}
		}
		return (workspaceId, root, docs)
	}
}
